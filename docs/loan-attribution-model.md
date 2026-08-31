# Loan Attribution Model

> Heuristic performance attribution for loan outcomes — the `loan_attribution` module.

---

## Overview

The loan attribution module tracks which factors plausibly drove a loan's outcome (repaid or defaulted) and computes a signed contribution score for each factor. This gives protocol operators and researchers a lightweight, explainable way to answer questions like: "Did credit score or vouch quality matter more in last quarter's defaults?"

The model is intentionally a **heuristic** — it uses fixed basis-point coefficients tuned to be directionally correct rather than empirically fit. Soroban smart contracts cannot run arbitrary numerical libraries, so full statistical regression is out of scope; this module provides the closest practical approximation that can execute within Soroban's compute budget.

**Key capabilities:**

- Snapshot loan-origination factors at disbursement time (`record_performance_factors`).
- Compute signed contribution scores per factor once the loan resolves (`analyze_loan_performance_attribution`).
- Accumulate running historical aggregates per factor for protocol-level reporting.
- Predict loan success probability from historical data blended with raw input signals.

---

## Factors Tracked

Four factors are tracked. Weights are denominated in basis points of 10,000 (i.e., `weight_bps / 10_000` gives the decimal weight).

| Factor | Weight | Basis Points | Notes |
|---|---|---|---|
| `credit_score` | 40% | 4,000 bps | Borrower credit score on a 0–1,000 scale |
| `vouch_quality` | 35% | 3,500 bps | Aggregate reputation-weighted vouch strength (bps, neutral = 10,000) |
| `sector` | 15% | 1,500 bps | Borrower-declared or admin-tagged sector (qualitative) |
| `region` | 10% | 1,000 bps | Borrower-declared or admin-tagged region (qualitative) |

**Total weight: 10,000 bps (100%).**

---

## Data Structures

### `PerformanceFactors`

The origination-time snapshot of a loan's driving factors.

```rust
pub struct PerformanceFactors {
    pub loan_id: u64,
    pub borrower: Address,
    pub credit_score: u32,           // 0–1,000 scale
    pub vouch_quality_bps: u32,      // Reputation-weighted vouch strength (10,000 = neutral)
    pub sector: String,              // e.g. "agriculture", "retail"
    pub region: String,              // e.g. "west-africa", "southeast-asia"
    pub recorded_at: u64,            // Ledger timestamp
}
```

This struct is captured **at or shortly after disbursement** so it reflects origination conditions. It is never back-filled after the outcome is known — doing so would introduce look-ahead bias into the aggregates.

### `LoanOutcome` (enum)

The realized outcome of a loan, derived from `LoanStatus`.

```rust
pub enum LoanOutcome {
    Success,   // LoanStatus::Repaid
    Failure,   // LoanStatus::Defaulted | PartialDefault | ForgivenDefault
    Pending,   // LoanStatus::Active | None
}
```

Pending loans are excluded from aggregate updates to avoid polluting historical statistics with unresolved data.

### `FactorContribution`

The contribution of a single factor to a specific loan's outcome.

```rust
pub struct FactorContribution {
    pub factor_name: String,
    pub contribution_bps: i128,   // Signed; positive → pushes toward success
    pub weight_bps: u32,          // Factor's share of the model (see table above)
}
```

A positive `contribution_bps` means the factor was above its neutral value and is associated with a better outcome. A negative value means the factor was below neutral and is associated with a worse outcome.

### `Attribution`

The full attribution result for a single loan.

```rust
pub struct Attribution {
    pub loan_id: u64,
    pub outcome: LoanOutcome,
    pub contributions: Vec<FactorContribution>,   // One entry per tracked factor
    pub total_score_bps: i128,                    // Weighted aggregate across all factors
    pub generated_at: u64,
}
```

`total_score_bps > 0` indicates that, overall, the loan's origination-time factors were above neutral. This does not guarantee repayment — it is a heuristic signal, not a deterministic predictor.

### `FactorAggregate`

Running historical aggregate for a single factor, accumulated across all analyzed loans.

```rust
pub struct FactorAggregate {
    pub factor_name: String,
    pub loans_observed: u32,
    pub successes: u32,
    pub failures: u32,
    pub cumulative_contribution_bps: i128,   // Sum of contribution_bps across all loans
}
```

### `FactorPerformanceReport`

Protocol-wide performance report, broken down by factor.

```rust
pub struct FactorPerformanceReport {
    pub factors: Vec<FactorAggregate>,
    pub total_loans_analyzed: u32,
    pub generated_at: u64,
}
```

