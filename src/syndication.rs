//! Loan syndication: multiple borrowers/guarantors pool collateral and vouch
//! stake behind a single shared loan request.
//!
//! Issue #1466: `request_syndication_loan` re-validates that the syndication's
//! currently committed stake (collateral + vouch stake still held by members
//! who have not left) covers the requested loan amount, and `join_syndication`
//! enforces the configured `max_members` cap so a syndication cannot grow
//! without bound.

use soroban_sdk::{symbol_short, Address, Env, String, Vec};
use crate::errors::ContractError;
use crate::helpers::require_allowed_token;
use crate::types::{
    DataKey, LoanSyndication, SyndicationConfig, SyndicationMember, SyndicationRepayment,
    SyndicationRole, SyndicationStatus, DEFAULT_SYNDICATION_CONFIG,
};

/// Ceiling division used to compute the number of approvals required from a
/// percentage-of-members threshold (e.g. 7500 bps of 3 members = 3 approvals).
fn required_approvals(member_count: u32, min_approval_percentage: u32) -> u32 {
    ((member_count as u64 * min_approval_percentage as u64 + 9_999) / 10_000) as u32
}

fn config(env: &Env) -> SyndicationConfig {
    env.storage()
        .instance()
        .get(&DataKey::SyndicationConfig)
        .unwrap_or(DEFAULT_SYNDICATION_CONFIG)
}

fn load_syndication(env: &Env, syndication_id: u64) -> Result<LoanSyndication, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)
}

fn save_syndication(env: &Env, syndication: &LoanSyndication) {
    env.storage().persistent().set(
        &DataKey::LoanSyndication(syndication.syndication_id),
        syndication,
    );
}

/// Sum of collateral + vouch stake currently committed by a syndication's
/// members. Recomputed fresh each time so that members who left in the
/// interim are no longer counted.
fn committed_stake(syndication: &LoanSyndication) -> i128 {
    syndication
        .members
        .iter()
        .fold(0i128, |acc, m| acc.saturating_add(m.collateral).saturating_add(m.vouch_stake))
}

fn total_repaid(env: &Env, syndication_id: u64) -> i128 {
    let count: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicationRepaymentCounter(syndication_id))
        .unwrap_or(0u64);

    let mut total: i128 = 0;
    for idx in 1..=count {
        if let Some(r) = env
            .storage()
            .persistent()
            .get::<DataKey, SyndicationRepayment>(&DataKey::SyndicationRepayment(syndication_id, idx))
        {
            total = total.saturating_add(r.amount);
        }
    }
    total
}

pub fn create_syndication(
    env: Env,
    creator: Address,
    loan_purpose: String,
    token_address: Address,
    total_amount: i128,
) -> Result<u64, ContractError> {
    creator.require_auth();

    if total_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    let cfg = config(&env);
    if total_amount > cfg.max_loan_amount {
        return Err(ContractError::LoanExceedsMaxAmount);
    }
    require_allowed_token(&env, &token_address)?;

    let syndication_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::SyndicationCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;

    let syndication = LoanSyndication {
        syndication_id,
        loan_id: None,
        members: Vec::new(&env),
        total_amount,
        total_collateral: 0,
        total_vouch_stake: 0,
        loan_purpose,
        token_address,
        created_at: env.ledger().timestamp(),
        disbursed_at: None,
        status: SyndicationStatus::Forming,
        min_approvals: 0,
        approval_count: 0,
    };

    save_syndication(&env, &syndication);
    env.storage()
        .instance()
        .set(&DataKey::SyndicationCounter, &syndication_id);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("created")),
        (syndication_id, creator, total_amount),
    );

    Ok(syndication_id)
}

