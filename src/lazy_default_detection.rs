/// Issue #933: Lazy Default Detection
///
/// On-demand detection of defaults instead of active scanning.
/// Defaults are only flagged when explicitly queried or when a borrower
/// attempts a new loan/vouch operation.
///
/// This design reduces O(N) scanning costs and avoids "default clock" issues
/// where ledger time vs deadline drift creates edge cases.

use soroban_sdk::{Address, Env, Vec};
use crate::types::{DataKey, LoanRecord, LoanStatus};
use crate::errors::ContractError;

/// Issue #1429: Record a loan id in the borrower's loan-history index.
///
/// Appends `loan_id` to `DataKey::BorrowerLoanIds(borrower)` (creating the vector
/// on first use, and skipping duplicates). Call this from `request_loan` right
/// after a loan id is assigned so `check_all_defaults_for_borrower` has a
/// complete history to walk.
pub fn record_borrower_loan_id(env: &Env, borrower: &Address, loan_id: u64) {
    let key = DataKey::BorrowerLoanIds(borrower.clone());
    let mut ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    let mut already_present = false;
    for existing in ids.iter() {
        if existing == loan_id {
            already_present = true;
            break;
        }
    }

    if !already_present {
        ids.push_back(loan_id);
        env.storage().persistent().set(&key, &ids);
    }
}

/// Issue #1429: Fetch the borrower's recorded loan ids (empty if none).
pub fn borrower_loan_ids(env: &Env, borrower: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::BorrowerLoanIds(borrower.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

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

    // Issue #1429: walk the borrower's recorded loan history and lazily mark any
    // loan that has passed its deadline unpaid. Loans that are missing, already
    // repaid, or already flagged are skipped by `check_and_mark_default`.
    let loan_ids = borrower_loan_ids(env, borrower);
    for loan_id in loan_ids.iter() {
        match check_and_mark_default(env, loan_id) {
            Ok(true) => count += 1,
            Ok(false) => {}
            // A stale/missing id in the index must not abort the pre-check.
            Err(ContractError::NoActiveLoan) => {}
            Err(e) => return Err(e),
        }
    }

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
    use crate::QuorumCreditContract;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{String, Vec as SVec};

    fn mk_loan(env: &Env, id: u64, borrower: &Address, amount: i128, repaid: i128, deadline: u64, status: LoanStatus) -> LoanRecord {
        LoanRecord {
            id,
            borrower: borrower.clone(),
            guarantor: None,
            buyback_price: 0,
            auto_repay_enabled: false,
            auto_repay_attempts: 0,
            escrow_status: EscrowStatus::None,
            co_borrowers: SVec::new(env),
            amount,
            amount_repaid: repaid,
            total_yield: 0,
            status,
            repaid: repaid >= amount,
            defaulted: false,
            created_at: 0,
            disbursement_timestamp: 0,
            repayment_timestamp: None,
            deadline,
            loan_purpose: String::from_str(env, "test"),
            token_address: Address::generate(env),
            amortization_schedule: SVec::new(env),
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

    /// Issue #1429: a borrower with active, repaid, and defaulted loans in their
    /// history has exactly the one past-due unpaid loan newly flagged.
    #[test]
    fn check_all_defaults_iterates_mixed_state_history() {
        let env = Env::default();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        env.ledger().set_timestamp(10_000);

        env.as_contract(&contract_id, || {
            let borrower = Address::generate(&env);

            // id 1: active, not past deadline -> stays active
            let l1 = mk_loan(&env, 1, &borrower, 1_000, 0, 20_000, LoanStatus::Active);
            // id 2: fully repaid, past deadline -> not a default
            let l2 = mk_loan(&env, 2, &borrower, 1_000, 1_000, 5_000, LoanStatus::Repaid);
            // id 3: active, past deadline, unpaid -> should be marked Defaulted
            let l3 = mk_loan(&env, 3, &borrower, 1_000, 200, 5_000, LoanStatus::Active);

            for l in [&l1, &l2, &l3] {
                env.storage().persistent().set(&DataKey::Loan(l.id), l);
                record_borrower_loan_id(&env, &borrower, l.id);
            }
            // include a stale id that no longer resolves to a loan
            record_borrower_loan_id(&env, &borrower, 999);

            let newly = check_all_defaults_for_borrower(&env, &borrower).unwrap();
            assert_eq!(newly, 1, "only loan 3 should be newly defaulted");

            let after: LoanRecord = env.storage().persistent().get(&DataKey::Loan(3)).unwrap();
            assert_eq!(after.status, LoanStatus::Defaulted);
            let untouched: LoanRecord = env.storage().persistent().get(&DataKey::Loan(1)).unwrap();
            assert_eq!(untouched.status, LoanStatus::Active);

            let count: u32 = env.storage().instance().get(&DataKey::DefaultCount(borrower.clone())).unwrap_or(0);
            assert_eq!(count, 1);

            // idempotent: a second sweep finds nothing new
            let again = check_all_defaults_for_borrower(&env, &borrower).unwrap();
            assert_eq!(again, 0);
        });
    }

    /// Issue #1429: an unknown borrower with no recorded history returns Ok(0).
    #[test]
    fn check_all_defaults_empty_history_is_zero() {
        let env = Env::default();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        env.as_contract(&contract_id, || {
            let borrower = Address::generate(&env);
            assert_eq!(check_all_defaults_for_borrower(&env, &borrower).unwrap(), 0);
        });
    }

    /// Issue #1429: the loan-id index dedups repeat records.
    #[test]
    fn record_borrower_loan_id_dedups() {
        let env = Env::default();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        env.as_contract(&contract_id, || {
            let borrower = Address::generate(&env);
            record_borrower_loan_id(&env, &borrower, 7);
            record_borrower_loan_id(&env, &borrower, 7);
            record_borrower_loan_id(&env, &borrower, 8);
            assert_eq!(borrower_loan_ids(&env, &borrower).len(), 2);
        });
    }
}
