use crate::errors::ContractError;
use crate::helpers::{
    config, extend_ttl, is_zero_address, require_admin_approval, require_valid_token,
    validate_admin_config,
};
use crate::types::{Config, DataKey, TokenConfig};
use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env, Vec};

pub fn add_admin(env: Env, admin_signers: Vec<Address>, new_admin: Address) {
    require_admin_approval(&env, &admin_signers);

    let mut cfg = config(&env);

    assert!(
        !cfg.admins.iter().any(|a| a == new_admin),
        "address is already an admin"
    );

    cfg.admins.push_back(new_admin.clone());
    env.storage().instance().set(&DataKey::Config, &cfg);

    log_admin_action(&env, &admin_signers.get(0).unwrap(), "add_admin");

    env.events()
        .publish((symbol_short!("admin"), symbol_short!("added")), new_admin);
}

pub fn remove_admin(env: Env, admin_signers: Vec<Address>, admin_to_remove: Address) {
    require_admin_approval(&env, &admin_signers);

    let mut cfg = config(&env);

    let idx = cfg
        .admins
        .iter()
        .position(|a| a == admin_to_remove)
        .expect("address is not an admin") as u32;

    cfg.admins.remove(idx);

    assert!(!cfg.admins.is_empty(), "cannot remove the last admin");
    assert!(
        cfg.admin_threshold <= cfg.admins.len(),
        "removal would make threshold unsatisfiable"
    );

    env.storage().instance().set(&DataKey::Config, &cfg);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("removed")),
        admin_to_remove,
    );
}

pub fn rotate_admin(env: Env, admin_signers: Vec<Address>, old_admin: Address, new_admin: Address) {
    require_admin_approval(&env, &admin_signers);

    assert!(old_admin != new_admin, "old and new admin must differ");

    let mut cfg = config(&env);

    assert!(
        !cfg.admins.iter().any(|a| a == new_admin),
        "new admin is already in the admin set"
    );

    let idx = cfg
        .admins
        .iter()
        .position(|a| a == old_admin)
        .expect("old admin not found") as u32;

    cfg.admins.set(idx, new_admin.clone());
    env.storage().instance().set(&DataKey::Config, &cfg);

    // Clear expiry for new admin
    env.storage()
        .persistent()
        .remove(&DataKey::AdminKeyExpiry(new_admin.clone()));

    log_admin_action(&env, &admin_signers.get(0).unwrap(), "rotate_admin");

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("rotated")),
        (old_admin, new_admin),
    );
}

pub fn set_admin_threshold(env: Env, admin_signers: Vec<Address>, new_threshold: u32) {
    require_admin_approval(&env, &admin_signers);

    let mut cfg = config(&env);

    assert!(new_threshold > 0, "threshold must be greater than zero");
    assert!(
        new_threshold <= cfg.admins.len(),
        "threshold cannot exceed admin count"
    );

    cfg.admin_threshold = new_threshold;
    env.storage().instance().set(&DataKey::Config, &cfg);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("thresh")),
        new_threshold,
    );
}

pub fn set_protocol_fee(env: Env, admin_signers: Vec<Address>, fee_bps: u32) {
    require_admin_approval(&env, &admin_signers);
    assert!(fee_bps <= 10_000, "fee_bps must not exceed 10000");
    env.storage()
        .instance()
        .set(&DataKey::ProtocolFeeBps, &fee_bps);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("fee")),
        (
            admin_signers.get(0).unwrap(),
            fee_bps,
            env.ledger().timestamp(),
        ),
    );
}

pub fn whitelist_voucher(env: Env, admin_signers: Vec<Address>, voucher: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .persistent()
        .set(&DataKey::VoucherWhitelist(voucher.clone()), &true);
    extend_ttl(&env, &DataKey::VoucherWhitelist(voucher));
}

pub fn add_voucher_to_whitelist(env: Env, admin_signers: Vec<Address>, voucher: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .persistent()
        .set(&DataKey::VoucherWhitelist(voucher.clone()), &true);
    extend_ttl(&env, &DataKey::VoucherWhitelist(voucher));
}

pub fn remove_voucher_from_whitelist(env: Env, admin_signers: Vec<Address>, voucher: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .persistent()
        .remove(&DataKey::VoucherWhitelist(voucher));
}

pub fn enable_voucher_whitelist(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .instance()
        .set(&DataKey::VoucherWhitelistEnabled, &true);
}

