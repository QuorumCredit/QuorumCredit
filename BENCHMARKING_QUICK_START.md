# Gas Benchmarking Quick Start

## For Developers: Running Benchmarks Locally

```bash
# Run full benchmark suite (tests + analysis + report)
./tools/run_benchmarks.sh

# Just analyze existing measurements (no tests)
./tools/run_benchmarks.sh --report-only
```

This generates `docs/benchmark_report.html` with:
- Parameterized measurements at sizes 1–100
- Complexity class fitting (O(1), O(n), O(n²))
- Regression detection vs. baseline

## For Code Reviewers: Interpreting CI Results

When a PR fails the benchmarking check:

1. **Check the artifact**: Download `benchmark-report` from CI run
2. **Look for warnings**: Red "Complexity Mismatch" or "⚠️ Detected Regressions"
3. **Compare numbers**: Table shows baseline vs. latest costs
4. **Fit quality**: R² value shows confidence (>0.95 = good)

Example regression:
```
⚠️ repay complexity regressed from O(n) to O(n²)
  @ n=50: +125% CPU (expected linear, got quadratic)
```

→ Code likely has new nested loop that needs fixing

## For Maintainers: Updating Baselines

After legitimate optimization that reduces costs:

1. Run benchmarks locally: `./tools/run_benchmarks.sh`
2. Review report: Confirm new costs are lower/expected
3. Update baseline in `docs/benchmarks.json`:
   - Copy `size_variants` array from test output
   - Update timestamp and commit hash
   - Add notes explaining the change
4. Commit and push

## Key Files

| File | Purpose |
|------|---------|
| `src/gas_benchmark_test.rs` | Parameterized test code |
| `docs/benchmarks.json` | Historical measurements & baselines |
| `tools/benchmark_analyzer.py` | Complexity fitting & report generation |
| `tools/run_benchmarks.sh` | Local runner script |
| `.github/workflows/gas-benchmarks.yml` | CI automation |
| `BENCHMARKING.md` | Full developer guide |

## Understanding Complexity Classes

**O(1) - Constant Time**
```
n=1:   100 ops
n=50:  110 ops  (barely changes)
n=100: 120 ops
```

**O(n) - Linear**
```
n=1:   100 ops
n=50:  5000 ops   (50× increase)
n=100: 10000 ops  (100× increase)
```

**O(n²) - Quadratic (Bad!)**
```
n=1:   100 ops
n=50:  250,000 ops    (2500× increase!)
n=100: 1,000,000 ops  (10,000× increase!)
```

The benchmarking system catches O(n²) automatically.

## Regression Detection

- **<33% increase**: ✅ PASS (normal variance/optimization)
- **>33% increase**: ❌ FAIL (regression detected)
- **Complexity change**: ❌ FAIL (algorithm changed unexpectedly)

## Testing Locally Before Push

```bash
# Make your changes
cargo build

# Run benchmarks
./tools/run_benchmarks.sh

# Check report
open docs/benchmark_report.html

# If regressions found:
# 1. Review the operation that regressed
# 2. Check if it's a real change or false positive
# 3. If regression is legitimate (e.g., added feature), update baseline

# Push when happy
git push
```

## Negative Control Test

The system includes a test that **must pass**:

```rust
#[test]
fn test_negative_control_quadratic_detection() {
    // Deliberately runs O(n²) code
    // Must be detected as "O(n²)"
    assert_eq!(complexity, "O(n²)");
}
```

If this test ever fails, the complexity-fitting algorithm is broken and should not be merged.

## What Gets Measured?

Currently benchmarked operations:
- vouch (O(n) - linear scan over existing vouches)
- request_loan (O(n) - eligibility check)
- repay (O(n) - yield distribution)
- slash (O(n) - apply to all vouchers)
- auto_slash (O(n) - deadline-triggered)
- withdraw_queue_sort (O(n log n) - sorting)
- appeal_escrow_distribution (O(n) - pro-rata distribution)

## Size Points Tested

[1, 5, 10, 25, 50, 75, 100] vouchers/participants

Why 7 sizes?
- Can't detect O(n²) with only 2 points
- 7 points gives clear trend visibility
- Includes edge case (n=1) and max configured (n=100)

## Common Issues

### "Benchmark tests timed out"
- Normal on slow machines
- CI runs with --release for speed
- Check if there's an accidental O(n³) loop

### "Regression threshold seems arbitrary"
- 33% chosen because:
  - SDK version variance: ~10%
  - Compiler optimization flags: ~10-20%
  - Normal code changes: ~5%
  - Real bugs typically >50%

### "Why not just measure at n=50?"
- Can't distinguish O(n) from O(n²):
  - O(n): n=1 (1M), n=50 (50M) → looks linear
  - O(n²): n=1 (1M), n=50 (2.5B) → but we don't test intermediate points!
- 7-point sweep reveals the truth via inflection points

## For More Details

- **Full Guide**: See `BENCHMARKING.md`
- **Implementation Details**: See `GAS_BENCHMARKING_IMPLEMENTATION_SUMMARY.md`
- **Gas Budgets Doc**: See `docs/gas-budgets.md`

## FAQ

**Q: Do I need to update benchmarks every time I commit?**
A: No. The system runs automatically on CI. Only update baselines when making intentional cost changes.

**Q: What if my optimization reduces costs by 25%?**
A: Great! That's under 33% threshold, so it passes. Update baseline if you want to ratchet down budget.

**Q: Can I ignore a regression?**
A: No. CI will fail. Either optimize the code or update baseline with clear notes on why the change is acceptable.

**Q: What if benchmarks are flaky?**
A: Take multiple measurements (`--nocapture` output). If variance >10%, there may be environmental factors. Document in notes.

**Q: Do I need to know Python to use this?**
A: No. Just run `./tools/run_benchmarks.sh` and read the HTML report. The analyzer is automated.

**Q: How often should I run benchmarks?**
A: Before pushing any changes to operations that loop over vouchers/loans/participants.

## Next Steps

1. Read the full `BENCHMARKING.md` guide
2. Run `./tools/run_benchmarks.sh` to understand the workflow
3. Review `docs/benchmark_report.html` format
4. When fixing regressions, check complexity fitting for insight
