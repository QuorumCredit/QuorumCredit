# Gas Budget Benchmarking Guide

This document explains the new parameterized gas benchmarking system for detecting complexity-class regressions.

## Overview

The benchmarking system measures operations across multiple input sizes (1, 5, 10, 25, 50, 75, 100 vouchers) and automatically fits complexity classes (O(1), O(n), O(n log n), O(n²)) to detect regressions before deployment.

## Components

### 1. Benchmark Tests (`src/gas_benchmark_test.rs`)

Parameterized test harness with:
- **`test_linear_scan_complexity_sweep`**: Measures linear scan operations across size range
- **`test_negative_control_quadratic_detection`**: Proves the system detects O(n²) operations

Each test:
1. Creates operations at sizes [1, 5, 10, 25, 50, 75, 100]
2. Measures CPU instruction cost and memory bytes for each size
3. Fits empirical complexity class via ratio analysis
4. Asserts operations match their expected complexity

### 2. Historical Tracking (`docs/benchmarks.json`)

JSON file storing:
```json
{
  "operations": {
    "operation_name": {
      "expected_complexity": "O(n)",
      "measurements": [
        {
          "timestamp": "2026-07-21T00:00:00Z",
          "commit": "abc123",
          "size_variants": [
            {"size": 1, "cpu_instructions": 3000000, "memory_bytes": 3000000},
            ...
          ]
        }
      ]
    }
  }
}
```

Enables:
- Historical trend analysis
- Gradual regression detection
- Baseline comparison across commits

### 3. Analysis Tool (`tools/benchmark_analyzer.py`)

Python script that:
- Loads `docs/benchmarks.json`
- Fits complexity classes to measurements via linear regression
- Detects regressions (>33% cost increase at any size)
- Generates `docs/benchmark_report.html` dashboard

**Usage:**
```bash
python3 tools/benchmark_analyzer.py docs/benchmarks.json
```

**Output:**
- HTML report with complexity curves, measurements, and regressions
- Exit code 0 if no regressions, 1 if regressions detected
- Human-readable complexity class fitting with R² goodness-of-fit

### 4. CI Integration (`.github/workflows/gas-benchmarks.yml`)

Workflow runs on every commit to `main`:
1. Executes `cargo test --lib gas_benchmark`
2. Parses benchmark output
3. Runs `tools/benchmark_analyzer.py` to analyze
4. Comments on PRs with complexity summary
5. Uploads `benchmark_report.html` as artifact
6. Fails if any operation shows >33% regression

### 5. Local Runner (`tools/run_benchmarks.sh`)

Bash script for local development:
```bash
./tools/run_benchmarks.sh              # Run full suite
./tools/run_benchmarks.sh --report-only  # Just analyze
./tools/run_benchmarks.sh --update     # Update baseline
```

## Complexity Fitting Algorithm

The analyzer fits measurements to complexity classes using ratio-based detection:

### Model: O(1) (Constant)
```
Cost ≈ constant
Ratio between adjacent sizes: ~1.0
```

### Model: O(n) (Linear)
```
Cost = a*n + b
Ratio: size_ratio / cost_ratio ≈ 1.0
```

### Model: O(n²) (Quadratic)
```
Cost = a*n² + b*n + c
Ratio: cost_ratio / size_ratio >> 1.0
```

For each measurement pair:
```
slope = cost_ratio / size_ratio

avg_slope < 1.2  →  O(1)
1.2 ≤ avg_slope < 1.8  →  O(n)
avg_slope ≥ 1.8  →  O(n²)
```

Example: If n goes from 10 to 50 (5x) and cost goes from 10M to 35M (3.5x):
- `slope = 3.5 / 5 = 0.7` → detected as O(1)

Whereas if cost went from 10M to 125M (12.5x):
- `slope = 12.5 / 5 = 2.5` → detected as O(n²)

## Regression Detection

**Threshold:** >33% cost increase at any tested size

This is conservative to avoid false positives while catching genuine algorithmic regressions:
- 1-2% variance from SDK version changes: ✓ PASS
- 10-20% variance from optimization flags: ✓ PASS
- 50%+ increase from nested loop: ✗ FAIL

**Complexity Mismatch:** Any operation fitting to a different class than expected is flagged as critical.

## Adding New Operations

To benchmark a new operation:

1. **Add measurement function** in `src/gas_benchmark_test.rs`:
   ```rust
   fn measure_my_operation(env: &Env, num_items: usize) -> (u64, u64) {
       let budget_before = env.budget();
       
       // Create items and perform operation...
       let mut items: Vec<i64> = Vec::new(env);
       for i in 0..num_items { items.push_back(i as i64); }
       
       // Measure the operation
       for item in items.iter() {
           let _ = my_operation(*item);
       }
       
       let budget_after = env.budget();
       let cpu = (budget_before.cpu_instruction_cost() - budget_after.cpu_instruction_cost()).max(0);
       let mem = (budget_before.memory_bytes() - budget_after.memory_bytes()).max(0);
       
       (cpu as u64, mem as u64)
   }
   ```

2. **Add sweep test**:
   ```rust
   #[test]
   fn test_my_operation_complexity_sweep() {
       let env = Env::default();
       let sizes = [1, 5, 10, 25, 50, 75, 100];
       let mut measurements: Vec<GasMeasurement> = Vec::new(&env);

       for size in sizes.iter() {
           let (cpu, mem) = measure_my_operation(&env, *size);
           measurements.push_back(GasMeasurement {
               size: *size,
               cpu_instructions: cpu,
               memory_bytes: mem,
           });
       }

       let complexity = fit_complexity(measurements.as_slice());
       assert_eq!(complexity, "O(n)", "my_operation should be O(n)");
   }
   ```