pub fn disable_voucher_whitelist(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .instance()
        .set(&DataKey::VoucherWhitelistEnabled, &false);
}

pub fn is_voucher_whitelist_enabled(env: Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::VoucherWhitelistEnabled)
        .unwrap_or(false)
}

pub fn add_borrower_to_whitelist(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .persistent()
        .set(&DataKey::BorrowerWhitelist(borrower.clone()), &true);
    extend_ttl(&env, &DataKey::BorrowerWhitelist(borrower));
}

pub fn remove_borrower_from_whitelist(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .persistent()
        .remove(&DataKey::BorrowerWhitelist(borrower));
}

pub fn enable_borrower_whitelist(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .instance()
        .set(&DataKey::BorrowerWhitelistEnabled, &true);
}

pub fn disable_borrower_whitelist(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .instance()
        .set(&DataKey::BorrowerWhitelistEnabled, &false);
}

pub fn is_borrower_whitelisted(env: Env, borrower: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::BorrowerWhitelist(borrower))
        .unwrap_or(false)
}

pub fn is_borrower_whitelist_enabled(env: Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::BorrowerWhitelistEnabled)
        .unwrap_or(false)
}
pub fn set_fee_treasury(env: Env, admin_signers: Vec<Address>, treasury: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .instance()
        .set(&DataKey::FeeTreasury, &treasury);
}

pub fn upgrade(env: Env, admin_signers: Vec<Address>, new_wasm_hash: BytesN<32>) {
    require_admin_approval(&env, &admin_signers);
    env.deployer()
        .update_current_contract_wasm(new_wasm_hash.clone());
    env.events()
        .publish((symbol_short!("upgrade"),), new_wasm_hash);
}

pub fn pause(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);
    env.storage().instance().set(&DataKey::Paused, &true);
    log_admin_action(&env, &admin_signers.get(0).unwrap(), "pause");
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("pause")),
        (admin_signers.get(0).unwrap(), env.ledger().timestamp()),
    );
}

pub fn unpause(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);
    env.storage().instance().set(&DataKey::Paused, &false);
    log_admin_action(&env, &admin_signers.get(0).unwrap(), "unpause");
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("unpause")),
        (admin_signers.get(0).unwrap(), env.ledger().timestamp()),
    );
}

pub fn blacklist(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .persistent()
        .set(&DataKey::Blacklisted(borrower.clone()), &true);
    extend_ttl(&env, &DataKey::Blacklisted(borrower));
}

pub fn set_config(env: Env, admin_signers: Vec<Address>, config: Config) {
    require_admin_approval(&env, &admin_signers);
    validate_admin_config(&env, &config.admins, config.admin_threshold)
        .expect("invalid admin config");
    if config.yield_bps < 0 || config.yield_bps > 10_000 {
        panic_with_error!(&env, ContractError::InvalidBps);
    }
    assert!(
        config.slash_bps > 0 && config.slash_bps <= 10_000,
        "slash_bps must be 1-10000"
    );
    assert!(
        config.max_vouchers > 0,
        "max_vouchers must be greater than zero"
    );
    assert!(
        config.min_loan_amount > 0,
        "min_loan_amount must be greater than zero"
    );
    assert!(
        config.loan_duration > 0,
        "loan_duration must be greater than zero"
    );
    assert!(
        config.grace_period <= config.loan_duration,
        "grace_period must not exceed loan_duration"
    );
    assert!(
        config.max_loan_to_stake_ratio > 0,
        "max_loan_to_stake_ratio must be greater than zero"
    );
    env.storage().instance().set(&DataKey::Config, &config);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("config")),
        (admin_signers.get(0).unwrap(), env.ledger().timestamp()),
    );
}

pub fn update_config(
    env: Env,
    admin_signers: Vec<Address>,
    yield_bps: Option<i128>,
    slash_bps: Option<i128>,
) {
    require_admin_approval(&env, &admin_signers);

    let mut cfg = config(&env);

    if let Some(new_yield_bps) = yield_bps {
        if !(0..=10_000).contains(&new_yield_bps) {
            panic_with_error!(&env, ContractError::InvalidBps);
        }
        cfg.yield_bps = new_yield_bps;
    }

    if let Some(new_slash_bps) = slash_bps {
        if !(0..=10_000).contains(&new_slash_bps) {
            panic_with_error!(&env, ContractError::InvalidBps);
        }
        cfg.slash_bps = new_slash_bps;
    }

    env.storage().instance().set(&DataKey::Config, &cfg);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("upconfig")),
        (admin_signers.get(0).unwrap(), env.ledger().timestamp()),
    );
}

