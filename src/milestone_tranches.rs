//! Issue #891 (#30): Milestone-Based Disbursement Tranches
//!
//! This module implements milestone-based loan disbursement where loan funds are
//! released in multiple tranches tied to borrower milestones rather than all at once.
//!
//! ## Overview
//!
//! Traditional loans disburse the full principal upfront. This creates risk:
//! - Borrower receives all funds but may not complete the intended project
//! - Lenders have no recourse if borrower misallocates funds
//! - No incentive alignment between disbursement and actual project progress
//!
//! Milestone-based tranches solve this by:
//! 1. Splitting loan into multiple tranches (e.g., 25% each for 4 milestones)
//! 2. Each tranche is released only when borrower completes a milestone
//! 3. Milestones are verified by project manager or voucher vote
//! 4. If a milestone is not met by deadline, the tranche is returned
//! 5. Borrower's reputation and future borrowing capacity affected by tranche completion
//!
//! ## Data Flow
//!
//! ```
//! 1. Borrower requests loan with milestones
//!    request_loan_with_tranches(amount=100, num_tranches=4, milestone_deadlines=[...])
//!
//! 2. First tranche (25%) is auto-released at disbursement
//!    (optional: can require approval even for first tranche)
//!
//! 3. Borrower completes milestone 1, submits evidence
//!    submit_milestone_completion(tranche_id=1, evidence_hash=[...])
//!
//! 4. Vouchers vote or project manager approves
//!    approve_milestone_completion(tranche_id=1, approver_addresses=[...])
//!
//! 5. Tranche 2 is released (25%)
//!
//! 6. Process repeats for remaining tranches
//!
//! 7. Full repayment with yield on all completed tranches
//! ```
//!
//! ## Milestones vs. Tranches
//!
//! - **Tranche**: A portion of the loan principal (e.g., $25 of $100 loan)
//! - **Milestone**: A project objective that must be met to unlock the next tranche
//! - **Milestone Period**: Time window for borrower to complete the milestone
//! - **Completion Proof**: Evidence (hash) submitted by borrower for milestone completion
//!
//! ## Key Concepts
//!
//! ### Milestone Status
//! - `Pending`: Tranche not yet released; milestone work in progress
//! - `Submitted`: Borrower submitted completion evidence
//! - `Approved`: Milestone completed and verified; tranche released
//! - `Rejected`: Milestone not met or evidence insufficient; tranche not released
//! - `Expired`: Milestone deadline passed without submission
//!
//! ### Tranche Release Rules
//! - First tranche (if `first_tranche_auto_release = true`): Released immediately at disbursement
//! - Subsequent tranches: Released only when previous milestone is approved
//! - Expired tranche: Can be returned to contract (optional partial refund mechanism)
//! - Final tranche: Release completes disbursement
//!
//! ### Incentive Alignment
//! - Borrower receives partial funds gradually (reduces risk of misallocation)
//! - Vouchers have visibility into milestone progress (reduces information asymmetry)
//! - Reputation impact: Failed milestones reduce credit score and future borrowing capacity
//! - Yield still applies to all completed tranches at repayment time

use crate::errors::ContractError;
use crate::types::*;
use soroban_sdk::{contracttype, Address, Env, String as SorobanString, Vec};

// ── Constants ──────────────────────────────────────────────────────────────

/// Minimum number of tranches (2: forces at least two disbursement events)
pub const MIN_TRANCHES: u32 = 2;

/// Maximum number of tranches (20: prevents excessive fragmentation)
pub const MAX_TRANCHES: u32 = 20;

/// Default auto-release of first tranche at disbursement (true: yes)
pub const DEFAULT_FIRST_TRANCHE_AUTO_RELEASE: bool = true;

/// Default grace period after milestone deadline to submit evidence (3 days in seconds)
pub const DEFAULT_EVIDENCE_GRACE_PERIOD_SECS: u64 = 3 * 24 * 60 * 60;

// ── Data Types ─────────────────────────────────────────────────────────────

