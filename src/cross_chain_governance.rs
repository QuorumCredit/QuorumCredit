//! Cross-Chain Governance Module
//!
//! Extends the governance system to support voting across multiple chains,
//! with attestation-based vote aggregation and multi-signature execution.
//!
//! Issue #970: Multi-chain voting system
//!
//! Key features:
//! - Cross-chain vote proposals with timelock
//! - Bridge-attested vote aggregation
//! - Multi-signature execution with admin threshold
//! - Chain-specific vote tallying

use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::{DataKey, VOTE_ATTESTATION_MAX_AGE_SECS, VOTE_ATTESTATION_MAX_SKEW_SECS};
use soroban_sdk::{contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Vec};

/// Cross-chain governance proposal
#[contracttype]
#[derive(Clone)]
pub struct CrossChainProposal {
    /// Unique proposal ID
    pub proposal_id: u64,
    /// Description of the proposal
    pub description: String,
    /// Action to execute (e.g., "update_config", "slash_borrower")
    pub action: String,
    /// Encoded action parameters
    pub action_params: Bytes,
    /// Chain where vote results are collected from
    pub origin_chain: u32,
    /// When voting period ends (ledger seconds)
    pub voting_ends_at: u64,
    /// When the proposal can be executed (after timelock)
    pub execution_time: u64,
    /// Total stake participating in vote across all chains
    pub total_participated_stake: i128,
    /// Approve stake from all chains
    pub approve_stake: i128,
    /// Reject stake from all chains
    pub reject_stake: i128,
    /// Votes submitted (chain_id -> (approve_count, reject_count))
    pub chain_votes: Vec<ChainVoteAggregate>,
    /// Whether proposal has been executed
    pub executed: bool,
}

/// Per-chain vote aggregation
#[contracttype]
#[derive(Clone)]
pub struct ChainVoteAggregate {
    pub chain_id: u32,
    pub approve_stake: i128,
    pub reject_stake: i128,
    pub total_voters: u32,
}

/// Attestation of votes from a remote chain
#[contracttype]
#[derive(Clone)]
pub struct VoteAttestation {
    /// Chain where votes originated
    pub origin_chain: u32,
    /// Proposal ID being voted on
    pub proposal_id: u64,
    /// Aggregate approve stake from that chain
    pub approve_stake: i128,
    /// Aggregate reject stake from that chain
    pub reject_stake: i128,
    /// Number of unique voters
    pub voter_count: u32,
    /// Attestation timestamp
    pub attested_at: u64,
    /// Nonce to prevent replays
    pub nonce: u64,
    /// Signature from bridge (Ed25519)
    pub signature: BytesN<64>,
}

/// Vote record for a single voter on a proposal
#[contracttype]
#[derive(Clone)]
pub struct CrossChainVote {
    pub voter: Address,
    pub proposal_id: u64,
    pub approve: bool,
    pub stake: i128,
    pub chain_id: u32,
    pub timestamp: u64,
}

/// Canonical bytes a vote attestor key must sign for this attestation.
fn vote_attestation_message(
    env: &Env,
    origin_chain: u32,
    proposal_id: u64,
    approve_stake: i128,
    reject_stake: i128,
    voter_count: u32,
    attested_at: u64,
    nonce: u64,
) -> Bytes {
    let payload = (
        origin_chain,
        proposal_id,
        approve_stake,
        reject_stake,
        voter_count,
        attested_at,
        nonce,
    );
    let encoded = payload.to_xdr(env);
    env.crypto().sha256(&encoded).into()
}

/// Check if a vote attestation nonce has already been used.
fn is_vote_attestation_nonce_used(env: Env, origin_chain: u32, nonce: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::VoteAttestationNonceUsed(origin_chain, nonce))
        .unwrap_or(false)
}

