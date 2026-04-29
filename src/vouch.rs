extern crate alloc;

use crate::errors::ContractError;
use crate::helpers::{
    has_active_loan, require_allowed_token, require_not_paused, require_positive_amount,
};
use crate::types::{DataKey, VouchRecord, VouchHistoryEntry, WithdrawalRequest, WITHDRAWAL_TIMELOCK_DELAY};
use soroban_sdk::{panic_with_error, symbol_short, token, Address, Env, Vec};

struct VouchConfig {
    whitelist_enabled: bool,
    min_stake: i128,
    vouch_cooldown_secs: u64,
    max_vouchers_per_borrower: u32,
}

impl VouchConfig {
    fn load(env: &Env) -> Self {
        VouchConfig {
            whitelist_enabled: env.storage().instance().get(&DataKey::WhitelistEnabled).unwrap_or(false),
            min_stake: env.storage().instance().get(&DataKey::MinStake).unwrap_or(0),
            vouch_cooldown_secs: env.storage().instance().get(&DataKey::VouchCooldownSecs)
                .unwrap_or(crate::types::DEFAULT_VOUCH_COOLDOWN_SECS),
            max_vouchers_per_borrower: env.storage().instance().get(&DataKey::MaxVouchersPerBorrower)
                .unwrap_or(crate::types::DEFAULT_MAX_VOUCHERS_PER_BORROWER),
        }
    }
}

pub fn vouch(
    env: Env,
    voucher: Address,
    borrower: Address,
    stake: i128,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;
    let cfg = VouchConfig::load(&env);
    do_vouch(&env, &cfg, voucher, borrower, stake, token)
}

fn validate_vouch<'a>(
    env: &'a Env,
    cfg: &VouchConfig,
    voucher: &Address,
    borrower: &Address,
    stake: i128,
    token: &Address,
) -> Result<(token::Client<'a>, Vec<VouchRecord>), ContractError> {
    require_positive_amount(env, stake)?;

    if voucher == borrower {
        return Err(ContractError::SelfVouchNotAllowed);
    }

    if env.storage().persistent().get::<DataKey, bool>(&DataKey::Blacklisted(borrower.clone())).unwrap_or(false) {
        return Err(ContractError::Blacklisted);
    }

    if cfg.whitelist_enabled {
        let is_whitelisted: bool = env.storage().persistent()
            .get(&DataKey::VoucherWhitelist(voucher.clone()))
            .unwrap_or(false);
        if !is_whitelisted {
            return Err(ContractError::VoucherNotWhitelisted);
        }
    }

    let token_client = require_allowed_token(env, token)?;

    if cfg.min_stake > 0 && stake < cfg.min_stake {
        return Err(ContractError::MinStakeNotMet);
    }

    if cfg.vouch_cooldown_secs > 0 {
        let last: u64 = env.storage().persistent()
            .get(&DataKey::LastVouchTimestamp(voucher.clone()))
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        if now < last + cfg.vouch_cooldown_secs {
            return Err(ContractError::VouchCooldownActive);
        }
    }

    let vouches: Vec<VouchRecord> = env.storage().persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(env));

    for v in vouches.iter() {
        if v.voucher == *voucher && v.token == *token {
            return Err(ContractError::DuplicateVouch);
        }
    }

    if vouches.len() >= cfg.max_vouchers_per_borrower {
        return Err(ContractError::MaxVouchersPerBorrowerExceeded);
    }

    if has_active_loan(env, borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    if token_client.balance(voucher) < stake {
        return Err(ContractError::InsufficientVoucherBalance);
    }

    Ok((token_client, vouches))
}