pub fn set_reputation_nft(env: Env, admin_signers: Vec<Address>, nft_contract: Address) {
    require_admin_approval(&env, &admin_signers);
    env.storage()
        .instance()
        .set(&DataKey::ReputationNft, &nft_contract);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("repnft")),
        (
            admin_signers.get(0).unwrap(),
            nft_contract,
            env.ledger().timestamp(),
        ),
    );
}

pub fn set_min_stake(env: Env, admin_signers: Vec<Address>, amount: i128) {
    require_admin_approval(&env, &admin_signers);
    assert!(amount >= 0, "min stake cannot be negative");
    env.storage().instance().set(&DataKey::MinStake, &amount);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("minstake")),
        (
            admin_signers.get(0).unwrap(),
            amount,
            env.ledger().timestamp(),
        ),
    );
}

pub fn set_min_loan_amount(
    env: Env,
    admin_signers: Vec<Address>,
    amount: i128,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    let mut cfg = config(&env);
    cfg.min_loan_amount = amount;
    env.storage().instance().set(&DataKey::Config, &cfg);
    Ok(())
}
pub fn set_max_loan_amount(env: Env, admin_signers: Vec<Address>, amount: i128) {
    require_admin_approval(&env, &admin_signers);
    assert!(amount >= 0, "max loan amount cannot be negative");
    env.storage()
        .instance()
        .set(&DataKey::MaxLoanAmount, &amount);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("maxloan")),
        (
            admin_signers.get(0).unwrap(),
            amount,
            env.ledger().timestamp(),
        ),
    );
}

pub fn set_min_vouchers(env: Env, admin_signers: Vec<Address>, count: u32) {
    require_admin_approval(&env, &admin_signers);
    env.storage().instance().set(&DataKey::MinVouchers, &count);
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("minvchrs")),
        (
            admin_signers.get(0).unwrap(),
            count,
            env.ledger().timestamp(),
        ),
    );
}

pub fn set_max_loan_to_stake_ratio(env: Env, admin_signers: Vec<Address>, ratio: u32) {
    require_admin_approval(&env, &admin_signers);
    assert!(
        ratio > 0,
        "max_loan_to_stake_ratio must be greater than zero"
    );
    let mut cfg = config(&env);
    cfg.max_loan_to_stake_ratio = ratio;
    env.storage().instance().set(&DataKey::Config, &cfg);
}

pub fn set_grace_period(env: Env, admin_signers: Vec<Address>, period: u64) {
    require_admin_approval(&env, &admin_signers);
    let cfg = config(&env);
    if period > cfg.loan_duration {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }
    let mut cfg = cfg;
    cfg.grace_period = period;
    env.storage().instance().set(&DataKey::Config, &cfg);
}

// View functions
pub fn get_protocol_fee(env: Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ProtocolFeeBps)
        .unwrap_or(0)
}

pub fn get_fee_treasury(env: Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::FeeTreasury)
}

pub fn is_blacklisted(env: Env, borrower: Address) -> bool {
    env.storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Blacklisted(borrower))
        .unwrap_or(false)
}

pub fn get_min_stake(env: Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MinStake)
        .unwrap_or(0)
}

pub fn get_max_loan_amount(env: Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MaxLoanAmount)
        .unwrap_or(0)
}

pub fn get_min_vouchers(env: Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::MinVouchers)
        .unwrap_or(0)
}

pub fn get_max_loan_to_stake_ratio(env: Env) -> u32 {
    config(&env).max_loan_to_stake_ratio
}

pub fn get_config(env: Env) -> Config {
    config(&env)
}

pub fn add_allowed_token(
    env: Env,
    admin_signers: Vec<Address>,
    token: Address,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    require_valid_token(&env, &token).unwrap_or_else(|e| panic_with_error!(&env, e));
    let mut cfg = config(&env);
    if cfg.allowed_tokens.iter().any(|t| t == token) || token == cfg.token {
        return Err(ContractError::DuplicateToken);
    }
    cfg.allowed_tokens.push_back(token);
    env.storage().instance().set(&DataKey::Config, &cfg);
    Ok(())
}

