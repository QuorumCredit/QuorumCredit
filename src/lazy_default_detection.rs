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

    // Increment default count for the borrower
    let current_count: u32 = env.storage().instance()
        .get(&DataKey::DefaultCount(loan.borrower.clone()))
        .unwrap_or(0u32);

    env.storage().instance().set(
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
    use crate::types::{EscrowStatus, RateType};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{String, Vec};

    /// Build a minimal `LoanRecord` for `is_loan_defaulted` checks. Only
    /// `status`, `amount`, `amount_repaid` and `deadline` are meaningful here.
    fn mk_loan(
        env: &Env,
        status: LoanStatus,
        amount: i128,
        amount_repaid: i128,
        deadline: u64,
    ) -> LoanRecord {
        let dummy = Address::generate(env);
        LoanRecord {
            id: 1,
            borrower: dummy.clone(),
            guarantor: None,
            buyback_price: 0,
            auto_repay_enabled: false,
            auto_repay_attempts: 0,
            escrow_status: EscrowStatus::None,
            co_borrowers: Vec::new(env),
            amount,
            amount_repaid,
            total_yield: 0,
            status,
            repaid: false,
            defaulted: false,
            created_at: 0,
            disbursement_timestamp: 0,
            repayment_timestamp: None,
            deadline,
            loan_purpose: String::from_str(env, "test"),
            token_address: dummy,
            amortization_schedule: Vec::new(env),
            reminder_sent: false,
            risk_score: 0,
            deferment_periods: 0,
            maturity_date: None,
            rate_type: RateType::Fixed,
            index_reference: None,
            last_interest_calc: 0,
            accrued_interest: 0,
            milestone_bonus_applied: 0,
            retry_count: 0,
            suspension_timestamp: None,
            suspension_amount_repaid: 0,
        }
    }

    #[test]
    fn test_is_loan_defaulted_active_loan_not_past_deadline() {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        // Active loan, deadline still in the future, nothing repaid.
        let loan = mk_loan(&env, LoanStatus::Active, 1_000_000, 0, 5_000);
        assert!(!is_loan_defaulted(&env, &loan));
    }

    #[test]
    fn test_is_loan_defaulted_repaid() {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 10_000);
        // Deadline is in the past, but the loan is fully repaid.
        let loan = mk_loan(&env, LoanStatus::Active, 1_000_000, 1_000_000, 5_000);
        assert!(!is_loan_defaulted(&env, &loan));
    }

    #[test]
    fn test_is_loan_defaulted_past_deadline_unpaid() {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 10_000);
        // Deadline passed and the borrower still owes money.
        let loan = mk_loan(&env, LoanStatus::Active, 1_000_000, 400_000, 5_000);
        assert!(is_loan_defaulted(&env, &loan));
    }

    #[test]
    fn test_is_loan_defaulted_already_defaulted_returns_false() {
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 10_000);
        // Past deadline and unpaid, but the status is no longer Active: the
        // lazy check only transitions loans out of the Active state.
        let loan = mk_loan(&env, LoanStatus::Defaulted, 1_000_000, 0, 5_000);
        assert!(!is_loan_defaulted(&env, &loan));
    }
}
