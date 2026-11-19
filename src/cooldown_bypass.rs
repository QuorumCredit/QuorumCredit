/// Issue #1056 / #1372: Vouch Cooldown Bypass for Emergency Cases
///
/// See docs/vouch-cooldown-bypass-1056.md for the full design. A voucher who needs
/// to re-vouch or increase stake before `vouch_cooldown_secs` has elapsed can
/// request an admin-voted emergency waiver; once 2/3 of admins approve, the
/// cooldown is skipped for that (voucher, borrower) pair.
use soroban_sdk::{Address, Env, String, Vec};
use crate::errors::ContractError;
use crate::types::{
    CooldownBypassRequest, DataKey, VouchRecord, COOLDOWN_BYPASS_VALIDITY_SECS,
    MAX_COOLDOWN_BYPASS_REASON_LEN,
};

/// Number of admin approvals required to reach the 2/3 threshold for `total_admins`.
fn required_approvals(total_admins: u32) -> u32 {
    if total_admins == 0 {
        return 0;
    }
    (total_admins * 2 + 2) / 3
}

/// Request an emergency cooldown bypass for `(voucher, borrower)`.
///
/// Requires `voucher` auth and an existing active vouch from `voucher` for `borrower`.
/// Rejects a duplicate request for the same pair.
pub fn request_cooldown_bypass(
    env: Env,
    voucher: Address,
    borrower: Address,
    reason: String,
) -> Result<(), ContractError> {
    voucher.require_auth();

    if reason.len() > MAX_COOLDOWN_BYPASS_REASON_LEN {
        return Err(ContractError::CooldownBypassReasonTooLong);
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));
    if !vouches.iter().any(|v| v.voucher == voucher) {
        return Err(ContractError::VoucherNotFound);
    }

    let now = env.ledger().timestamp();
    let key = DataKey::CooldownBypass(borrower.clone(), voucher.clone());
    if let Some(existing) = env
        .storage()
        .persistent()
        .get::<DataKey, CooldownBypassRequest>(&key)
    {
        // An expired record no longer blocks a fresh request.
        if now <= existing.expires_at {
            return Err(ContractError::CooldownBypassAlreadyRequested);
        }
    }

    let request = CooldownBypassRequest {
        voucher: voucher.clone(),
        borrower: borrower.clone(),
        reason,
        requested_at: now,
        approvers: Vec::new(&env),
        approved: false,
        expires_at: now + COOLDOWN_BYPASS_VALIDITY_SECS,
    };
    env.storage().persistent().set(&key, &request);

    env.events().publish(
        ("cooldown_bypass", "requested"),
        (borrower, voucher),
    );

    Ok(())
}

/// Admin vote on a pending cooldown bypass request. Once approve-votes reach the
/// 2/3 admin threshold, the request is marked `approved`.
pub fn vote_bypass(
    env: Env,
    approver: Address,
    voucher: Address,
    borrower: Address,
    approve: bool,
) -> Result<(), ContractError> {
    approver.require_auth();

    let cfg = crate::helpers::config(&env);
    if !cfg.admins.iter().any(|a| a == approver) {
        return Err(ContractError::UnauthorizedCaller);
    }

    let key = DataKey::CooldownBypass(borrower.clone(), voucher.clone());
    let mut request: CooldownBypassRequest = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::CooldownBypassNotFound)?;

    if request.approved {
        return Err(ContractError::CooldownBypassAlreadyApproved);
    }

    let now = env.ledger().timestamp();
    if now > request.expires_at {
        return Err(ContractError::CooldownBypassExpired);
    }

    if !approve {
        env.events().publish(
            ("cooldown_bypass", "rejected"),
            (borrower, voucher, approver),
        );
        return Ok(());
    }

    if request.approvers.iter().any(|a| a == approver) {
        return Err(ContractError::AlreadyVoted);
    }
    request.approvers.push_back(approver.clone());

    if request.approvers.len() >= required_approvals(cfg.admins.len()) {
        request.approved = true;
        // The validity window restarts at approval so the voucher has a full window to use it.
        request.expires_at = now + COOLDOWN_BYPASS_VALIDITY_SECS;
    }

    env.storage().persistent().set(&key, &request);

    env.events().publish(
        ("cooldown_bypass", "voted"),
        (borrower, voucher, approver, request.approved),
    );

    Ok(())
}

/// Check if a voucher has a usable (approved, unexpired) cooldown bypass for a given borrower.
pub fn has_cooldown_bypass(env: &Env, voucher: &Address, borrower: &Address) -> bool {
    let key = DataKey::CooldownBypass(borrower.clone(), voucher.clone());
    let now = env.ledger().timestamp();
    env.storage()
        .persistent()
        .get::<DataKey, CooldownBypassRequest>(&key)
        .map(|r| r.approved && now <= r.expires_at)
        .unwrap_or(false)
}

/// Consume an approved cooldown bypass after it has been used to skip the cooldown.
///
/// Each approval grants exactly one waiver: the record is removed so a second vouch
/// inside the cooldown window requires a new request and approval. Callers must only
/// invoke this after `has_cooldown_bypass` returned `true`.
pub fn consume_cooldown_bypass(env: &Env, voucher: &Address, borrower: &Address) {
    let key = DataKey::CooldownBypass(borrower.clone(), voucher.clone());
    env.storage().persistent().remove(&key);
    env.events().publish(
        ("cooldown_bypass", "consumed"),
        (borrower.clone(), voucher.clone()),
    );
}

/// Fetch the raw bypass request record for `(voucher, borrower)`, if any.
pub fn get_cooldown_bypass_request(
    env: Env,
    voucher: Address,
    borrower: Address,
) -> Option<CooldownBypassRequest> {
    let key = DataKey::CooldownBypass(borrower, voucher);
    env.storage().persistent().get(&key)
}

/// Admin cleanup: remove a bypass record (e.g. after the emergency has passed).
pub fn clear_cooldown_bypass(
    env: Env,
    admin_signers: Vec<Address>,
    voucher: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);

    let key = DataKey::CooldownBypass(borrower.clone(), voucher.clone());
    if env.storage().persistent().get::<DataKey, CooldownBypassRequest>(&key).is_none() {
        return Err(ContractError::CooldownBypassNotFound);
    }
    env.storage().persistent().remove(&key);

    env.events().publish(
        ("cooldown_bypass", "cleared"),
        (borrower, voucher),
    );

    Ok(())
}
