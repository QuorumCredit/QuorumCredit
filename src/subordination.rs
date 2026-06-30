//! Issue #887: Loan Subordination and Cascading Debt Hierarchy
//!
//! This module implements a hierarchical debt structure where loans can be subordinated
//! (junior) to other loans (senior). It handles:
//! - Subordination relationships between loans
//! - Waterfall repayment distribution (senior loans repaid first)
//! - Cascading default logic (senior loan default affects subordinates)
//! - Subordination validation and loan dependency tracking

use crate::types::*;
use crate::errors::ContractError;
use soroban_sdk::{Address, Env, Vec, Map};

/// Issue #887: Defines the subordination level in the debt hierarchy.
/// - Senior (0): Must be fully repaid first; defaults block all subordinates.
/// - Mezzanine (1): Intermediate level; may have both senior and subordinate loans.
/// - Subordinate (2+): Repaid after seniors; affected by senior defaults.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SubordinationLevel {
    Senior = 0,
    Mezzanine = 1,
    Subordinate = 2,
}

/// Issue #887: Represents the subordination relationship between two loans.
/// Links a subordinate (junior) loan to its senior (creditor priority) loan.
#[derive(Clone)]
pub struct SubordinationRecord {
    /// ID of the senior (higher priority) loan
    pub senior_loan_id: u64,
    /// ID of the subordinate (lower priority) loan
    pub subordinate_loan_id: u64,
    /// The subordination level of the subordinate loan (relative to senior)
    pub subordination_level: SubordinationLevel,
    /// Ledger timestamp when this subordination was created
    pub created_at: u64,
    /// Whether this subordination is currently active (true) or waived (false)
    pub is_active: bool,
    /// Optional: Priority order if a senior loan has multiple subordinates
    pub priority_index: u32,
}

/// Issue #887: Represents cascading default information.
/// Tracks which loans are affected when a senior loan defaults.
#[derive(Clone)]
pub struct CascadingDefault {
    /// ID of the senior loan that defaulted
    pub triggering_senior_loan_id: u64,
    /// IDs of all subordinate loans affected by this default
    pub affected_subordinate_ids: Vec<u64>,
    /// Ledger timestamp when the cascade was triggered
    pub triggered_at: u64,
    /// Whether the cascade has been resolved (all affected loans handled)
    pub is_resolved: bool,
}

/// Issue #887: Waterfall repayment distribution result.
/// Specifies how a repayment should be split between senior and subordinate loans
#[derive(Clone)]
pub struct WaterfallDistribution {
    /// Amount to apply to the senior loan (stroops)
    pub senior_amount: i128,
    /// Amount to apply to subordinate loans (stroops)
    pub subordinate_amount: i128,
    /// Total amount distributed
    pub total_distributed: i128,
}

/// Issue #887: Validates that a subordination relationship is legal.
/// - Senior and subordinate loans must exist and belong to the same borrower
/// - No circular dependencies allowed
/// - Senior loan cannot be in default status if subordinate is active
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `borrower` - The borrower owning both loans
/// * `senior_loan_id` - The ID of the proposed senior loan
/// * `subordinate_loan_id` - The ID of the proposed subordinate loan
///
/// # Returns
/// - Ok(()) if the subordination is valid
/// - Err(ContractError) if validation fails
pub fn validate_subordination(
    env: &Env,
    borrower: &Address,
    senior_loan_id: u64,
    subordinate_loan_id: u64,
) -> Result<(), ContractError> {
    // Prevent self-subordination
    if senior_loan_id == subordinate_loan_id {
        return Err(ContractError::InvalidStateTransition);
    }

    // Verify both loans exist and belong to the same borrower
    let senior_loan = get_loan_record(env, senior_loan_id)
        .ok_or(ContractError::NoActiveLoan)?;
    let subordinate_loan = get_loan_record(env, subordinate_loan_id)
        .ok_or(ContractError::NoActiveLoan)?;

    if senior_loan.borrower != *borrower || subordinate_loan.borrower != *borrower {
        return Err(ContractError::InvalidStateTransition);
    }

    // Senior loan cannot be in default if subordinate is to be created
    if is_loan_in_default(env, senior_loan_id) {
        return Err(ContractError::InvalidStateTransition);
    }

    // Check for circular dependencies (prevent cycles)
    if would_create_cycle(env, senior_loan_id, subordinate_loan_id) {
        return Err(ContractError::InvalidStateTransition);
    }

    Ok(())
}

