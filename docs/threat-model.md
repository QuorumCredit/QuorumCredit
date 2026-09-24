# Threat Model

## Executive Summary

This document is the protocol's consolidated threat model. It covers two areas:

1. **Yield Reserve Depletion** — the original, narrowly-scoped analysis of attacks against
   protocol solvency (below).
2. **Protocol-Wide Threat Model** — reentrancy, access control, and oracle manipulation,
   with attack trees, a per-component risk assessment, documented mitigations and residual
   risk, and the trust model/assumptions the contract relies on.

The previous version of this document covered only reserve depletion; it did not account for
reentrancy, access-control bypass, or oracle manipulation as first-class threat categories.
This revision closes that gap.

## Protocol-Wide Threat Model

### Scope and Method

Each attack vector below is documented with: preconditions, an attack tree (root goal
decomposed into AND/OR sub-goals), impact, likelihood, current mitigations, and **residual
risk** — what remains exploitable even after mitigations are applied. Residual risk is rated
Low/Medium/High and is the figure operators should track over time, not the raw likelihood.

### Attack Vector: Reentrancy

**Description:** A malicious token contract or callback re-enters a state-mutating entry
point (`vouch`, `repay`, `transfer_vouch`, `delegate_vouch`, `revoke_delegation`,
`set_vouch_expiry`, `split_vouch`, `rotate_to_new_borrower`) mid-execution, observing or
mutating storage before the first call finishes writing its state.

**Preconditions:**
- The configured token contract's `transfer` (or a hook it invokes) executes attacker-controlled code.
- A state-mutating entry point performs a token transfer before all storage writes are finalized (a "check-effects-interaction" ordering violation).

**Attack Tree:**
```
GOAL: Drain funds or corrupt accounting via reentrancy
├── OR 1: Re-enter during `vouch()`
│   ├── AND: attacker supplies a malicious token as the vouch token
│   ├── AND: token's transfer callback re-enters `vouch`/`withdraw_vouch`
│   └── AND: reentrant call reads stake/lock state written before the outer call's transfer
├── OR 2: Re-enter during `repay()` / `verify_repayment()`
│   ├── AND: malicious loan token's transfer hook re-enters `repay`
│   └── AND: reentrant call double-spends the same repayment against remaining balance
├── OR 3: Re-enter during vouch transfer/delegation
│   ├── AND: re-entering `transfer_vouch` or `delegate_vouch` mid-call
│   └── AND: duplicate or inconsistent vouch records are created across the two nested calls
└── OR 4: Bypass the global reentrancy lock
    ├── AND: find an entry point that mutates state without calling `acquire_lock`/`release_lock`
    └── AND: exploit the unlocked path to re-enter a locked one indirectly
```

**Impact:** Double-spend of stake or repayment funds, corrupted vouch/loan accounting, fund loss.

**Likelihood:** Low — the contract already uses an explicit `acquire_lock`/`release_lock`
pair around the highest-risk entry points (`vouch`, `repay`, `transfer_vouch`,
`delegate_vouch`, `revoke_delegation`, `set_vouch_expiry`, `split_vouch`,
`rotate_to_new_borrower`; see `src/lib.rs`), and Soroban's synchronous, single-threaded host
execution model reduces (but does not eliminate) classic EVM-style reentrancy surface.

**Mitigations:**
- Global per-invocation lock (`acquire_lock` / `release_lock`) around state-mutating entry points.
- `require_allowed_token` restricts which token contracts can be used at all, shrinking the population of tokens that could carry a malicious transfer hook.
- Checks-effects-interactions ordering: storage is updated before external token transfers where the lock is not the only guard (e.g. `slash`, `auto_slash`).

**Residual Risk: Medium.** Not every state-mutating path is covered by the lock (e.g.
`slash`, `auto_slash`, `claim_expired_loan` rely on ordering rather than the lock), and new
entry points added over time risk omitting `acquire_lock` by oversight. Any newly-added
function that transfers tokens and touches shared per-borrower/per-voucher state should
either take the lock or be reviewed explicitly for checks-effects-interactions ordering.

### Attack Vector: Access Control Bypass

**Description:** An unauthorized caller executes an admin-, oracle-, or borrower-gated
operation by exploiting a missing `require_auth()`, a missing admin-threshold check, or an
identity-confusion bug (e.g. passing someone else's `Address` as `admin_signers` for a
one-of-many check where an all-of check was intended).

