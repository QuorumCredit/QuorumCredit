# ADR 0001: Use Soroban for smart contract execution

Date: 2026-04-25

## Context

QuorumCredit is a Stellar native protocol that requires secure, deterministic smart contract execution for token movement, staking, and governance.

## Decision

We use Soroban as the smart contract platform for QuorumCredit.

## Rationale

- Soroban is the native smart contract environment for Stellar, providing integration with Stellar accounts, assets, and network semantics.
- It supports WASM contracts written in Rust, which aligns with the existing repository language and developer skill set.
- Soroban offers deterministic execution and strong on-chain transaction validation.
- Using Soroban enables direct compatibility with Stellar's token standards and SEP-41 transfer flows.

## Consequences

- The contract is built and tested against the Soroban Rust SDK.
- Deployment and tooling are aligned to Stellar network conventions.
- Future maintainers can reason with Soroban contract patterns and refer to Stellar-specific design assumptions.
