//! Issue #892 (#31): Governance Proposal Queuing and Timelock
//!
//! This module implements a proposal queue system for governance with mandatory
//! timelock delays. Proposals are queued, undergo approval voting, and can only
//! be executed after a timelock period expires and before an execution window closes.
//!
//! ## Overview
//!
//! Traditional governance allows immediate execution of approved proposals. This
//! creates risk: a compromised admin could execute harmful changes instantly.
//!
//! Proposal queuing with timelock solves this by:
//! 1. Proposer queues a proposal with the action to execute
//! 2. Proposal enters queue and requires multi-sig approval
//! 3. After approval, proposal must wait for timelock delay (e.g., 24 hours)
//! 4. After timelock, proposal can be executed within a window (e.g., 7 days)
//! 5. After window expires, proposal expires and cannot be executed
//!
//! This gives users time to notice malicious proposals and exit before changes take effect.
//!
//! ## Data Flow
//!
//! ```
//! Queue Phase:
//!   queue_proposal(action=SetYieldBps(250))
//!   └─ Creates proposal with status=Pending, created_at=now
//!
//! Approval Phase:
//!   approve_proposal(proposal_id, approver_addresses=[...])
//!   └─ Multi-sig voting: if approvals >= threshold, status → Approved
//!      executable_at = now + timelock_delay
//!
//! Execution Phase (after timelock):
//!   execute_proposal(proposal_id)
//!   └─ If executable_at <= now <= expires_at: Execute, status → Executed
//!      Otherwise: Return error (not ready or expired)
//!
//! Cancellation Phase (anytime before execution):
//!   cancel_proposal(proposal_id, canceller_addresses=[...])
//!   └─ Multi-sig approval required, status → Cancelled
//! ```
//!
//! ## State Machine
//!
//! ```
//! Pending → Approved → Executed
//!   ↓         ↓           ↓
//!   └─────→ Cancelled ←─┘
//!   ↓         ↓
//!   └─────→ Expired ←─┘
//! ```

use crate::errors::ContractError;
use crate::types::*;
use soroban_sdk::{contracttype, Address, Env, String as SorobanString, Vec};

// ── Constants ──────────────────────────────────────────────────────────────

/// Default timelock delay before proposal can be executed (24 hours in seconds)
pub const DEFAULT_TIMELOCK_DELAY_SECS: u64 = 24 * 60 * 60;

/// Default execution window after timelock (7 days in seconds)
pub const DEFAULT_EXECUTION_WINDOW_SECS: u64 = 7 * 24 * 60 * 60;

/// Minimum timelock delay (1 hour in seconds)
pub const MIN_TIMELOCK_DELAY_SECS: u64 = 60 * 60;

/// Maximum timelock delay (365 days in seconds)
pub const MAX_TIMELOCK_DELAY_SECS: u64 = 365 * 24 * 60 * 60;

/// Minimum execution window (1 day in seconds)
pub const MIN_EXECUTION_WINDOW_SECS: u64 = 24 * 60 * 60;

/// Maximum execution window (90 days in seconds)
pub const MAX_EXECUTION_WINDOW_SECS: u64 = 90 * 24 * 60 * 60;

// ── Data Types ─────────────────────────────────────────────────────────────

/// Issue #892: Status of a queued governance proposal
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalQueueStatus {
    /// Proposal queued, awaiting approval votes
    Pending,
    /// Proposal approved, waiting for timelock to expire
    Approved,
    /// Proposal executed successfully
    Executed,
    /// Proposal was cancelled by admins
    Cancelled,
    /// Proposal timelock window expired without execution
    Expired,
}

/// Issue #892: A governance proposal in the execution queue
#[contracttype]
#[derive(Clone)]
pub struct QueuedProposal {
    /// Unique proposal ID (monotonically increasing)
    pub id: u64,
    /// The governance action to be executed
    pub action: GovernanceAction,
    /// Address that created this proposal
    pub proposer: Address,
    /// Addresses that have approved this proposal (multisig)
    pub approvals: Vec<Address>,
    /// Current status of the proposal
    pub status: ProposalQueueStatus,
    /// Ledger timestamp when proposal was queued
    pub created_at: u64,
    /// Ledger timestamp when proposal can be executed (after timelock)
    pub executable_at: u64,
    /// Ledger timestamp when proposal expires (after execution window)
    pub expires_at: u64,
    /// Optional description or justification
    pub description: SorobanString,
    /// Ledger timestamp when proposal was executed (if any)
    pub executed_at: Option<u64>,
}

/// Issue #892: Configuration for proposal queuing system
#[contracttype]
#[derive(Clone)]
pub struct ProposalQueueConfig {
    /// Delay after approval before execution is allowed (seconds)
    pub timelock_delay_secs: u64,
    /// Window after timelock during which execution is allowed (seconds)
    pub execution_window_secs: u64,
    /// Number of admin approvals required to move from Pending to Approved
    pub approvals_required: u32,
    /// Whether to allow non-admin proposers
    pub allow_public_proposals: bool,
}