**Preconditions:**
- An entry point exists (or is added later) that mutates security-relevant state without calling `require_auth()` on the correct principal.
- Or: `require_admin_approval` / `require_admin_signers` accepts a signer set that does not meet `admin_threshold`, or accepts non-admin addresses.

**Attack Tree:**
```
GOAL: Perform a privileged action without authorization
├── OR 1: Forge borrower actions
│   ├── AND: call a borrower-gated function (e.g. `repay`, `confirm_repayment`)
│   └── AND: the function is missing/misordered `borrower.require_auth()`
├── OR 2: Forge admin actions
│   ├── AND: call an admin-gated function (`add_admin`, `slash`, `slash_treasury`, config updates)
│   ├── OR 2a: supply an `admin_signers` vector below `admin_threshold` that still passes validation
│   └── OR 2b: supply signer addresses that are not in `Config.admins`
├── OR 3: Forge oracle actions
│   ├── AND: call `set_oracle_price` / `verify_repayment`
│   └── AND: caller address is not compared against `Config.oracle_address`, or comparison is skipped on an error path
└── OR 4: Role confusion
    └── AND: a function intended for one RBAC permission bit (see `src/rbac.rs`) is reachable via a caller lacking that bit due to a missing `check_permission` call
```

**Impact:** Unauthorized slashing, unauthorized admin changes (yield/slash rates, token
allow-list), forged repayment confirmation, forged oracle price/verification — all of which
cascade into fund loss or protocol insolvency.

**Likelihood:** Low for existing, reviewed entry points (which consistently call
`require_auth()` and `require_admin_approval`/RBAC checks); Medium for future entry points
added without following the established pattern, since there is no compiler-enforced
guarantee that a new `#[contractimpl]` function checks authorization.

**Mitigations:**
- `require_auth()` on the acting principal at the top of every state-mutating function.
- `helpers::require_admin_approval` centralizes the admin-threshold check rather than reimplementing it per function.
- `rbac.rs` permission bits (`check_permission`) for finer-grained borrower/voucher capabilities beyond simple admin/non-admin.
- Oracle identity is compared against `Config.oracle_address`, a single admin-settable value, rather than an ad hoc allow-list.

**Residual Risk: Medium.** Authorization correctness in this codebase is a per-function
convention, not a type-level guarantee. A missing `require_auth()` or an admin check
performed after a side effect (rather than before) would not be caught by the compiler and
requires manual review on every change. This is the single highest-value target for
recurring code review and static analysis (`solidity-auditor`/equivalent Soroban lint passes
on every PR that touches `src/lib.rs`, `src/admin.rs`, `src/rbac.rs`).

### Attack Vector: Oracle Manipulation

**Description:** The registered oracle (`Config.oracle_address`) has privileged influence
over repayment verification (`verify_repayment`) and price feeds used for dynamic-rate
loans (`set_oracle_price`). If the oracle key is compromised, stale, or economically
incentivized to misreport, it can approve fraudulent repayments or skew variable-rate
interest calculations.

**Preconditions:**
- A single oracle address is trusted (no on-chain oracle redundancy or median-of-N).
- Oracle key compromise, or an oracle operator with a conflict of interest.
- No staleness check on `set_oracle_price` — a price can be set once and never updated.

**Attack Tree:**
```
GOAL: Extract value via oracle manipulation
├── OR 1: Forge repayment approval
│   ├── AND: compromise the oracle key (or collude with the borrower)
│   └── AND: call `verify_repayment(oracle, borrower, approved=true)` for a loan that was never actually repaid off-chain
├── OR 2: Suppress a legitimate repayment
│   └── AND: oracle refuses/delays `verify_repayment` approval to force `auto_slash` on an honest borrower
├── OR 3: Skew variable-rate pricing
│   ├── AND: push a manipulated price via `set_oracle_price`
│   └── AND: dynamic-rate loans (`rate_type: Variable`) reprice using the manipulated `index_reference`
└── OR 4: Replay/stale price
    └── AND: an old, favorable price is never superseded and continues to be used because there is no on-chain freshness check
```

**Impact:** Fraudulent fund release from escrow, wrongful slashing of honest borrowers,
mispriced variable-rate loans, and in the worst case, systemic mispricing across all
oracle-dependent loans.

**Likelihood:** Medium — this is a single point of trust by design (see Trust Model below),
so the likelihood is really "likelihood the oracle key/operator is compromised or
misbehaves," which is an operational/off-chain risk this contract cannot fully mitigate
on its own.