pub fn join_syndication(
    env: Env,
    syndication_id: u64,
    member: Address,
    role: SyndicationRole,
    share_bps: u32,
    collateral: i128,
    vouch_stake: i128,
) -> Result<(), ContractError> {
    member.require_auth();

    let mut syndication = load_syndication(&env, syndication_id)?;
    if syndication.status != SyndicationStatus::Forming {
        return Err(ContractError::InvalidSyndicationStatus);
    }
    if collateral < 0 || vouch_stake < 0 {
        return Err(ContractError::InvalidAmount);
    }
    if share_bps == 0 || share_bps > 10_000 {
        return Err(ContractError::InvalidSyndicationShare);
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::SyndicationMember(syndication_id, member.clone()))
    {
        return Err(ContractError::SyndicationMemberExists);
    }

    // Issue #1466: enforce the configured cap on syndication size.
    let cfg = config(&env);
    if syndication.members.len() >= cfg.max_members {
        return Err(ContractError::SyndicationMaxMembersExceeded);
    }

    let existing_share: u32 = syndication
        .members
        .iter()
        .fold(0u32, |acc, m| acc.saturating_add(m.share_bps));
    if existing_share.saturating_add(share_bps) > 10_000 {
        return Err(ContractError::InvalidSyndicationShare);
    }

    let token_client = require_allowed_token(&env, &syndication.token_address)?;
    let contribution = collateral.saturating_add(vouch_stake);
    if contribution > 0 {
        token_client.transfer(&member, &env.current_contract_address(), &contribution);
    }

    let syndication_member = SyndicationMember {
        address: member.clone(),
        role,
        share_bps,
        collateral,
        vouch_stake,
        approved: false,
        joined_at: env.ledger().timestamp(),
    };

    syndication.members.push_back(syndication_member.clone());
    syndication.total_collateral = syndication.total_collateral.saturating_add(collateral);
    syndication.total_vouch_stake = syndication.total_vouch_stake.saturating_add(vouch_stake);
    syndication.min_approvals = required_approvals(syndication.members.len(), cfg.min_approval_percentage);

    env.storage().persistent().set(
        &DataKey::SyndicationMember(syndication_id, member.clone()),
        &syndication_member,
    );
    save_syndication(&env, &syndication);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("joined")),
        (syndication_id, member),
    );

    Ok(())
}

pub fn approve_syndication(
    env: Env,
    syndication_id: u64,
    member: Address,
) -> Result<(), ContractError> {
    member.require_auth();

    let mut syndication = load_syndication(&env, syndication_id)?;
    if syndication.status != SyndicationStatus::Forming {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    let idx = syndication
        .members
        .iter()
        .position(|m| m.address == member)
        .ok_or(ContractError::SyndicationMemberNotFound)? as u32;

    let mut member_record = syndication.members.get(idx).unwrap();
    if member_record.approved {
        return Ok(());
    }
    member_record.approved = true;
    syndication.members.set(idx, member_record.clone());
    syndication.approval_count = syndication.approval_count.saturating_add(1);

    let cfg = config(&env);
    syndication.min_approvals = required_approvals(syndication.members.len(), cfg.min_approval_percentage);
    if syndication.members.len() >= cfg.min_members
        && syndication.approval_count >= syndication.min_approvals
    {
        syndication.status = SyndicationStatus::Ready;
    }

    env.storage().persistent().set(
        &DataKey::SyndicationMember(syndication_id, member.clone()),
        &member_record,
    );
    save_syndication(&env, &syndication);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("approved")),
        (syndication_id, member),
    );

    Ok(())
}

