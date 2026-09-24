# Design Document: Gas Optimization Tests

## Overview

Gas optimization tests measure the CPU instruction count and memory bytes consumed by each core contract function using the Soroban SDK's `env.budget()` API. The feature establishes per-function budgets, writes regression tests that fail if any function exceeds its budget, and implements at least one concrete optimization. All work is additive: a new `src/gas_test.rs` module and a `docs/gas-budgets.md` document.

## Architecture

```mermaid
flowchart TD
    subgraph gas_test.rs
        S[setup()] --> M[Measurement tests]
        S --> R[Regression tests]
        M -->|env.budget().reset_default()| F[Function under test]
        F -->|env.budget().cpu_instruction_count()| CPU[CPU count]
        F -->|env.budget().mem_bytes_used()| MEM[Memory bytes]
        CPU & MEM -->|print| OUT[stdout --nocapture]
        CPU -->|assert ≤ CPU_BUDGET_*| PASS[Test passes]
        MEM -->|assert ≤ MEM_BUDGET_*| PASS
    end
    subgraph docs
        CPU & MEM -->|record| GB[docs/gas-budgets.md]
    end
```

## Components and Interfaces

### `src/gas_test.rs` — Gas Test Module

```rust
#[cfg(test)]
mod gas_test {
    // ── Budget constants ──────────────────────────────────────────────────────
    // Set to measured_baseline * 1.5, rounded up to nearest 1_000.
    const CPU_BUDGET_VOUCH_TYPICAL: u64 = /* TBD after baseline run */;
    const MEM_BUDGET_VOUCH_TYPICAL: u64 = /* TBD after baseline run */;
    const CPU_BUDGET_VOUCH_WORST: u64 = /* TBD */;
    // ... one pair per function per scenario

    fn setup(env: &Env) -> GasFixture { /* ... */ }

    // ── Measurement tests (run with -- --nocapture) ───────────────────────────
    #[test]
    fn measure_vouch_typical() {
        let env = Env::default();
        env.mock_all_auths();
        let f = setup(&env);
        env.budget().reset_default();
        f.client.vouch(&f.voucher, &f.borrower, &1_000_000, &f.token, &None);
        let cpu = env.budget().cpu_instruction_count();
        let mem = env.budget().mem_bytes_used();
        println!("vouch [typical] cpu={cpu} mem={mem}");
    }

    // ── Regression tests ──────────────────────────────────────────────────────
    #[test]
    fn regression_vouch_typical() {
        // same setup as measure_vouch_typical
        assert!(cpu <= CPU_BUDGET_VOUCH_TYPICAL, "vouch typical CPU regression: {cpu} > {CPU_BUDGET_VOUCH_TYPICAL}");
        assert!(mem <= MEM_BUDGET_VOUCH_TYPICAL, "vouch typical MEM regression: {mem} > {MEM_BUDGET_VOUCH_TYPICAL}");
    }
}
```

### `docs/gas-budgets.md` — Budget Table

```markdown
# Gas Budgets

| Function | Scenario | CPU Budget | Memory Budget | Notes |
|---|---|---|---|---|
| vouch | typical (1 voucher) | TBD | TBD | |
| vouch | worst (50 vouchers) | TBD | TBD | |
| request_loan | typical | TBD | TBD | |
| repay | typical | TBD | TBD | includes yield distribution |
| slash | typical | TBD | TBD | via vote_slash + execute |
| auto_slash | typical | TBD | TBD | |
| withdraw_vouch | typical | TBD | TBD | |
| batch_vouch | worst (50 borrowers) | TBD | TBD | |

## Optimization Log

| Date | Function | Change | CPU Before | CPU After | Reduction |
|---|---|---|---|---|---|
```

## Data Models

No new storage keys. The only persistent artefact is `docs/gas-budgets.md`.

Budget constants are defined as `const u64` values at the top of `src/gas_test.rs`. They are the single source of truth — both measurement tests and regression tests reference them.

## Correctness Properties

### Property 1: Budget non-regression

For any function in scope, the CPU instruction count and memory bytes measured in the regression test SHALL be ≤ the documented budget constant. A regression test that passes on a clean checkout SHALL continue to pass unless the contract logic changes.

**Validates: Requirements 3.1, 3.4**

### Property 2: Budget conservatism

Every budget constant SHALL be set to at least 1.5× the measured baseline. This provides headroom for minor toolchain-version variance while still catching significant regressions.

**Validates: Requirements 2.3**

### Property 3: Measurement isolation