**Mitigations:**
- Oracle identity check (`oracle.require_auth()` plus comparison against `Config.oracle_address`) on every oracle-gated call.
- Oracle address is admin-settable, so a compromised oracle can be rotated out via the multisig.
- Escrow-based repayment (`EscrowStatus::Pending` → `Released`/`Rejected`) means a rejected verification returns funds to the borrower rather than silently failing, limiting one-sided fund loss.

**Residual Risk: High.** There is no on-chain price/verification staleness window, no
multi-oracle quorum, and no slashing/bonding mechanism for oracle misbehavior. Until a
median-of-N or staleness-gated design ships, oracle compromise remains a full-trust,
single-key risk. Operators should treat oracle key management (HSM-backed signing, key
rotation runbook, monitoring for anomalous `verify_repayment`/`set_oracle_price` calls) as
equivalent in sensitivity to admin multisig key management.

### Per-Component Risk Assessment

| Component | Primary Threats | Current Controls | Residual Risk |
|---|---|---|---|
| Vouch lifecycle (`vouch.rs`) | Reentrancy, stake accounting errors, sybil vouching | Reentrancy lock, min stake, cooldown, sybil cost estimator | Medium |
| Loan lifecycle (`loan.rs`, `lib.rs`) | Reentrancy, double-disbursement, rate-limit bypass | Reentrancy lock, `has_active_loan` check, rate limiting | Medium |
| Slashing (`slash`/`auto_slash`) | Access control bypass, incorrect slash math | Admin-threshold approval (manual slash); permissionless-but-deadline-gated (auto_slash) | Medium |
| Admin/config (`admin.rs`) | Access control bypass, key compromise | Multisig threshold, two-step admin transfer | Medium |
| Oracle integration (`verify_repayment`, `set_oracle_price`) | Oracle manipulation, stale prices | Address pinning, `require_auth` | High |
| Yield reserve | Insolvency via over-promising or drain | See "Yield Reserve Depletion" section above | Medium |
| Cross-chain bridge (`bridge.rs`, `cross_chain.rs`) | Forged attestations, replay | Bridge public key pinning per origin chain, nonce tracking | Medium |
| Zero-knowledge paths (`zk_snarks.rs`) | Invalid/forged proofs accepted as valid | Proof verification before state mutation, audit trail (`ZkProofRecord`) | Medium |

### Trust Model and Assumptions

This contract makes the following trust assumptions. Anyone relying on the protocol
(borrowers, vouchers, integrators) should understand these are **assumed**, not
cryptographically enforced, unless stated otherwise:

1. **Admin multisig honesty-in-aggregate.** The admin set is trusted not to collude below
   `admin_threshold` to push malicious config (e.g. unsustainable yield, disabling checks).
   Enforcement: on-chain threshold signature check. Not enforced: collusion above threshold.
2. **Oracle honesty and availability.** `Config.oracle_address` is trusted to report
   accurate repayment status and prices, and to remain available. Enforcement: identity
   pinning only. Not enforced: correctness or liveness of the oracle's off-chain data source.
3. **Token contract non-maliciousness.** Tokens passed through `require_allowed_token` are
   assumed to implement the standard token interface without malicious transfer hooks.
   Enforcement: allow-list membership. Not enforced: static or dynamic analysis of the
   token contract's own code.
4. **Bridge attestation authenticity.** Cross-chain vouches/loans trust that
   `BridgePublicKey(origin_chain)` correctly identifies the bridge relay for that chain, and
   that the relay itself is honest. Enforcement: Ed25519 signature verification, nonce
   replay protection. Not enforced: correctness of the origin chain's own state.
- **Reentrancy lock covers "the important paths."** See residual risk above — this is a
  convention, not a proof of coverage.
5. **Borrowers and vouchers are pseudonymous but not anonymous economic actors.** Sybil
  resistance relies on economic cost (`estimate_sybil_attack_cost`), not identity
  verification.

Where an assumption is violated (e.g. oracle key compromised, admin threshold colluding,
malicious token onboarded), the contract's guarantees degrade to whatever the remaining,
uncompromised controls provide — which is why residual risk is tracked per-component above
rather than assumed away.

### Confidentiality Model: `zk_snarks.rs` Is Not Zero-Knowledge

**This is the single most important disclaimer in this document for integrators.**
`src/zk_snarks.rs` (types `ZkProof`, `ConfidentialCommitment`; functions
`create_vouch_proof`, `create_loan_proof`, `commit_amount`, `reveal_commitment`, and the
`*_confidential` entry points in `lib.rs`) is **not a zero-knowledge proof system and
provides no cryptographic privacy**, despite the "zk" naming.