fn commit_vouch(
    env: &Env,
    token_client: &token::Client,
    voucher: Address,
    borrower: Address,
    stake: i128,
    token: Address,
    mut vouches: Vec<VouchRecord>,
) -> Result<(), ContractError> {
    let contract = env.current_contract_address();

    // ✅ SECURITY FIX
    let before = token_client.balance(&contract);
    token_client.transfer(&voucher, &contract, &stake);
    let after = token_client.balance(&contract);

    let received = after.checked_sub(before).ok_or(ContractError::StakeOverflow)?;

    if received != stake {
        return Err(ContractError::InsufficientFunds);
    }

    let mut history: Vec<Address> = env.storage().persistent()
        .get(&DataKey::VoucherHistory(voucher.clone()))
        .unwrap_or(Vec::new(env));
    history.push_back(borrower.clone());

    env.storage().persistent().set(&DataKey::VoucherHistory(voucher.clone()), &history);

    let timestamp = env.ledger().timestamp();

    vouches.push_back(VouchRecord {
        voucher: voucher.clone(),
        stake,
        vouch_timestamp: timestamp,
        token: token.clone(),
        expiry_timestamp: None,
        delegate: None,
    });

    env.storage().persistent().set(&DataKey::Vouches(borrower.clone()), &vouches);

    let mut vouch_history: Vec<VouchHistoryEntry> = env.storage().persistent()
        .get(&DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()))
        .unwrap_or(Vec::new(env));

    vouch_history.push_back(VouchHistoryEntry {
        timestamp,
        modification_type: soroban_sdk::String::from_slice(env, "created"),
        stake_amount: stake,
        delegate: None,
    });

    env.storage().persistent().set(
        &DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()),
        &vouch_history,
    );

    env.storage().persistent().set(
        &DataKey::LastVouchTimestamp(voucher.clone()),
        &timestamp,
    );

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("added")),
        (voucher, borrower, stake, token),
    );

    Ok(())
}

fn do_vouch(
    env: &Env,
    cfg: &VouchConfig,
    voucher: Address,
    borrower: Address,
    stake: i128,
    token: Address,
) -> Result<(), ContractError> {
    let (token_client, vouches) =
        validate_vouch(env, cfg, &voucher, &borrower, stake, &token)?;

    commit_vouch(env, &token_client, voucher, borrower, stake, token, vouches)
}

pub fn increase_stake(
    env: Env,
    voucher: Address,
    borrower: Address,
    additional: i128,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;
    require_positive_amount(&env, additional)?;

    let mut vouches: Vec<VouchRecord> = env.storage().persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .expect("vouch not found");

    let idx = vouches.iter()
        .position(|v| v.voucher == voucher)
        .expect("vouch not found") as u32;

    let mut vouch_rec = vouches.get(idx).unwrap();

    // Issue #599: If there is an active loan, queue a timelocked withdrawal instead of
    // immediately reducing stake. This prevents vouchers from rug-pulling mid-loan.
    if has_active_loan(&env, &borrower) {
        let now = env.ledger().timestamp();
        let unlock_at = now + crate::types::DECREASE_STAKE_TIMELOCK;
        env.storage().persistent().set(
            &DataKey::PendingWithdrawal(voucher.clone(), borrower.clone()),
            &crate::types::WithdrawalRequest {
                voucher: voucher.clone(),
                borrower: borrower.clone(),
                token: vouch_rec.token.clone(),
                requested_at: now,
            },
        );
        env.events().publish(
            (symbol_short!("vouch"), symbol_short!("dec_qued")),
            (voucher, borrower, amount, unlock_at),
        );
        return Ok(());
    }

    let token_client = require_allowed_token(&env, &vouch_rec.token)?;
    let contract = env.current_contract_address();

    // ✅ SECURITY FIX
    let before = token_client.balance(&contract);
    token_client.transfer(&voucher, &contract, &additional);
    let after = token_client.balance(&contract);

    let received = after.checked_sub(before).ok_or(ContractError::StakeOverflow)?;
    if received != additional {
        return Err(ContractError::InsufficientFunds);
    }

    vouch_rec.stake = vouch_rec.stake.checked_add(additional)
        .ok_or(ContractError::StakeOverflow)?;

    vouches.set(idx, vouch_rec.clone());

    env.storage().persistent().set(&DataKey::Vouches(borrower.clone()), &vouches);

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("increased")),
        (voucher, borrower, additional),
    );

    Ok(())
}
