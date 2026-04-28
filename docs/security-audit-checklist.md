# Security Audit Checklist

Pre-audit and auditor reference checklist for QuorumCredit. Every item must be
reviewed before mainnet deployment. Link each finding to the relevant section of
[docs/threat-model.md](./threat-model.md).

---

## 1. Reentrancy

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 1.1 | All token transfers (`transfer`, `transfer_from`) happen **after** state mutations (checks-effects-interactions) | ☐ | Verify in `loan.rs`, `vouch.rs`, `governance.rs` |
| 1.2 | No cross-contract calls made before internal state is fully updated | ☐ | Soroban host serialises calls, but verify ordering |
| 1.3 | `repay` updates `loan.status` to `Repaid` before transferring tokens to vouchers | ☐ | |
| 1.4 | `slash` / `execute_slash_vote` marks loan as `Defaulted` before burning stakes | ☐ | |
| 1.5 | `withdraw_vouch` removes the vouch record before returning stake | ☐ | |

---

## 2. Integer Overflow / Underflow

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 2.1 | `Cargo.toml` `[profile.release]` has `overflow-checks = true` | ☐ | Already set — verify not overridden |
| 2.2 | All stake summation uses `checked_add` or panics on overflow (`StakeOverflow`) | ☐ | `helpers::next_loan_id`, `vouch.rs` |
| 2.3 | Yield calculation `stake * yield_bps / BPS_DENOMINATOR` cannot overflow `i128` for max stake values | ☐ | Max i128 ≈ 1.7 × 10³⁸; max XLM supply ≈ 5 × 10¹⁵ stroops |
| 2.4 | Slash calculation `stake * slash_bps / BPS_DENOMINATOR` is safe for all valid inputs | ☐ | |
| 2.5 | Loan ID counter uses `checked_add` | ☐ | `helpers::next_loan_id` |
| 2.6 | No subtraction that could underflow (e.g., `amount_repaid - amount`) | ☐ | |

---

## 3. Authentication & Authorization Bypass

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 3.1 | `initialize` requires `deployer.require_auth()` | ☐ | Prevents front-running |
| 3.2 | `repay` verifies caller == `loan.borrower` (`UnauthorizedCaller`) | ☐ | Issue 108 fix |
| 3.3 | All admin functions call `require_admin_approval` with threshold check | ☐ | `pause`, `unpause`, `set_config`, `slash`, `upgrade` |
| 3.4 | `require_admin_approval` calls `signer.require_auth()` for every signer | ☐ | |
| 3.5 | `vote_slash` verifies caller is a registered voucher for the borrower | ☐ | |
| 3.6 | `withdraw_vouch` verifies caller is the voucher on record | ☐ | |
| 3.7 | `upgrade` requires admin quorum — single key cannot upgrade unilaterally | ☐ | |
| 3.8 | No function accepts an arbitrary `Address` as an auth bypass | ☐ | |
| 3.9 | Self-vouch is rejected (`SelfVouchNotAllowed`) | ☐ | |

---

## 4. Storage Expiry & TTL Management

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 4.1 | Persistent storage entries (`Loan`, `Vouches`, `ActiveLoan`) have TTL extended on every access | ☐ | Soroban persistent storage expires; verify `extend_ttl` calls |
| 4.2 | Instance storage entries (`Config`, `Paused`, `SlashTreasury`) have TTL extended on initialization and admin updates | ☐ | |
| 4.3 | TTL extension values are sufficient for the expected loan duration (`DEFAULT_LOAN_DURATION = 30 days`) | ☐ | |
| 4.4 | Expired storage does not silently return `None` in a way that bypasses security checks | ☐ | e.g., `get_active_loan_record` returning `NoActiveLoan` on expired entry |
| 4.5 | Temporary storage is not used for security-critical state | ☐ | |

---

## 5. Soroban-Specific: Host Function Panics

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 5.1 | All `panic_with_error!` calls use typed `ContractError` variants (not raw strings) | ☐ | Enables integrators to match on error codes |
| 5.2 | No `unwrap()` or `expect()` on user-controlled inputs that could panic | ☐ | Use `?` or explicit error handling |
| 5.3 | `require_valid_token` uses `try_balance` to avoid host trap on invalid address | ☐ | |
| 5.4 | Cross-contract calls (`token::Client`) use `try_` variants where failure is recoverable | ☐ | |
| 5.5 | No infinite loops or unbounded iterations over user-supplied data | ☐ | e.g., `get_vouches` iterating over `Vec<VouchRecord>` |

---

