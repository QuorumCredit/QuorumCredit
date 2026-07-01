use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// All monetary values are in stroops (1 XLM = 10,000,000 stroops).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolMetrics {
    /// Total Value Locked: sum of all active loan amounts in stroops
    pub tvl: i128,
    pub active_loans: u32,
    pub total_loans: u32,
    pub defaulted_loans: u32,
    /// default_rate = defaulted_loans / total_loans (0.0–1.0); 0.0 when total_loans == 0
    pub default_rate: f64,
    /// Total yield distributed to vouchers in stroops
    pub total_yield_distributed: i128,
    /// Number of slash events
    pub slash_count: u32,
    /// Accumulated protocol fees in stroops
    pub fee_revenue: i128,
    /// Top borrowers by loan amount: (address, total_borrowed_stroops)
    pub top_borrowers: Vec<(String, i128)>,
    /// Top vouchers by total staked: (address, total_staked_stroops)
    pub top_vouchers: Vec<(String, i128)>,
    pub timestamp: i64,
}

impl ProtocolMetrics {
    pub fn new() -> Self {
        Self {
            tvl: 0,
            active_loans: 0,
            total_loans: 0,
            defaulted_loans: 0,
            default_rate: 0.0,
            total_yield_distributed: 0,
            slash_count: 0,
            fee_revenue: 0,
            top_borrowers: Vec::new(),
            top_vouchers: Vec::new(),
            timestamp: 0,
        }
    }
}

/// Input record describing a single loan snapshot for aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanSnapshot {
    pub borrower: String,
    pub amount: i128,
    pub status: LoanStatusInput,
    pub yield_distributed: i128,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LoanStatusInput {
    Active,
    Repaid,
    Defaulted,
}

/// Input record for a vouch snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VouchSnapshot {
    pub voucher: String,
    pub stake: i128,
}

/// Filter parameters for the metrics endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsFilter {
    /// Unix timestamp lower bound (inclusive)
    pub from: Option<i64>,
    /// Unix timestamp upper bound (inclusive)
    pub to: Option<i64>,
    /// "small" (<1M stroops), "medium" (1M–100M), "large" (>100M)
    pub loan_size: Option<String>,
}

/// Configurable alert thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Alert when default_rate exceeds this (e.g. 0.05 = 5%)
    pub max_default_rate: f64,
    /// Alert when TVL drops by more than this fraction from peak (e.g. 0.10 = 10%)
    pub max_tvl_drop_fraction: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_default_rate: 0.05,
            max_tvl_drop_fraction: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Alert {
    pub kind: String,
    pub message: String,
}

/// Loan outcome tracking for impact measurement (Issue #886)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanOutcome {
    pub loan_id: u64,
    pub borrower: String,
    pub outcome_status: OutcomeStatus,
    pub loan_purpose: String,
    pub loan_amount: i128,
    pub amount_repaid: i128,
    pub repayment_percentage: f64,
    pub time_to_repayment_days: Option<i64>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OutcomeStatus {
    Active,
    Successful,
    Defaulted,
    PartiallyRepaid,
}

/// Impact metrics aggregated by borrower
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BorrowerImpactMetrics {
    pub borrower: String,
    pub total_loans: u32,
    pub successful_loans: u32,
    pub defaulted_loans: u32,
    pub success_rate: f64,
    pub total_borrowed: i128,
    pub total_repaid: i128,
    pub average_repayment_days: f64,
    pub repeat_borrower: bool,
    pub repeat_count: u32,
}

/// Impact metrics aggregated by loan purpose
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanPurposeMetrics {
    pub purpose: String,
    pub total_loans: u32,
    pub successful_loans: u32,
    pub defaulted_loans: u32,
    pub success_rate: f64,
    pub total_value: i128,
    pub average_loan_amount: i128,
    pub average_repayment_days: f64,
}

/// Comprehensive loan impact report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanImpactReport {
    pub report_timestamp: i64,
    pub from_timestamp: i64,
    pub to_timestamp: i64,
    pub total_outcomes_tracked: u32,
    pub successful_outcomes: u32,
    pub defaulted_outcomes: u32,
    pub success_rate: f64,
    pub average_time_to_repayment: f64,
    pub borrower_metrics: Vec<BorrowerImpactMetrics>,
    pub purpose_metrics: Vec<LoanPurposeMetrics>,
    pub top_purposes: Vec<(String, u32)>,
    pub repeat_borrower_rate: f64,
}

impl LoanImpactReport {
    pub fn new(
        report_timestamp: i64,
        from_timestamp: i64,
        to_timestamp: i64,
    ) -> Self {
        Self {
            report_timestamp,
            from_timestamp,
            to_timestamp,
            total_outcomes_tracked: 0,
            successful_outcomes: 0,
            defaulted_outcomes: 0,
            success_rate: 0.0,
            average_time_to_repayment: 0.0,
            borrower_metrics: Vec::new(),
            purpose_metrics: Vec::new(),
            top_purposes: Vec::new(),
            repeat_borrower_rate: 0.0,
        }
    }

