# ADR 0003: Use an FBA-inspired trust model

Date: 2026-04-25

## Context

The QuorumCredit protocol seeks to replace traditional financial collateral with social collateral derived from trusted relationships.

## Decision

We base borrower eligibility on an FBA-inspired trust model rather than centralized credit scoring.

## Rationale

- FBA-inspired models allow each participant to define their own trusted subset of the network.
- This design mirrors Stellar’s quorum slice concept and supports decentralized trust decisions.
- It enables a social trust graph approach where vouches reflect explicit trust from known actors.
- Using this model avoids centralized reputation dependencies and supports community-driven lending.

## Consequences

- Borrower eligibility and governance decisions depend on personal trust relationships and voucher thresholds.
- The protocol design emphasizes social consensus and careful multisig administration.
- Future extensions can build on the trust graph model, such as referral scoring and reputation adjustments.