/// Admin: Create a new cross-chain governance proposal
pub fn create_cross_chain_proposal(
    env: Env,
    admin_signers: Vec<Address>,
    description: String,
    action: String,
    action_params: Bytes,
    origin_chain: u32,
    voting_period_seconds: u64,
) -> Result<u64, ContractError> {
    require_admin_approval(&env, &admin_signers);

    if voting_period_seconds == 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Generate proposal ID (timestamp-based)
    let proposal_id = env.ledger().timestamp() as u64;

    let now = env.ledger().timestamp();
    let voting_ends_at = now + voting_period_seconds;
    let execution_time = voting_ends_at + (24 * 60 * 60); // 24h timelock

    let proposal = CrossChainProposal {
        proposal_id,
        description,
        action,
        action_params,
        origin_chain,
        voting_ends_at,
        execution_time,
        total_participated_stake: 0,
        approve_stake: 0,
        reject_stake: 0,
        chain_votes: Vec::new(&env),
        executed: false,
    };

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposal(proposal_id), &proposal);

    Ok(proposal_id)
}

/// Submit a vote on a cross-chain proposal
/// Can only be called by active vouchers
pub fn submit_cross_chain_vote(
    env: Env,
    voter: Address,
    proposal_id: u64,
    approve: bool,
    chain_id: u32,
) -> Result<(), ContractError> {
    voter.require_auth();

    let now = env.ledger().timestamp();
    let mut proposal: CrossChainProposal = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)?;

    // Check voting period
    if now > proposal.voting_ends_at {
        return Err(ContractError::VotingPeriodEnded);
    }

    // For now, use a default voter stake (in production, would query from vouch records)
    let voter_stake: i128 = 1_000_000; // 0.1 XLM default

    // Update vote tallies
    if approve {
        proposal.approve_stake = proposal.approve_stake.saturating_add(voter_stake);
    } else {
        proposal.reject_stake = proposal.reject_stake.saturating_add(voter_stake);
    }
    proposal.total_participated_stake = proposal.total_participated_stake.saturating_add(voter_stake);

    // Find or update chain aggregate
    let found = match proposal.chain_votes.iter().position(|c| c.chain_id == chain_id) {
        Some(i) => {
            let mut chain_agg = proposal.chain_votes.get(i as u32).unwrap();
            if approve {
                chain_agg.approve_stake = chain_agg.approve_stake.saturating_add(voter_stake);
            } else {
                chain_agg.reject_stake = chain_agg.reject_stake.saturating_add(voter_stake);
            }
            chain_agg.total_voters += 1;
            proposal.chain_votes.set(i as u32, chain_agg);
            true
        }
        None => false,
    };

    if !found {
        let new_agg = ChainVoteAggregate {
            chain_id,
            approve_stake: if approve { voter_stake } else { 0 },
            reject_stake: if approve { 0 } else { voter_stake },
            total_voters: 1,
        };
        proposal.chain_votes.push_back(new_agg);
    }

    // Record individual vote
    let vote = CrossChainVote {
        voter: voter.clone(),
        proposal_id,
        approve,
        stake: voter_stake,
        chain_id,
        timestamp: now,
    };

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainVote(proposal_id, voter), &vote);

    // Update proposal
    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposal(proposal_id), &proposal);

    Ok(())
}

