# Contract Invariants

This document specifies every invariant that must hold at all times in the
QuorumCredit contract.  The `verify_invariants` function in
`src/invariants_test.rs` asserts all of these after **every state-changing
operation** in the test suite, and the proptest fuzzing harness in
`src/invariant_fuzz_test.rs` checks them across thousands of randomised
operation sequences.

---

## Quick Reference

| ID  | Name                                   | Checked in `verify_invariants` |
|-----|----------------------------------------|--------------------------------|
| I1  | Solvency: balance ≥ locked stake       | ✓                              |
| I2  | Loan ≤ total vouched stake             | ✓                              |
| I3  | No active loan without vouches         | ✓                              |
| I4  | amount_repaid ≤ amount + yield         | ✓                              |
| I5  | Loan status transitions monotonic      | ✓                              |
| I6  | Slash treasury ≥ 0                     | ✓                              |
| I7  | yield_bps ∈ [0, 10 000]               | ✓                              |
| I8  | 1 ≤ admin_threshold ≤ admins.len       | ✓                              |
| I9  | slash_bps ∈ [0, 10 000]               | ✓                              |
| I10 | All stake values ≥ 0                   | ✓                              |
| I11 | ActiveLoan pointer consistency         | ✓                              |
| I12 | min_loan_amount > 0                    | ✓                              |

---

## Invariant Specifications

### I1 — Solvency: Contract Balance ≥ Total Locked Stake

At any point, the contract's token balance must be ≥ the sum of all active
voucher stakes.  Voucher stakes are locked inside the contract until the loan
is repaid or slashed.

```
contract_balance >= sum(vouch.stake  for every active VouchRecord)
```

**Violation trigger:** A loan disbursement or slash that releases more tokens
than the contract holds.

---

### I2 — Loan Amount ≤ Total Vouched Stake at Disbursement

When a loan is active, the loan amount must not exceed the total vouched stake
for that borrower.

```
loan.status == Active  =>  loan.amount <= total_primary_token_stake(borrower)
```

**Violation trigger:** `request_loan` bypassing the stake-threshold check.

---

### I3 — No Active Loan Without Vouches

A borrower cannot have an active loan if they have zero vouches on record.

```
loan.status == Active  =>  get_vouches(borrower).len() > 0
```

**Violation trigger:** Vouches cleared while a loan is still active.

---

### I4 — Repaid Amount Never Exceeds Principal + Yield

The cumulative `amount_repaid` on a loan record must never exceed
`amount + total_yield`.

```
loan.amount_repaid <= loan.amount + loan.total_yield
```

**Violation trigger:** Double-repayment or overpayment accepted by `repay`.

---

### I5 — Loan Status Transitions Are Monotonic

A loan's status flags must be internally consistent.  A `Repaid` loan must
have `repaid = true`; a `Defaulted` loan must have `defaulted = true`; a
`None`-status loan must never appear in the active-loan pointer map.

```
loan.status == Repaid    =>  loan.repaid    == true
loan.status == Defaulted =>  loan.defaulted == true
loan.status != None      (no None-status records in active storage)
```

**Violation trigger:** `InvalidStateTransition` guard missing on a code path.

---

### I6 — Slash Treasury Is Non-Negative

The slash treasury balance must always be ≥ 0.

```
slash_treasury >= 0
```

**Violation trigger:** Arithmetic underflow in slash accounting.

---

### I7 — Yield BPS Within Valid Range

The configured `yield_bps` must be in `[0, 10_000]` (0 %–100 %).

```
0 <= config.yield_bps <= 10_000
```

**Violation trigger:** `update_config` accepting out-of-range values.

---

### I8 — Admin Threshold Consistency

`admin_threshold` must always satisfy `1 <= admin_threshold <= admins.len()`.

```
1 <= config.admin_threshold <= config.admins.len()
```

**Violation trigger:** Admin removal reducing the admin list below the
threshold.

---

### I9 — Slash BPS Within Valid Range

The configured `slash_bps` must be in `[0, 10_000]` (0 %–100 %).

```
0 <= config.slash_bps <= 10_000
```

**Violation trigger:** `update_config` accepting negative or >100 % slash
rates.

---

### I10 — All Stake Values Are Non-Negative

Every `VouchRecord.stake` field must be ≥ 0.

```
for every VouchRecord v: v.stake >= 0
```

**Violation trigger:** Arithmetic underflow in `decrease_stake` or a slash
path.

---

### I11 — ActiveLoan Pointer Consistency

If `DataKey::ActiveLoan(borrower)` points to a loan ID, the referenced
`LoanRecord` must have `status == Active`.  Terminal-state loans (Repaid,
Defaulted, …) must have the pointer removed.