    /// Generate report from loan outcomes
    pub fn from_outcomes(
        outcomes: &[LoanOutcome],
        report_timestamp: i64,
        from_timestamp: i64,
        to_timestamp: i64,
    ) -> Self {
        let mut report = Self::new(report_timestamp, from_timestamp, to_timestamp);

        if outcomes.is_empty() {
            return report;
        }

        report.total_outcomes_tracked = outcomes.len() as u32;

        let mut borrower_map: HashMap<String, Vec<LoanOutcome>> = HashMap::new();
        let mut purpose_map: HashMap<String, Vec<LoanOutcome>> = HashMap::new();
        let mut total_repayment_days: f64 = 0.0;
        let mut repayment_count: u32 = 0;

        for outcome in outcomes {
            // Track by borrower
            borrower_map
                .entry(outcome.borrower.clone())
                .or_insert_with(Vec::new)
                .push(outcome.clone());

            // Track by purpose
            let purpose = outcome.loan_purpose.clone();
            if !purpose.is_empty() {
                purpose_map
                    .entry(purpose)
                    .or_insert_with(Vec::new)
                    .push(outcome.clone());
            }

            // Count outcomes
            match outcome.outcome_status {
                OutcomeStatus::Successful => report.successful_outcomes += 1,
                OutcomeStatus::Defaulted => report.defaulted_outcomes += 1,
                _ => {}
            }

            // Calculate repayment time average
            if let Some(days) = outcome.time_to_repayment_days {
                if days >= 0 {
                    total_repayment_days += days as f64;
                    repayment_count += 1;
                }
            }
        }

        // Calculate success rate
        if report.total_outcomes_tracked > 0 {
            report.success_rate = report.successful_outcomes as f64 / report.total_outcomes_tracked as f64;
        }

        // Calculate average repayment time
        if repayment_count > 0 {
            report.average_time_to_repayment = total_repayment_days / repayment_count as f64;
        }

        // Build borrower metrics
        for (borrower, borrower_outcomes) in borrower_map.iter() {
            let mut successful = 0;
            let mut defaulted = 0;
            let mut total_borrowed = 0i128;
            let mut total_repaid = 0i128;
            let mut repayment_days: f64 = 0.0;
            let mut repayment_count = 0u32;

            for outcome in borrower_outcomes {
                total_borrowed = total_borrowed.saturating_add(outcome.loan_amount);
                total_repaid = total_repaid.saturating_add(outcome.amount_repaid);

                match outcome.outcome_status {
                    OutcomeStatus::Successful => successful += 1,
                    OutcomeStatus::Defaulted => defaulted += 1,
                    _ => {}
                }

                if let Some(days) = outcome.time_to_repayment_days {
                    if days >= 0 {
                        repayment_days += days as f64;
                        repayment_count += 1;
                    }
                }
            }

            let total = borrower_outcomes.len() as u32;
            let success_rate = if total > 0 {
                successful as f64 / total as f64
            } else {
                0.0
            };

            let avg_repayment_days = if repayment_count > 0 {
                repayment_days / repayment_count as f64
            } else {
                0.0
            };

            report.borrower_metrics.push(BorrowerImpactMetrics {
                borrower: borrower.clone(),
                total_loans: total,
                successful_loans: successful,
                defaulted_loans: defaulted,
                success_rate,
                total_borrowed,
                total_repaid,
                average_repayment_days: avg_repayment_days,
                repeat_borrower: total > 1,
                repeat_count: total.saturating_sub(1),
            });
        }

        // Build purpose metrics
        for (purpose, purpose_outcomes) in purpose_map.iter() {
            let mut successful = 0;
            let mut defaulted = 0;
            let mut total_value = 0i128;
            let mut repayment_days: f64 = 0.0;
            let mut repayment_count = 0u32;

            for outcome in purpose_outcomes {
                total_value = total_value.saturating_add(outcome.loan_amount);

                match outcome.outcome_status {
                    OutcomeStatus::Successful => successful += 1,
                    OutcomeStatus::Defaulted => defaulted += 1,
                    _ => {}
                }

                if let Some(days) = outcome.time_to_repayment_days {
                    if days >= 0 {
                        repayment_days += days as f64;
                        repayment_count += 1;
                    }
                }
            }

            let total = purpose_outcomes.len() as u32;
            let success_rate = if total > 0 {
                successful as f64 / total as f64
            } else {
                0.0
            };

            let avg_repayment_days = if repayment_count > 0 {
                repayment_days / repayment_count as f64
            } else {
                0.0
            };

            let avg_loan_amount = if total > 0 {
                total_value / total as i128
            } else {
                0
            };

            report.purpose_metrics.push(LoanPurposeMetrics {
                purpose: purpose.clone(),
                total_loans: total,
                successful_loans: successful,
                defaulted_loans: defaulted,
                success_rate,
                total_value,
                average_loan_amount: avg_loan_amount,
                average_repayment_days: avg_repayment_days,
            });
        }

        // Sort purposes by frequency
        let mut purpose_freq: HashMap<String, u32> = HashMap::new();
        for outcome in outcomes {
            if !outcome.loan_purpose.is_empty() {
                *purpose_freq
                    .entry(outcome.loan_purpose.clone())
                    .or_insert(0) += 1;
            }
        }
        let mut top_purposes: Vec<_> = purpose_freq.into_iter().collect();
        top_purposes.sort_by(|a, b| b.1.cmp(&a.1));
        report.top_purposes = top_purposes.into_iter().take(10).collect();

        // Calculate repeat borrower rate
        let repeat_borrowers = report
            .borrower_metrics
            .iter()
            .filter(|m| m.repeat_borrower)
            .count() as u32;
        if !report.borrower_metrics.is_empty() {
            report.repeat_borrower_rate =
                repeat_borrowers as f64 / report.borrower_metrics.len() as f64;
        }

        report
    }
}

/// Portfolio health metrics for performance dashboard (Issue #888)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanSizeDistribution {
    pub small_count: u32,     // < 1M stroops
    pub medium_count: u32,    // 1M - 100M stroops
    pub large_count: u32,     // > 100M stroops
    pub small_value: i128,
    pub medium_value: i128,
    pub large_value: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortfolioHealthMetrics {
    /// Portfolio timestamp
    pub timestamp: i64,
    /// Total active loans
    pub total_active_loans: u32,
    /// Total portfolio value (TVL)
    pub portfolio_value: i128,
    /// Concentration: ratio of top 5 borrowers
    pub concentration_top_5: f64,
    /// Concentration: ratio of top 10 borrowers
    pub concentration_top_10: f64,
    /// Loan size distribution
    pub size_distribution: LoanSizeDistribution,
    /// Average loan size in stroops
    pub average_loan_size: i128,
    /// Portfolio weighted average days to maturity
    pub weighted_avg_maturity_days: f64,
    /// Success rate across all active portfolio
    pub portfolio_success_rate: f64,
    /// Expected yield rate (total_yield / portfolio_value)
    pub expected_yield_rate: f64,
    /// Current arrears ratio (loans >= 7 days overdue / active loans)
    pub arrears_ratio: f64,
    /// Portfolio health score (0-100)
    pub health_score: f64,
    /// Top borrowers with their portfolio percentage
    pub top_borrowers: Vec<(String, f64)>,
    /// Loan purpose distribution with percentages
    pub purpose_distribution: Vec<(String, f64)>,
}

