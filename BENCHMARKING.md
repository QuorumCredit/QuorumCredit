# Benchmarking

This document describes how to benchmark the project and how to profile and
optimize contract gas usage.

## Running Benchmarks

```bash
cargo bench
```

## Contract Gas Optimization Suite

The gas optimization suite profiles contract operations, surfaces optimization
suggestions, compares benchmarks between runs/implementations, and tracks gas
cost trends over time.

### 1. Gas Profiling

Profile the gas consumed by each contract operation to find the most expensive
paths.

```bash
# Profile a single contract operation
cargo bench -- gas_profile --operation <operation_name>

# Profile all contract operations
cargo bench -- gas_profile
```

Each profile reports, per operation:

| Field | Description |
| --- | --- |
| `operation` | Name of the profiled contract operation |
| `gas_used` | Total gas consumed by the operation |
| `cpu_insns` | CPU instructions executed |
| `mem_bytes` | Peak memory allocated |
| `storage_reads` | Number of storage reads |
| `storage_writes` | Number of storage writes |

### 2. Optimization Suggestions

After profiling, the suite inspects the results and emits optimization
suggestions for operations that exceed their expected gas budget.

Suggestions are grouped by category:

- **Storage** — redundant reads/writes, packing opportunities, cacheable values.
- **Computation** — hot loops, repeated hashing, avoidable allocations.
- **Control flow** — early returns, short-circuiting, unreachable branches.
- **Serialization** — oversized payloads, unnecessary copies.

Example output:

```
[storage] transfer: 3 redundant storage reads detected
          -> cache balance before the loop
[computation] mint: repeated hashing in hot loop
          -> hoist hash outside the loop
```

### 3. Benchmark Comparisons

Compare gas usage between two runs or two implementations to quantify the
impact of a change.

```bash
# Compare the current run against a saved baseline
cargo bench -- gas_compare --baseline <baseline.json>

# Compare two implementations directly
cargo bench -- gas_compare --a <impl_a.json> --b <impl_b.json>
```

The comparison reports the absolute and relative delta per operation:

| Operation | Baseline | Current | Delta | Change |
| --- | --- | --- | --- | --- |
| `transfer` | 21,000 | 18,500 | -2,500 | -11.9% |
| `mint` | 45,000 | 44,200 | -800 | -1.8% |

### 4. Gas Cost Trends

Track gas cost trends over time by persisting each run's results and comparing
against historical data.

```bash
# Record the current run into the trend history
cargo bench -- gas_trend --record

# Show the trend for an operation over time
cargo bench -- gas_trend --operation <operation_name>
```

Trends are stored as a time series so regressions are visible across commits:

```
operation: transfer
  2024-01-01  21,000
  2024-02-01  20,100  (-4.3%)
  2024-03-01  18,500  (-8.0%)
```

A regression (an increase in gas usage) is flagged so it can be reviewed before
merging.

## Interpreting Results

- Run the gas optimization suite before and after any contract change.
- Treat any gas regression as a review item; confirm it is intentional.
- Use the optimization suggestions as a starting point, not a mandate — verify
  each change against the benchmark comparison.