**What it actually is:** commitment-based self-attestation with on-chain input
verification. A "proof" is a deterministic SHA3-256 hash of values the caller supplies in
plaintext, bound to their identity, token, and amount. On-chain code checks that the hash
is well-formed and that the plaintext values satisfy protocol constraints (balance,
blacklist status, eligibility, vouch count). Nothing about the underlying amount, balance,
blacklist status, or vouch count is hidden — all of it is a plain call argument or is read
directly from on-chain storage, and is visible to any observer of the transaction or ledger
state.

**What the "commitment" gives you:** `ConfidentialCommitment` (produced by `commit_amount`)
lets a caller bind themselves to a value at commit time and later prove, via
`reveal_commitment`, that the value they settle with matches what they committed to. This
prevents a commitment/settlement-value mismatch — it does **not** hide the value from
observers at any point; the amount is a plaintext argument to `vouch_confidential` /
`request_loan_confidential` regardless of what is later revealed.

**Threat this creates:** an integrator who assumes `vouch_confidential` /
`request_loan_confidential` hide stake amounts, balances, or eligibility from
counterparties or the public ledger will build a system with a privacy hole matching that
assumption. There is no attack to mitigate here beyond correcting the assumption — the
mitigation is documentation (this section, the `zk_snarks.rs` module doc, and the doc
comments on the `*_confidential` entry points in `lib.rs`), not code, because there is no
cryptographic privacy primitive to harden.

**Residual risk:** High for any integrator who has not read this section — the naming
(`Zk*`, `PROOF_TYPE_*`) still invites the wrong assumption. Tracked in issue #1471.

---

## Yield Reserve Depletion (Original Analysis)

The yield reserve is critical to protocol solvency. This document identifies attack vectors targeting reserve depletion and mitigation strategies.

## Threat: Reserve Draining Attack

### Attack Vector 1: Yield Over-Promising

**Description:** Attacker manipulates yield rate to exceed reserve capacity.

**Preconditions:**
- Attacker controls admin multisig (compromised key)
- Yield rate set to unsustainable level (e.g., 50% instead of 2%)

**Attack Flow:**
1. Attacker calls `update_config()` with `yield_bps = 50000` (500%)
2. Borrowers request loans
3. On repayment, yield payout exceeds reserve balance
4. Contract panics with `InsufficientFunds`
5. Protocol halts

**Impact:**
- Denial of service (contract paused)
- Vouchers cannot receive yield
- Borrowers cannot repay
- Reputation damage

**Likelihood:** Low (requires admin compromise)

### Attack Vector 2: Loan Disbursement Without Reserve Check

**Description:** Contract disburses loans without verifying yield reserve sufficiency.

**Preconditions:**
- Reserve balance < (loan_amount * (1 + yield_bps/10000))
- No pre-disbursement reserve check

**Attack Flow:**
1. Attacker requests large loan
2. Contract disburses without checking reserve
3. Reserve depleted
4. Future repayments fail
5. Protocol becomes insolvent

**Impact:**
- Protocol insolvency
- Vouchers lose yield
- Borrowers cannot repay

**Likelihood:** Medium (if reserve checks not implemented)

### Attack Vector 3: Coordinated Default + Slash Drain

**Description:** Attacker coordinates defaults to drain slash treasury, then exploits reserve.

**Preconditions:**
- Attacker controls multiple borrower accounts
- Attacker controls voucher accounts
- Slash treasury used to replenish yield reserve

**Attack Flow:**
1. Attacker vouches for own borrower accounts
2. Requests large loans
3. Defaults intentionally
4. Slash treasury accumulates slashed funds
5. Attacker withdraws slash treasury
6. Yield reserve depleted for future loans

**Impact:**
- Yield reserve depletion
- Protocol insolvency
- Loss of funds for legitimate vouchers

**Likelihood:** Low (requires multiple account control)

## Threat: Yield Calculation Precision Loss

### Attack Vector 4: Rounding Down Yield to Zero

**Description:** Attacker creates many small vouches to accumulate yield through rounding errors.

**Preconditions:**
- Yield calculation uses integer division
- Minimum stake < 50 stroops (current minimum)

**Attack Flow:**
1. Attacker creates 1000 vouches of 1 stroop each
2. Loan repaid with 2% yield
3. Each vouch: `1 * 200 / 10000 = 0` (rounds down)
4. Attacker receives 0 yield but protocol owes 20 stroops
5. Repeated across many loans drains reserve

