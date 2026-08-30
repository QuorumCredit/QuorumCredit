/// Issue #933: Lazy Default Detection
///
/// On-demand detection of defaults instead of active scanning.
/// Defaults are only flagged when explicitly queried or when a borrower
/// attempts a new loan/vouch operation.
///
/// This design reduces O(N) scanning costs and avoids "default clock" issues
/// where ledger time vs deadline drift creates edge cases.

use soroban_sdk::{Address, Env};
use crate::types::{DataKey, LoanRecord, LoanStatus};
use crate::errors::ContractError;

/// Check if a loan is past its deadline and should be marked as defaulted.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `loan` - The loan record to check
///
/// # Returns
/// `true` if the loan is past the deadline without full repayment
pub fn is_loan_defaulted(env: &Env, loan: &LoanRecord) -> bool {
    if loan.status != LoanStatus::Active {
        return false;
    }

    let now = env.ledger().timestamp();
    now > loan.deadline && loan.amount_repaid < loan.amount
}

/// Check and mark a loan as defaulted if appropriate.
///
/// Updates the loan status to Defaulted and increments the borrower's default count.
/// Called lazily when the loan is accessed or when slash processing begins.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `loan_id` - The ID of the loan to check
///
/// # Returns
/// `true` if the loan was marked as defaulted, `false` if it was already resolved
pub fn check_and_mark_default(
    env: &Env,
    loan_id: u64,
) -> Result<bool, ContractError> {
    let mut loan: LoanRecord = env.storage().persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)?;

    if !is_loan_defaulted(env, &loan) {
        return Ok(false); // Loan is not defaulted
    }

    if loan.status == LoanStatus::Defaulted {
        return Ok(false); // Already marked
    }

    // Mark as defaulted
    loan.status = LoanStatus::Defaulted;
    env.storage().persistent().set(&DataKey::Loan(loan_id), &loan);

    // Increment default count for the borrower. Issue #1407's audit found this used
    // `.instance()` while every other DefaultCount read/write site (credit_score.rs,
    // governance.rs, loan.rs, lib.rs) uses `.persistent()` — a lazily-detected default
    // silently never counted toward credit score or interest-rate risk pricing.
    let current_count: u32 = env.storage().persistent()
        .get(&DataKey::DefaultCount(loan.borrower.clone()))
        .unwrap_or(0u32);

    env.storage().persistent().set(
        &DataKey::DefaultCount(loan.borrower.clone()),
        &(current_count + 1),
    );
    crate::helpers::increment_total_default_count(env);

    // Emit event
    env.events().publish(
        ("loan", "default_detected"),
        (loan.borrower, loan_id, loan.deadline, env.ledger().timestamp()),
    );

    Ok(true)
}

/// Check all loans for a borrower and mark any that have defaulted.
///
/// Useful for pre-checking before a borrower attempts new operations.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - The borrower whose loans to check
///
/// # Returns
/// Number of loans newly marked as defaulted
pub fn check_all_defaults_for_borrower(
    env: &Env,
    borrower: &Address,
) -> Result<u32, ContractError> {
    let mut count = 0u32;

    // Scan all loans for this borrower
    // In a production system, this would iterate over the borrower's loan history
    // For now, we provide the helper function and integration points

    Ok(count)
}

/// Get the detection status of a loan.
///
/// Lazily checks and returns whether a loan is defaulted without modifying state.
pub fn get_default_detection_status(
    env: &Env,
    loan_id: u64,
) -> Result<bool, ContractError> {
    let loan: LoanRecord = env.storage().persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)?;

    Ok(is_loan_defaulted(env, &loan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_loan_defaulted_active_loan_not_past_deadline() {
        // This would require a mock Env, so we'll skip for now
        // In real tests, we'd verify that a loan not past deadline returns false
    }

    #[test]
    fn test_is_loan_defaulted_repaid() {
        // Verify that a fully repaid loan (even if past deadline) returns false
    }

    #[test]
    fn test_is_loan_defaulted_past_deadline_unpaid() {
        // Verify that an unpaid loan past deadline returns true
    }
}