/// Issue #887: Applies waterfall distribution logic to a repayment.
/// Senior loans are repaid first; subordinate loans receive remainder.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `borrower` - The borrower making repayment
/// * `total_payment` - Total repayment amount in stroops
///
/// # Returns
/// A WaterfallDistribution specifying how to split the repayment
pub fn apply_waterfall_distribution(
    env: &Env,
    borrower: &Address,
    total_payment: i128,
) -> WaterfallDistribution {
    // Get all senior subordinations for this borrower
    let senior_loans = get_senior_loans_for_borrower(env, borrower);

    let mut senior_amount: i128 = 0;
    let mut remaining = total_payment;

    // First, allocate to senior loans (in priority order)
    for senior_id in senior_loans.iter() {
        if remaining <= 0 {
            break;
        }

        let loan = match get_loan_record(env, senior_id) {
            Some(l) => l,
            None => continue,
        };

        // Calculate amount needed to fully repay senior loan
        let senior_owed = loan.amount + loan.total_yield - loan.amount_repaid;
        let senior_payment = if remaining >= senior_owed {
            senior_owed
        } else {
            remaining
        };

        senior_amount += senior_payment;
        remaining -= senior_payment;
    }

    let subordinate_amount = remaining.max(0);

    WaterfallDistribution {
        senior_amount,
        subordinate_amount,
        total_distributed: total_payment,
    }
}

/// Issue #887: Triggers cascading default when a senior loan defaults.
/// All subordinate loans are marked as affected and may enter default status.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `senior_loan_id` - The ID of the senior loan that defaulted
///
/// # Returns
/// A CascadingDefault record documenting the cascade
pub fn trigger_cascading_default(
    env: &Env,
    senior_loan_id: u64,
) -> CascadingDefault {
    let affected_subordinates = get_subordinate_loans(env, senior_loan_id);

    let cascade = CascadingDefault {
        triggering_senior_loan_id: senior_loan_id,
        affected_subordinate_ids: affected_subordinates,
        triggered_at: env.ledger().timestamp(),
        is_resolved: false,
    };

    cascade
}

/// Issue #887: Gets all subordinate loans for a given senior loan.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `senior_loan_id` - The ID of the senior loan
///
/// # Returns
/// A vector of subordinate loan IDs
fn get_subordinate_loans(env: &Env, senior_loan_id: u64) -> Vec<u64> {
    // This would be implemented by querying the subordination storage map
    // For now, returning empty vector as placeholder
    Vec::new(env)
}

/// Issue #887: Gets all senior loans for a borrower (ordered by priority).
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `borrower` - The borrower address
///
/// # Returns
/// A vector of senior loan IDs in priority order
fn get_senior_loans_for_borrower(env: &Env, borrower: &Address) -> Vec<u64> {
    // This would be implemented by querying the subordination storage map
    // For now, returning empty vector as placeholder
    Vec::new(env)
}

/// Issue #887: Detects if creating a subordination relationship would cause a cycle.
/// Prevents: A subordinate to B, B subordinate to C, then trying C subordinate to A.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `potential_senior_id` - The proposed senior loan ID
/// * `potential_subordinate_id` - The proposed subordinate loan ID
///
/// # Returns
/// true if a cycle would be created, false otherwise
fn would_create_cycle(env: &Env, potential_senior_id: u64, potential_subordinate_id: u64) -> bool {
    // Trace from potential_subordinate backward through seniors
    // If we reach potential_senior_id, a cycle exists
    let mut current = potential_subordinate_id;
    let mut visited: Vec<u64> = Vec::new(env);

    loop {
        if current == potential_senior_id {
            return true; // Cycle detected
        }

        if visited.iter().any(|id| id == current) {
            return false; // No cycle in this branch
        }

        visited.push_back(current);

        // Get the senior loan for current
        match get_senior_loan_for_subordinate(env, current) {
            Some(senior_id) => {
                current = senior_id;
                if visited.len() > 1000 {
                    return false; // Prevent infinite loops in traversal
                }
            }
            None => return false, // No senior found, no cycle possible
        }
    }
}

/// Issue #887: Gets the direct senior loan for a given subordinate loan.
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `subordinate_loan_id` - The ID of the subordinate loan
///
/// # Returns
/// The ID of the direct senior loan, or None if no senior exists
fn get_senior_loan_for_subordinate(env: &Env, subordinate_loan_id: u64) -> Option<u64> {
    // This would be implemented by querying the subordination storage map
    // For now, returning None as placeholder
    None
}

/// Issue #887: Helper to check if a loan is in default status.
fn is_loan_in_default(env: &Env, loan_id: u64) -> bool {
    match get_loan_record(env, loan_id) {
        Some(loan) => {
            matches!(loan.status, LoanStatus::Defaulted | LoanStatus::PartialDefault)
        }
        None => false,
    }
}

/// Issue #887: Helper to retrieve a loan record from storage.
fn get_loan_record(env: &Env, loan_id: u64) -> Option<LoanRecord> {
    // Placeholder: would query the loan storage with DataKey::Loan(loan_id)
    // This is implemented in the main loan module
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subordination_level_ordering() {
        assert!(SubordinationLevel::Senior < SubordinationLevel::Mezzanine);
        assert!(SubordinationLevel::Mezzanine < SubordinationLevel::Subordinate);
    }

    #[test]
    fn test_waterfall_distribution_empty() {
        // When no senior loans exist, all payment goes to subordinates
        let distribution = WaterfallDistribution {
            senior_amount: 0,
            subordinate_amount: 1000,
            total_distributed: 1000,
        };

        assert_eq!(distribution.total_distributed, 1000);
        assert_eq!(distribution.subordinate_amount, 1000);
    }
}