pub fn remove_allowed_token(env: Env, admin_signers: Vec<Address>, token: Address) {
    require_admin_approval(&env, &admin_signers);
    let mut cfg = config(&env);
    let idx = cfg
        .allowed_tokens
        .iter()
        .position(|t| t == token)
        .expect("token not in allowed list") as u32;
    cfg.allowed_tokens.remove(idx);
    env.storage().instance().set(&DataKey::Config, &cfg);
}

pub fn set_token_config(
    env: Env,
    admin_signers: Vec<Address>,
    token: Address,
    token_cfg: TokenConfig,
) {
    require_admin_approval(&env, &admin_signers);
    assert!(
        token_cfg.yield_bps >= 0 && token_cfg.yield_bps <= 10_000,
        "yield_bps must be 0-10000"
    );
    assert!(
        token_cfg.slash_bps > 0 && token_cfg.slash_bps <= 10_000,
        "slash_bps must be 1-10000"
    );
    env.storage()
        .persistent()
        .set(&DataKey::TokenConfig(token.clone()), &token_cfg);
    extend_ttl(&env, &DataKey::TokenConfig(token.clone()));
    env.events().publish(
        (symbol_short!("admin"), symbol_short!("tkcfg")),
        (admin_signers.get(0).unwrap(), token),
    );
}

pub fn get_token_config(env: Env, token: Address) -> Option<TokenConfig> {
    env.storage().persistent().get(&DataKey::TokenConfig(token))
}

pub fn get_admins(env: Env) -> Vec<Address> {
    config(&env).admins
}

pub fn get_admin_threshold(env: Env) -> u32 {
    config(&env).admin_threshold
}

pub fn is_whitelisted(env: Env, voucher: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::VoucherWhitelist(voucher))
        .unwrap_or(false)
}

pub fn propose_admin(
    env: Env,
    admin_signers: Vec<Address>,
    new_admin: Address,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if is_zero_address(&env, &new_admin) {
        return Err(ContractError::ZeroAddress);
    }

    env.storage()
        .instance()
        .set(&DataKey::PendingAdmin, &new_admin);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("proposed")),
        new_admin,
    );

    Ok(())
}

pub fn accept_admin(env: Env) -> Result<(), ContractError> {
    let new_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::PendingAdmin)
        .ok_or(ContractError::UnauthorizedCaller)?;

    new_admin.require_auth();

    let mut cfg = config(&env);
    cfg.admins.push_back(new_admin.clone());
    env.storage().instance().set(&DataKey::Config, &cfg);

    // Clear the pending admin
    env.storage().instance().remove(&DataKey::PendingAdmin);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("accepted")),
        new_admin,
    );

    Ok(())
}


// ── Audit Logging ─────────────────────────────────────────────────────────────

pub fn log_admin_action(env: &Env, admin: &Address, action: &str) {
    let mut log: Vec<crate::types::AdminAuditEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::AdminAuditLog)
        .unwrap_or(Vec::new(env));

    log.push_back(crate::types::AdminAuditEntry {
        admin: admin.clone(),
        action: soroban_sdk::String::from_slice(env, action),
        timestamp: env.ledger().timestamp(),
    });

    env.storage()
        .persistent()
        .set(&DataKey::AdminAuditLog, &log);
}

pub fn get_admin_audit_log(env: Env) -> Vec<crate::types::AdminAuditEntry> {
    env.storage()
        .persistent()
        .get(&DataKey::AdminAuditLog)
        .unwrap_or(Vec::new(&env))
}

// ── Admin Key Expiry ──────────────────────────────────────────────────────────

pub fn set_admin_key_expiry(env: Env, admin_signers: Vec<Address>, admin: Address, expiry: u64) {
    require_admin_approval(&env, &admin_signers);

    let cfg = config(&env);
    assert!(
        cfg.admins.iter().any(|a| a == admin),
        "address is not an admin"
    );

    env.storage()
        .persistent()
        .set(&DataKey::AdminKeyExpiry(admin.clone()), &expiry);

    log_admin_action(&env, &admin_signers.get(0).unwrap(), "set_admin_key_expiry");

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("expiry")),
        (admin, expiry),
    );
}

pub fn get_admin_key_expiry(env: Env, admin: Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::AdminKeyExpiry(admin))
        .unwrap_or(0)
}