impl PortfolioHealthMetrics {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            total_active_loans: 0,
            portfolio_value: 0,
            concentration_top_5: 0.0,
            concentration_top_10: 0.0,
            size_distribution: LoanSizeDistribution {
                small_count: 0,
                medium_count: 0,
                large_count: 0,
                small_value: 0,
                medium_value: 0,
                large_value: 0,
            },
            average_loan_size: 0,
            weighted_avg_maturity_days: 0.0,
            portfolio_success_rate: 0.0,
            expected_yield_rate: 0.0,
            arrears_ratio: 0.0,
            health_score: 0.0,
            top_borrowers: Vec::new(),
            purpose_distribution: Vec::new(),
        }
    }

    /// Calculate portfolio health metrics from active loans and impact data
    pub fn from_loans_and_outcomes(
        loans: &[LoanSnapshot],
        outcomes: &[LoanOutcome],
        timestamp: i64,
    ) -> Self {
        let mut metrics = Self::new(timestamp);

        // Filter active loans only
        let active_loans: Vec<&LoanSnapshot> = loans
            .iter()
            .filter(|l| l.status == LoanStatusInput::Active)
            .collect();

        if active_loans.is_empty() {
            return metrics;
        }

        metrics.total_active_loans = active_loans.len() as u32;

        // Calculate TVL and size distribution
        let mut borrower_totals: HashMap<String, i128> = HashMap::new();
        let mut purpose_totals: HashMap<String, (i128, u32)> = HashMap::new();

        for loan in &active_loans {
            metrics.portfolio_value = metrics.portfolio_value.saturating_add(loan.amount);

            // Track by borrower
            *borrower_totals.entry(loan.borrower.clone()).or_insert(0) += loan.amount;

            // Categorize by size
            match loan.amount {
                a if a < 1_000_000 => {
                    metrics.size_distribution.small_count += 1;
                    metrics.size_distribution.small_value =
                        metrics.size_distribution.small_value.saturating_add(loan.amount);
                }
                a if a <= 100_000_000 => {
                    metrics.size_distribution.medium_count += 1;
                    metrics.size_distribution.medium_value =
                        metrics.size_distribution.medium_value.saturating_add(loan.amount);
                }
                _ => {
                    metrics.size_distribution.large_count += 1;
                    metrics.size_distribution.large_value =
                        metrics.size_distribution.large_value.saturating_add(loan.amount);
                }
            }
        }

        // Calculate average loan size
        if metrics.total_active_loans > 0 {
            metrics.average_loan_size = metrics.portfolio_value / metrics.total_active_loans as i128;
        }

        // Calculate concentration ratios
        let mut borrower_amounts: Vec<_> = borrower_totals.into_iter().collect();
        borrower_amounts.sort_by(|a, b| b.1.cmp(&a.1));

        let top_5_value: i128 = borrower_amounts
            .iter()
            .take(5)
            .map(|(_, amount)| amount)
            .sum();
        let top_10_value: i128 = borrower_amounts
            .iter()
            .take(10)
            .map(|(_, amount)| amount)
            .sum();

        if metrics.portfolio_value > 0 {
            metrics.concentration_top_5 = top_5_value as f64 / metrics.portfolio_value as f64;
            metrics.concentration_top_10 = top_10_value as f64 / metrics.portfolio_value as f64;

            // Top borrowers with percentages
            metrics.top_borrowers = borrower_amounts
                .iter()
                .take(5)
                .map(|(borrower, amount)| {
                    (borrower.clone(), *amount as f64 / metrics.portfolio_value as f64)
                })
                .collect();
        }

        // Calculate portfolio metrics from outcomes
        let mut total_yield_expected = 0i128;
        let mut success_count = 0u32;
        let mut maturity_weighted_sum = 0.0;

        for outcome in outcomes {
            if outcome.outcome_status == OutcomeStatus::Successful
                || outcome.outcome_status == OutcomeStatus::PartiallyRepaid
            {
                success_count += 1;
            }

            // Estimate yield (simplified: use repaid amount minus principal)
            if outcome.amount_repaid >= outcome.loan_amount {
                let estimated_yield = outcome.amount_repaid - outcome.loan_amount;
                total_yield_expected = total_yield_expected.saturating_add(estimated_yield);
            }

            // Weight maturity by loan amount
            if let Some(days) = outcome.time_to_repayment_days {
                if days >= 0 {
                    maturity_weighted_sum +=
                        (days as f64) * (outcome.loan_amount as f64 / metrics.portfolio_value as f64);
                }
            }
        }

        // Calculate success rate from outcomes
        if !outcomes.is_empty() {
            metrics.portfolio_success_rate = success_count as f64 / outcomes.len() as f64;
        }

        // Calculate expected yield rate
        if metrics.portfolio_value > 0 {
            metrics.expected_yield_rate = total_yield_expected as f64 / metrics.portfolio_value as f64;
            metrics.weighted_avg_maturity_days = maturity_weighted_sum;
        }

        // Calculate health score (0-100)
        // Components: success rate (40%), concentration (20%), diversification (20%), yield (20%)
        let concentration_penalty = (metrics.concentration_top_5 - 0.3).max(0.0) * 20.0; // Penalize if > 30%
        let yield_score = (metrics.expected_yield_rate * 1000.0).min(20.0); // Cap at 20 points
        let success_score = metrics.portfolio_success_rate * 40.0;
        let diversification_score = (1.0 - metrics.concentration_top_10).min(1.0) * 20.0;

        metrics.health_score =
            (success_score + diversification_score + yield_score - concentration_penalty).max(0.0);

        // Purpose distribution from outcomes
        let mut purpose_dist: HashMap<String, (i128, u32)> = HashMap::new();
        for outcome in outcomes {
            let entry = purpose_dist
                .entry(outcome.loan_purpose.clone())
                .or_insert((0, 0));
            entry.0 = entry.0.saturating_add(outcome.loan_amount);
            entry.1 += 1;
        }

        for (purpose, (value, count)) in purpose_dist.iter() {
            let percentage = *value as f64 / metrics.portfolio_value as f64;
            metrics.purpose_distribution.push((purpose.clone(), percentage));
        }

        metrics.purpose_distribution.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        metrics
    }
}

/// Early loan buyout tracking (Issue #889)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoanBuyoutRecord {
    pub loan_id: u64,
    pub borrower: String,
    pub buyer: String,
    pub buyout_amount: i128,
    pub remaining_principal: i128,
    pub remaining_yield: i128,
    pub buyout_timestamp: i64,
    pub days_to_maturity: i64,
    pub interest_saved: i128,
    pub buyout_status: BuyoutStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BuyoutStatus {
    Proposed,
    Accepted,
    Completed,
    Cancelled,
}

/// Buyout metrics aggregated for portfolio analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuyoutMetrics {
    pub total_buyouts_completed: u32,
    pub total_buyout_value: i128,
    pub total_interest_saved: i128,
    pub average_days_to_maturity_at_buyout: f64,
    pub average_buyout_amount: i128,
    pub unique_buyers: u32,
    pub unique_borrowers: u32,
    pub buyout_adoption_rate: f64, // completed buyouts / total loans
}

impl BuyoutMetrics {
    pub fn new() -> Self {
        Self {
            total_buyouts_completed: 0,
            total_buyout_value: 0,
            total_interest_saved: 0,
            average_days_to_maturity_at_buyout: 0.0,
            average_buyout_amount: 0,
            unique_buyers: 0,
            unique_borrowers: 0,
            buyout_adoption_rate: 0.0,
        }
    }

    /// Calculate buyout metrics from buyout records
    pub fn from_buyout_records(
        records: &[LoanBuyoutRecord],
        total_loans: u32,
    ) -> Self {
        let mut metrics = Self::new();

        if records.is_empty() {
            return metrics;
        }

        let completed: Vec<&LoanBuyoutRecord> = records
            .iter()
            .filter(|r| r.buyout_status == BuyoutStatus::Completed)
            .collect();

        if completed.is_empty() {
            return metrics;
        }

        metrics.total_buyouts_completed = completed.len() as u32;

        let mut buyers = std::collections::HashSet::new();
        let mut borrowers = std::collections::HashSet::new();
        let mut total_days = 0.0;

        for record in &completed {
            buyers.insert(record.buyer.clone());
            borrowers.insert(record.borrower.clone());
            metrics.total_buyout_value = metrics.total_buyout_value.saturating_add(record.buyout_amount);
            metrics.total_interest_saved = metrics.total_interest_saved.saturating_add(record.interest_saved);
            total_days += record.days_to_maturity as f64;
        }

        metrics.unique_buyers = buyers.len() as u32;
        metrics.unique_borrowers = borrowers.len() as u32;

        if !completed.is_empty() {
            metrics.average_buyout_amount = metrics.total_buyout_value / completed.len() as i128;
            metrics.average_days_to_maturity_at_buyout = total_days / completed.len() as f64;
        }

        if total_loans > 0 {
            metrics.buyout_adoption_rate = metrics.total_buyouts_completed as f64 / total_loans as f64;
        }

        metrics
    }
}

/// High-risk yield multiplier tracking (Issue #890)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskYieldRecord {
    pub loan_id: u64,
    pub borrower: String,
    pub risk_score: u32,           // 0-100
    pub base_yield_bps: u32,       // base yield in basis points
    pub risk_multiplier: f64,      // applied multiplier (e.g., 1.5x for high risk)
    pub adjusted_yield_bps: u32,   // final yield after multiplier
    pub yield_compensation: i128,  // additional yield in stroops for risk compensation
    pub risk_category: RiskCategory,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum RiskCategory {
    Low,      // 0-30 risk score
    Medium,   // 31-60 risk score
    High,     // 61-85 risk score
    Critical, // 86-100 risk score
}

impl RiskCategory {
    pub fn from_risk_score(score: u32) -> Self {
        match score {
            0..=30 => RiskCategory::Low,
            31..=60 => RiskCategory::Medium,
            61..=85 => RiskCategory::High,
            _ => RiskCategory::Critical,
        }
    }