**Impact:**
- Yield reserve depletion through accumulated rounding errors
- Legitimate vouchers receive no yield

**Likelihood:** Low (minimum stake enforced at 50 stroops)

## Mitigations

### Mitigation 1: Pre-Disbursement Reserve Check

**Implementation:**
```rust
fn request_loan(...) {
    // Calculate required reserve
    let required_reserve = amount + (amount * yield_bps / 10_000);
    
    // Check reserve before disbursement
    let current_reserve = get_yield_reserve();
    assert!(current_reserve >= required_reserve, "InsufficientFunds");
    
    // Disburse loan
    transfer_to_borrower(amount);
}
```

**Effectiveness:** Prevents loans when reserve insufficient

**Operational Impact:** May reject valid loans if reserve low

### Mitigation 2: Yield Rate Bounds

**Implementation:**
```rust
const MAX_YIELD_BPS: i128 = 1000; // 10% max

fn update_config(yield_bps: i128) {
    assert!(yield_bps <= MAX_YIELD_BPS, "InvalidYield");
}
```

**Effectiveness:** Prevents unsustainable yield rates

**Operational Impact:** Limits protocol flexibility

### Mitigation 3: Reserve Monitoring and Alerts

**Implementation:**
- Prometheus metric: `qc_yield_reserve_balance`
- Alert when reserve < 110% of max loan amount
- Alert when reserve < 10% of total loan volume

**Effectiveness:** Early warning of reserve depletion

**Operational Impact:** Requires active monitoring

### Mitigation 4: Minimum Stake Enforcement

**Implementation:**
```rust
const MIN_STAKE_FOR_YIELD: i128 = 50; // stroops

fn vouch(stake: i128) {
    assert!(stake >= MIN_STAKE_FOR_YIELD, "MinStakeNotMet");
}
```

**Effectiveness:** Prevents rounding errors from small stakes

**Operational Impact:** Minimum stake requirement

### Mitigation 5: Multisig Admin Control

**Implementation:**
- All config changes require `admin_threshold` signatures
- Prevents single key compromise from changing yield rate
- Requires 2-of-3 or 3-of-5 multisig

**Effectiveness:** Prevents unilateral yield manipulation

**Operational Impact:** Slower config changes

### Mitigation 6: Reserve Replenishment Procedure

**Implementation:**
- Admin-only function to transfer XLM to contract
- Requires multisig approval
- Logged and auditable

```rust
fn replenish_reserve(admin_signers: Vec<Address>, amount: i128) {
    require_admin_approval(admin_signers);
    transfer_from_admin(amount);
}
```

**Effectiveness:** Allows reserve recovery

**Operational Impact:** Requires admin action

## Operator Recommendations

### Daily Checks

```bash
# Check reserve balance
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_fee_treasury \
  --network mainnet

# Calculate reserve health
# reserve_health = reserve / (max_loan_amount * 1.02)
# Alert if < 1.1 (110%)
```

### Weekly Review

- Review loan volume trends
- Check yield distribution
- Verify no unusual defaults
- Audit admin actions

### Monthly Actions

- Replenish reserve if needed
- Review yield rate sustainability
- Update monitoring thresholds
- Audit slash treasury

### Reserve Sizing Formula

```
Required Reserve = (Max Concurrent Loans) × (Max Loan Amount) × (1 + Yield Rate)

Example:
- Max concurrent loans: 100
- Max loan amount: 1000 XLM
- Yield rate: 2%
- Required reserve: 100 × 1000 × 1.02 = 102,000 XLM
- Recommended buffer: 110% = 112,200 XLM
```

## Detection Strategies

### Metric-Based Detection

Monitor these metrics for anomalies:

| Metric | Normal Range | Alert Threshold |
|--------|--------------|-----------------|
| Reserve balance | > 110% required | < 110% required |
| Yield payout rate | 2% of repayments | > 5% of repayments |
| Default rate | < 5% | > 10% |
| Loan volume | Steady growth | > 50% spike |

### Transaction-Based Detection

```python
def detect_reserve_drain():
    """Detect unusual reserve depletion patterns"""
    
    # Get reserve history
    reserve_history = get_reserve_history(days=7)
    
    # Calculate daily change
    daily_changes = [
        reserve_history[i] - reserve_history[i-1]
        for i in range(1, len(reserve_history))
    ]
    
    # Alert if > 20% daily decrease
    for change in daily_changes:
        if change < -0.2 * reserve_history[0]:
            alert("Unusual reserve depletion detected")
```

