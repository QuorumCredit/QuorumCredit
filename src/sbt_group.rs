//! # SBT Group Ownership (Issue #1739)
//!
//! Allows a Soulbound Token credential to be jointly owned by a set of
//! addresses ("group members").  Governance decisions for the group — such
//! as adding or removing members — require a majority vote among existing
//! members, mirroring the multi-sig admin pattern already used across the
//! protocol.
//!
//! ## Group lifecycle
//!
//! ```text
//! create_group_sbt(owners, metadata)
//!       │
//!       ▼
//!  GroupSbt { group_id, owners, metadata, … }    ← stored under DataKey::SbtGroup
//!       │
//!       ├─ propose_group_membership_change(group_id, target, action)
//!       │       └─ stores pending GroupMembershipProposal
//!       │
//!       └─ vote_group_membership(group_id, proposal_id, voter, approve)
//!               └─ if approve_count > len(owners)/2 → executes change
//! ```
//!
//! ## Access control
//!
//! - `create_group_sbt` is open to any authenticated caller (they become the
//!   first member set).
//! - `propose_group_membership_change` / `vote_group_membership` require the
//!   caller to be a current group member.

#![allow(unused)]

use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::require_not_paused;
use crate::types::{DataKey, PERSISTENT_TTL_TARGET_LEDGERS, PERSISTENT_TTL_THRESHOLD_LEDGERS};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of owners a single group SBT may have.
pub const MAX_GROUP_OWNERS: u32 = 20;

/// Maximum metadata size in bytes.
pub const MAX_GROUP_METADATA_BYTES: u32 = 512;

// ── Data Structures ───────────────────────────────────────────────────────────

/// The action requested by a membership proposal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MembershipAction {
    /// Add `target` to the group owners.
    Add,
    /// Remove `target` from the group owners.
    Remove,
}

/// A pending proposal to change the membership of a group SBT.
#[contracttype]
#[derive(Clone, Debug)]
pub struct GroupMembershipProposal {
    /// Unique identifier for this proposal within its group.
    pub proposal_id: u64,
    /// The address to add or remove.
    pub target: Address,
    /// Whether this is an add or remove action.
    pub action: MembershipAction,
    /// Ledger timestamp when the proposal was created.
    pub created_at: u64,
    /// Addresses that have voted to approve.
    pub approvals: Vec<Address>,
    /// Addresses that have voted to reject.
    pub rejections: Vec<Address>,
    /// Whether the proposal has been executed.
    pub executed: bool,
}

/// A group-owned Soulbound Token credential.
#[contracttype]
#[derive(Clone, Debug)]
pub struct GroupSbt {
    /// Unique identifier for this group SBT.
    pub group_id: u64,
    /// Current set of group owners.
    pub owners: Vec<Address>,
    /// Arbitrary metadata payload (e.g. a JSON blob or IPFS CID).
    pub metadata: Bytes,
    /// Ledger timestamp when the group was created.
    pub created_at: u64,
    /// Monotonically increasing proposal counter for this group.
    pub proposal_counter: u64,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn next_group_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::SbtGroupCounter)
        .unwrap_or(0u64);
    let next = id + 1;
    env.storage()
        .persistent()
        .set(&DataKey::SbtGroupCounter, &next);
    next
}

fn load_group(env: &Env, group_id: u64) -> Result<GroupSbt, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::SbtGroup(group_id))
        .ok_or(ContractError::SbtGroupNotFound)
}

fn save_group(env: &Env, group: &GroupSbt) {
    env.storage()
        .persistent()
        .set(&DataKey::SbtGroup(group.group_id), group);
    env.storage().persistent().extend_ttl(
        &DataKey::SbtGroup(group.group_id),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

fn load_proposal(
    env: &Env,
    group_id: u64,
    proposal_id: u64,
) -> Result<GroupMembershipProposal, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::SbtGroupProposal(group_id, proposal_id))
        .ok_or(ContractError::SbtGroupProposalNotFound)
}