Each measurement SHALL call `env.budget().reset_default()` immediately before the function under test and read the counters immediately after. No other contract calls SHALL occur between reset and read.

**Validates: Requirements 1.2**

### Property 4: Worst-case coverage

For functions with linear complexity over the voucher list (`repay`, `slash`, `auto_slash`, `batch_vouch`), the worst-case scenario (max vouchers) SHALL be measured and budgeted separately from the typical-case scenario.

**Validates: Requirements 1.3**

### Property 5: Optimization correctness

After any optimization is applied, all existing functional tests SHALL pass. An optimization that changes observable behaviour is a bug, not an optimization.

**Validates: Requirements 4.3**

## Error Handling

| Scenario | Behaviour |
|---|---|
| `env.budget().reset_default()` not called before measurement | CPU/memory counts include setup overhead; measurement is inaccurate. Mitigated by always calling reset immediately before the function under test. |
| Budget constant set too low | Regression test fails on first run. Developer increases the constant and re-runs. |
| Budget constant set too high | Regression test passes but provides no protection. Mitigated by the 1.5× rule. |
| Optimization breaks a functional test | `cargo test` fails; optimization is reverted. |

## Testing Strategy

### Two-phase test structure

1. **Measurement tests** (`measure_*`): print CPU and memory counts to stdout. Run with `cargo test gas -- --nocapture` to see results. These tests always pass — they are for observation only.
2. **Regression tests** (`regression_*`): assert CPU ≤ budget AND memory ≤ budget. These are the CI gate.

This separation means developers can run measurement tests to gather data without the regression tests interfering, and CI only runs regression tests.

### Functions in scope

| Function | Typical scenario | Worst-case scenario |
|---|---|---|
| `vouch` | 1 existing vouch | 49 existing vouches (max-1) |
| `request_loan` | 1 voucher | 50 vouchers |
| `repay` | 1 voucher | 50 vouchers |
| `slash` | 1 voucher | 50 vouchers |
| `auto_slash` | 1 voucher | 50 vouchers |
| `withdraw_vouch` | 1 voucher | 1 voucher (not loop-dependent) |
| `batch_vouch` | 1 borrower | 50 borrowers |

### Optimization candidates

Based on the contract source, the most likely optimization opportunities are:

1. **Redundant `config()` reads in loops** — `src/governance.rs` and `src/vouch.rs` call `config(&env)` inside loops. Caching the config before the loop eliminates repeated storage reads.
2. **Redundant vouches iteration** — `repay` and `slash` both iterate the vouches list twice (once for total stake, once for distribution). A single-pass accumulator eliminates the second iteration.
3. **Unnecessary clones** — Several functions clone `borrower` and `voucher` addresses more times than necessary. Reducing clones reduces memory allocation.

## Gas Profiling

Gas profiling is the process of attributing CPU instruction and memory consumption to individual contract operations so that hotspots can be identified before optimization. Profiling reuses the same `env.budget()` counters as the measurement tests but records a structured snapshot per operation rather than a single aggregate number.

### `GasProfile` data model

```rust
/// A single profiling sample for one contract operation.
#[derive(Clone, Debug)]
pub struct GasProfile {
    /// Contract function name, e.g. "vouch".
    pub function: Symbol,
    /// Scenario label, e.g. "typical" or "worst".
    pub scenario: Symbol,
    /// CPU instructions consumed by the operation.
    pub cpu: u64,
    /// Memory bytes consumed by the operation.
    pub mem: u64,
}
```

### Profiling helper

```rust
/// Run `op` with a fresh budget and capture its gas profile.
fn profile<F: FnOnce()>(env: &Env, function: &str, scenario: &str, op: F) -> GasProfile {
    env.budget().reset_default();
    op();
    GasProfile {
        function: Symbol::new(env, function),
        scenario: Symbol::new(env, scenario),
        cpu: env.budget().cpu_instruction_count(),
        mem: env.budget().mem_bytes_used(),
    }
}
```

Each measurement test is rewritten to call `profile(...)` and print the resulting `GasProfile`. This keeps the reset/read discipline of Property 3 while producing a uniform record that the benchmark and trend tooling can consume.

## Optimization Suggestions

After profiling, the suite derives actionable suggestions by comparing each `GasProfile` against its budget and against the previous recorded profile. Suggestions are emitted as plain text so they surface in `cargo test -- --nocapture` output and in CI logs.

### Suggestion rules

