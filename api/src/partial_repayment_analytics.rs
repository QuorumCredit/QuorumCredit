use serde::{Deserialize, Serialize};

/// Partial repayment analytics tracking (Issue #838)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialRepaymentRecord {
    pub borrower: String,
    pub timestamp: i64,
    pub amount_paid: i128,
    pub total_amount: i128,
    pub outstanding_balance: i128,
    pub repayment_percentage: f64,
    pub milestone_achieved: bool,
    pub accrued_interest: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialRepaymentMetrics {
    pub total_partial_repayments: u32,
    pub borrowers_with_partial_repayments: u32,
    pub total_repaid_via_partial: i128,
    pub average_repayment_size: i128,
    pub milestone_achievements: u32,
    pub total_accrued_interest: i128,
}

/// Calculate daily compound interest
pub fn calculate_daily_compound_interest(
    principal: i128,
    annual_rate_bps: i128,
    days_elapsed: u64,
) -> i128 {
    if principal <= 0 || annual_rate_bps <= 0 || days_elapsed == 0 {
        return 0;
    }

    let daily_rate_numerator = annual_rate_bps;
    let daily_rate_denominator: i128 = 365 * 10_000;

    (principal * daily_rate_numerator * days_elapsed as i128) / daily_rate_denominator
}

/// Check if 50% milestone is achieved
pub fn check_milestone_50_percent(amount_repaid: i128, total_amount: i128) -> bool {
    if total_amount <= 0 {
        return false;
    }
    
    let repayment_bps = (amount_repaid * 10_000) / total_amount;
    repayment_bps >= 5_000 // 50%
}

/// Calculate milestone bonus yield
pub fn calculate_milestone_bonus_yield(
    base_yield_bps: i128,
    has_milestone: bool,
) -> i128 {
    if has_milestone {
        base_yield_bps + 100 // +1%
    } else {
        base_yield_bps
    }
}

/// Generate partial repayment report
pub fn generate_repayment_report(
    records: &[PartialRepaymentRecord],
) -> PartialRepaymentMetrics {
    let mut borrowers = std::collections::HashSet::new();
    let mut total_partial_repayments = 0u32;
    let mut total_repaid = 0i128;
    let mut total_interest = 0i128;
    let mut milestone_count = 0u32;

    for record in records {
        borrowers.insert(record.borrower.clone());
        total_partial_repayments += 1;
        total_repaid = total_repaid.saturating_add(record.amount_paid);
        total_interest = total_interest.saturating_add(record.accrued_interest);
        if record.milestone_achieved {
            milestone_count += 1;
        }
    }

    let average_repayment = if total_partial_repayments > 0 {
        total_repaid / total_partial_repayments as i128
    } else {
        0
    };

    PartialRepaymentMetrics {
        total_partial_repayments,
        borrowers_with_partial_repayments: borrowers.len() as u32,
        total_repaid_via_partial: total_repaid,
        average_repayment_size: average_repayment,
        milestone_achievements: milestone_count,
        total_accrued_interest: total_interest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daily_compound_interest_calculation() {
        let principal = 100_000_000; // 10 XLM
        let annual_rate = 200; // 2%
        let days = 365;

        let interest = calculate_daily_compound_interest(principal, annual_rate, days);
        
        // ~2% of principal
        assert!(interest > 1_900_000 && interest < 2_100_000);
    }

    #[test]
    fn test_milestone_50_percent_threshold() {
        let total = 1_000_000_000;
        
        assert!(!check_milestone_50_percent(400_000_000, total));
        assert!(check_milestone_50_percent(500_000_000, total));
        assert!(check_milestone_50_percent(600_000_000, total));
    }

    #[test]
    fn test_milestone_bonus_yield() {
        let base_yield = 200;
        
        assert_eq!(calculate_milestone_bonus_yield(base_yield, false), 200);
        assert_eq!(calculate_milestone_bonus_yield(base_yield, true), 300);
    }

    #[test]
    fn test_repayment_report_generation() {
        let records = vec![
            PartialRepaymentRecord {
                borrower: "b1".to_string(),
                timestamp: 1000,
                amount_paid: 100_000_000,
                total_amount: 200_000_000,
                outstanding_balance: 100_000_000,
                repayment_percentage: 50.0,
                milestone_achieved: true,
                accrued_interest: 1_000_000,
            },
            PartialRepaymentRecord {
                borrower: "b2".to_string(),
                timestamp: 2000,
                amount_paid: 50_000_000,
                total_amount: 200_000_000,
                outstanding_balance: 150_000_000,
                repayment_percentage: 25.0,
                milestone_achieved: false,
                accrued_interest: 500_000,
            },
        ];

        let report = generate_repayment_report(&records);
        
        assert_eq!(report.total_partial_repayments, 2);
        assert_eq!(report.borrowers_with_partial_repayments, 2);
        assert_eq!(report.total_repaid_via_partial, 150_000_000);
        assert_eq!(report.milestone_achievements, 1);
        assert_eq!(report.total_accrued_interest, 1_500_000);
    }
}