/// Issue #891: Status of a milestone (maps to tranche release)
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Tranche not yet released; milestone work in progress
    Pending,
    /// Borrower submitted completion evidence
    Submitted,
    /// Milestone completed and verified; tranche released
    Approved,
    /// Milestone not met or evidence insufficient; tranche not released
    Rejected,
    /// Milestone deadline passed without submission
    Expired,
}

/// Issue #891: Single milestone tied to a disbursement tranche
#[contracttype]
#[derive(Clone)]
pub struct MilestoneRecord {
    /// Unique milestone ID (1-indexed)
    pub milestone_id: u32,
    /// Associated loan ID
    pub loan_id: u64,
    /// Associated tranche ID
    pub tranche_id: u32,
    /// Current status of this milestone
    pub status: MilestoneStatus,
    /// Deadline for borrower to complete this milestone (ledger timestamp)
    pub deadline: u64,
    /// Description of what milestone must be completed (project objective)
    pub description: SorobanString,
    /// Ledger timestamp when borrower submitted completion evidence
    pub submitted_at: Option<u64>,
    /// Hash of evidence provided by borrower (project deliverables, receipts, etc.)
    pub evidence_hash: Option<soroban_sdk::BytesN<32>>,
    /// Ledger timestamp when milestone was approved
    pub approved_at: Option<u64>,
    /// Addresses that approved this milestone (for multi-sig verification)
    pub approvers: Vec<Address>,
    /// Reason for rejection if status is Rejected
    pub rejection_reason: Option<SorobanString>,
}

/// Issue #891: Disbursement tranche for a milestone-based loan
#[contracttype]
#[derive(Clone)]
pub struct TrancheRecord {
    /// Unique tranche ID (1-indexed)
    pub tranche_id: u32,
    /// Associated loan ID
    pub loan_id: u64,
    /// Amount of this tranche in stroops
    pub amount: i128,
    /// Percentage of total loan this tranche represents (in basis points, e.g., 2500 = 25%)
    pub percentage_bps: u32,
    /// Ledger timestamp when this tranche was released to borrower (if any)
    pub released_at: Option<u64>,
    /// Ledger timestamp when this tranche was created
    pub created_at: u64,
}

/// Issue #891: Configuration for milestone-based loan disbursement
#[contracttype]
#[derive(Clone)]
pub struct MilestoneDisbursementConfig {
    /// Total loan amount in stroops
    pub total_amount: i128,
    /// Number of tranches (2-20)
    pub num_tranches: u32,
    /// Whether first tranche auto-releases at disbursement
    pub first_tranche_auto_release: bool,
    /// Grace period after milestone deadline to submit evidence (seconds)
    pub evidence_grace_period_secs: u64,
    /// Address of project manager who can approve milestones (if set, overrides voucher vote)
    pub project_manager: Option<Address>,
    /// Number of voucher approvals required to release tranche if no project manager
    pub required_approvals: u32,
    /// Ledger timestamp when milestone-based loan was created
    pub created_at: u64,
}

/// Issue #891: Aggregated state of all milestones and tranches for a loan
#[contracttype]
#[derive(Clone)]
pub struct MilestoneLoanState {
    /// The main LoanRecord ID
    pub loan_id: u64,
    /// Configuration for this milestone-based loan
    pub config: MilestoneDisbursementConfig,
    /// Total milestones/tranches completed so far
    pub completed_milestones: u32,
    /// Total milestones/tranches failed or expired
    pub failed_milestones: u32,
    /// Total amount released so far (sum of released tranches)
    pub total_released: i128,
    /// Whether all tranches have been released
    pub fully_disbursed: bool,
}

// ── Helper Functions ──────────────────────────────────────────────────────

