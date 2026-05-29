# Contract Invariants

This document defines the invariants that must hold at all times in the QuorumCredit contract.
The `verify_invariants` function in `src/invariants_test.rs` asserts all of these after every
state-changing operation in tests.

---

## I1 — Solvency: Contract Balance ≥ Total Locked Stake

At any point, the contract's token balance must be ≥ the sum of all active voucher stakes.
Voucher stakes are locked inside the contract until the loan is repaid or slashed.

```
contract_balance >= sum(vouch.stake for all active vouches)
```

**Violation trigger:** A loan disbursement or slash that releases more tokens than the contract holds.

---

## I2 — Loan Amount ≤ Total Vouched Stake at Disbursement

When a loan is disbursed, the loan amount must not exceed the total vouched stake for that borrower
(subject to the `max_loan_to_stake_ratio` config).

```
loan.amount <= total_vouched(borrower) * max_loan_to_stake_ratio / 100
```

**Violation trigger:** `request_loan` bypassing the stake threshold check.

---

## I3 — No Active Loan Without Vouches

A borrower cannot have an active loan if they have zero vouches on record.

```
loan.status == Active  =>  get_vouches(borrower).len() > 0
```

**Violation trigger:** Vouches cleared while a loan is still active.

---

## I4 — Repaid Amount Never Exceeds Principal + Yield

The cumulative `amount_repaid` on a loan record must never exceed `amount + total_yield`.

```
loan.amount_repaid <= loan.amount + loan.total_yield
```

**Violation trigger:** Double-repayment or overpayment accepted by `repay`.

---

## I5 — Loan Status Transitions Are Monotonic

A loan status can only move forward: `None → Active → Repaid | Defaulted`.
It can never go backwards (e.g., `Repaid → Active`).

```
status transitions: None → Active → {Repaid, Defaulted}  (no reverse)
```

**Violation trigger:** `InvalidStateTransition` guard missing on a code path.

---

## I6 — Slash Treasury Is Non-Negative

The slash treasury balance must always be ≥ 0.

```
slash_treasury >= 0
```

**Violation trigger:** Arithmetic underflow in slash accounting.

---

## I7 — Yield BPS Within Valid Range

The configured `yield_bps` must be in `[0, 10_000]` (0%–100%).

```
0 <= config.yield_bps <= 10_000
```

**Violation trigger:** `update_config` accepting out-of-range values.

---

## I8 — Admin Threshold Consistency

`admin_threshold` must always satisfy `1 <= admin_threshold <= admins.len()`.

```
1 <= config.admin_threshold <= config.admins.len()
```

**Violation trigger:** Admin removal reducing the admin list below the threshold.

---

## Testing

All invariants are checked by `verify_invariants(env, client, token, borrowers)` in
`src/invariants_test.rs`. Call this helper after every state-changing operation in tests.

See also: [threat-model.md](./threat-model.md)
