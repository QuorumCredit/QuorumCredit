//! # Cross-Chain Governance / Multi-chain Voting (Issue #970)
//!
//! Enables governance proposals that must collect ratification votes from
//! multiple chains before they are executed on Stellar.
//!
//! ## Overview
//!
//! Traditional on-chain governance is single-chain. QuorumCredit spans multiple
//! networks; a parameter change on Stellar should only take effect once a quorum
//! of participating chains has also agreed.
//!
//! ### Lifecycle
//!
//! ```
//! Initiator (admin) calls propose_cross_chain(...)
//!   └─ Creates CrossChainProposalSync with status=Pending
//!      votes_required = len(target_chains)
//!
//! Bridge relayers (one per target chain) call vote_cross_chain(sync_id, chain_id, approve)
//!   └─ Accumulates approve / reject votes
//!   └─ Each chain may only vote once (voted_chains guard)
//!   └─ When approve votes == votes_required → status = Approved
//!
//! After ETA has passed, anyone calls execute_cross_chain(sync_id)
//!   └─ Validates status == Approved and now >= eta and now <= expires_at
//!   └─ Emits xchain/executed event; integrators act on it off-chain
//!
//! Admin or proposer calls cancel_cross_chain(sync_id, admin_signers)
//!   └─ Marks status = Cancelled (before execution only)
//! ```
//!
//! ### Security properties
//!
//! - Only an admin may create proposals.
//! - Each chain identifier may vote at most once (double-vote prevention).
//! - Votes after the expiry timestamp are rejected.
//! - Execution is only possible within the [eta, expires_at] window.
//! - Rejected votes do not count toward quorum but are recorded in the event log.
//! - All counters use checked arithmetic to prevent overflow.

use crate::errors::ContractError;
use crate::helpers::{require_admin_approval, require_not_paused};
use crate::types::{
    CrossChainProposalSync, DataKey, GovernanceProposalStatus,
    CROSS_CHAIN_PROPOSAL_ETA_SECS, CROSS_CHAIN_PROPOSAL_EXPIRY_SECS,
};
use soroban_sdk::{symbol_short, Address, Bytes, Env, String as SorobanString, Vec};

// ── Constants ──────────────────────────────────────────────────────────────

/// Minimum number of target chains required in a proposal.
const MIN_TARGET_CHAINS: u32 = 1;
/// Maximum number of target chains supported in a single proposal.
const MAX_TARGET_CHAINS: u32 = 20;

// ── Public functions ───────────────────────────────────────────────────────

/// Propose a new cross-chain governance action.
///
/// The proposal is created with `status = Pending` and `votes_required`
/// set to the number of target chains. Bridge relayers for each chain
/// must subsequently call [`vote_cross_chain`].
///
/// # Parameters
/// - `admin_signers` – must satisfy `admin_threshold` (admin-only action).
/// - `source_chain`  – human-readable name of the originating chain.
/// - `target_chains` – non-empty list of chains that must ratify the proposal.
/// - `proposal_type` – one of `"risk"`, `"fee"`, or `"timelock"`.
/// - `proposal_data` – opaque payload forwarded to each chain's bridge contract.
///
/// # Errors
/// - [`ContractError::InvalidAmount`] if `target_chains` is empty or exceeds the cap.
/// - [`ContractError::ContractPaused`] if the contract is paused.
///
/// # Returns
/// The new `sync_id` that callers must retain to vote or execute later.
pub fn propose_cross_chain(
    env: Env,
    admin_signers: Vec<Address>,
    source_chain: SorobanString,
    target_chains: Vec<SorobanString>,
    proposal_type: SorobanString,
    proposal_data: Bytes,
) -> Result<u64, ContractError> {
    require_not_paused(&env)?;
    require_admin_approval(&env, &admin_signers);

    let chain_count = target_chains.len();
    if chain_count < MIN_TARGET_CHAINS || chain_count > MAX_TARGET_CHAINS {
        return Err(ContractError::InvalidAmount);
    }

    let sync_id: u64 = next_sync_id(&env);
    let now = env.ledger().timestamp();
    let eta = now
        .checked_add(CROSS_CHAIN_PROPOSAL_ETA_SECS)
        .expect("eta overflow");
    let expires_at = eta
        .checked_add(CROSS_CHAIN_PROPOSAL_EXPIRY_SECS)
        .expect("expires_at overflow");

    let sync = CrossChainProposalSync {
        id: sync_id,
        source_chain: source_chain.clone(),
        target_chains: target_chains.clone(),
        proposal_type: proposal_type.clone(),
        proposal_data: proposal_data.clone(),
        votes_required: chain_count,
        votes_received: 0,
        voted_chains: Vec::new(&env),
        status: GovernanceProposalStatus::Pending,
        created_at: now,
        eta,
        expires_at,
    };

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposalSync(sync_id), &sync);

    env.events().publish(
        (symbol_short!("xchain"), symbol_short!("proposed")),
        (sync_id, source_chain, chain_count, eta),
    );

    Ok(sync_id)
}