## Incident Response

### If Reserve Depleted

1. **Immediate (< 5 min):**
   - Pause contract
   - Alert ops team
   - Notify stakeholders

2. **Short-term (< 1 hour):**
   - Investigate cause
   - Review recent transactions
   - Check admin logs

3. **Medium-term (< 24 hours):**
   - Replenish reserve
   - Audit all loans
   - Verify yield calculations

4. **Long-term (< 1 week):**
   - Root cause analysis
   - Update monitoring
   - Implement additional safeguards

### Communication Template

```
INCIDENT: Yield Reserve Depletion

SEVERITY: Critical
TIME: [timestamp]
DURATION: [duration]

IMPACT:
- Repayment transactions failing
- Vouchers cannot receive yield
- Protocol paused

CAUSE: [root cause]

RESOLUTION:
- Reserve replenished with [amount] XLM
- Contract unpaused at [time]

PREVENTION:
- [mitigation implemented]
```

## Testing

### Stress Test: Reserve Depletion

```rust
#[test]
fn test_reserve_depletion_protection() {
    // Setup: Create contract with 100 XLM reserve
    let reserve = 100_000_000_000i128; // 100 XLM
    
    // Attempt to request loan > reserve
    let loan_amount = 150_000_000_000i128; // 150 XLM
    
    // Should fail with InsufficientFunds
    assert_eq!(
        request_loan(borrower, loan_amount, threshold, token),
        Err(ContractError::InsufficientFunds)
    );
}
```

### Fuzz Test: Yield Calculation

```rust
#[test]
fn fuzz_yield_calculation() {
    for stake in 1..1_000_000_000 {
        let yield_amount = (stake * 200) / 10_000;
        
        // Verify yield never exceeds 2%
        assert!(yield_amount <= (stake * 2) / 100);
        
        // Verify no negative yields
        assert!(yield_amount >= 0);
    }
}
```

## References

- [Yield Accounting & Solvency](../README.md#-yield-accounting--solvency)
- [Error Reference](../README.md#error-reference)
- [Deployment Guide](./deployment-guide.md)
- [Monitoring Guide](./monitoring-guide.md)

---

## Slash Appeal Mechanisms — Mutual Exclusion

The contract implements **two parallel slash-appeal paths** that were historically independent:

| Mechanism | Reference | Initiator | Decision body | Storage key |
|---|---|---|---|---|
| **Evidence-based appeal (#552)** | `appeal_slash_with_evidence` / `vote_on_slash_appeal` / `execute_slash_appeal` | Any slashed voucher | Admin multi-sig vote | `DataKey::SlashAppeal(borrower, voucher)` |
| **Escrow-quorum appeal (#841)** | `appeal_slash` / `vote_appeal` / `finalize_appeal` | Borrower | 2/3 voucher-stake quorum | `DataKey::SlashEscrow(borrower)` |

### Threat: Double-Refund via Concurrent Appeals

**Description:** If both mechanisms could be active simultaneously for the same borrower,
a voucher could receive a refund from both paths — first from an approved evidence-based appeal
(`execute_slash_appeal` transfers restored_amount to the voucher) and then again from an
approved escrow-quorum appeal (`finalize_appeal_internal` distributes escrow_amount pro-rata).

**Impact:** A voucher's effective loss from a slash (which was legitimate) would be fully or
partially reversed twice, draining up to `2 × slashed_amount` from the contract.

**Mitigation (Issue #1450):** A mutual-exclusion flag `DataKey::EvidenceAppealPending(borrower)`
is set to `true` when `appeal_slash_with_evidence` is called and cleared when
`execute_slash_appeal` runs or when `vote_on_slash_appeal` rejects the appeal. The `appeal_slash`
(#841 path) checks this flag and returns `ContractError::AppealAlreadyPending` if it is set.
Symmetrically, `appeal_slash_with_evidence` checks for an active `DataKey::SlashEscrow` with
`status == AppealStatus::Pending` and rejects with `ContractError::AppealAlreadyPending`.

**Superseding rule:** Whichever mechanism is initiated first takes priority. The other
mechanism is blocked until the active appeal is resolved (approved, rejected, or expired).
There is no automatic escalation from one mechanism to the other.

**Residual Risk: Low.** The mutual-exclusion check is a storage read (not a lock), so a
theoretical TOCTOU window exists within a single ledger transaction if both calls could be
batched by a malicious client. Soroban's single-threaded, sequential transaction model means
two invocations cannot race within one ledger close, so the practical residual risk is
negligible.
