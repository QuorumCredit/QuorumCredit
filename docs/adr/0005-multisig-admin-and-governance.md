# ADR 0005: Use multisig admin and governance for critical actions

Date: 2026-04-25

## Context

QuorumCredit includes sensitive admin operations such as slashing, pausing, upgrades, and contract initialization.

## Decision

We require a multisig-style admin threshold and governance workflow for critical contract operations.

## Rationale

- Single-key administration creates a high-risk centralization point.
- A threshold-based admin model increases operational security for emergency and governance actions.
- It supports safer decision-making around slash execution and contract upgrades.
- Multisig also reduces the chance of accidental or malicious unilateral actions.

## Consequences

- Admin and governance flows must be clearly documented and audited.
- Access control is designed to require multiple approvals for high-impact transactions.
- Emergency procedures and signer rotation practices become essential operational controls.