/// Submit a vote from a target chain on an open proposal.
///
/// Each `chain_id` may cast exactly one vote. Passing `approve = true`
/// counts toward quorum; when `votes_received == votes_required` the
/// status transitions to `Approved` automatically.
///
/// Passing `approve = false` records the rejection in the event log but
/// does not transition the proposal — it can still reach quorum via other
/// chains. A fully-rejected proposal stays `Pending` until it expires.
///
/// This function is intended to be called by a bridge relayer address that
/// has been granted admin rights or a dedicated bridge role; the `relayer`
/// parameter is authenticated via `require_auth`.
///
/// # Errors
/// - [`ContractError::ProposalNotFound`] if `sync_id` does not exist.
/// - [`ContractError::InvalidStateTransition`] if the proposal is not Pending.
/// - [`ContractError::ProposalExpired`] if the proposal has passed `expires_at`.
/// - [`ContractError::AlreadyVoted`] if `chain_id` already voted.
/// - [`ContractError::InvalidBridgeChain`] if `chain_id` is not in `target_chains`.
/// - [`ContractError::ContractPaused`] if the contract is paused.
pub fn vote_cross_chain(
    env: Env,
    relayer: Address,
    sync_id: u64,
    chain_id: SorobanString,
    approve: bool,
) -> Result<(), ContractError> {
    relayer.require_auth();
    require_not_paused(&env)?;

    let mut sync: CrossChainProposalSync = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposalSync(sync_id))
        .ok_or(ContractError::ProposalNotFound)?;

    // Only open (Pending) proposals accept votes.
    if sync.status != GovernanceProposalStatus::Pending {
        return Err(ContractError::InvalidStateTransition);
    }

    // Reject votes past expiry.
    let now = env.ledger().timestamp();
    if now > sync.expires_at {
        sync.status = GovernanceProposalStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::CrossChainProposalSync(sync_id), &sync);
        return Err(ContractError::ProposalExpired);
    }

    // Ensure chain_id is in target_chains.
    let is_target = sync.target_chains.iter().any(|c| c == chain_id);
    if !is_target {
        return Err(ContractError::InvalidBridgeChain);
    }

    // Prevent double-voting by the same chain.
    if sync.voted_chains.iter().any(|c| c == chain_id) {
        return Err(ContractError::AlreadyVoted);
    }

    // Record the vote.
    sync.voted_chains.push_back(chain_id.clone());

    if approve {
        sync.votes_received = sync
            .votes_received
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;

        // Quorum reached → transition to Approved.
        if sync.votes_received >= sync.votes_required {
            sync.status = GovernanceProposalStatus::Approved;
        }
    }

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposalSync(sync_id), &sync);

    env.events().publish(
        (symbol_short!("xchain"), symbol_short!("voted")),
        (sync_id, chain_id, approve, sync.votes_received),
    );

    Ok(())
}

