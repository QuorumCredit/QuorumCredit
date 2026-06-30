use crate::errors::ContractError;
use crate::helpers::{config, require_admin_approval, require_not_paused};
use crate::types::{CooldownBypassRequest, DataKey};
use soroban_sdk::{symbol_short, Address, Env, Vec};

/// Request a cooldown bypass for emergency cases (e.g., imminent loan default).
/// Only an active voucher for the given borrower can make this request.
/// Stores the request for admin voting; requires 2/3 admin approval to activate.
pub fn request_cooldown_bypass(
    env: Env,
    voucher: Address,
    borrower: Address,
    reason: soroban_sdk::String,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    // Verify the voucher has an active vouch for this borrower
    let vouches: Vec<crate::types::VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));
    let has_vouch = vouches.iter().any(|v| v.voucher == voucher);
    if !has_vouch {
        return Err(ContractError::VoucherNotFound);
    }

    // Check no existing bypass request for this pair
    if env
        .storage()
        .persistent()
        .has(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()))
    {
        return Err(ContractError::CooldownBypassAlreadyRequested);
    }

    let admins = config(&env).admins;
    let total_admins = admins.len();

    let request = CooldownBypassRequest {
        voucher: voucher.clone(),
        borrower: borrower.clone(),
        reason,
        requested_at: env.ledger().timestamp(),
        approvers: Vec::new(&env),
        approved: false,
    };

    env.storage()
        .persistent()
        .set(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()), &request);

    env.events().publish(
        (symbol_short!("bypass"), symbol_short!("requested")),
        (voucher, borrower, total_admins),
    );

    Ok(())
}

/// Vote on a cooldown bypass request.
/// Each admin can vote once. When 2/3 of total admins have approved, the bypass is granted.
/// Once approved, `has_cooldown_bypass` will return true, allowing the voucher to vouch
/// despite the cooldown.
pub fn vote_bypass(
    env: Env,
    approver: Address,
    voucher: Address,
    borrower: Address,
    approve: bool,
) -> Result<(), ContractError> {
    approver.require_auth();
    require_not_paused(&env)?;

    // Verify caller is an admin
    let cfg = config(&env);
    if !cfg.admins.iter().any(|a| a == approver) {
        return Err(ContractError::UnauthorizedCaller);
    }

    // Load the bypass request
    let mut request: CooldownBypassRequest = env
        .storage()
        .persistent()
        .get(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()))
        .ok_or(ContractError::CooldownBypassNotFound)?;

    // Prevent double-vote
    if request.approvers.iter().any(|a| a == approver) {
        return Err(ContractError::AlreadyVoted);
    }

    if request.approved {
        return Err(ContractError::CooldownBypassAlreadyApproved);
    }

    if !approve {
        // Rejection — just record it and return
        request.approvers.push_back(approver.clone());
        env.storage()
            .persistent()
            .set(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()), &request);
        env.events().publish(
            (symbol_short!("bypass"), symbol_short!("rejected")),
            (approver, voucher, borrower),
        );
        return Ok(());
    }

    // Record the approval
    request.approvers.push_back(approver.clone());

    // Check 2/3 threshold: approve_count * 3 >= total_admins * 2
    let total_admins = cfg.admins.len();
    let approve_count = request.approvers.len();
    let required = (total_admins * 2 + 2) / 3; // ceil(2/3)

    if approve_count >= required {
        request.approved = true;
        env.storage()
            .persistent()
            .set(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()), &request);
        env.events().publish(
            (symbol_short!("bypass"), symbol_short!("approved")),
            (voucher, borrower, approve_count, required),
        );
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()), &request);
        env.events().publish(
            (symbol_short!("bypass"), symbol_short!("voted")),
            (approver, voucher, borrower, approve_count, required),
        );
    }

    Ok(())
}

/// Check if there's an active (approved) cooldown bypass for a given (voucher, borrower) pair.
pub fn has_cooldown_bypass(env: &Env, voucher: &Address, borrower: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<DataKey, CooldownBypassRequest>(&DataKey::CooldownBypass(
            borrower.clone(),
            voucher.clone(),
        ))
        .map(|r| r.approved)
        .unwrap_or(false)
}

/// Get the current bypass request for a (borrower, voucher) pair.
pub fn get_cooldown_bypass(
    env: Env,
    borrower: Address,
    voucher: Address,
) -> Option<CooldownBypassRequest> {
    env.storage()
        .persistent()
        .get(&DataKey::CooldownBypass(borrower, voucher))
}

/// Clear a cooldown bypass request (admin only).
pub fn clear_cooldown_bypass(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
    voucher: Address,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if !env
        .storage()
        .persistent()
        .has(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()))
    {
        return Err(ContractError::CooldownBypassNotFound);
    }

    env.storage()
        .persistent()
        .remove(&DataKey::CooldownBypass(borrower.clone(), voucher.clone()));

    env.events().publish(
        (symbol_short!("bypass"), symbol_short!("cleared")),
        (voucher, borrower),
    );

    Ok(())
}
