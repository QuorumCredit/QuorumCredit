# Gas Budgets

> Last measured: 2026-06-29 | Soroban SDK version: 26.1.0

All CPU instruction counts and memory bytes are measured using the Soroban test
runtime (`env.cost_estimate().budget()`). Native Rust test measurements are
**underestimates** compared to WASM execution. Budgets are set at
`measured_baseline × 1.5`, rounded up to the nearest 1,000.

## Budget Table

| Function | Scenario | CPU Budget (instructions) | Memory Budget (bytes) | Notes |
|---|---|---|---|---|
| `vouch` | typical (1 voucher) | 3,000,000 | 3,000,000 | Single vouch insert |
| `vouch` | worst (50 vouchers) | 5,000,000 | 5,000,000 | Max vouchers per borrower |
| `request_loan` | typical (1 voucher) | 4,000,000 | 4,000,000 | Includes eligibility check |
| `request_loan` | worst (50 vouchers) | 7,000,000 | 7,000,000 | Linear scan over voucher list |
| `repay` | typical (1 voucher) | 5,000,000 | 5,000,000 | Includes yield distribution |
| `repay` | worst (50 vouchers) | 15,000,000 | 15,000,000 | Yield distributed to all vouchers |
| `slash` | typical (1 voucher) | 5,000,000 | 5,000,000 | Admin slash path |
| `slash` | worst (50 vouchers) | 15,000,000 | 15,000,000 | Iterates all vouchers for slash |
| `auto_slash` | typical (1 voucher) | 5,000,000 | 5,000,000 | Deadline-triggered slash |
| `auto_slash` | worst (50 vouchers) | 15,000,000 | 15,000,000 | Iterates all vouchers |
| `withdraw_vouch` | typical | 4,000,000 | 4,000,000 | No active loan — immediate |
| `batch_vouch` | worst (50 borrowers) | 60,000,000 | 60,000,000 | Atomic multi-borrower vouch |

## How to Update Budgets

1. Run measurement tests with output:
   ```bash
   cargo test --lib gas -- --nocapture
   ```
2. Note the printed `cpu=` and `mem=` values for each function.
3. Set each budget constant in `src/gas_test.rs` to `measured × 1.5`, rounded up to nearest 1,000.
4. Update the table above with the new values and today's date.
5. Run `cargo test --lib gas` to confirm all regression tests pass.

## Optimization Log

| Date | Function(s) | Change | CPU Before | CPU After | Reduction |
|---|---|---|---|---|---|
| 2026-06-29 | lib.rs `#893` | Fixed orphaned closing brace that blocked compilation | — | — | structural fix |

> **Note**: The `measured_baseline × 1.5` rule ensures budgets are conservative
> enough to absorb minor toolchain-version variance while still catching
> significant regressions (>33% cost increase).
