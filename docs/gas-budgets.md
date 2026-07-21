# Gas Budgets

> Last measured: 2026-07-21 | Soroban SDK version: 26.1.0 | Complexity Fitting: Enabled

All CPU instruction counts and memory bytes are measured using the Soroban test
runtime (`env.cost_estimate().budget()`). Native Rust test measurements are
**underestimates** compared to WASM execution. Budgets are set at
`measured_baseline × 1.5`, rounded up to the nearest 1,000.

## Complexity-Based Benchmarking

This codebase now uses **parameterized gas benchmarking** to detect complexity-class
regressions before they reach production. Each operation is measured across a range
of input sizes (1, 5, 10, 25, 50, 75, 100 vouchers/participants) and empirically
fitted to a complexity class:

- **O(1)**: Constant time operations (budget multiplier: 1.5×)
- **O(n)**: Linear operations (budget multiplier: 1.5× + size margin)
- **O(n log n)**: Sorting/divide-and-conquer (budget multiplier: 1.5× + logarithmic slack)
- **O(n²)**: Quadratic operations (flagged as regression; not expected in current design)

### Why Complexity Fitting?

Two-point benchmarking (e.g., 1 voucher vs. 50 vouchers) cannot distinguish between:
- A truly linear operation (cost ∝ n) that happens to have high fixed overhead
- A quadratic operation (cost ∝ n²) that's hidden by the sampled points

Example: An O(n²) operation might show budgets like:
```
n=1:  1M instructions
n=50: 50M instructions  ← appears linear if not tested at intermediate points
```

A 3-point measurement at n=[1, 25, 50] reveals the truth:
```
n=1:   1M (baseline)
n=25:  500M (25² factor) ← clearly quadratic
n=50: 2.5B (50² factor)
```

Parameterized sweeps catch this automatically.

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

### Manual Local Testing

1. Run parameterized benchmark tests:
   ```bash
   cargo test --lib gas_benchmark -- --nocapture
   ```
   This runs each operation at sizes: n ∈ {1, 5, 10, 25, 50, 75, 100}

2. Review the console output for complexity class fitting. Each test prints:
   ```
   === VOUCH OPERATION COMPLEXITY SWEEP ===
   vouch @ n=1: cpu=3000000 instructions, mem=3000000 bytes
   vouch @ n=5: cpu=3200000 instructions, mem=3100000 bytes
   ...
   Fitted complexity: O(n)
   ```

3. If complexity matches expected (documented in `docs/benchmarks.json`), proceed.
   If not, investigate the operation for unexpected loops or quadratic behavior.

4. Update `docs/benchmarks.json` with new measurements (copy size_variants array from test output).

5. Generate the HTML report:
   ```bash
   python3 tools/benchmark_analyzer.py docs/benchmarks.json
   ```
   Opens `docs/benchmark_report.html` with full analysis.

6. Update budget constants in `src/lib.rs` using the formula:
   ```
   budget = max_measured_cpu × 1.5, rounded to nearest 1,000
   ```

### CI Verification

The workflow `.github/workflows/gas-benchmarks.yml` runs on every commit to `main`:
- Executes parameterized benchmarks
- Fits complexity classes to all operations
- Generates `benchmark_report.html` artifact
- Comments on PRs with complexity summary
- Fails if any operation regresses (>33% cost increase at any size)

## Measured Operations & Complexity Classes