pub fn is_admin_key_expired(env: &Env, admin: &Address) -> bool {
    let expiry: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::AdminKeyExpiry(admin.clone()))
        .unwrap_or(0);

    expiry > 0 && env.ledger().timestamp() > expiry
}

// ── Admin Action Timelock ────────────────────────────────────────────────────

pub fn queue_admin_action(
    env: Env,
    admin_signers: Vec<Address>,
    action: crate::types::AdminTimelockAction,
    delay_secs: u64,
) -> Result<u64, ContractError> {
    require_admin_approval(&env, &admin_signers);

    let action_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::AdminActionTimelockCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .expect("action ID overflow");

    let eta = env.ledger().timestamp() + delay_secs;

    let timelock = crate::types::AdminTimelock {
        id: action_id,
        action,
        proposer: admin_signers.get(0).unwrap().clone(),
        eta,
        executed: false,
        cancelled: false,
    };

    env.storage()
        .instance()
        .set(&DataKey::AdminActionTimelock(action_id), &timelock);
    env.storage()
        .instance()
        .set(&DataKey::AdminActionTimelockCounter, &action_id);

    log_admin_action(&env, &admin_signers.get(0).unwrap(), "queue_admin_action");

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("queued")),
        (action_id, eta),
    );

    Ok(action_id)
}

pub fn execute_admin_action(env: Env, action_id: u64) -> Result<(), ContractError> {
    let mut timelock: crate::types::AdminTimelock = env
        .storage()
        .instance()
        .get(&DataKey::AdminActionTimelock(action_id))
        .ok_or(ContractError::TimelockNotFound)?;

    if timelock.executed {
        return Err(ContractError::SlashAlreadyExecuted);
    }
    if timelock.cancelled {
        return Err(ContractError::TimelockNotFound);
    }

    if env.ledger().timestamp() < timelock.eta {
        return Err(ContractError::TimelockNotReady);
    }

    const TIMELOCK_EXPIRY: u64 = 72 * 60 * 60;
    if env.ledger().timestamp() > timelock.eta + TIMELOCK_EXPIRY {
        return Err(ContractError::TimelockExpired);
    }

    timelock.executed = true;
    env.storage()
        .instance()
        .set(&DataKey::AdminActionTimelock(action_id), &timelock);

    match &timelock.action {
        crate::types::AdminTimelockAction::Pause => {
            env.storage().instance().set(&DataKey::Paused, &true);
        }
        crate::types::AdminTimelockAction::Unpause => {
            env.storage().instance().set(&DataKey::Paused, &false);
        }
        crate::types::AdminTimelockAction::UpdateConfig(cfg) => {
            env.storage().instance().set(&DataKey::Config, cfg);
        }
        crate::types::AdminTimelockAction::SetAdminThreshold(threshold) => {
            let mut cfg = config(&env);
            cfg.admin_threshold = *threshold;
            env.storage().instance().set(&DataKey::Config, &cfg);
        }
    }

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("executed")),
        action_id,
    );

    Ok(())
}

pub fn cancel_admin_action(env: Env, caller: Address, action_id: u64) -> Result<(), ContractError> {
    caller.require_auth();

    let mut timelock: crate::types::AdminTimelock = env
        .storage()
        .instance()
        .get(&DataKey::AdminActionTimelock(action_id))
        .ok_or(ContractError::TimelockNotFound)?;

    if caller != timelock.proposer {
        return Err(ContractError::UnauthorizedCaller);
    }

    if timelock.executed || timelock.cancelled {
        return Err(ContractError::SlashAlreadyExecuted);
    }

    timelock.cancelled = true;
    env.storage()
        .instance()
        .set(&DataKey::AdminActionTimelock(action_id), &timelock);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("cancelled")),
        action_id,
    );

    Ok(())
}

pub fn get_admin_timelock(env: Env, action_id: u64) -> Option<crate::types::AdminTimelock> {
    env.storage()
        .instance()
        .get(&DataKey::AdminActionTimelock(action_id))
}

pub fn set_governance_token(env: Env, admin_signers: Vec<Address>, token: Address) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    require_valid_token(&env, &token)?;

    governance::set_governance_token(&env, token.clone());
    log_admin_action(&env, &admin_signers.get(0).unwrap(), "set_governance_token");

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("gov_token")),
        token,
    );

    Ok(())
}