// ── Helper Functions ──────────────────────────────────────────────────────

/// Issue #892: Validates proposal queue configuration
///
/// # Checks
/// - Timelock delay within bounds (1 hour to 365 days)
/// - Execution window within bounds (1 day to 90 days)
/// - Approvals required reasonable (1-10)
///
/// # Returns
/// Ok(()) if valid, Err(ContractError) otherwise
pub fn validate_queue_config(
    timelock_delay_secs: u64,
    execution_window_secs: u64,
    approvals_required: u32,
) -> Result<(), ContractError> {
    // Check timelock bounds
    if timelock_delay_secs < MIN_TIMELOCK_DELAY_SECS
        || timelock_delay_secs > MAX_TIMELOCK_DELAY_SECS
    {
        return Err(ContractError::InvalidAmount);
    }

    // Check execution window bounds
    if execution_window_secs < MIN_EXECUTION_WINDOW_SECS
        || execution_window_secs > MAX_EXECUTION_WINDOW_SECS
    {
        return Err(ContractError::InvalidAmount);
    }

    // Check approvals required
    if approvals_required == 0 || approvals_required > 10 {
        return Err(ContractError::InvalidAmount);
    }

    Ok(())
}

/// Issue #892: Checks if a proposal can be executed (timelock expired, not window passed)
///
/// Returns true if executable_at <= now <= expires_at
pub fn can_execute_proposal(env: &Env, proposal: &QueuedProposal) -> bool {
    let now = env.ledger().timestamp();
    now >= proposal.executable_at && now <= proposal.expires_at
}

/// Issue #892: Checks if a proposal has expired (execution window passed)
///
/// Returns true if now > expires_at
pub fn is_proposal_expired(env: &Env, proposal: &QueuedProposal) -> bool {
    let now = env.ledger().timestamp();
    now > proposal.expires_at
}

/// Issue #892: Checks if a proposal is waiting for timelock (not yet executable)
///
/// Returns true if now < executable_at and status is Approved
pub fn is_proposal_timelocked(env: &Env, proposal: &QueuedProposal) -> bool {
    let now = env.ledger().timestamp();
    proposal.status == ProposalQueueStatus::Approved && now < proposal.executable_at
}

/// Issue #892: Calculates remaining timelock seconds for a proposal
///
/// Returns 0 if timelock has already expired, otherwise returns seconds remaining
pub fn timelock_remaining_secs(env: &Env, proposal: &QueuedProposal) -> u64 {
    let now = env.ledger().timestamp();
    if now >= proposal.executable_at {
        0
    } else {
        proposal.executable_at.saturating_sub(now)
    }
}

/// Issue #892: Calculates remaining execution window seconds for a proposal
///
/// Returns 0 if window has closed, otherwise returns seconds remaining
pub fn execution_window_remaining_secs(env: &Env, proposal: &QueuedProposal) -> u64 {
    let now = env.ledger().timestamp();
    if now > proposal.expires_at {
        0
    } else {
        proposal.expires_at.saturating_sub(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_queue_config_valid() {
        let result = validate_queue_config(
            DEFAULT_TIMELOCK_DELAY_SECS,
            DEFAULT_EXECUTION_WINDOW_SECS,
            2,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_queue_config_min_bounds() {
        let result = validate_queue_config(
            MIN_TIMELOCK_DELAY_SECS,
            MIN_EXECUTION_WINDOW_SECS,
            1,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_queue_config_max_bounds() {
        let result = validate_queue_config(
            MAX_TIMELOCK_DELAY_SECS,
            MAX_EXECUTION_WINDOW_SECS,
            10,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_queue_config_timelock_below_min() {
        let result = validate_queue_config(
            MIN_TIMELOCK_DELAY_SECS - 1,
            DEFAULT_EXECUTION_WINDOW_SECS,
            2,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_queue_config_timelock_above_max() {
        let result = validate_queue_config(
            MAX_TIMELOCK_DELAY_SECS + 1,
            DEFAULT_EXECUTION_WINDOW_SECS,
            2,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_queue_config_window_below_min() {
        let result = validate_queue_config(
            DEFAULT_TIMELOCK_DELAY_SECS,
            MIN_EXECUTION_WINDOW_SECS - 1,
            2,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_queue_config_window_above_max() {
        let result = validate_queue_config(
            DEFAULT_TIMELOCK_DELAY_SECS,
            MAX_EXECUTION_WINDOW_SECS + 1,
            2,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_queue_config_approvals_below_min() {
        let result = validate_queue_config(
            DEFAULT_TIMELOCK_DELAY_SECS,
            DEFAULT_EXECUTION_WINDOW_SECS,
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_queue_config_approvals_above_max() {
        let result = validate_queue_config(
            DEFAULT_TIMELOCK_DELAY_SECS,
            DEFAULT_EXECUTION_WINDOW_SECS,
            11,
        );
        assert!(result.is_err());
    }
}