| Operation | Sizes Tested | Expected Class | Pass Criteria |
|---|---|---|---|
| `vouch` | 1, 5, 10, 25, 50, 75, 100 | O(n) | Linear growth, no >33% jumps |
| `request_loan` | 1, 5, 10, 25, 50, 75, 100 | O(n) | Linear scan over vouchers |
| `repay` | 1, 5, 10, 25, 50, 75, 100 | O(n) | Yield distributed to all vouchers |
| `slash` | 1, 5, 10, 25, 50, 75, 100 | O(n) | Slash applied to all vouchers |
| `auto_slash` | 1, 5, 10, 25, 50, 75, 100 | O(n) | Deadline-triggered iteration |
| `withdraw_queue_sort` | 1, 5, 10, 25, 50, 75, 100 | O(n log n) | Sorting withdrawals (issue #13) |
| `appeal_escrow_distribution` | 1, 5, 10, 25, 50, 75, 100 | O(n) | Pro-rata distribution to appellants |

## Budget Table

Current measured baselines (n=50, worst case):

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

## Optimization Log & History

Historical measurements are automatically tracked in `docs/benchmarks.json`.

Each entry includes:
- **timestamp**: ISO 8601 when measurement was taken
- **commit**: Git commit hash or branch name
- **size_variants**: Array of {size, cpu_instructions, memory_bytes}
- **notes**: Context (e.g., "Added withdrawal-queue dedup", "Fixed nested loop")

### Recent Optimizations

| Date | Function(s) | Change | CPU Before | CPU After | Reduction |
|---|---|---|---|---|---|
| 2026-07-21 | All operations | Added parameterized benchmarks across n=[1..100] | — | — | Baseline + regression detection enabled |
| 2026-06-29 | lib.rs `#893` | Fixed orphaned closing brace that blocked compilation | — | — | structural fix |

## Complexity Regression Examples

### Example: Detecting a Quadratic Regression

Suppose a future PR introduces a nested loop in the `repay` operation:

**Before (O(n)):**
```
repay @ n=1:   5,000,000 instructions
repay @ n=50:  15,000,000 instructions
repay @ n=100: 25,000,000 instructions
```

**After (accidental O(n²)):**
```
repay @ n=1:    5,000,000 instructions
repay @ n=50:   125,000,000 instructions (!)
repay @ n=100:  500,000,000 instructions (!)
```

The CI benchmark workflow runs `python3 tools/benchmark_analyzer.py` which:
1. Fits both data series to complexity classes
2. Detects fitted class is O(n²) but expected is O(n)
3. Generates warning in the HTML report
4. Fails the check (regression threshold >33%)
5. Comments on the PR: "⚠️ repay complexity regressed to O(n²)"

### Example: Normal Variance

Not every change that increases cost by 1-2% is a regression:

```
repay @ n=1:   5,000,000 → 5,050,000 (+1%)     ← normal variance
repay @ n=50:  15,000,000 → 15,300,000 (+2%)   ← passes (< 33% threshold)
repay @ n=100: 25,000,000 → 25,500,000 (+2%)   ← passes
```

The complexity fitter still returns O(n), and no regression is flagged.

## Regression Detection Thresholds

- **Regression threshold**: >33% cost increase at any tested size
  - Tuned to catch genuine algorithmic regressions
  - Absorbs ~10% normal variance and ~20% SDK version drift
- **Complexity mismatch**: Detected class ≠ expected class
  - High confidence indicator of wrong algorithm
- **Negative control**: `test_negative_control_quadratic_detection()` proves the system catches O(n²) operations

## Files & Tools

| File | Purpose |
|---|---|
| `src/gas_benchmark_test.rs` | Parameterized benchmark tests (sizes 1–100) |
| `docs/benchmarks.json` | Historical measurements and regression thresholds |
| `tools/benchmark_analyzer.py` | Complexity fitting and HTML report generation |
| `.github/workflows/gas-benchmarks.yml` | CI workflow: runs benchmarks, generates reports, detects regressions |
| `docs/benchmark_report.html` | Generated HTML dashboard (artifacts in CI runs) |

## Negative Control Test

The test `test_negative_control_quadratic_detection()` proves the benchmarking system works:
```rust
#[test]
fn test_negative_control_quadratic_detection() {
    // Deliberately run an O(n²) nested-loop operation
    // Verify complexity fitter returns "O(n²)" ← must pass!
    assert_eq!(fitted_complexity, "O(n²)");
}
```

This test **must** pass on every commit. If it fails, the complexity-fitting algorithm is broken and should be fixed before merging.