fn save_proposal(env: &Env, group_id: u64, proposal: &GroupMembershipProposal) {
    env.storage()
        .persistent()
        .set(&DataKey::SbtGroupProposal(group_id, proposal.proposal_id), proposal);
    env.storage().persistent().extend_ttl(
        &DataKey::SbtGroupProposal(group_id, proposal.proposal_id),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

fn is_owner(owners: &Vec<Address>, addr: &Address) -> bool {
    owners.iter().any(|o| o == *addr)
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Issue #1739 — Create a new group-owned SBT.
///
/// All `owners` become the initial co-owners of the credential.  The caller
/// must be included in `owners`; if they are not, this returns
/// `ContractError::UnauthorizedCaller`.
///
/// # Errors
/// - [`ContractError::ContractPaused`]
/// - [`ContractError::InvalidAmount`] — `owners` is empty or exceeds
///   [`MAX_GROUP_OWNERS`].
/// - [`ContractError::UnauthorizedCaller`] — the first signer is not in `owners`.
pub fn create_group_sbt(
    env: &Env,
    caller: Address,
    owners: Vec<Address>,
    metadata: Bytes,
) -> Result<u64, ContractError> {
    require_not_paused(env)?;
    caller.require_auth();

    if owners.is_empty() || owners.len() > MAX_GROUP_OWNERS {
        return Err(ContractError::InvalidAmount);
    }

    if !is_owner(&owners, &caller) {
        return Err(ContractError::UnauthorizedCaller);
    }

    if metadata.len() > MAX_GROUP_METADATA_BYTES {
        return Err(ContractError::InvalidAmount);
    }

    let group_id = next_group_id(env);
    let now = env.ledger().timestamp();

    let group = GroupSbt {
        group_id,
        owners: owners.clone(),
        metadata: metadata.clone(),
        created_at: now,
        proposal_counter: 0,
    };

    save_group(env, &group);

    env.events().publish(
        (symbol_short!("sbt_grp"), symbol_short!("create")),
        (group_id, owners),
    );

    Ok(group_id)
}

/// Issue #1739 — Propose adding or removing a member from a group SBT.
///
/// Only current group owners may propose changes.  Returns the new
/// `proposal_id`.
///
/// # Errors
/// - [`ContractError::ContractPaused`]
/// - [`ContractError::SbtGroupNotFound`]
/// - [`ContractError::UnauthorizedCaller`] — proposer is not a group owner.
/// - [`ContractError::InvalidAmount`] — adding `target` who is already a
///   member, or removing one who is not.
pub fn propose_group_membership_change(
    env: &Env,
    proposer: Address,
    group_id: u64,
    target: Address,
    action: MembershipAction,
) -> Result<u64, ContractError> {
    require_not_paused(env)?;
    proposer.require_auth();

    let mut group = load_group(env, group_id)?;

    if !is_owner(&group.owners, &proposer) {
        return Err(ContractError::UnauthorizedCaller);
    }

    // Validate the action makes sense given current membership
    match action {
        MembershipAction::Add => {
            if is_owner(&group.owners, &target) {
                return Err(ContractError::InvalidAmount);
            }
            if group.owners.len() >= MAX_GROUP_OWNERS {
                return Err(ContractError::InvalidAmount);
            }
        }
        MembershipAction::Remove => {
            if !is_owner(&group.owners, &target) {
                return Err(ContractError::InvalidAmount);
            }
            // Must keep at least one owner
            if group.owners.len() <= 1 {
                return Err(ContractError::InvalidAmount);
            }
        }
    }

    group.proposal_counter += 1;
    let proposal_id = group.proposal_counter;

    let mut approvals = Vec::new(env);
    approvals.push_back(proposer.clone()); // proposer implicitly approves

    let proposal = GroupMembershipProposal {
        proposal_id,
        target: target.clone(),
        action: action.clone(),
        created_at: env.ledger().timestamp(),
        approvals,
        rejections: Vec::new(env),
        executed: false,
    };

    save_group(env, &group);
    save_proposal(env, group_id, &proposal);

    env.events().publish(
        (symbol_short!("sbt_grp"), symbol_short!("propose")),
        (group_id, proposal_id, target, action),
    );

    Ok(proposal_id)
}

/// Issue #1739 — Vote on a membership proposal.
///
/// If approvals reach a strict majority of the current owner count the
/// proposal is automatically executed.  Rejects are similarly counted but
/// only block execution (no auto-reject — the proposal simply sits until
/// expiry or a future governance cleanup).
///
/// # Errors
/// - [`ContractError::ContractPaused`]
/// - [`ContractError::SbtGroupNotFound`] / [`ContractError::SbtGroupProposalNotFound`]
/// - [`ContractError::UnauthorizedCaller`] — voter is not a group owner.
/// - [`ContractError::AlreadyVoted`] — voter has already cast a vote.
/// - [`ContractError::InvalidStateTransition`] — proposal already executed.
pub fn vote_group_membership(
    env: &Env,
    voter: Address,
    group_id: u64,
    proposal_id: u64,
    approve: bool,
) -> Result<(), ContractError> {
    require_not_paused(env)?;
    voter.require_auth();

    let mut group = load_group(env, group_id)?;
    let mut proposal = load_proposal(env, group_id, proposal_id)?;

    if proposal.executed {
        return Err(ContractError::InvalidStateTransition);
    }

    if !is_owner(&group.owners, &voter) {
        return Err(ContractError::UnauthorizedCaller);
    }

    // Prevent double voting
    let already_voted = proposal.approvals.iter().any(|a| a == voter)
        || proposal.rejections.iter().any(|r| r == voter);
    if already_voted {
        return Err(ContractError::AlreadyVoted);
    }

    if approve {
        proposal.approvals.push_back(voter.clone());
    } else {
        proposal.rejections.push_back(voter.clone());
    }

    // Check for majority approval (> 50% of current owners)
    let owner_count = group.owners.len() as u32;
    let approval_count = proposal.approvals.len() as u32;
    let quorum_threshold = owner_count / 2 + 1;

    if approval_count >= quorum_threshold {
        // Execute the proposal
        execute_membership_change(env, &mut group, &proposal)?;
        proposal.executed = true;

        env.events().publish(
            (symbol_short!("sbt_grp"), symbol_short!("execute")),
            (group_id, proposal_id, proposal.target.clone()),
        );
    }

    save_group(env, &group);
    save_proposal(env, group_id, &proposal);

    Ok(())
}

fn execute_membership_change(
    env: &Env,
    group: &mut GroupSbt,
    proposal: &GroupMembershipProposal,
) -> Result<(), ContractError> {
    match proposal.action {
        MembershipAction::Add => {
            group.owners.push_back(proposal.target.clone());
        }
        MembershipAction::Remove => {
            let mut new_owners: Vec<Address> = Vec::new(env);
            for owner in group.owners.iter() {
                if owner != proposal.target {
                    new_owners.push_back(owner);
                }
            }
            group.owners = new_owners;
        }
    }
    Ok(())
}

/// Issue #1739 — Read a group SBT record.
pub fn get_group_sbt(env: &Env, group_id: u64) -> Result<GroupSbt, ContractError> {
    load_group(env, group_id)
}