3. **Add to `docs/benchmarks.json`**:
   ```json
   "my_operation": {
       "description": "What this operation does",
       "expected_complexity": "O(n)",
       "measurements": [
           {
               "timestamp": "2026-07-21T00:00:00Z",
               "commit": "initial-baseline",
               "size_variants": [
                   {"size": 1, "cpu_instructions": ..., "memory_bytes": ...},
                   ...
               ]
           }
       ]
   }
   ```

4. **Run and verify**:
   ```bash
   cargo test --lib gas_benchmark::test_my_operation_complexity_sweep
   python3 tools/benchmark_analyzer.py docs/benchmarks.json
   ```

## Interpreting the HTML Report

The generated `benchmark_report.html` contains:

### Metadata Section
- Report generation time
- Soroban SDK version
- Budget multiplier (1.5×)
- Regression threshold (33%)

### Regression Warnings
- Highlighted in red if any operations regressed
- Shows size, baseline, current cost, and percentage increase

### Per-Operation Sections
- **Operation name** and description
- **Complexity badge**: Expected vs. detected class
- **Stats grid**: Expected complexity, goodness of fit, last measurement
- **Measurement table**: Size, CPU, memory, cost-per-unit
- **Notes**: Context about the measurement

## Negative Control Test

The test `test_negative_control_quadratic_detection()` proves the benchmarking system works by:
1. Running deliberately quadratic code (nested loop)
2. Measuring across sizes [1, 5, 10, 25, 50]
3. Asserting fitted complexity is "O(n²)"

This test **must pass** on every commit. If it fails, the complexity-fitting algorithm is broken.

## Example: Detecting a Real Regression

Suppose a future PR accidentally changes `repay` from O(n) to O(n²):

**Before (correct O(n)):**
```
repay @ n=1:   5,000,000 instructions
repay @ n=10:  5,900,000 instructions (1.18x, slope ~0.18)
repay @ n=50:  15,000,000 instructions (2.55x from n=1, linear pattern)
```
Fitted: **O(n)** ✓ PASS

**After (accidental O(n²)):**
```
repay @ n=1:   5,000,000 instructions
repay @ n=10:  50,000,000 instructions (10x, slope ~2.0)
repay @ n=50:  125,000,000 instructions (25x from n=1, quadratic pattern!)
```
Fitted: **O(n²)** ✗ FAIL

CI workflow:
1. Runs benchmarks
2. Fits complexity → detects O(n²)
3. Compares to expected O(n)
4. Generates warning: "⚠️ repay complexity regressed from O(n) to O(n²)"
5. Fails PR check
6. Comments with link to `benchmark_report.html`

## Budget Constants

Current measured baselines (from `docs/benchmarks.json`):

| Operation | n=1 | n=50 | Budget (1.5×) |
|---|---|---|---|
| vouch | 3M | 5M | 7,500,000 |
| request_loan | 4M | 7M | 10,500,000 |
| repay | 5M | 15M | 22,500,000 |
| slash | 5M | 15M | 22,500,000 |
| auto_slash | 5M | 15M | 22,500,000 |

Set these constants in `src/lib.rs` per the formula:
```
budget = max_measured_cpu × 1.5, round to nearest 1,000
```

## Maintenance

### Regular Workflow
1. **Local development**: Run `./tools/run_benchmarks.sh` before pushing
2. **CI validation**: Workflow automatically checks on push
3. **Baseline updates**: When optimization changes cost legitimately, update `docs/benchmarks.json`

### Responding to Regressions
1. **PR failing**: Check `benchmark_report.html` artifact
2. **Identify operation**: Which operation regressed?
3. **Compare complexity**: Did fitted class change unexpectedly?
4. **Fix root cause**: Usually a new loop or nested iteration added
5. **Re-measure**: Run benchmarks again to verify fix

### Updating for New SDK Versions
When upgrading Soroban SDK:
1. Run `./tools/run_benchmarks.sh` locally
2. Review percentage changes in `benchmark_report.html`
3. If >10% variance across all operations, update baseline in `docs/benchmarks.json`
4. Update `soroban_sdk_version` in metadata

## Known Limitations

1. **Soroban SDK variance**: Native test measurements are underestimates vs. WASM execution. Budget multiplier (1.5×) absorbs ~20% variance.

2. **Fixed size range**: Currently tests sizes [1, 5, 10, 25, 50, 75, 100]. For operations scaling beyond 100, add larger sizes to `src/gas_benchmark_test.rs`.

3. **No per-operation budget limits**: This system detects regressions but doesn't enforce individual operation budgets in the contract itself. That's done in `src/lib.rs` via `env.budget()`.

4. **Complexity classes are discrete**: An operation can't be "between" O(n) and O(n²). Algorithms must be fixed to achieve the intended class.

## References

- **Soroban SDK Docs**: https://docs.rs/soroban-sdk/
- **Budget API**: `env.budget().cpu_instruction_cost()`, `memory_bytes()`
- **Gas Budgets Doc**: `docs/gas-budgets.md`
- **Historical Data**: `docs/benchmarks.json`