pub fn leave_syndication(
    env: Env,
    syndication_id: u64,
    member: Address,
) -> Result<(), ContractError> {
    member.require_auth();

    let mut syndication = load_syndication(&env, syndication_id)?;
    // A member may back out any time before the loan is actually disbursed.
    if syndication.status != SyndicationStatus::Forming && syndication.status != SyndicationStatus::Ready {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    let idx = syndication
        .members
        .iter()
        .position(|m| m.address == member)
        .ok_or(ContractError::SyndicationMemberNotFound)? as u32;

    let member_record = syndication.members.get(idx).unwrap();
    syndication.members.remove(idx);
    syndication.total_collateral = syndication
        .total_collateral
        .saturating_sub(member_record.collateral);
    syndication.total_vouch_stake = syndication
        .total_vouch_stake
        .saturating_sub(member_record.vouch_stake);
    if member_record.approved {
        syndication.approval_count = syndication.approval_count.saturating_sub(1);
    }
    let cfg = config(&env);
    syndication.min_approvals = required_approvals(syndication.members.len(), cfg.min_approval_percentage);
    // If the departure drops membership/approvals below quorum, a syndication
    // that was Ready reverts to Forming; note this does NOT by itself catch a
    // syndication that stays Ready with quorum intact but insufficient stake
    // left to fund the loan — that is checked in `request_syndication_loan`.
    if syndication.status == SyndicationStatus::Ready
        && (syndication.members.len() < cfg.min_members || syndication.approval_count < syndication.min_approvals)
    {
        syndication.status = SyndicationStatus::Forming;
    }

    let refund = member_record.collateral.saturating_add(member_record.vouch_stake);
    if refund > 0 {
        let token_client = require_allowed_token(&env, &syndication.token_address)?;
        token_client.transfer(&env.current_contract_address(), &member, &refund);
    }

    env.storage()
        .persistent()
        .remove(&DataKey::SyndicationMember(syndication_id, member.clone()));
    save_syndication(&env, &syndication);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("left")),
        (syndication_id, member),
    );

    Ok(())
}

pub fn cancel_syndication(
    env: Env,
    syndication_id: u64,
    caller: Address,
) -> Result<(), ContractError> {
    caller.require_auth();

    let mut syndication = load_syndication(&env, syndication_id)?;
    if syndication.status != SyndicationStatus::Forming && syndication.status != SyndicationStatus::Ready {
        return Err(ContractError::InvalidSyndicationStatus);
    }
    if !syndication.members.iter().any(|m| m.address == caller) {
        return Err(ContractError::UnauthorizedCaller);
    }

    if syndication.total_collateral.saturating_add(syndication.total_vouch_stake) > 0 {
        let token_client = require_allowed_token(&env, &syndication.token_address)?;
        for m in syndication.members.iter() {
            let refund = m.collateral.saturating_add(m.vouch_stake);
            if refund > 0 {
                token_client.transfer(&env.current_contract_address(), &m.address, &refund);
            }
        }
    }

    syndication.status = SyndicationStatus::Cancelled;
    save_syndication(&env, &syndication);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("cancel")),
        (syndication_id, caller),
    );

    Ok(())
}

pub fn request_syndication_loan(
    env: Env,
    syndication_id: u64,
    lead_borrower: Address,
) -> Result<u64, ContractError> {
    lead_borrower.require_auth();

    let mut syndication = load_syndication(&env, syndication_id)?;
    if syndication.status != SyndicationStatus::Ready {
        return Err(ContractError::InvalidSyndicationStatus);
    }
    if syndication.loan_id.is_some() {
        return Err(ContractError::SyndicationHasLoan);
    }
    if !syndication
        .members
        .iter()
        .any(|m| m.address == lead_borrower && m.role == SyndicationRole::LeadBorrower)
    {
        return Err(ContractError::UnauthorizedCaller);
    }

    // Issue #1466: re-validate that the stake still committed by current
    // members (some may have left since joining/approving) covers the
    // requested loan amount before disbursing.
    let remaining_stake = committed_stake(&syndication);
    if remaining_stake < syndication.total_amount {
        return Err(ContractError::MinStakeNotMet);
    }

    let token_client = require_allowed_token(&env, &syndication.token_address)?;
    token_client.transfer(
        &env.current_contract_address(),
        &lead_borrower,
        &syndication.total_amount,
    );

    syndication.status = SyndicationStatus::Active;
    syndication.disbursed_at = Some(env.ledger().timestamp());
    syndication.loan_id = Some(syndication_id);
    save_syndication(&env, &syndication);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("loan")),
        (syndication_id, lead_borrower, syndication.total_amount),
    );

    Ok(syndication_id)
}

