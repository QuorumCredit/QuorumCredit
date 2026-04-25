# ADR 0004: Use a 2% yield and 50% slash model

Date: 2026-04-25

## Context

QuorumCredit needs a risk/reward mechanism that incentivizes voucher participation while discouraging borrower default.

## Decision

We use a 2% yield on successful repayment and a 50% stake slash on borrower default.

## Rationale

- A modest 2% reward balances borrower affordability with incentive for vouchers.
- 50% slash is significant enough to deter frivolous borrowing and protect voucher funds.
- The model is simple to understand, audit, and communicate to users.
- It aligns with the contract’s focus on social collateral rather than over-collateralized lending.

## Consequences

- All repayment accounting uses the configured yield basis points to compute voucher rewards.
- Default flows require reliable slash execution and transparent governance.
- Future changes to yield or slash ratios must be evaluated carefully against borrower access and voucher risk.