    pub fn multiplier(&self) -> f64 {
        match self {
            RiskCategory::Low => 1.0,
            RiskCategory::Medium => 1.2,
            RiskCategory::High => 1.5,
            RiskCategory::Critical => 2.0,
        }
    }
}

/// Risk-adjusted yield metrics for portfolio analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HighRiskYieldMetrics {
    pub total_high_risk_loans: u32,
    pub total_critical_risk_loans: u32,
    pub total_risk_compensation: i128,
    pub average_risk_score: f64,
    pub average_yield_multiplier: f64,
    pub risk_distribution: Vec<(String, u32)>, // (risk_category, count)
    pub total_adjusted_yield: i128,
    pub yield_increase_from_risk: i128,
    pub weighted_avg_multiplier: f64,
}

impl HighRiskYieldMetrics {
    pub fn new() -> Self {
        Self {
            total_high_risk_loans: 0,
            total_critical_risk_loans: 0,
            total_risk_compensation: 0,
            average_risk_score: 0.0,
            average_yield_multiplier: 1.0,
            risk_distribution: Vec::new(),
            total_adjusted_yield: 0,
            yield_increase_from_risk: 0,
            weighted_avg_multiplier: 1.0,
        }
    }

    /// Calculate risk-adjusted yield metrics from risk-yield records
    pub fn from_risk_yield_records(records: &[RiskYieldRecord]) -> Self {
        let mut metrics = Self::new();

        if records.is_empty() {
            return metrics;
        }

        let mut risk_counts: HashMap<String, u32> = HashMap::new();
        let mut total_risk_score: f64 = 0.0;
        let mut total_multiplier: f64 = 0.0;
        let mut total_base_yield: i128 = 0;
        let mut weighted_multiplier_sum: f64 = 0.0;

        for record in records {
            metrics.total_risk_compensation = metrics.total_risk_compensation.saturating_add(record.yield_compensation);
            metrics.total_adjusted_yield = metrics.total_adjusted_yield.saturating_add(record.adjusted_yield_bps as i128);

            total_risk_score += record.risk_score as f64;
            total_multiplier += record.risk_multiplier;
            total_base_yield = total_base_yield.saturating_add(record.base_yield_bps as i128);
            weighted_multiplier_sum += record.risk_multiplier * (record.base_yield_bps as f64);

            // Count by risk category
            let category_name = match record.risk_category {
                RiskCategory::Low => "Low".to_string(),
                RiskCategory::Medium => "Medium".to_string(),
                RiskCategory::High => "High".to_string(),
                RiskCategory::Critical => "Critical".to_string(),
            };
            *risk_counts.entry(category_name).or_insert(0) += 1;

            // Track high-risk and critical
            if record.risk_score > 60 {
                metrics.total_high_risk_loans += 1;
            }
            if record.risk_score > 85 {
                metrics.total_critical_risk_loans += 1;
            }
        }

        let record_count = records.len() as f64;
        metrics.average_risk_score = total_risk_score / record_count;
        metrics.average_yield_multiplier = total_multiplier / record_count;

        if total_base_yield > 0 {
            metrics.weighted_avg_multiplier = weighted_multiplier_sum / (total_base_yield as f64);
        }

        // Calculate yield increase from risk compensation
        metrics.yield_increase_from_risk = metrics.total_adjusted_yield
            .saturating_sub(total_base_yield.saturating_mul(records.len() as i128));

        // Build risk distribution
        let mut dist: Vec<(String, u32)> = risk_counts.into_iter().collect();
        dist.sort_by(|a, b| {
            // Sort: Critical > High > Medium > Low
            let order_a = match a.0.as_str() {
                "Critical" => 3,
                "High" => 2,
                "Medium" => 1,
                "Low" => 0,
                _ => -1,
            };
            let order_b = match b.0.as_str() {
                "Critical" => 3,
                "High" => 2,
                "Medium" => 1,
                "Low" => 0,
                _ => -1,
            };
            order_b.cmp(&order_a)
        });
        metrics.risk_distribution = dist;

        metrics
    }
}

/// Compute `ProtocolMetrics` from raw loan + vouch snapshots, applying optional filters.
pub fn aggregate_metrics(
    loans: &[LoanSnapshot],
    vouches: &[VouchSnapshot],
    slash_count: u32,
    fee_revenue: i128,
    filter: &MetricsFilter,
    now_ts: i64,
) -> ProtocolMetrics {
    // Apply filters
    let filtered: Vec<&LoanSnapshot> = loans
        .iter()
        .filter(|l| {
            if let Some(from) = filter.from {
                if l.created_at < from {
                    return false;
                }
            }
            if let Some(to) = filter.to {
                if l.created_at > to {
                    return false;
                }
            }
            if let Some(size) = &filter.loan_size {
                match size.as_str() {
                    "small" if l.amount >= 1_000_000 => return false,
                    "medium" if l.amount < 1_000_000 || l.amount > 100_000_000 => return false,
                    "large" if l.amount <= 100_000_000 => return false,
                    _ => {}
                }
            }
            true
        })
        .collect();

    let total_loans = filtered.len() as u32;
    let active_loans = filtered
        .iter()
        .filter(|l| l.status == LoanStatusInput::Active)
        .count() as u32;
    let defaulted_loans = filtered
        .iter()
        .filter(|l| l.status == LoanStatusInput::Defaulted)
        .count() as u32;

    let tvl: i128 = filtered
        .iter()
        .filter(|l| l.status == LoanStatusInput::Active)
        .map(|l| l.amount)
        .sum();

    let total_yield_distributed: i128 = filtered.iter().map(|l| l.yield_distributed).sum();

    let default_rate = if total_loans > 0 {
        defaulted_loans as f64 / total_loans as f64
    } else {
        0.0
    };

    // Top 5 borrowers by amount
    let mut borrower_totals: HashMap<String, i128> = HashMap::new();
    for l in &filtered {
        *borrower_totals.entry(l.borrower.clone()).or_insert(0) += l.amount;
    }
    let mut top_borrowers: Vec<(String, i128)> = borrower_totals.into_iter().collect();
    top_borrowers.sort_by(|a, b| b.1.cmp(&a.1));
    top_borrowers.truncate(5);

    // Top 5 vouchers by stake
    let mut voucher_totals: HashMap<String, i128> = HashMap::new();
    for v in vouches {
        *voucher_totals.entry(v.voucher.clone()).or_insert(0) += v.stake;
    }
    let mut top_vouchers: Vec<(String, i128)> = voucher_totals.into_iter().collect();
    top_vouchers.sort_by(|a, b| b.1.cmp(&a.1));
    top_vouchers.truncate(5);

    ProtocolMetrics {
        tvl,
        active_loans,
        total_loans,
        defaulted_loans,
        default_rate,
        total_yield_distributed,
        slash_count,
        fee_revenue,
        top_borrowers,
        top_vouchers,
        timestamp: now_ts,
    }
}

/// Check thresholds and return any triggered alerts.
pub fn check_alerts(
    metrics: &ProtocolMetrics,
    peak_tvl: i128,
    thresholds: &AlertThresholds,
) -> Vec<Alert> {
    let mut alerts = Vec::new();

    if metrics.default_rate > thresholds.max_default_rate {
        alerts.push(Alert {
            kind: "high_default_rate".to_string(),
            message: format!(
                "Default rate {:.1}% exceeds threshold {:.1}%",
                metrics.default_rate * 100.0,
                thresholds.max_default_rate * 100.0
            ),
        });
    }

    if peak_tvl > 0 {
        let drop = (peak_tvl - metrics.tvl) as f64 / peak_tvl as f64;
        if drop > thresholds.max_tvl_drop_fraction {
            alerts.push(Alert {
                kind: "tvl_drop".to_string(),
                message: format!(
                    "TVL dropped {:.1}% from peak, exceeds threshold {:.1}%",
                    drop * 100.0,
                    thresholds.max_tvl_drop_fraction * 100.0
                ),
            });
        }
    }

    alerts
}