pub fn repay_syndication_loan(
    env: Env,
    syndication_id: u64,
    repayer: Address,
    amount: i128,
) -> Result<(), ContractError> {
    repayer.require_auth();

    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    let mut syndication = load_syndication(&env, syndication_id)?;
    if syndication.status != SyndicationStatus::Active {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    let token_client = require_allowed_token(&env, &syndication.token_address)?;
    token_client.transfer(&repayer, &env.current_contract_address(), &amount);

    let next_idx: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicationRepaymentCounter(syndication_id))
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;

    env.storage().persistent().set(
        &DataKey::SyndicationRepayment(syndication_id, next_idx),
        &SyndicationRepayment {
            syndication_id,
            repayer: repayer.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
        },
    );
    env.storage()
        .persistent()
        .set(&DataKey::SyndicationRepaymentCounter(syndication_id), &next_idx);

    if total_repaid(&env, syndication_id) >= syndication.total_amount {
        syndication.status = SyndicationStatus::Repaid;
        save_syndication(&env, &syndication);
    }

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("repay")),
        (syndication_id, repayer, amount),
    );

    Ok(())
}

/// Handle a default on the syndication's loan: any unrepaid shortfall is
/// covered by forfeiting each member's escrowed collateral + vouch stake,
/// proportional to their share of the total stake still held. A member
/// with zero remaining stake is charged zero loss (never a negative payout).
pub fn handle_syndication_default(
    env: Env,
    syndication_id: u64,
    caller: Address,
) -> Result<(), ContractError> {
    caller.require_auth();

    let mut syndication = load_syndication(&env, syndication_id)?;
    if syndication.status != SyndicationStatus::Active {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    let repaid = total_repaid(&env, syndication_id);
    let shortfall = syndication.total_amount.saturating_sub(repaid);
    if shortfall <= 0 {
        return Err(ContractError::AlreadyRepaid);
    }

    let total_stake = committed_stake(&syndication);

    if total_stake > 0 {
        for idx in 0..syndication.members.len() {
            let mut member = syndication.members.get(idx).unwrap();
            let member_stake = member.collateral.saturating_add(member.vouch_stake);
            if member_stake <= 0 {
                continue;
            }

            // Proportional to this member's share of total remaining stake,
            // never exceeding what the member actually has committed.
            let raw_loss = (shortfall.saturating_mul(member_stake)) / total_stake;
            let loss = raw_loss.min(member_stake).max(0);

            let from_vouch = loss.min(member.vouch_stake);
            member.vouch_stake = member.vouch_stake.saturating_sub(from_vouch);
            let remaining_loss = loss.saturating_sub(from_vouch);
            let from_collateral = remaining_loss.min(member.collateral);
            member.collateral = member.collateral.saturating_sub(from_collateral);

            syndication.members.set(idx, member.clone());
            env.storage().persistent().set(
                &DataKey::SyndicationMember(syndication_id, member.address.clone()),
                &member,
            );
        }

        syndication.total_collateral = syndication
            .members
            .iter()
            .fold(0i128, |acc, m| acc.saturating_add(m.collateral));
        syndication.total_vouch_stake = syndication
            .members
            .iter()
            .fold(0i128, |acc, m| acc.saturating_add(m.vouch_stake));
    }

    syndication.status = SyndicationStatus::Defaulted;
    save_syndication(&env, &syndication);

    env.events().publish(
        (symbol_short!("syndicat"), symbol_short!("default")),
        (syndication_id, caller, shortfall),
    );

    Ok(())
}

pub fn get_syndication(env: Env, syndication_id: u64) -> Option<LoanSyndication> {
    env.storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
}

pub fn get_syndication_member(
    env: Env,
    syndication_id: u64,
    member: Address,
) -> Option<SyndicationMember> {
    env.storage()
        .persistent()
        .get(&DataKey::SyndicationMember(syndication_id, member))
}

pub fn get_syndication_config_view(env: Env) -> SyndicationConfig {
    config(&env)
}

pub fn set_syndication_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: SyndicationConfig,
) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);

    if config.min_members == 0
        || config.max_members < config.min_members
        || config.min_approval_percentage > 10_000
        || config.max_loan_amount <= 0
    {
        return Err(ContractError::InvalidSyndicationConfig);
    }

    env.storage()
        .instance()
        .set(&DataKey::SyndicationConfig, &config);

    Ok(())
}

pub fn get_syndication_count(env: Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::SyndicationCounter)
        .unwrap_or(0u64)
}