/// Execute an approved cross-chain proposal after the timelock has elapsed.
///
/// Execution emits an `xchain/executed` event; off-chain bridge relayers
/// observe this event and apply the parameter change on each target chain.
/// The contract itself does not push changes to external chains — Stellar
/// does not support outbound cross-chain calls natively.
///
/// # Errors
/// - [`ContractError::ProposalNotFound`] if `sync_id` does not exist.
/// - [`ContractError::ProposalNotFound`] (QuorumNotMet) if status ≠ Approved.
/// - [`ContractError::TimelockNotReady`] if `now < eta`.
/// - [`ContractError::TimelockExpired`] if `now > expires_at`.
/// - [`ContractError::ContractPaused`] if the contract is paused.
pub fn execute_cross_chain(env: Env, sync_id: u64) -> Result<(), ContractError> {
    require_not_paused(&env)?;

    let mut sync: CrossChainProposalSync = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposalSync(sync_id))
        .ok_or(ContractError::ProposalNotFound)?;

    if sync.status == GovernanceProposalStatus::Executed {
        return Err(ContractError::ProposalAlreadyApproved);
    }

    if sync.status != GovernanceProposalStatus::Approved {
        return Err(ContractError::QuorumNotMet);
    }

    let now = env.ledger().timestamp();

    if now < sync.eta {
        return Err(ContractError::TimelockNotReady);
    }
    if now > sync.expires_at {
        sync.status = GovernanceProposalStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::CrossChainProposalSync(sync_id), &sync);
        return Err(ContractError::TimelockExpired);
    }

    sync.status = GovernanceProposalStatus::Executed;
    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposalSync(sync_id), &sync);

    env.events().publish(
        (symbol_short!("xchain"), symbol_short!("executed")),
        (
            sync_id,
            sync.source_chain.clone(),
            sync.proposal_type.clone(),
            now,
        ),
    );

    Ok(())
}

/// Cancel an open or approved (but not yet executed) cross-chain proposal.
///
/// Only admins may cancel proposals; the action requires `admin_threshold`
/// signatures.
///
/// # Errors
/// - [`ContractError::ProposalNotFound`] if `sync_id` does not exist.
/// - [`ContractError::InvalidStateTransition`] if the proposal has already been
///   executed or cancelled.
/// - [`ContractError::ContractPaused`] if the contract is paused.
pub fn cancel_cross_chain(
    env: Env,
    admin_signers: Vec<Address>,
    sync_id: u64,
) -> Result<(), ContractError> {
    require_not_paused(&env)?;
    require_admin_approval(&env, &admin_signers);

    let mut sync: CrossChainProposalSync = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposalSync(sync_id))
        .ok_or(ContractError::ProposalNotFound)?;

    match sync.status {
        GovernanceProposalStatus::Executed | GovernanceProposalStatus::Cancelled => {
            return Err(ContractError::InvalidStateTransition);
        }
        _ => {}
    }

    sync.status = GovernanceProposalStatus::Cancelled;
    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposalSync(sync_id), &sync);

    env.events().publish(
        (symbol_short!("xchain"), symbol_short!("cancelled")),
        (sync_id, sync.source_chain),
    );

    Ok(())
}

/// Query a cross-chain proposal by ID.
pub fn get_cross_chain_proposal(
    env: Env,
    sync_id: u64,
) -> Option<CrossChainProposalSync> {
    env.storage()
        .persistent()
        .get(&DataKey::CrossChainProposalSync(sync_id))
}

/// Return the total number of cross-chain proposals ever created.
pub fn get_cross_chain_proposal_count(env: Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::CrossChainSyncCounter)
        .unwrap_or(0u64)
}

// ── Internal helpers ───────────────────────────────────────────────────────

/// Increment and return the next sync ID.
fn next_sync_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::CrossChainSyncCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .expect("sync ID overflow");
    env.storage()
        .instance()
        .set(&DataKey::CrossChainSyncCounter, &id);
    id
}