/// Serialize metrics to CSV string.
/// Columns: timestamp,tvl,active_loans,total_loans,defaulted_loans,default_rate,
///          total_yield_distributed,slash_count,fee_revenue
pub fn metrics_to_csv(rows: &[ProtocolMetrics]) -> String {
    let mut out = String::from(
        "timestamp,tvl,active_loans,total_loans,defaulted_loans,\
         default_rate,total_yield_distributed,slash_count,fee_revenue\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{:.6},{},{},{}\n",
            r.timestamp,
            r.tvl,
            r.active_loans,
            r.total_loans,
            r.defaulted_loans,
            r.default_rate,
            r.total_yield_distributed,
            r.slash_count,
            r.fee_revenue,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_loans() -> Vec<LoanSnapshot> {
        vec![
            LoanSnapshot {
                borrower: "addr_a".into(),
                amount: 5_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 1000,
            },
            LoanSnapshot {
                borrower: "addr_b".into(),
                amount: 3_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 2000,
            },
            LoanSnapshot {
                borrower: "addr_c".into(),
                amount: 1_000_000_000,
                status: LoanStatusInput::Defaulted,
                yield_distributed: 0,
                created_at: 3000,
            },
            LoanSnapshot {
                borrower: "addr_d".into(),
                amount: 2_000_000_000,
                status: LoanStatusInput::Repaid,
                yield_distributed: 40_000_000,
                created_at: 4000,
            },
        ]
    }

    fn sample_vouches() -> Vec<VouchSnapshot> {
        vec![
            VouchSnapshot { voucher: "v1".into(), stake: 1_000_000_000 },
            VouchSnapshot { voucher: "v2".into(), stake: 500_000_000 },
            VouchSnapshot { voucher: "v1".into(), stake: 200_000_000 },
        ]
    }

    // Test 1: TVL = sum of active loan amounts only
    #[test]
    fn test_tvl_equals_sum_of_active_loans() {
        let metrics = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &MetricsFilter::default(), 0,
        );
        // active: addr_a (5B) + addr_b (3B) = 8B stroops
        assert_eq!(metrics.tvl, 8_000_000_000);
    }

    // Test 2: Default rate = defaulted / total
    #[test]
    fn test_default_rate_calculation() {
        let loans: Vec<LoanSnapshot> = (0..10)
            .map(|i| LoanSnapshot {
                borrower: format!("addr_{}", i),
                amount: 1_000_000_000,
                status: if i < 2 {
                    LoanStatusInput::Defaulted
                } else {
                    LoanStatusInput::Repaid
                },
                yield_distributed: 0,
                created_at: i as i64,
            })
            .collect();
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.total_loans, 10);
        assert_eq!(m.defaulted_loans, 2);
        assert!((m.default_rate - 0.2).abs() < 1e-9);
    }

    // Test 3: Zero loans → default_rate = 0.0, no panic
    #[test]
    fn test_default_rate_no_loans() {
        let m = aggregate_metrics(&[], &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.default_rate, 0.0);
        assert_eq!(m.tvl, 0);
    }

    // Test 4: active_loans count is correct
    #[test]
    fn test_active_loans_count() {
        let m = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &MetricsFilter::default(), 0,
        );
        assert_eq!(m.active_loans, 2);
    }

    // Test 5: Yield distributed is summed across all filtered loans
    #[test]
    fn test_yield_distribution_sum() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 100, status: LoanStatusInput::Repaid,
                yield_distributed: 20_000_000, created_at: 0,
            },
            LoanSnapshot {
                borrower: "b".into(), amount: 100, status: LoanStatusInput::Repaid,
                yield_distributed: 10_000_000, created_at: 0,
            },
        ];
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.total_yield_distributed, 30_000_000);
    }

    // Test 6: Date range filter excludes out-of-range loans
    #[test]
    fn test_date_range_filter() {
        let filter = MetricsFilter { from: Some(1500), to: Some(3500), loan_size: None };
        let m = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &filter, 0,
        );
        // Only loans with created_at in [1500, 3500]: addr_b (2000), addr_c (3000)
        assert_eq!(m.total_loans, 2);
    }

    // Test 7: Loan size filter "small" keeps only < 1M stroops
    #[test]
    fn test_loan_size_filter_small() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 500_000,
                status: LoanStatusInput::Active, yield_distributed: 0, created_at: 0,
            },
            LoanSnapshot {
                borrower: "b".into(), amount: 2_000_000,
                status: LoanStatusInput::Active, yield_distributed: 0, created_at: 0,
            },
        ];
        let filter = MetricsFilter { loan_size: Some("small".into()), ..Default::default() };
        let m = aggregate_metrics(&loans, &[], 0, 0, &filter, 0);
        assert_eq!(m.total_loans, 1);
        assert_eq!(m.tvl, 500_000);
    }

    // Test 8: Top borrowers sorted by descending total amount
    #[test]
    fn test_top_borrowers_sorted() {
        let m = aggregate_metrics(
            &sample_loans(), &[], 0, 0, &MetricsFilter::default(), 0,
        );
        // addr_a=5B, addr_b=3B, addr_d=2B, addr_c=1B
        assert_eq!(m.top_borrowers[0].0, "addr_a");
        assert_eq!(m.top_borrowers[0].1, 5_000_000_000);
    }

    // Test 9: Top vouchers aggregates by voucher address
    #[test]
    fn test_top_vouchers_aggregated() {
        let m = aggregate_metrics(
            &[], &sample_vouches(), 0, 0, &MetricsFilter::default(), 0,
        );
        // v1 = 1.2B, v2 = 0.5B
        assert_eq!(m.top_vouchers[0].0, "v1");
        assert_eq!(m.top_vouchers[0].1, 1_200_000_000);
    }

    // Test 10: Top lists capped at 5 entries
    #[test]
    fn test_top_borrowers_capped_at_5() {
        let loans: Vec<LoanSnapshot> = (0..10)
            .map(|i| LoanSnapshot {
                borrower: format!("addr_{}", i),
                amount: (i as i128 + 1) * 1_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            })
            .collect();
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.top_borrowers.len(), 5);
    }

    // Test 11: Alert fires when default rate exceeds threshold
    #[test]
    fn test_alert_high_default_rate() {
        let m = ProtocolMetrics {
            default_rate: 0.06,
            ..ProtocolMetrics::new()
        };
        let alerts = check_alerts(&m, 0, &AlertThresholds::default());
        assert!(alerts.iter().any(|a| a.kind == "high_default_rate"));
    }

    // Test 12: No alert when default rate is below threshold
    #[test]
    fn test_no_alert_default_rate_below_threshold() {
        let m = ProtocolMetrics {
            default_rate: 0.03,
            ..ProtocolMetrics::new()
        };
        let alerts = check_alerts(&m, 0, &AlertThresholds::default());
        assert!(!alerts.iter().any(|a| a.kind == "high_default_rate"));
    }

    // Test 13: Alert fires when TVL drops > 10% from peak
    #[test]
    fn test_alert_tvl_drop() {
        let m = ProtocolMetrics {
            tvl: 8_000_000_000,
            ..ProtocolMetrics::new()
        };
        let peak = 10_000_000_000i128;
        let alerts = check_alerts(&m, peak, &AlertThresholds::default());
        assert!(alerts.iter().any(|a| a.kind == "tvl_drop"));
    }

    // Test 14: No TVL alert when drop is within threshold
    #[test]
    fn test_no_alert_tvl_small_drop() {
        let m = ProtocolMetrics {
            tvl: 9_500_000_000,
            ..ProtocolMetrics::new()
        };
        let peak = 10_000_000_000i128;
        let alerts = check_alerts(&m, peak, &AlertThresholds::default());
        assert!(!alerts.iter().any(|a| a.kind == "tvl_drop"));
    }

    // Test 15: Custom alert threshold respected
    #[test]
    fn test_custom_alert_threshold() {
        let m = ProtocolMetrics {
            default_rate: 0.03,
            ..ProtocolMetrics::new()
        };
        let thresholds = AlertThresholds {
            max_default_rate: 0.02,
            max_tvl_drop_fraction: 0.10,
        };
        let alerts = check_alerts(&m, 0, &thresholds);
        assert!(alerts.iter().any(|a| a.kind == "high_default_rate"));
    }

    // Test 16: CSV has correct headers
    #[test]
    fn test_csv_headers() {
        let csv = metrics_to_csv(&[]);
        assert!(csv.starts_with(
            "timestamp,tvl,active_loans,total_loans,defaulted_loans,\
             default_rate,total_yield_distributed,slash_count,fee_revenue"
        ));
    }

    // Test 17: CSV data rows contain correct values
    #[test]
    fn test_csv_data_rows() {
        let row = ProtocolMetrics {
            tvl: 5_000_000_000,
            active_loans: 2,
            total_loans: 4,
            defaulted_loans: 1,
            default_rate: 0.25,
            total_yield_distributed: 100_000_000,
            slash_count: 1,
            fee_revenue: 50_000,
            top_borrowers: vec![],
            top_vouchers: vec![],
            timestamp: 9999,
        };
        let csv = metrics_to_csv(&[row]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 data row
        assert!(lines[1].starts_with("9999,5000000000,2,4,1,"));
    }

    // Test 18: slash_count and fee_revenue pass through unchanged
    #[test]
    fn test_slash_count_and_fee_revenue_passthrough() {
        let m = aggregate_metrics(&[], &[], 7, 1_234_567, &MetricsFilter::default(), 42);
        assert_eq!(m.slash_count, 7);
        assert_eq!(m.fee_revenue, 1_234_567);
        assert_eq!(m.timestamp, 42);
    }

    // Test 19: Defaulted loans are excluded from TVL
    #[test]
    fn test_defaulted_loans_excluded_from_tvl() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 1_000_000_000,
                status: LoanStatusInput::Defaulted, yield_distributed: 0, created_at: 0,
            },
        ];
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.tvl, 0);
        assert_eq!(m.defaulted_loans, 1);
    }

    // Test 20: Repaid loans are excluded from TVL and active count
    #[test]
    fn test_repaid_loans_excluded_from_tvl_and_active_count() {
        let loans = vec![
            LoanSnapshot {
                borrower: "a".into(), amount: 1_000_000_000,
                status: LoanStatusInput::Repaid, yield_distributed: 20_000_000, created_at: 0,
            },
        ];
        let m = aggregate_metrics(&loans, &[], 0, 0, &MetricsFilter::default(), 0);
        assert_eq!(m.tvl, 0);
        assert_eq!(m.active_loans, 0);
        assert_eq!(m.total_yield_distributed, 20_000_000);
    }

    // Test 21: LoanImpactReport generates correctly from outcomes
    #[test]
    fn test_loan_impact_report_from_outcomes() {
        let outcomes = vec![
            LoanOutcome {
                loan_id: 1,
                borrower: "b1".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 1_000_000_000,
                amount_repaid: 1_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(30),
                created_at: 1000,
                completed_at: Some(2000),
            },
            LoanOutcome {
                loan_id: 2,
                borrower: "b2".to_string(),
                outcome_status: OutcomeStatus::Defaulted,
                loan_purpose: "education".to_string(),
                loan_amount: 500_000_000,
                amount_repaid: 200_000_000,
                repayment_percentage: 40.0,
                time_to_repayment_days: None,
                created_at: 1500,
                completed_at: None,
            },
        ];

        let report = LoanImpactReport::from_outcomes(&outcomes, 5000, 0, 5000);

        assert_eq!(report.total_outcomes_tracked, 2);
        assert_eq!(report.successful_outcomes, 1);
        assert_eq!(report.defaulted_outcomes, 1);
        assert!(report.success_rate > 0.49 && report.success_rate < 0.51); // ~50%
        assert_eq!(report.average_time_to_repayment, 30.0);
        assert_eq!(report.borrower_metrics.len(), 2);
        assert_eq!(report.purpose_metrics.len(), 2);
        assert!(report.repeat_borrower_rate >= 0.0);
    }

    // Test 22: Borrower impact metrics track repeat borrowers
    #[test]
    fn test_borrower_repeat_tracking() {
        let outcomes = vec![
            LoanOutcome {
                loan_id: 1,
                borrower: "repeat_b".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 1_000_000_000,
                amount_repaid: 1_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(25),
                created_at: 1000,
                completed_at: Some(2000),
            },
            LoanOutcome {
                loan_id: 2,
                borrower: "repeat_b".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 500_000_000,
                amount_repaid: 500_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(35),
                created_at: 3000,
                completed_at: Some(4000),
            },
        ];

        let report = LoanImpactReport::from_outcomes(&outcomes, 5000, 0, 5000);

        assert_eq!(report.borrower_metrics.len(), 1);
        assert_eq!(report.borrower_metrics[0].repeat_borrower, true);
        assert_eq!(report.borrower_metrics[0].repeat_count, 1);
        assert_eq!(report.borrower_metrics[0].total_loans, 2);
        assert_eq!(report.repeat_borrower_rate, 1.0);
    }

    // Test 23: Purpose metrics aggregate correctly
    #[test]
    fn test_purpose_metrics_aggregation() {
        let outcomes = vec![
            LoanOutcome {
                loan_id: 1,
                borrower: "b1".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 2_000_000_000,
                amount_repaid: 2_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(20),
                created_at: 1000,
                completed_at: Some(2000),
            },
            LoanOutcome {
                loan_id: 2,
                borrower: "b2".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 1_000_000_000,
                amount_repaid: 1_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(40),
                created_at: 1500,
                completed_at: Some(3000),
            },
        ];

        let report = LoanImpactReport::from_outcomes(&outcomes, 5000, 0, 5000);

        assert_eq!(report.purpose_metrics.len(), 1);
        let business_metrics = &report.purpose_metrics[0];
        assert_eq!(business_metrics.purpose, "business");
        assert_eq!(business_metrics.total_loans, 2);
        assert_eq!(business_metrics.successful_loans, 2);
        assert_eq!(business_metrics.total_value, 3_000_000_000);
        assert_eq!(business_metrics.average_loan_amount, 1_500_000_000);
        assert_eq!(business_metrics.average_repayment_days, 30.0);
    }

    // Test 24: Empty outcomes generate empty report
    #[test]
    fn test_empty_outcomes_report() {
        let report = LoanImpactReport::from_outcomes(&[], 5000, 0, 5000);

        assert_eq!(report.total_outcomes_tracked, 0);
        assert_eq!(report.successful_outcomes, 0);
        assert_eq!(report.defaulted_outcomes, 0);
        assert_eq!(report.success_rate, 0.0);
        assert!(report.borrower_metrics.is_empty());
        assert!(report.purpose_metrics.is_empty());
    }

    // Test 25: Portfolio health metrics from loans and outcomes
    #[test]
    fn test_portfolio_health_metrics_calculation() {
        let loans = vec![
            LoanSnapshot {
                borrower: "b1".into(),
                amount: 5_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 100_000_000,
                created_at: 1000,
            },
            LoanSnapshot {
                borrower: "b2".into(),
                amount: 3_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 60_000_000,
                created_at: 2000,
            },
            LoanSnapshot {
                borrower: "b3".into(),
                amount: 500_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 10_000_000,
                created_at: 3000,
            },
        ];

        let outcomes = vec![
            LoanOutcome {
                loan_id: 1,
                borrower: "b1".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 5_000_000_000,
                amount_repaid: 5_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(25),
                created_at: 1000,
                completed_at: Some(2000),
            },
            LoanOutcome {
                loan_id: 2,
                borrower: "b2".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 3_000_000_000,
                amount_repaid: 3_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(30),
                created_at: 2000,
                completed_at: Some(3000),
            },
            LoanOutcome {
                loan_id: 3,
                borrower: "b3".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "education".to_string(),
                loan_amount: 500_000_000,
                amount_repaid: 500_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(20),
                created_at: 3000,
                completed_at: Some(4000),
            },
        ];

        let health = PortfolioHealthMetrics::from_loans_and_outcomes(&loans, &outcomes, 5000);

        assert_eq!(health.total_active_loans, 3);
        assert_eq!(health.portfolio_value, 8_500_000_000);
        assert_eq!(health.average_loan_size, 2_833_333_333);
        assert!(health.concentration_top_5 > 0.0);
        assert_eq!(health.size_distribution.large_count, 2);
        assert_eq!(health.size_distribution.medium_count, 1);
        assert_eq!(health.portfolio_success_rate, 1.0); // 100% successful
        assert!(health.health_score > 50.0);
    }

    // Test 26: Loan size distribution categories correctly
    #[test]
    fn test_loan_size_distribution() {
        let loans = vec![
            LoanSnapshot {
                borrower: "b1".into(),
                amount: 500_000,      // small
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b2".into(),
                amount: 50_000_000,    // medium
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b3".into(),
                amount: 200_000_000,   // large
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
        ];

        let outcomes = vec![];
        let health = PortfolioHealthMetrics::from_loans_and_outcomes(&loans, &outcomes, 0);

        assert_eq!(health.size_distribution.small_count, 1);
        assert_eq!(health.size_distribution.medium_count, 1);
        assert_eq!(health.size_distribution.large_count, 1);
    }

    // Test 27: Concentration metrics calculated correctly
    #[test]
    fn test_concentration_metrics() {
        let loans = vec![
            LoanSnapshot {
                borrower: "b1".into(),
                amount: 5_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b2".into(),
                amount: 2_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b3".into(),
                amount: 1_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b4".into(),
                amount: 1_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b5".into(),
                amount: 500_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
        ];

        let outcomes = vec![];
        let health = PortfolioHealthMetrics::from_loans_and_outcomes(&loans, &outcomes, 0);

        // Top 5: 5B + 2B + 1B + 1B + 0.5B = 9.5B / 10B = 0.95
        assert!(health.concentration_top_5 > 0.94 && health.concentration_top_5 < 0.96);
        assert_eq!(health.top_borrowers.len(), 5);
        assert_eq!(health.top_borrowers[0].0, "b1");
    }

    // Test 28: Portfolio health score calculation
    #[test]
    fn test_portfolio_health_score() {
        let loans = vec![
            LoanSnapshot {
                borrower: "b1".into(),
                amount: 2_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
            LoanSnapshot {
                borrower: "b2".into(),
                amount: 2_000_000_000,
                status: LoanStatusInput::Active,
                yield_distributed: 0,
                created_at: 0,
            },
        ];

        let outcomes = vec![
            LoanOutcome {
                loan_id: 1,
                borrower: "b1".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 2_000_000_000,
                amount_repaid: 2_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(20),
                created_at: 0,
                completed_at: Some(1000),
            },
            LoanOutcome {
                loan_id: 2,
                borrower: "b2".to_string(),
                outcome_status: OutcomeStatus::Successful,
                loan_purpose: "business".to_string(),
                loan_amount: 2_000_000_000,
                amount_repaid: 2_000_000_000,
                repayment_percentage: 100.0,
                time_to_repayment_days: Some(20),
                created_at: 0,
                completed_at: Some(1000),
            },
        ];

        let health = PortfolioHealthMetrics::from_loans_and_outcomes(&loans, &outcomes, 0);

        assert!(health.health_score > 0.0);
        assert!(health.health_score <= 100.0);
        assert_eq!(health.portfolio_success_rate, 1.0);
    }

    // Test 29: Buyout metrics calculation from records
    #[test]
    fn test_buyout_metrics_from_records() {
        let records = vec![
            LoanBuyoutRecord {
                loan_id: 1,
                borrower: "borrower1".to_string(),
                buyer: "buyer1".to_string(),
                buyout_amount: 5_000_000_000,
                remaining_principal: 4_500_000_000,
                remaining_yield: 500_000_000,
                buyout_timestamp: 1000,
                days_to_maturity: 30,
                interest_saved: 100_000_000,
                buyout_status: BuyoutStatus::Completed,
            },
            LoanBuyoutRecord {
                loan_id: 2,
                borrower: "borrower2".to_string(),
                buyer: "buyer2".to_string(),
                buyout_amount: 3_000_000_000,
                remaining_principal: 2_700_000_000,
                remaining_yield: 300_000_000,
                buyout_timestamp: 2000,
                days_to_maturity: 20,
                interest_saved: 60_000_000,
                buyout_status: BuyoutStatus::Completed,
            },
        ];

        let metrics = BuyoutMetrics::from_buyout_records(&records, 100);

        assert_eq!(metrics.total_buyouts_completed, 2);
        assert_eq!(metrics.total_buyout_value, 8_000_000_000);
        assert_eq!(metrics.total_interest_saved, 160_000_000);
        assert_eq!(metrics.unique_buyers, 2);
        assert_eq!(metrics.unique_borrowers, 2);
        assert_eq!(metrics.average_buyout_amount, 4_000_000_000);
        assert_eq!(metrics.average_days_to_maturity_at_buyout, 25.0);
        assert!(metrics.buyout_adoption_rate > 0.01 && metrics.buyout_adoption_rate < 0.03);
    }

    // Test 30: Buyout metrics with proposed/cancelled status filtered
    #[test]
    fn test_buyout_metrics_ignore_non_completed() {
        let records = vec![
            LoanBuyoutRecord {
                loan_id: 1,
                borrower: "b1".to_string(),
                buyer: "buyer1".to_string(),
                buyout_amount: 5_000_000_000,
                remaining_principal: 4_500_000_000,
                remaining_yield: 500_000_000,
                buyout_timestamp: 1000,
                days_to_maturity: 30,
                interest_saved: 100_000_000,
                buyout_status: BuyoutStatus::Completed,
            },
            LoanBuyoutRecord {
                loan_id: 2,
                borrower: "b2".to_string(),
                buyer: "buyer2".to_string(),
                buyout_amount: 3_000_000_000,
                remaining_principal: 2_700_000_000,
                remaining_yield: 300_000_000,
                buyout_timestamp: 2000,
                days_to_maturity: 20,
                interest_saved: 60_000_000,
                buyout_status: BuyoutStatus::Proposed,
            },
            LoanBuyoutRecord {
                loan_id: 3,
                borrower: "b3".to_string(),
                buyer: "buyer3".to_string(),
                buyout_amount: 2_000_000_000,
                remaining_principal: 1_800_000_000,
                remaining_yield: 200_000_000,
                buyout_timestamp: 3000,
                days_to_maturity: 15,
                interest_saved: 40_000_000,
                buyout_status: BuyoutStatus::Cancelled,
            },
        ];

        let metrics = BuyoutMetrics::from_buyout_records(&records, 100);

        // Only loan 1 is completed
        assert_eq!(metrics.total_buyouts_completed, 1);
        assert_eq!(metrics.total_buyout_value, 5_000_000_000);
        assert_eq!(metrics.total_interest_saved, 100_000_000);
    }

    // Test 31: Buyout metrics with empty records
    #[test]
    fn test_buyout_metrics_empty() {
        let records: Vec<LoanBuyoutRecord> = vec![];
        let metrics = BuyoutMetrics::from_buyout_records(&records, 100);

        assert_eq!(metrics.total_buyouts_completed, 0);
        assert_eq!(metrics.total_buyout_value, 0);
        assert_eq!(metrics.total_interest_saved, 0);
        assert_eq!(metrics.average_buyout_amount, 0);
        assert_eq!(metrics.buyout_adoption_rate, 0.0);
    }

    // Test 32: Buyout record tracks interest savings correctly
    #[test]
    fn test_buyout_record_interest_savings() {
        let record = LoanBuyoutRecord {
            loan_id: 1,
            borrower: "borrower1".to_string(),
            buyer: "buyer1".to_string(),
            buyout_amount: 5_000_000_000,
            remaining_principal: 4_500_000_000,
            remaining_yield: 500_000_000,
            buyout_timestamp: 1000,
            days_to_maturity: 60,
            interest_saved: 250_000_000,
            buyout_status: BuyoutStatus::Completed,
        };

        // Interest saved should be positive when buyout occurs before maturity
        assert!(record.interest_saved > 0);
        assert_eq!(record.days_to_maturity, 60);
    }

    // Test 33: Risk category classification from risk score
    #[test]
    fn test_risk_category_from_score() {
        assert_eq!(RiskCategory::from_risk_score(20), RiskCategory::Low);
        assert_eq!(RiskCategory::from_risk_score(45), RiskCategory::Medium);
        assert_eq!(RiskCategory::from_risk_score(70), RiskCategory::High);
        assert_eq!(RiskCategory::from_risk_score(95), RiskCategory::Critical);
    }

    // Test 34: Risk multiplier values correct
    #[test]
    fn test_risk_multiplier_values() {
        assert_eq!(RiskCategory::Low.multiplier(), 1.0);
        assert_eq!(RiskCategory::Medium.multiplier(), 1.2);
        assert_eq!(RiskCategory::High.multiplier(), 1.5);
        assert_eq!(RiskCategory::Critical.multiplier(), 2.0);
    }

    // Test 35: High-risk yield metrics calculation
    #[test]
    fn test_high_risk_yield_metrics() {
        let records = vec![
            RiskYieldRecord {
                loan_id: 1,
                borrower: "b1".to_string(),
                risk_score: 25,
                base_yield_bps: 200,
                risk_multiplier: 1.0,
                adjusted_yield_bps: 200,
                yield_compensation: 0,
                risk_category: RiskCategory::Low,
                created_at: 1000,
            },
            RiskYieldRecord {
                loan_id: 2,
                borrower: "b2".to_string(),
                risk_score: 50,
                base_yield_bps: 200,
                risk_multiplier: 1.2,
                adjusted_yield_bps: 240,
                yield_compensation: 400_000_000,
                risk_category: RiskCategory::Medium,
                created_at: 2000,
            },
            RiskYieldRecord {
                loan_id: 3,
                borrower: "b3".to_string(),
                risk_score: 75,
                base_yield_bps: 200,
                risk_multiplier: 1.5,
                adjusted_yield_bps: 300,
                yield_compensation: 1_000_000_000,
                risk_category: RiskCategory::High,
                created_at: 3000,
            },
            RiskYieldRecord {
                loan_id: 4,
                borrower: "b4".to_string(),
                risk_score: 90,
                base_yield_bps: 200,
                risk_multiplier: 2.0,
                adjusted_yield_bps: 400,
                yield_compensation: 2_000_000_000,
                risk_category: RiskCategory::Critical,
                created_at: 4000,
            },
        ];

        let metrics = HighRiskYieldMetrics::from_risk_yield_records(&records);

        assert_eq!(metrics.total_high_risk_loans, 2); // High + Critical
        assert_eq!(metrics.total_critical_risk_loans, 1); // Critical only
        assert_eq!(metrics.total_risk_compensation, 3_400_000_000);
        assert!(metrics.average_risk_score > 50.0 && metrics.average_risk_score < 60.0);
        assert!(metrics.average_yield_multiplier > 1.4 && metrics.average_yield_multiplier < 1.5);
        assert_eq!(metrics.risk_distribution.len(), 4);
    }

    // Test 36: Risk metrics with only low-risk loans
    #[test]
    fn test_risk_metrics_low_risk_only() {
        let records = vec![
            RiskYieldRecord {
                loan_id: 1,
                borrower: "b1".to_string(),
                risk_score: 20,
                base_yield_bps: 200,
                risk_multiplier: 1.0,
                adjusted_yield_bps: 200,
                yield_compensation: 0,
                risk_category: RiskCategory::Low,
                created_at: 1000,
            },
            RiskYieldRecord {
                loan_id: 2,
                borrower: "b2".to_string(),
                risk_score: 25,
                base_yield_bps: 200,
                risk_multiplier: 1.0,
                adjusted_yield_bps: 200,
                yield_compensation: 0,
                risk_category: RiskCategory::Low,
                created_at: 2000,
            },
        ];

        let metrics = HighRiskYieldMetrics::from_risk_yield_records(&records);

        assert_eq!(metrics.total_high_risk_loans, 0);
        assert_eq!(metrics.total_critical_risk_loans, 0);
        assert_eq!(metrics.average_yield_multiplier, 1.0);
        assert_eq!(metrics.total_risk_compensation, 0);
    }

    // Test 37: Risk metrics with empty records
    #[test]
    fn test_risk_metrics_empty() {
        let records: Vec<RiskYieldRecord> = vec![];
        let metrics = HighRiskYieldMetrics::from_risk_yield_records(&records);

        assert_eq!(metrics.total_high_risk_loans, 0);
        assert_eq!(metrics.total_critical_risk_loans, 0);
        assert_eq!(metrics.average_yield_multiplier, 1.0);
    }

    // Test 38: Yield increase from risk properly calculated
    #[test]
    fn test_yield_increase_from_risk() {
        let records = vec![
            RiskYieldRecord {
                loan_id: 1,
                borrower: "b1".to_string(),
                risk_score: 70,
                base_yield_bps: 100,
                risk_multiplier: 1.5,
                adjusted_yield_bps: 150,
                yield_compensation: 500_000_000,
                risk_category: RiskCategory::High,
                created_at: 1000,
            },
        ];

        let metrics = HighRiskYieldMetrics::from_risk_yield_records(&records);

        // Adjusted (150) - base (100) = 50 bps increase
        assert!(metrics.yield_increase_from_risk >= 0);
    }
}