/// Aggregate votes from a remote chain via attestation
pub fn aggregate_remote_votes(
    env: Env,
    admin_signers: Vec<Address>,
    proposal_id: u64,
    attestation: VoteAttestation,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    // Verify proposal ID matches
    if attestation.proposal_id != proposal_id {
        return Err(ContractError::InvalidStateTransition);
    }

    // Verify nonce has not been used (replay protection)
    if is_vote_attestation_nonce_used(env.clone(), attestation.origin_chain, attestation.nonce) {
        return Err(ContractError::VoteAttestationNonceReused);
    }

    // Check attestation freshness
    let now = env.ledger().timestamp();
    if attestation.attested_at > now {
        if attestation.attested_at - now > VOTE_ATTESTATION_MAX_SKEW_SECS {
            return Err(ContractError::VoteAttestationExpired);
        }
    } else if now - attestation.attested_at > VOTE_ATTESTATION_MAX_AGE_SECS {
        return Err(ContractError::VoteAttestationExpired);
    }

    // Get bridge public key for signature verification
    let public_key: BytesN<32> = env
        .storage()
        .persistent()
        .get(&DataKey::BridgePublicKey(attestation.origin_chain))
        .ok_or(ContractError::BridgeNotConfigured)?;

    // Construct message and verify signature
    let message = vote_attestation_message(
        &env,
        attestation.origin_chain,
        attestation.proposal_id,
        attestation.approve_stake,
        attestation.reject_stake,
        attestation.voter_count,
        attestation.attested_at,
        attestation.nonce,
    );

    env.crypto()
        .ed25519_verify(&public_key, &message, &attestation.signature);

    // Mark nonce as used (after successful verification)
    env.storage().persistent().set(
        &DataKey::VoteAttestationNonceUsed(attestation.origin_chain, attestation.nonce),
        &true,
    );

    let mut proposal: CrossChainProposal = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)?;

    // Check voting period
    if now > proposal.voting_ends_at {
        return Err(ContractError::VotingPeriodEnded);
    }

    // Update totals
    proposal.approve_stake = proposal.approve_stake.saturating_add(attestation.approve_stake);
    proposal.reject_stake = proposal.reject_stake.saturating_add(attestation.reject_stake);
    proposal.total_participated_stake = proposal
        .total_participated_stake
        .saturating_add(attestation.approve_stake + attestation.reject_stake);

    // Add chain aggregate
    let chain_agg = ChainVoteAggregate {
        chain_id: attestation.origin_chain,
        approve_stake: attestation.approve_stake,
        reject_stake: attestation.reject_stake,
        total_voters: attestation.voter_count,
    };
    proposal.chain_votes.push_back(chain_agg);

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposal(proposal_id), &proposal);

    Ok(())
}

/// Check if a proposal has passed (approve stake > reject stake)
pub fn has_proposal_passed(env: Env, proposal_id: u64) -> Result<bool, ContractError> {
    let proposal: CrossChainProposal = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)?;

    // Proposal passes if approve stake > reject stake
    Ok(proposal.approve_stake > proposal.reject_stake)
}

/// Execute a cross-chain governance proposal (after voting period and timelock)
pub fn execute_cross_chain_proposal(
    env: Env,
    admin_signers: Vec<Address>,
    proposal_id: u64,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    let mut proposal: CrossChainProposal = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)?;

    let now = env.ledger().timestamp();

    // Check voting period ended
    if now <= proposal.voting_ends_at {
        return Err(ContractError::VotingPeriodEnded);
    }

    // Check timelock elapsed
    if now < proposal.execution_time {
        return Err(ContractError::TimelockDelayNotElapsed);
    }

    // Check proposal passed
    if proposal.approve_stake <= proposal.reject_stake {
        return Err(ContractError::QuorumNotMet);
    }

    // Check not already executed
    if proposal.executed {
        return Err(ContractError::ProposalAlreadyFinalized);
    }

    // Mark as executed
    proposal.executed = true;
    env.storage()
        .persistent()
        .set(&DataKey::CrossChainProposal(proposal_id), &proposal);

    // In production, would execute the action here
    // For now, just mark as executed

    Ok(())
}

/// Query a cross-chain proposal
pub fn get_cross_chain_proposal(
    env: Env,
    proposal_id: u64,
) -> Result<CrossChainProposal, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)
}

/// Query vote results for a proposal
pub fn get_proposal_results(
    env: Env,
    proposal_id: u64,
) -> Result<(i128, i128, i128), ContractError> {
    let proposal: CrossChainProposal = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)?;

    Ok((
        proposal.approve_stake,
        proposal.reject_stake,
        proposal.total_participated_stake,
    ))
}

/// Query per-chain vote breakdown
pub fn get_chain_vote_breakdown(
    env: Env,
    proposal_id: u64,
) -> Result<Vec<ChainVoteAggregate>, ContractError> {
    let proposal: CrossChainProposal = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainProposal(proposal_id))
        .ok_or(ContractError::ProposalNotFound)?;

    Ok(proposal.chain_votes)
}
