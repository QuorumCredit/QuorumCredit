use crate::errors::ContractError;
use crate::helpers::{
    config, extend_ttl, is_zero_address, require_admin_approval, require_valid_token,
    validate_admin_config,
};
use crate::types::{Config, DataKey, TokenConfig};
use crate::governance;
use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env, Vec};

/// ─────────────────────────────────────────────
/// ADMIN MANAGEMENT
/// ─────────────────────────────────────────────

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
        "threshold invalid after removal"
    );

    env.storage().instance().set(&DataKey::Config, &cfg);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("removed")),
        admin_to_remove,
    );
}

pub fn rotate_admin(
    env: Env,
    admin_signers: Vec<Address>,
    old_admin: Address,
    new_admin: Address,
) {
    require_admin_approval(&env, &admin_signers);

    assert!(old_admin != new_admin, "old and new admin must differ");

    let mut cfg = config(&env);

    assert!(
        !cfg.admins.iter().any(|a| a == new_admin),
        "new admin already exists"
    );

    let idx = cfg
        .admins
        .iter()
        .position(|a| a == old_admin)
        .expect("old admin not found") as u32;

    cfg.admins.set(idx, new_admin.clone());
    env.storage().instance().set(&DataKey::Config, &cfg);

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

    assert!(new_threshold > 0, "threshold must be > 0");
    assert!(
        new_threshold <= cfg.admins.len(),
        "threshold exceeds admin count"
    );

    cfg.admin_threshold = new_threshold;
    env.storage().instance().set(&DataKey::Config, &cfg);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("thresh")),
        new_threshold,
    );
}

/// ─────────────────────────────────────────────
/// PROTOCOL FEE
/// ─────────────────────────────────────────────

pub fn set_protocol_fee(env: Env, admin_signers: Vec<Address>, fee_bps: u32) {
    require_admin_approval(&env, &admin_signers);
    assert!(fee_bps <= 10_000, "fee too high");

    env.storage()
        .instance()
        .set(&DataKey::ProtocolFeeBps, &fee_bps);

    env.events().publish(
        (symbol_short!("admin"), symbol_short!("fee")),
        (admin_signers.get(0).unwrap(), fee_bps),
    );
}

/// ─────────────────────────────────────────────
/// VOUCHER WHITELIST
/// ─────────────────────────────────────────────

pub fn whitelist_voucher(env: Env, admin_signers: Vec<Address>, voucher: Address) {
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

/// ─────────────────────────────────────────────
/// BORROWER WHITELIST
/// ─────────────────────────────────────────────

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

/// ─────────────────────────────────────────────
/// CORE CONFIG (UPDATED FOR DYNAMIC YIELD)
/// ─────────────────────────────────────────────

pub fn set_config(env: Env, admin_signers: Vec<Address>, config: Config) {
    require_admin_approval(&env, &admin_signers);

    validate_admin_config(&env, &config.admins, config.admin_threshold)
        .expect("invalid admin config");

    assert!(config.yield_bps <= 10_000, "invalid yield bps");
    assert!(config.slash_bps <= 10_000, "invalid slash bps");
    assert!(config.min_loan_amount > 0, "invalid min loan");
    assert!(config.loan_duration > 0, "invalid duration");

    // NEW: dynamic yield support validation
    assert!(config.base_yield_bps <= 10_000, "invalid base yield");
    assert!(config.min_yield_bps <= config.max_yield_bps, "invalid yield range");

    env.storage().instance().set(&DataKey::Config, &config);
}

pub fn update_config(env: Env, admin_signers: Vec<Address>, yield_bps: Option<i128>, slash_bps: Option<i128>) {
    require_admin_approval(&env, &admin_signers);

    let mut cfg = config(&env);

    if let Some(y) = yield_bps {
        assert!((0..=10_000).contains(&y), "invalid yield");
        cfg.yield_bps = y;
    }

    if let Some(s) = slash_bps {
        assert!((0..=10_000).contains(&s), "invalid slash");
        cfg.slash_bps = s;
    }

    env.storage().instance().set(&DataKey::Config, &cfg);
}

/// ─────────────────────────────────────────────
/// FEE + TREASURY
/// ─────────────────────────────────────────────

pub fn set_fee_treasury(env: Env, admin_signers: Vec<Address>, treasury: Address) {
    require_admin_approval(&env, &admin_signers);

    env.storage()
        .instance()
        .set(&DataKey::FeeTreasury, &treasury);
}

/// ─────────────────────────────────────────────
/// UPGRADES
/// ─────────────────────────────────────────────

pub fn upgrade(env: Env, admin_signers: Vec<Address>, new_wasm_hash: BytesN<32>) {
    require_admin_approval(&env, &admin_signers);

    env.deployer()
        .update_current_contract_wasm(new_wasm_hash.clone());

    env.events()
        .publish((symbol_short!("upgrade"),), new_wasm_hash);
}

/// ─────────────────────────────────────────────
/// PAUSE CONTROL
/// ─────────────────────────────────────────────

pub fn pause(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);

    env.storage().instance().set(&DataKey::Paused, &true);
}

pub fn unpause(env: Env, admin_signers: Vec<Address>) {
    require_admin_approval(&env, &admin_signers);

    env.storage().instance().set(&DataKey::Paused, &false);
}

/// ─────────────────────────────────────────────
/// BLACKLIST
/// ─────────────────────────────────────────────

pub fn blacklist(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);

    env.storage()
        .persistent()
        .set(&DataKey::Blacklisted(borrower.clone()), &true);

    extend_ttl(&env, &DataKey::Blacklisted(borrower));
}

/// ─────────────────────────────────────────────
/// TOKEN CONFIG
/// ─────────────────────────────────────────────

pub fn set_token_config(
    env: Env,
    admin_signers: Vec<Address>,
    token: Address,
    token_cfg: TokenConfig,
) {
    require_admin_approval(&env, &admin_signers);

    assert!(token_cfg.yield_bps <= 10_000, "invalid yield");
    assert!(token_cfg.slash_bps <= 10_000, "invalid slash");

    env.storage()
        .persistent()
        .set(&DataKey::TokenConfig(token.clone()), &token_cfg);

    extend_ttl(&env, &DataKey::TokenConfig(token.clone()));
}

/// ─────────────────────────────────────────────
/// VIEW FUNCTIONS (UNCHANGED)
/// ─────────────────────────────────────────────

pub fn get_config(env: Env) -> Config {
    config(&env)
}

pub fn is_blacklisted(env: Env, borrower: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Blacklisted(borrower))
        .unwrap_or(false)
}

pub fn get_protocol_fee(env: Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ProtocolFeeBps)
        .unwrap_or(0)
}