## 6. Soroban-Specific: Ledger Limits

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 6.1 | `Vec<VouchRecord>` per borrower is bounded by `DEFAULT_MAX_VOUCHERS_PER_BORROWER` | ☐ | Prevents storage bloat |
| 6.2 | `batch_vouch` input length is validated before processing | ☐ | |
| 6.3 | No single transaction writes an unbounded number of storage entries | ☐ | |
| 6.4 | `loan_purpose` string length is validated (max length enforced) | ☐ | |
| 6.5 | Contract WASM size is within Soroban limits (< 64 KB optimised) | ☐ | Check with `wasm-opt` output |

---

## 7. Business Logic

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 7.1 | Loan amount ≤ total vouched stake × `max_loan_to_stake_ratio / 100` | ☐ | I2 invariant |
| 7.2 | Loan cannot be disbursed if contract balance < loan amount | ☐ | I1 invariant |
| 7.3 | Yield reserve is sufficient before disbursement | ☐ | See threat-model.md §Reserve Draining |
| 7.4 | `amount_repaid` never exceeds `amount + total_yield` | ☐ | I4 invariant |
| 7.5 | Loan status transitions are monotonic (`None → Active → Repaid|Defaulted`) | ☐ | I5 invariant |
| 7.6 | Slash treasury is non-negative at all times | ☐ | I6 invariant |
| 7.7 | `yield_bps` and `slash_bps` are bounded to `[0, 10_000]` | ☐ | I7 invariant |
| 7.8 | Vouch cooldown prevents rapid re-vouching to game eligibility | ☐ | `DEFAULT_VOUCH_COOLDOWN_SECS` |
| 7.9 | `MIN_VOUCH_AGE` prevents flash-vouch attacks | ☐ | `VouchTooRecent` error |
| 7.10 | Blacklisted borrowers cannot request loans | ☐ | `Blacklisted` error |

---

## 8. Token Safety

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 8.1 | Only SEP-41-compliant tokens are accepted (`require_valid_token`) | ☐ | `InvalidToken` error |
| 8.2 | Token address is validated against `allowed_tokens` list | ☐ | |
| 8.3 | Token transfers check return values / use `try_` variants | ☐ | |
| 8.4 | No token address can be the zero address | ☐ | `ZeroAddress` error |
| 8.5 | Duplicate tokens in `allowed_tokens` are rejected | ☐ | `DuplicateToken` error |

---

## 9. Governance & Upgrade Safety

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 9.1 | `upgrade` requires admin quorum (not a single key) | ☐ | |
| 9.2 | Timelock delay (`TIMELOCK_DELAY = 24 h`) is enforced before executing governance actions | ☐ | |
| 9.3 | Timelock expiry (`TIMELOCK_EXPIRY = 72 h`) prevents stale actions from executing | ☐ | |
| 9.4 | Slash vote quorum is correctly calculated as a fraction of total stake | ☐ | |
| 9.5 | `execute_slash_vote` is idempotent — cannot execute twice (`SlashAlreadyExecuted`) | ☐ | Issue 109 fix |
| 9.6 | Admin removal cannot reduce admin count below `admin_threshold` | ☐ | I8 invariant |

---

## 10. Denial of Service

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 10.1 | `pause` / `unpause` mechanism works correctly under all conditions | ☐ | |
| 10.2 | A single malicious voucher cannot block a slash vote | ☐ | Quorum based on stake weight |
| 10.3 | A borrower cannot prevent repayment by manipulating loan state | ☐ | |
| 10.4 | Contract cannot be permanently locked by a single admin key compromise | ☐ | Multisig required |

---

## Pre-Audit Checklist

Before engaging an audit firm, confirm:

- [ ] All unit tests pass (`cargo test`)
- [ ] No `cargo clippy` warnings (`cargo clippy -- -D warnings`)
- [ ] Code formatted (`cargo fmt --check`)
- [ ] `cargo audit` shows no high-severity advisories
- [ ] All invariant tests pass (`cargo test invariants`)
- [ ] All regression tests pass (`cargo test regression`)
- [ ] Testnet integration tests pass (`./scripts/testnet_integration_test.sh`)
- [ ] WASM size within limits (`ls -lh target/wasm32-unknown-unknown/release/*.wasm`)
- [ ] Codebase frozen (no new features during audit window)
- [ ] Technical documentation provided to auditors (README, this checklist, threat-model.md)

---

## Recommended Audit Firms

| Firm | Stellar/Soroban Experience |
|------|---------------------------|
| CertiK | Yes — Stellar ecosystem audits |
| Trail of Bits | Deep Rust/WASM expertise |
| Halborn | Blockchain security specialists |
| OtterSec | Rust smart contract focus |
| Quantstamp | Automated + manual analysis |

---

## See Also

- [Threat Model](./threat-model.md)
- [Contract Invariants](./contract-invariants.md)
- [Security Best Practices](../SECURITY_BEST_PRACTICES.md)
- [SECURITY.md](../SECURITY.md)