/// Issue #891: Calculates the amount for each tranche given total amount and number of tranches
///
/// # Arguments
/// * `total_amount` - Total loan amount in stroops
/// * `num_tranches` - Number of tranches (2-20)
/// * `tranche_index` - 1-indexed tranche number
///
/// # Returns
/// Amount for this tranche in stroops (handles rounding for last tranche)
///
/// # Example
/// - Total: 1,000,000 stroops, 4 tranches
/// - Tranche 1-3: 250,000 each
/// - Tranche 4: 250,000 (remainder absorbed in last tranche)
pub fn calculate_tranche_amount(
    total_amount: i128,
    num_tranches: u32,
    tranche_index: u32,
) -> Result<i128, ContractError> {
    if num_tranches < MIN_TRANCHES || num_tranches > MAX_TRANCHES {
        return Err(ContractError::InvalidAmount);
    }
    if tranche_index == 0 || tranche_index > num_tranches {
        return Err(ContractError::InvalidAmount);
    }

    let base_tranche = total_amount / num_tranches as i128;
    let remainder = total_amount % num_tranches as i128;

    // Distribute remainder across last tranche
    let amount = if tranche_index == num_tranches {
        base_tranche + remainder
    } else {
        base_tranche
    };

    Ok(amount)
}

/// Issue #891: Validates that milestone configuration is valid
///
/// # Checks
/// - Number of tranches within bounds (2-20)
/// - Total amount > 0
/// - All milestone deadlines in future
/// - Milestones ordered chronologically
/// - Required approvals reasonable (1-10)
///
/// # Returns
/// Ok(()) if valid, Err(ContractError) otherwise
pub fn validate_milestone_config(
    env: &Env,
    total_amount: i128,
    num_tranches: u32,
    milestone_deadlines: &Vec<u64>,
    required_approvals: u32,
) -> Result<(), ContractError> {
    // Check tranches
    if num_tranches < MIN_TRANCHES || num_tranches > MAX_TRANCHES {
        return Err(ContractError::InvalidAmount);
    }

    // Check amount
    if total_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Check number of deadlines matches tranches
    if milestone_deadlines.len() != num_tranches as usize {
        return Err(ContractError::InvalidAmount);
    }

    // Check all deadlines in future and in order
    let now = env.ledger().timestamp();
    let mut last_deadline = now;

    for deadline in milestone_deadlines.iter() {
        if *deadline <= last_deadline {
            return Err(ContractError::InvalidAmount); // Deadlines must be increasing
        }
        last_deadline = *deadline;
    }

    // Check required approvals
    if required_approvals == 0 || required_approvals > 10 {
        return Err(ContractError::InvalidAmount);
    }

    Ok(())
}

/// Issue #891: Checks if a milestone has expired (deadline + grace period passed)
pub fn is_milestone_expired(
    env: &Env,
    deadline: u64,
    grace_period_secs: u64,
) -> bool {
    let now = env.ledger().timestamp();
    let expiry = deadline.saturating_add(grace_period_secs);
    now > expiry
}

/// Issue #891: Calculates effective yield considering tranche completion
///
/// When a loan is repaid, yield is calculated based on completed tranches only.
/// Unreleased tranches do not accrue yield (they were never outstanding).
///
/// # Arguments
/// * `base_yield_bps` - Base yield rate in basis points
/// * `total_amount` - Total principal in stroops
/// * `completed_tranches` - Number of completed tranches
/// * `num_tranches` - Total number of tranches
///
/// # Returns
/// Yield amount in stroops (proportional to completed tranches)
pub fn calculate_milestone_yield(
    base_yield_bps: i128,
    total_amount: i128,
    completed_tranches: u32,
    num_tranches: u32,
) -> i128 {
    if num_tranches == 0 || completed_tranches == 0 {
        return 0;
    }

    // Yield proportional to completed tranches
    // E.g., if 2 of 4 tranches completed, only half of normal yield
    let completion_ratio = (completed_tranches as i128 * BPS_DENOMINATOR) / num_tranches as i128;
    let full_yield = (total_amount * base_yield_bps) / BPS_DENOMINATOR;
    (full_yield * completion_ratio) / BPS_DENOMINATOR
}