| Condition | Suggestion |
|---|---|
| `cpu > CPU_BUDGET_*` | "`{function}` [{scenario}] exceeds CPU budget by {delta}; inspect loops and storage reads." |
| `mem > MEM_BUDGET_*` | "`{function}` [{scenario}] exceeds memory budget by {delta}; reduce clones/allocations." |
| `cpu` within 10% of budget | "`{function}` [{scenario}] is close to its CPU budget; consider optimizing before it regresses." |
| `cpu` grew > 5% vs previous run | "`{function}` [{scenario}] CPU regressed {pct}% vs previous run." |

### Suggestion helper

```rust
/// Produce human-readable optimization suggestions for a profile.
fn suggestions(p: &GasProfile, cpu_budget: u64, mem_budget: u64, prev: Option<&GasProfile>) -> Vec<String> {
    let mut out = Vec::new();
    if p.cpu > cpu_budget {
        out.push(format!("{} [{}] exceeds CPU budget by {}", p.function, p.scenario, p.cpu - cpu_budget));
    } else if p.cpu * 10 >= cpu_budget * 9 {
        out.push(format!("{} [{}] is close to its CPU budget", p.function, p.scenario));
    }
    if p.mem > mem_budget {
        out.push(format!("{} [{}] exceeds memory budget by {}", p.function, p.scenario, p.mem - mem_budget));
    }
    if let Some(prev) = prev {
        if prev.cpu > 0 && p.cpu > prev.cpu && (p.cpu - prev.cpu) * 100 > prev.cpu * 5 {
            out.push(format!("{} [{}] CPU regressed vs previous run", p.function, p.scenario));
        }
    }
    out
}
```

## Benchmark Comparisons

Benchmark comparisons put two profiles for the same function/scenario side by side so a change can be judged objectively. Comparisons are used both for before/after optimization and for cross-implementation checks.

### `GasBenchmark` data model

```rust
/// A before/after comparison for one function/scenario pair.
#[derive(Clone, Debug)]
pub struct GasBenchmark {
    pub function: Symbol,
    pub scenario: Symbol,
    pub before: GasProfile,
    pub after: GasProfile,
}

impl GasBenchmark {
    /// CPU reduction as a percentage (0 when no change or growth).
    pub fn cpu_reduction_pct(&self) -> u64 {
        if self.before.cpu == 0 || self.after.cpu >= self.before.cpu {
            return 0;
        }
        (self.before.cpu - self.after.cpu) * 100 / self.before.cpu
    }

    /// Memory reduction as a percentage (0 when no change or growth).
    pub fn mem_reduction_pct(&self) -> u64 {
        if self.before.mem == 0 || self.after.mem >= self.before.mem {
            return 0;
        }
        (self.before.mem - self.after.mem) * 100 / self.before.mem
    }
}
```

A benchmark test profiles the same operation twice — once against the pre-optimization code path and once against the optimized path — and asserts the reduction is non-negative. The resulting percentages are appended to the Optimization Log in `docs/gas-budgets.md`.

## Gas Cost Trends

Trend tracking records each profile over time so slow regressions that stay under budget are still visible. Trends are stored as an append-only table in `docs/gas-budgets.md`; the suite never rewrites history.

### Trend record

```rust
/// One row of the gas cost trend table.
#[derive(Clone, Debug)]
pub struct GasTrendPoint {
    /// ISO-8601 date the sample was taken.
    pub date: Symbol,
    pub function: Symbol,
    pub scenario: Symbol,
    pub cpu: u64,
    pub mem: u64,
}
```

### Trend table format

```markdown
## Gas Cost Trends

| Date | Function | Scenario | CPU | Memory | Δ CPU vs prev |
|---|---|---|---|---|---|
| 2024-01-01 | vouch | typical | TBD | TBD | — |
```

A trend test reads the last recorded row for each function/scenario, compares it to the freshly measured profile, and emits a suggestion when CPU grows by more than 5% (the same threshold used by the suggestion rules). This keeps trend detection consistent with the optimization suggestions and avoids a second, divergent threshold.

## Data Models

No new storage keys. The only persistent artefact is `docs/gas-budgets.md`.

Budget constants are defined as `const u64` values at the top of `src/gas_test.rs`. They are the single source of truth — both measurement tests and regression tests reference them.

Profiling, benchmarking, and trend tracking add three in-memory data models — `GasProfile`, `GasBenchmark`, and `GasTrendPoint` — all defined in `src/gas_test.rs`. None of them touch contract storage; they exist only for the duration of a test run and are persisted solely through the markdown tables in `docs/gas-budgets.md`.