```
ActiveLoan(borrower) exists  =>  Loan(id).status == Active
```

**Violation trigger:** `repay` or `slash` failing to remove the
`ActiveLoan` key.

---

### I12 — Minimum Loan Amount Is Positive

`config.min_loan_amount` must always be `> 0`.

```
config.min_loan_amount > 0
```

**Violation trigger:** An admin setting `min_loan_amount` to zero or a
negative value.

---

## Implementation

### `verify_invariants` (src/invariants_test.rs)

```rust
pub fn verify_invariants(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    borrowers: &[&Address],
) -> Result<(), InvariantViolation>
```

Checks every invariant above.  Returns `Ok(())` when all hold, or
`Err(InvariantViolation { id, message })` for the **first** violation
(fail-fast).  Call this after every state-changing operation in your tests:

```rust
client.vouch(&voucher, &borrower, &stake, &token, &None);
verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
```

### Proptest Fuzzing (src/invariant_fuzz_test.rs)

Three property tests generate randomised operation sequences and assert
invariants after every step:

| Property | Operations | Cases (CI) |
|---|---|---|
| `invariant_fuzz_sequence` | vouch / loan / repay / slash / config / stake ops | 512 |
| `invariant_fuzz_config_only` | random yield_bps × slash_bps pairs | 512 |
| `invariant_fuzz_single_vouch` | single vouch with arbitrary stake value | 512 |

### Negative Controls (src/negative_invariant_test.rs)

11 tests deliberately corrupt storage and assert `verify_invariants` returns
the expected `Err`.  This proves the harness is not vacuous — every invariant
is shown to _catch_ a real violation:

| Test | Violates |
|---|---|
| `test_nc_i1_solvency_violation_detected` | I1 |
| `test_nc_i2_loan_exceeds_stake_detected` | I2 |
| `test_nc_i3_active_loan_without_vouches_detected` | I3 |
| `test_nc_i4_over_repayment_detected` | I4 |
| `test_nc_i5_*` | I5 (via I11 stale pointer) |
| `test_nc_i6_negative_slash_treasury_detected` | I6 |
| `test_nc_i7_out_of_range_yield_bps_detected` | I7 |
| `test_nc_i8_admin_threshold_zero_detected` | I8 |
| `test_nc_i8_admin_threshold_exceeds_admins_detected` | I8 |
| `test_nc_i9_negative_slash_bps_detected` | I9 |
| `test_nc_i10_negative_stake_detected` | I10 |
| `test_nc_i11_stale_active_loan_pointer_detected` | I11 |
| `test_nc_i12_zero_min_loan_amount_detected` | I12 |

### CI Gate (.github/workflows/invariant-gate.yml)

The `Invariant Gate` workflow is a **required status check** for all PRs that
touch:

```
src/vouch.rs
src/loan.rs
src/governance.rs
src/admin.rs
src/helpers.rs
src/types.rs
src/lib.rs
src/invariants_test.rs
src/invariant_fuzz_test.rs
src/negative_invariant_test.rs
docs/contract-invariants.md
```

It runs three steps in order:
1. `cargo build --tests` — fail fast on compilation errors.
2. Invariant test suite (invariants_test + negative_invariant_test + invariant_fuzz).
3. Full test suite — catch regressions outside the invariant scope.

A blocking job `invariant-gate-required` is used as the branch protection
target so that GitHub cannot merge a PR if the invariant tests fail.

---

## What the Harness Guarantees

The harness verifies all 12 invariants above **within the Soroban test
environment** (`Env::default()`), which gives:

- Full access to persistent and instance storage.
- Real token contract balances via `StellarAssetClient`.
- Exact same contract logic as the production WASM build.

The following are **not covered** and remain outside scope:

| Gap | Reason / Mitigation |
|---|---|
| Cross-contract calls to external token contracts | Token contracts are simulated with `StellarAssetClient`; real custom SEP-41 tokens are not fuzzed |
| Upgrade-time invariants | The WASM upgrade path (`upgrade()`) is not exercised by the harness; a pre-upgrade snapshot check is a future addition |
| Ledger-level double-spends | Prevented by the Soroban runtime, not by this contract |
| Concurrent/parallel transaction ordering | Soroban is single-threaded; not applicable |
| Off-chain invariants (indexer, SDK, frontend) | Out of scope for this harness |

---

## Running Locally

```bash
# Run the full invariant suite
cargo test -- invariants_test negative_invariant_test invariant_fuzz

# Run with extra proptest cases for deeper fuzzing
PROPTEST_CASES=2048 cargo test invariant_fuzz

# Run only the negative controls
cargo test -- negative_invariant_test

# Run all tests
cargo test
```

---

*See also:* [threat-model.md](./threat-model.md)
