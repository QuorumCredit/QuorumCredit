//! Issue #1171: Vouch syndication for risk pooling.
//!
//! A syndicate pool lets multiple vouchers combine their stake into a single
//! pool so that vouching risk (slashing losses) and reward (yield) are shared
//! proportionally to each member's contribution, instead of one voucher
//! bearing the full risk of the vouches they make individually.

use soroban_sdk::{symbol_short, Address, Env, String, Vec};

use crate::errors::ContractError;
use crate::helpers::require_allowed_token;
use crate::types::{
    DataKey, SyndicateContribution, SyndicateMember, SyndicatePerformance, SyndicatePool,
    SyndicateProposal, SyndicateProposalStatus,
};

/// Create a new syndicate pool from a set of member contributions, pooling
/// their stake so that risk and returns are shared across the syndicate.
pub fn create_vouch_syndicate(
    env: Env,
    creator: Address,
    pool_id: u64,
    token: Address,
    contributions: Vec<SyndicateContribution>,
) -> Result<(), ContractError> {
    creator.require_auth();

    if env
        .storage()
        .persistent()
        .has(&DataKey::SyndicatePool(pool_id))
    {
        return Err(ContractError::SyndicatePoolExists);
    }
    if contributions.is_empty() {
        return Err(ContractError::SyndicateEmpty);
    }

    let token_client = require_allowed_token(&env, &token)?;

    let mut total_stake: i128 = 0;
    let mut members: Vec<Address> = Vec::new(&env);
    for c in contributions.iter() {
        if c.amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        c.member.require_auth();
        token_client.transfer(&c.member, &env.current_contract_address(), &c.amount);
        total_stake = total_stake
            .checked_add(c.amount)
            .ok_or(ContractError::StakeOverflow)?;
        members.push_back(c.member.clone());
    }

    for c in contributions.iter() {
        let share_bps = bps_of(c.amount, total_stake);
        env.storage().persistent().set(
            &DataKey::SyndicateMember(pool_id, c.member.clone()),
            &SyndicateMember {
                member: c.member.clone(),
                contribution: c.amount,
                share_bps,
                rewards_received: 0,
            },
        );
    }

    let pool = SyndicatePool {
        pool_id,
        creator: creator.clone(),
        token,
        members,
        total_stake,
        pending_rewards: 0,
        created_at: env.ledger().timestamp(),
        active: true,
    };
    env.storage()
        .persistent()
        .set(&DataKey::SyndicatePool(pool_id), &pool);
    env.storage().persistent().set(
        &DataKey::SyndicatePerformance(pool_id),
        &SyndicatePerformance {
            pool_id,
            total_rewards_distributed: 0,
            total_slashed: 0,
            distribution_count: 0,
        },
    );

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("created")),
        (pool_id, creator, total_stake),
    );

    Ok(())
}

/// Credit yield/reward earned by loans this syndicate backed into the pool's
/// pending-rewards balance, ready to be distributed pro-rata to members.
pub fn credit_syndicate_reward(env: &Env, pool_id: u64, amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    let mut pool = get_syndicate_pool(env, pool_id).ok_or(ContractError::SyndicatePoolNotFound)?;
    if !pool.active {
        return Err(ContractError::SyndicateNotActive);
    }
    pool.pending_rewards = pool
        .pending_rewards
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .persistent()
        .set(&DataKey::SyndicatePool(pool_id), &pool);
    Ok(())
}

/// Record a slash against the syndicate's pooled stake for performance tracking.
pub fn record_syndicate_slash(env: &Env, pool_id: u64, amount: i128) -> Result<(), ContractError> {
    let mut perf: SyndicatePerformance = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicatePerformance(pool_id))
        .ok_or(ContractError::SyndicatePoolNotFound)?;
    perf.total_slashed = perf
        .total_slashed
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .persistent()
        .set(&DataKey::SyndicatePerformance(pool_id), &perf);
    Ok(())
}

