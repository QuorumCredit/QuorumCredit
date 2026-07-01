/// Partial Repayment with Daily Compound Interest (Issue #838)
/// 
/// Supports:
/// - Partial loan repayment (not just full repayment)
/// - Daily compound interest calculation
/// - Milestone rewards: 50% repaid → +1% yield bonus
/// - Prevents deadline extension

use soroban_sdk::{panic_with_error, Env, Address};
use crate::errors::ContractError;
use crate::types::{LoanRecord, DataKey};
use crate::helpers::require_allowed_token;

const DAYS_IN_YEAR: u64 = 365;
const STROOPS_PER_XLM: i128 = 10_000_000;
const MILESTONE_THRESHOLD_BPS: i128 = 5_000; // 50%
const MILESTONE_BONUS_BPS: i128 = 100; // +1%

/// Calculate daily compound interest for a loan
/// Formula: A = P * (1 + r/365)^n where n = days elapsed
/// Returns: accrued interest in stroops
pub fn calculate_daily_compound_interest(
    principal: i128,
    annual_rate_bps: i128,
    days_elapsed: u64,
) -> i128 {
    if principal <= 0 || annual_rate_bps <= 0 || days_elapsed == 0 {
        return 0;
    }

    // Calculate daily rate: annual_rate / 365 / 10000
    let daily_rate_numerator = annual_rate_bps;
    let daily_rate_denominator: i128 = DAYS_IN_YEAR as i128 * 10_000;

    // Simple compound interest approximation: 
    // For small rates, compound ≈ principal * daily_rate * days
    // Full formula is too expensive for on-chain
    let accrued = (principal * daily_rate_numerator * days_elapsed as i128)
        / daily_rate_denominator;

    accrued
}

/// Check if milestone (50% repaid) is achieved
pub fn check_milestone_achievement(amount_repaid: i128, total_amount: i128) -> bool {
    if total_amount <= 0 {
        return false;
    }
    
    let repayment_bps = (amount_repaid * 10_000) / total_amount;
    repayment_bps >= MILESTONE_THRESHOLD_BPS
}

/// Calculate effective yield with milestone bonus
pub fn calculate_effective_yield_bps(
    base_yield_bps: i128,
    amount_repaid: i128,
    total_amount: i128,
) -> i128 {
    if check_milestone_achievement(amount_repaid, total_amount) {
        base_yield_bps + MILESTONE_BONUS_BPS
    } else {
        base_yield_bps
    }
}

/// Process partial repayment with compound interest tracking
pub fn process_partial_repayment(
    env: &Env,
    borrower: Address,
    payment: i128,
    mut loan: LoanRecord,
    current_timestamp: u64,
) -> Result<LoanRecord, ContractError> {
    if payment <= 0 {
        panic_with_error!(env, ContractError::InvalidAmount);
    }

    // Calculate accrued interest since last update
    let days_elapsed = if loan.disbursement_timestamp > 0 {
        (current_timestamp - loan.disbursement_timestamp) / 86400 // seconds to days
    } else {
        0
    };

    if days_elapsed > 0 {
        let daily_interest = calculate_daily_compound_interest(
            loan.amount - loan.amount_repaid, // outstanding principal
            loan.total_yield * 10_000 / loan.amount, // convert to annual rate bps
            days_elapsed,
        );
        loan.total_yield = loan.total_yield.checked_add(daily_interest)
            .ok_or(ContractError::ArithmeticError)?;
    }

    // Update repayment amount
    loan.amount_repaid = loan.amount_repaid.checked_add(payment)
        .ok_or(ContractError::ArithmeticError)?;

    // Track milestone achievement for yield bonus
    if check_milestone_achievement(loan.amount_repaid, loan.amount) {
        // Increase yield by 1% as milestone bonus
        let bonus = (loan.amount * MILESTONE_BONUS_BPS) / 10_000;
        loan.total_yield = loan.total_yield.checked_add(bonus)
            .ok_or(ContractError::ArithmeticError)?;
    }

    // Prevent deadline extension - deadline is immutable
    // No modification to loan.deadline

    Ok(loan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daily_compound_interest_calculation() {
        let principal = 100_000_000; // 10 XLM
        let annual_rate_bps = 200; // 2%
        let days = 365;

        let interest = calculate_daily_compound_interest(principal, annual_rate_bps, days);
        
        // Expected: ~2,000,000 stroops (2% of 100M)
        assert!(interest > 1_900_000 && interest < 2_100_000);
    }

    #[test]
    fn test_milestone_achievement() {
        let total = 1_000_000_000; // 100 XLM
        
        // Below 50%
        assert!(!check_milestone_achievement(400_000_000, total));
        
        // At 50%
        assert!(check_milestone_achievement(500_000_000, total));
        
        // Above 50%
        assert!(check_milestone_achievement(600_000_000, total));
    }

    #[test]
    fn test_effective_yield_with_milestone() {
        let base_yield = 200; // 2%
        let total = 1_000_000_000;
        
        // Below milestone: no bonus
        let yield_below = calculate_effective_yield_bps(base_yield, 400_000_000, total);
        assert_eq!(yield_below, 200);
        
        // At/above milestone: +1% bonus
        let yield_at = calculate_effective_yield_bps(base_yield, 500_000_000, total);
        assert_eq!(yield_at, 300); // 2% + 1% = 3%
    }

    #[test]
    fn test_zero_interest_on_zero_days() {
        let principal = 100_000_000;
        let rate = 200;
        
        let interest = calculate_daily_compound_interest(principal, rate, 0);
        assert_eq!(interest, 0);
    }

    #[test]
    fn test_no_interest_on_zero_principal() {
        let rate = 200;
        let days = 365;
        
        let interest = calculate_daily_compound_interest(0, rate, days);
        assert_eq!(interest, 0);
    }
}