---

## Functions

### `record_performance_factors(env, loan_id, borrower, credit_score, vouch_quality_bps, sector, region) -> PerformanceFactors`

Snapshot and persist the origination-time factors for a loan.

- Should be called **once**, close to disbursement time, before the outcome is known.
- Stores the `PerformanceFactors` struct in persistent storage under `AttrKey::Factors(loan_id)`.
- Increments the `FactorLoanCount` instance counter.
- Emits event `(attr, recorded)` with data `(loan_id, credit_score, vouch_quality_bps)`.
- Calling this function multiple times for the same `loan_id` will overwrite the previous record. To preserve origination integrity, call it only once.

### `analyze_loan_performance_attribution(env, loan_id) -> Attribution`

Compute and persist the attribution for a loan.

- Reads the loan's current `LoanStatus` to derive `LoanOutcome`.
- Reads the stored `PerformanceFactors` for the loan (if none recorded, returns an `Attribution` with empty contributions).
- Computes `contribution_bps` for each factor (see [Attribution Model](#attribution-model)).
- Calls `update_factor_aggregates` to accumulate historical data (only for resolved loans).
- Persists the `Attribution` result under `AttrKey::Attribution(loan_id)`.
- Returns the `Attribution` struct.

Can be called on the same loan multiple times (e.g. to refresh after a status change), but aggregate updates only fire for resolved (non-Pending) outcomes.

### `generate_factor_performance_report(env) -> FactorPerformanceReport`

Generate a protocol-wide report aggregating all four factors across every loan analyzed so far.

- Reads `FactorAggregate` records for `credit_score`, `vouch_quality`, `sector`, and `region`.
- Returns only factors that have at least one observed loan; factors with no data are omitted.
- No auth required. Safe to call at any time.

### `predict_loan_success_probability_bps(env, credit_score, vouch_quality_bps, sector, region) -> u32`

Predict the likelihood of successful repayment for a hypothetical loan, given its factors.

- Returns a probability estimate in basis points: `0` = certain failure, `10,000` = certain success.
- Clamps the output to `[0, 10_000]`.
- See [Prediction Model](#prediction-model) below.

---

## Attribution Model

### Credit Score Contribution

Credit score is normalized against the 0–1,000 scale and centered at `550` (the "fair credit" boundary).

```
credit_delta       = credit_score - 550
credit_contribution = (credit_delta * 4000) / 450
```

- At `credit_score = 1000` (max): `contribution_bps = +4000`
- At `credit_score = 550` (neutral): `contribution_bps = 0`
- At `credit_score = 100` (low): `contribution_bps ≈ -4000`
- Denominator `450 = 1000 - 550` maps the full upside range to ±4,000 bps.

### Vouch Quality Contribution

Vouch quality is centered at `10,000` (the neutral, "average pool" baseline).

```
quality_delta        = vouch_quality_bps - 10_000
quality_contribution = (quality_delta * 3500) / 10_000
```

- At `vouch_quality_bps = 20,000` (strong pool): `contribution_bps = +3500`
- At `vouch_quality_bps = 10,000` (neutral):     `contribution_bps = 0`
- At `vouch_quality_bps = 0` (weak pool):        `contribution_bps = -3500`

### Sector & Region Contributions

Sector and region are qualitative tags. A single loan cannot self-attribute to a sector or region — there is no on-chain distribution to compare against for a single data point. Therefore:

- `contribution_bps = 0` for both sector and region on a per-loan basis.
- `weight_bps` is recorded at the full 1,500 / 1,000 values so that `update_factor_aggregates` accumulates the outcome (success/failure) against the sector/region key.
- Over many loans, `FactorAggregate` for a sector will reveal whether loans tagged with that sector have a higher or lower success rate than the protocol average — a useful aggregate signal even with zero per-loan contribution.

### `total_score_bps`

```
total_score_bps = credit_contribution + quality_contribution
                  + sector_contribution (0) + region_contribution (0)
```

Only the quantitative factors (credit score, vouch quality) drive the per-loan score. Sector and region contribute zero per-loan, but accumulate in the aggregates for protocol-level analysis.

---

## Prediction Model

`predict_loan_success_probability_bps` blends two signals:

1. **Historical success rates** from `FactorAggregate` — how often loans with similar factor values actually repaid. More observations → more weight.
2. **Raw input signal** — the raw credit score and vouch quality of the candidate loan, folded in directly (useful when the sector/region is new and has no history).

**Algorithm (Bayesian-style blending):**

```
prior = 50% success (5,000 bps) with weight 100

for each FactorAggregate relevant to this loan:
    success_rate_bps = (successes / loans_observed) * 10,000
    weighted_sum += success_rate_bps * loans_observed
    weight_total += loans_observed

// Raw credit signal
credit_signal_bps = (credit_score / 1000) * 10,000
weighted_sum += credit_signal_bps * 50
weight_total += 50

// Raw vouch quality signal
weighted_sum += clamp(vouch_quality_bps, 0, 20,000) * 30
weight_total += 30

prediction = weighted_sum / weight_total
prediction = clamp(prediction, 0, 10,000)
```

With no historical data at all (fresh deployment), the prediction degrades gracefully to the prior (≈ 50%), shifted slightly by the raw credit and vouch quality inputs.

---

## Known Limitations

1. **Weights are fixed heuristic coefficients, not empirically fit.** The 40/35/15/10 split was chosen to be directionally reasonable but has not been validated against real loan data. Treat the attribution scores as an exploratory signal, not ground truth.

2. **Sector and region contribute zero per-loan signal.** Until enough loans accumulate for a given sector/region tag, those factors provide no predictive signal for individual loans. They only become meaningful at the aggregate level.

3. **Pending loans are excluded from aggregates.** `update_factor_aggregates` skips loans with `LoanOutcome::Pending`. This is intentional — including unresolved loans would inflate `loans_observed` and dilute success rates with ambiguous data. However, it means that `generate_factor_performance_report` only reflects completed loans.

4. **The prediction model is not recalibrated over time.** The `predict_loan_success_probability_bps` function uses raw historical counts; there is no decay, weighting by recency, or periodic recalibration. Older data has the same weight as recent data.

5. **`analyze_loan_performance_attribution` can be called multiple times.** Each call re-runs aggregate updates for resolved loans. If called more than once on the same loan (e.g., via a monitoring script), aggregate counters may be over-counted. Integrators should call this function once per loan, after the outcome is finalized.

---

## Events

| Topic | Data | Trigger |
|---|---|---|
| `(attr, recorded)` | `(loan_id, credit_score, vouch_quality_bps)` | `record_performance_factors` called |

Attribution analysis and report generation do not emit events (they are read-only from a state-change perspective).

---

## Storage

| Key | Type | Purpose |
|---|---|---|
| `AttrKey::Factors(loan_id)` | `PerformanceFactors` | Persistent — origination snapshot per loan |
| `AttrKey::Attribution(loan_id)` | `Attribution` | Persistent — cached attribution result per loan |
| `AttrKey::FactorAggregate(name)` | `FactorAggregate` | Persistent — running aggregate per factor name |
| `AttrKey::FactorLoanCount` | `u32` | Instance — total loans with recorded factors |

---

## Example Flow

```javascript
// 1. At disbursement: record origination-time factors
await contract.recordPerformanceFactors(
    loan_id,
    borrower,
    720,           // credit_score (above 550 neutral)
    12_000,        // vouch_quality_bps (above 10,000 neutral — strong pool)
    "agriculture", // sector
    "west-africa"  // region
);

// 2. After the loan resolves (repaid or defaulted): analyze attribution
const attr = await contract.analyzeLoanPerformanceAttribution(loan_id);
console.log(`Outcome: ${attr.outcome}`);
attr.contributions.forEach(c => {
    console.log(`  ${c.factor_name}: ${c.contribution_bps} bps (weight: ${c.weight_bps} bps)`);
});
// credit_score:   +1511 bps  (720 is well above 550 neutral)
// vouch_quality:  +700  bps  (12,000 is 2,000 above 10,000 neutral)
// agriculture:    0     bps  (qualitative — no per-loan signal)
// west-africa:    0     bps  (qualitative — no per-loan signal)

// 3. Protocol-level report
const report = await contract.generateFactorPerformanceReport();
report.factors.forEach(agg => {
    const successRate = agg.successes / agg.loans_observed;
    console.log(`${agg.factor_name}: ${successRate * 100}% success over ${agg.loans_observed} loans`);
});

// 4. Predict success probability for a new candidate loan
const probabilityBps = await contract.predictLoanSuccessProbabilityBps(
    680,           // credit_score
    11_000,        // vouch_quality_bps
    "retail",      // sector
    "south-asia"   // region
);
console.log(`Predicted success: ${probabilityBps / 100}%`);
```