/// Distribute the pool's accumulated pending rewards to members, proportional
/// to each member's `share_bps`.
pub fn distribute_syndicate_rewards(env: Env, pool_id: u64) -> Result<(), ContractError> {
    let mut pool = get_syndicate_pool(&env, pool_id).ok_or(ContractError::SyndicatePoolNotFound)?;
    if !pool.active {
        return Err(ContractError::SyndicateNotActive);
    }
    if pool.total_stake <= 0 {
        return Err(ContractError::SyndicateEmpty);
    }
    if pool.pending_rewards <= 0 {
        return Ok(());
    }

    let token_client = require_allowed_token(&env, &pool.token)?;
    let rewards = pool.pending_rewards;
    let mut distributed: i128 = 0;

    for member_addr in pool.members.iter() {
        let mut member: SyndicateMember = env
            .storage()
            .persistent()
            .get(&DataKey::SyndicateMember(pool_id, member_addr.clone()))
            .ok_or(ContractError::NotSyndicateMember)?;

        let share = (rewards * member.share_bps as i128) / 10_000;
        if share > 0 {
            token_client.transfer(&env.current_contract_address(), &member_addr, &share);
            member.rewards_received = member
                .rewards_received
                .checked_add(share)
                .ok_or(ContractError::ArithmeticError)?;
            env.storage()
                .persistent()
                .set(&DataKey::SyndicateMember(pool_id, member_addr.clone()), &member);
            distributed = distributed
                .checked_add(share)
                .ok_or(ContractError::ArithmeticError)?;
        }
    }

    pool.pending_rewards = pool
        .pending_rewards
        .checked_sub(distributed)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .persistent()
        .set(&DataKey::SyndicatePool(pool_id), &pool);

    let mut perf: SyndicatePerformance = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicatePerformance(pool_id))
        .ok_or(ContractError::SyndicatePoolNotFound)?;
    perf.total_rewards_distributed = perf
        .total_rewards_distributed
        .checked_add(distributed)
        .ok_or(ContractError::ArithmeticError)?;
    perf.distribution_count = perf.distribution_count.checked_add(1).unwrap_or(u32::MAX);
    env.storage()
        .persistent()
        .set(&DataKey::SyndicatePerformance(pool_id), &perf);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("distrib")),
        (pool_id, distributed),
    );

    Ok(())
}

/// Raise a member-voted governance proposal for a syndicate pool (e.g. to
/// dissolve the pool or change a shared policy). Any member may propose.
pub fn propose_syndicate_action(
    env: Env,
    pool_id: u64,
    proposer: Address,
    description: String,
) -> Result<u64, ContractError> {
    proposer.require_auth();
    require_member(&env, pool_id, &proposer)?;

    let proposal_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicateProposalCounter(pool_id))
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;

    let proposal = SyndicateProposal {
        pool_id,
        proposal_id,
        proposer: proposer.clone(),
        description,
        votes_for_bps: 0,
        votes_against_bps: 0,
        status: SyndicateProposalStatus::Pending,
        created_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::SyndicateProposal(pool_id, proposal_id), &proposal);
    env.storage()
        .persistent()
        .set(&DataKey::SyndicateProposalCounter(pool_id), &proposal_id);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("proposed")),
        (pool_id, proposal_id, proposer),
    );

    Ok(proposal_id)
}

/// Vote on an open syndicate proposal. Voting weight is the member's
/// `share_bps` in the pool; the proposal is approved once votes-for exceed
/// 50% of the pool's total share (10_000 bps).
pub fn vote_syndicate_proposal(
    env: Env,
    pool_id: u64,
    proposal_id: u64,
    voter: Address,
    approve: bool,
) -> Result<(), ContractError> {
    voter.require_auth();
    let member: SyndicateMember = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicateMember(pool_id, voter.clone()))
        .ok_or(ContractError::NotSyndicateMember)?;

    if env
        .storage()
        .persistent()
        .has(&DataKey::SyndicateProposalVote(pool_id, proposal_id, voter.clone()))
    {
        return Err(ContractError::SyndicateAlreadyVoted);
    }

    let mut proposal: SyndicateProposal = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicateProposal(pool_id, proposal_id))
        .ok_or(ContractError::SyndicateProposalNotFound)?;

    if proposal.status != SyndicateProposalStatus::Pending {
        return Ok(());
    }

    if approve {
        proposal.votes_for_bps = proposal.votes_for_bps.saturating_add(member.share_bps);
    } else {
        proposal.votes_against_bps = proposal.votes_against_bps.saturating_add(member.share_bps);
    }

    if proposal.votes_for_bps > 5_000 {
        proposal.status = SyndicateProposalStatus::Approved;
    } else if proposal.votes_against_bps >= 5_000 {
        proposal.status = SyndicateProposalStatus::Rejected;
    }

    env.storage().persistent().set(
        &DataKey::SyndicateProposalVote(pool_id, proposal_id, voter.clone()),
        &true,
    );
    env.storage()
        .persistent()
        .set(&DataKey::SyndicateProposal(pool_id, proposal_id), &proposal);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("voted")),
        (pool_id, proposal_id, voter, approve),
    );

    Ok(())
}

pub fn get_syndicate_pool(env: &Env, pool_id: u64) -> Option<SyndicatePool> {
    env.storage().persistent().get(&DataKey::SyndicatePool(pool_id))
}