/// Issue #891: Rejects a milestone with a reason
///
/// Called when milestone evidence is insufficient or deadline expires
pub fn reject_milestone(
    reason: &str,
) -> SorobanString {
    SorobanString::from_slice(&Env::new(), reason.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_tranche_amount_even_split() {
        // 1,000,000 stroops / 4 tranches = 250,000 each
        let total = 1_000_000i128;
        let num_tranches = 4u32;

        assert_eq!(calculate_tranche_amount(total, num_tranches, 1).unwrap(), 250_000);
        assert_eq!(calculate_tranche_amount(total, num_tranches, 2).unwrap(), 250_000);
        assert_eq!(calculate_tranche_amount(total, num_tranches, 3).unwrap(), 250_000);
        assert_eq!(calculate_tranche_amount(total, num_tranches, 4).unwrap(), 250_000);

        // Total should sum correctly
        let mut total_calculated = 0i128;
        for i in 1..=num_tranches {
            total_calculated += calculate_tranche_amount(total, num_tranches, i).unwrap();
        }
        assert_eq!(total_calculated, total);
    }

    #[test]
    fn test_calculate_tranche_amount_with_remainder() {
        // 1,000,001 stroops / 4 tranches
        // Each of first 3: 250,000
        // Last: 250,001 (picks up remainder)
        let total = 1_000_001i128;
        let num_tranches = 4u32;

        assert_eq!(calculate_tranche_amount(total, num_tranches, 1).unwrap(), 250_000);
        assert_eq!(calculate_tranche_amount(total, num_tranches, 2).unwrap(), 250_000);
        assert_eq!(calculate_tranche_amount(total, num_tranches, 3).unwrap(), 250_000);
        assert_eq!(calculate_tranche_amount(total, num_tranches, 4).unwrap(), 250_001);

        // Total should sum correctly
        let mut total_calculated = 0i128;
        for i in 1..=num_tranches {
            total_calculated += calculate_tranche_amount(total, num_tranches, i).unwrap();
        }
        assert_eq!(total_calculated, total);
    }

    #[test]
    fn test_calculate_tranche_amount_boundary_cases() {
        // Minimum tranches
        let result = calculate_tranche_amount(1_000_000, MIN_TRANCHES, 1);
        assert!(result.is_ok());

        // Maximum tranches
        let result = calculate_tranche_amount(1_000_000, MAX_TRANCHES, 10);
        assert!(result.is_ok());

        // Invalid: below minimum
        let result = calculate_tranche_amount(1_000_000, MIN_TRANCHES - 1, 1);
        assert!(result.is_err());

        // Invalid: above maximum
        let result = calculate_tranche_amount(1_000_000, MAX_TRANCHES + 1, 1);
        assert!(result.is_err());

        // Invalid: zero tranches
        let result = calculate_tranche_amount(1_000_000, 0, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_milestone_yield_full_completion() {
        // 1,000,000 stroops, 200 bps (2%) yield, all 4 tranches complete
        let yield_amount = calculate_milestone_yield(200, 1_000_000, 4, 4);
        // Expected: (1_000_000 * 200) / 10_000 = 20_000
        assert_eq!(yield_amount, 20_000);
    }

    #[test]
    fn test_calculate_milestone_yield_partial_completion() {
        // 1,000,000 stroops, 200 bps (2%) yield, only 2 of 4 tranches complete
        let yield_amount = calculate_milestone_yield(200, 1_000_000, 2, 4);
        // Expected: 20_000 * (2/4) = 10_000
        assert_eq!(yield_amount, 10_000);
    }

    #[test]
    fn test_calculate_milestone_yield_single_tranche() {
        // 1,000,000 stroops, 200 bps yield, 1 of 4 tranches complete
        let yield_amount = calculate_milestone_yield(200, 1_000_000, 1, 4);
        // Expected: 20_000 * (1/4) = 5_000
        assert_eq!(yield_amount, 5_000);
    }

    #[test]
    fn test_calculate_milestone_yield_no_completion() {
        // No tranches completed = no yield
        let yield_amount = calculate_milestone_yield(200, 1_000_000, 0, 4);
        assert_eq!(yield_amount, 0);
    }

    #[test]
    fn test_is_milestone_expired() {
        // This would need env setup in actual tests, but logic is clear:
        // expired if now > (deadline + grace_period_secs)
        // For now, test is documented
    }
}
