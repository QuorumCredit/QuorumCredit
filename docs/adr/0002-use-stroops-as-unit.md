# ADR 0002: Use stroops as the native monetary unit

Date: 2026-04-25

## Context

Stellar native assets use stroops as the smallest indivisible unit. Contract accounting must avoid floating-point errors and preserve compatibility with Stellar transaction amounts.

## Decision

We represent all XLM-denominated amounts in the contract and off-chain tooling using stroops.

## Rationale

- Stroops are the exact integer representation of XLM values on Stellar.
- Using stroops avoids precision loss from decimals and floating-point math.
- It provides consistency with Stellar SDKs and reduces the risk of rounding errors in yield, loan, and stake calculations.
- Documentation and tooling can convert between XLM and stroops explicitly, preserving trust and correctness.

## Consequences

- All monetary fields, parameters, and stored balances use integer stroop units.
- The README includes a stroop convention section for developers and integrators.
- Off-chain tooling must convert user-facing XLM amounts into stroops before contract calls.