pub fn get_syndicate_member(env: Env, pool_id: u64, member: Address) -> Option<SyndicateMember> {
    env.storage()
        .persistent()
        .get(&DataKey::SyndicateMember(pool_id, member))
}

pub fn get_syndicate_performance(env: Env, pool_id: u64) -> Option<SyndicatePerformance> {
    env.storage()
        .persistent()
        .get(&DataKey::SyndicatePerformance(pool_id))
}

pub fn get_syndicate_proposal(
    env: Env,
    pool_id: u64,
    proposal_id: u64,
) -> Option<SyndicateProposal> {
    env.storage()
        .persistent()
        .get(&DataKey::SyndicateProposal(pool_id, proposal_id))
}

/// Allow a syndicate member to exit the pool and reclaim their proportional stake.
///
/// # Rules
/// - The member must exist in the pool.
/// - Exit is rejected if the pool is active and removing this member would leave `total_stake == 0`
///   (i.e., the member holds 100% of the stake while active vouches may depend on it).
/// - The member receives their `contribution` back (proportional share of current `total_stake`).
/// - Remaining members' `share_bps` are recomputed from the reduced pool.
/// - Emits `syndicat/exit` with `(pool_id, member, returned_stake)`.
///
/// # Errors
/// - `SyndicatePoolNotFound` — pool does not exist.
/// - `NotSyndicateMember` — caller is not a member of the pool.
/// - `InvalidStateTransition` — exit would leave pool with zero stake while pool is active.
pub fn leave_syndicate(
    env: Env,
    pool_id: u64,
    member: Address,
) -> Result<(), ContractError> {
    // Step 1: Require auth from the exiting member.
    member.require_auth();

    // Step 2: Load the pool or return SyndicatePoolNotFound.
    let mut pool = get_syndicate_pool(&env, pool_id).ok_or(ContractError::SyndicatePoolNotFound)?;

    // Step 3: Check the member exists in the pool.
    require_member(&env, pool_id, &member)?;

    // Step 4: Load the SyndicateMember record.
    let member_record: SyndicateMember = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicateMember(pool_id, member.clone()))
        .ok_or(ContractError::NotSyndicateMember)?;

    // Step 5: Compute returned_stake as the member's proportional share of
    // the current total_stake, derived from share_bps.  Cap at pool.total_stake
    // to guard against rounding artefacts.
    let returned_stake = {
        let raw = (pool.total_stake * member_record.share_bps as i128) / 10_000;
        raw.min(pool.total_stake)
    };

    // Step 6: Reject exit that would drain an active pool to zero stake.
    if pool.active && (pool.total_stake - returned_stake) <= 0 {
        return Err(ContractError::InvalidStateTransition);
    }

    // Step 7: Transfer the member's stake back to them.
    let token_client = require_allowed_token(&env, &pool.token)?;
    token_client.transfer(&env.current_contract_address(), &member, &returned_stake);

    // Step 8: Remove the member's SyndicateMember storage entry.
    env.storage()
        .persistent()
        .remove(&DataKey::SyndicateMember(pool_id, member.clone()));

    // Step 9: Update the pool — deduct stake and remove from member list.
    pool.total_stake -= returned_stake;
    let mut new_members: Vec<Address> = Vec::new(&env);
    for m in pool.members.iter() {
        if m != member {
            new_members.push_back(m);
        }
    }
    pool.members = new_members;

    // Step 10: Recompute share_bps for all remaining members.
    let new_total_stake = pool.total_stake;
    if new_total_stake > 0 {
        for remaining_addr in pool.members.iter() {
            let mut remaining: SyndicateMember = env
                .storage()
                .persistent()
                .get(&DataKey::SyndicateMember(pool_id, remaining_addr.clone()))
                .ok_or(ContractError::NotSyndicateMember)?;
            remaining.share_bps = bps_of(remaining.contribution, new_total_stake);
            env.storage().persistent().set(
                &DataKey::SyndicateMember(pool_id, remaining_addr.clone()),
                &remaining,
            );
        }
    }

    // Step 11: Persist the updated pool.
    env.storage()
        .persistent()
        .set(&DataKey::SyndicatePool(pool_id), &pool);

    // Step 12: Emit syndicat/exit event.
    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("exit")),
        (pool_id, member.clone(), returned_stake),
    );

    Ok(())
}

fn require_member(env: &Env, pool_id: u64, member: &Address) -> Result<(), ContractError> {
    if env
        .storage()
        .persistent()
        .has(&DataKey::SyndicateMember(pool_id, member.clone()))
    {
        Ok(())
    } else {
        Err(ContractError::NotSyndicateMember)
    }
}

fn bps_of(amount: i128, total: i128) -> u32 {
    if total <= 0 {
        return 0;
    }
    ((amount * 10_000) / total) as u32
}
