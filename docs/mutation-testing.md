# Mutation Testing

This document tracks mutation testing for QuorumCredit using [`cargo-mutants`](https://mutants.rs).

## Configuration

Scope is defined in `mutants.toml` at the repository root:

| File | Rationale |
|------|-----------|
| `src/lib.rs` | Contract entry points and withdrawal queue guards |
| `src/vouch.rs` | Stake validation, cooldown, and transfer checks |
| `src/governance.rs` | Quorum arithmetic and slash-threshold voting |
| `src/admin.rs` | Admin cooldown and pause configuration |
| `src/helpers.rs` | Shared guards and protocol health scoring |

Test modules (`src/*_test.rs`) and generated code are excluded.

## Baseline run

Run locally:

```bash
cargo install cargo-mutants --locked
cargo mutants --jobs 4
```

The kill rate must be ≥ 80%. Parse `mutants.out/outcomes.json` after each run.

Record results here after the first completed run:

| Metric | Value |
|--------|-------|
| Date | _pending_ |
| `cargo-mutants` version | _pending_ |
| Total mutants | _pending_ |
| Killed | _pending_ |
| Survived | _pending_ |
| Timeouts | _pending_ |
| Kill rate | _pending_ (target ≥ 80%) |

## Weak test areas identified

Static review of the in-scope modules against the existing test suite surfaced the following gaps. Targeted tests were added to kill the highest-impact mutation operators (comparison flips, boundary off-by-one, guard removal).

| Source area | Weak spot | Remediation | Test module |
|-------------|-----------|-------------|-------------|
| `governance.rs` | Tie vote (`approve_votes == reject_votes`) must not update `slash_bps` | Add test | `slash_threshold_voting_test` |
| `governance.rs` | Invalid threshold guards (`<= 0`, `> 10_000`) | Add test | `slash_threshold_voting_test` |
| `governance.rs` | Duplicate voter guard | Add test | `slash_threshold_voting_test` |
| `governance.rs` | Finalize expiry window | Add test | `slash_threshold_voting_test` |
| `governance.rs` | `execute_slash_vote` quorum floor | Add test | `property_stake_loan_invariants_test` |
| `vouch.rs` | `min_stake` exact-boundary (`<` vs `<=`) | Add test | `cross_chain_vouch_test` |
| `vouch.rs` | Vouch cooldown between successive vouches | Add test | `cross_chain_vouch_test` |
| `vouch.rs` | `require_positive_amount` zero-stake guard | Add test | `cross_chain_vouch_test` |
| `helpers.rs` | `calculate_protocol_health_score` component weights | Add test | `property_stake_loan_invariants_test` |

## Surviving mutants

_Update this section after each `cargo mutants` run that changes the kill rate by more than 2 percentage points._

| File | Line | Original | Mutant | Decision |
|------|------|----------|--------|----------|
| _pending baseline run_ | | | | |
