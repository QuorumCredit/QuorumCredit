extern crate alloc;

use crate::errors::ContractError;
use crate::helpers::{
    has_active_loan, require_allowed_token, require_not_paused, require_positive_amount,
};
use crate::types::{DataKey, VouchRecord, VouchHistoryEntry};
use soroban_sdk::{panic_with_error, symbol_short, token, Address, Env, Vec};

/// Cached instance-storage values read once per vouch call to reduce storage reads (#501).
struct VouchConfig {
    whitelist_enabled: bool,
    min_stake: i128,
    vouch_cooldown_secs: u64,
    max_vouchers_per_borrower: u32,
}

impl VouchConfig {
    /// Read all instance-level vouch config in a single pass.
    fn load(env: &Env) -> Self {
        VouchConfig {
            whitelist_enabled: env
                .storage()
                .instance()
                .get(&DataKey::WhitelistEnabled)
                .unwrap_or(false),
            min_stake: env
                .storage()
                .instance()
                .get(&DataKey::MinStake)
                .unwrap_or(0),
            vouch_cooldown_secs: env
                .storage()
                .instance()
                .get(&DataKey::VouchCooldownSecs)
                .unwrap_or(crate::types::DEFAULT_VOUCH_COOLDOWN_SECS),
            max_vouchers_per_borrower: env
                .storage()
                .instance()
                .get(&DataKey::MaxVouchersPerBorrower)
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
    // Cache config once for this call (#501).
    let cfg = VouchConfig::load(&env);
    do_vouch(&env, &cfg, voucher, borrower, stake, token)
}

/// Pure validation: checks all preconditions without mutating state or transferring tokens.
/// Returns the token client and current vouches so the commit phase can reuse them.
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

    if env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Blacklisted(borrower.clone()))
        .unwrap_or(false)
    {
        return Err(ContractError::Blacklisted);
    }

    // Use cached whitelist_enabled (#501).
    if cfg.whitelist_enabled {
        let is_whitelisted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::VoucherWhitelist(voucher.clone()))
            .unwrap_or(false);
        if !is_whitelisted {
            return Err(ContractError::VoucherNotWhitelisted);
        }
    }

    let token_client = require_allowed_token(env, token)?;

    // Use cached min_stake (#501).
    if cfg.min_stake > 0 && stake < cfg.min_stake {
        return Err(ContractError::MinStakeNotMet);
    }

    // Use cached vouch_cooldown_secs (#501).
    if cfg.vouch_cooldown_secs > 0 {
        let last_vouch_time: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LastVouchTimestamp(voucher.clone()))
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        if now < last_vouch_time + cfg.vouch_cooldown_secs {
            return Err(ContractError::VouchCooldownActive);
        }
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(env));

    for v in vouches.iter() {
        if v.voucher == *voucher && v.token == *token {
            return Err(ContractError::DuplicateVouch);
        }
    }

    // Use cached max_vouchers_per_borrower (#501).
    if vouches.len() >= cfg.max_vouchers_per_borrower {
        return Err(ContractError::MaxVouchersPerBorrowerExceeded);
    }

    if has_active_loan(env, borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let voucher_balance = token_client.balance(voucher);
    if voucher_balance < stake {
        return Err(ContractError::InsufficientVoucherBalance);
    }

    Ok((token_client, vouches))
}

/// Commit phase: mutates state and transfers tokens. Only called after all validations pass.
fn commit_vouch(
    env: &Env,
    token_client: &token::Client,
    voucher: Address,
    borrower: Address,
    stake: i128,
    token: Address,
    mut vouches: Vec<VouchRecord>,
) {
    token_client.transfer(&voucher, &env.current_contract_address(), &stake);

    let mut history: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherHistory(voucher.clone()))
        .unwrap_or(Vec::new(env));
    history.push_back(borrower.clone());
    env.storage()
        .persistent()
        .set(&DataKey::VoucherHistory(voucher.clone()), &history);

    let timestamp = env.ledger().timestamp();
    vouches.push_back(VouchRecord {
        voucher: voucher.clone(),
        stake,
        vouch_timestamp: timestamp,
        token: token.clone(),
        expiry_timestamp: None,
        delegate: None,
    });
    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    // Issue #534: Record vouch creation in modification history
    let mut vouch_history: Vec<VouchHistoryEntry> = env
        .storage()
        .persistent()
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
    commit_vouch(env, &token_client, voucher, borrower, stake, token, vouches);
    Ok(())
}

/// Atomically vouch for multiple borrowers in a single transaction (#530).
///
/// Guarantees all-or-nothing semantics: all vouches are validated before any
/// state is mutated or tokens are transferred. If any vouch fails validation,
/// the entire batch is rejected and no state changes occur.
pub fn batch_vouch(
    env: Env,
    voucher: Address,
    borrowers: Vec<Address>,
    stakes: Vec<i128>,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    if borrowers.len() != stakes.len() {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }
    if borrowers.is_empty() {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }

    // Cache config once for the entire batch (#501).
    let cfg = VouchConfig::load(&env);

    // ── Phase 1: Validate all vouches before committing any ──────────────────
    // Collect (token_client, vouches) for each entry so Phase 2 can reuse them.
    // Use alloc::vec::Vec (not soroban Vec) because token::Client is not storable.
    let mut validated: alloc::vec::Vec<(token::Client, soroban_sdk::Vec<VouchRecord>)> = alloc::vec::Vec::new();
    for i in 0..borrowers.len() {
        let borrower = borrowers.get(i).unwrap();
        let stake = stakes.get(i).unwrap();
        let result = validate_vouch(&env, &cfg, &voucher, &borrower, stake, &token)?;
        validated.push(result);
    }

    // ── Phase 2: Commit all vouches now that every entry is valid ─────────────
    for (i, (token_client, vouches)) in validated.into_iter().enumerate() {
        let borrower = borrowers.get(i as u32).unwrap();
        let stake = stakes.get(i as u32).unwrap();
        commit_vouch(
            &env,
            &token_client,
            voucher.clone(),
            borrower,
            stake,
            token.clone(),
            vouches,
        );
    }

    Ok(())
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

    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .expect("vouch not found");

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher)
        .expect("vouch not found") as u32;

    let mut vouch_rec = vouches.get(idx).unwrap();
    // Use the token stored on the vouch record.
    let token_client = require_allowed_token(&env, &vouch_rec.token)?;
    let token = vouch_rec.token.clone();

    // Check for overflow before transferring tokens.
    vouch_rec.stake = vouch_rec
        .stake
        .checked_add(additional)
        .ok_or(ContractError::StakeOverflow)?;

    token_client.transfer(&voucher, &env.current_contract_address(), &additional);
    vouches.set(idx, vouch_rec.clone());

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    // Issue #534: Record stake increase in modification history
    let timestamp = env.ledger().timestamp();
    let mut vouch_history: Vec<VouchHistoryEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()))
        .unwrap_or(Vec::new(&env));

    vouch_history.push_back(VouchHistoryEntry {
        timestamp,
        modification_type: soroban_sdk::String::from_slice(&env, "increased"),
        stake_amount: additional,
        delegate: None,
    });

    env.storage().persistent().set(
        &DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()),
        &vouch_history,
    );

    // Issue #370: Emit event for stake increase
    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("increased")),
        (voucher, borrower, additional),
    );

    Ok(())
}

/// Issue #599: Request a decrease in stake during an active loan.
/// The decrease is timelocked for 7 days to prevent rug-pulling.
/// If no active loan exists, the decrease is applied immediately.
pub fn decrease_stake(
    env: Env,
    voucher: Address,
    borrower: Address,
    amount: i128,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    if voucher == borrower {
        return Err(ContractError::SelfVouchNotAllowed);
    }
    if amount <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .expect("vouch not found");

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher)
        .expect("vouch not found") as u32;

    let vouch_rec = vouches.get(idx).unwrap();
    if amount > vouch_rec.stake {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }

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
    let token = vouch_rec.token.clone();
    let mut vouches_mut = vouches;
    let mut vouch_rec_mut = vouches_mut.get(idx).unwrap();
    vouch_rec_mut.stake -= amount;
    if vouch_rec_mut.stake == 0 {
        vouches_mut.remove(idx);
    } else {
        vouches_mut.set(idx, vouch_rec_mut);
    }

    if vouches_mut.is_empty() {
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::Vouches(borrower.clone()), &vouches_mut);
    }

    token_client.transfer(&env.current_contract_address(), &voucher, &amount);

    // Issue #534: Record stake decrease in modification history
    let timestamp = env.ledger().timestamp();
    let mut vouch_history: Vec<VouchHistoryEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()))
        .unwrap_or(Vec::new(&env));

    vouch_history.push_back(VouchHistoryEntry {
        timestamp,
        modification_type: soroban_sdk::String::from_slice(&env, "decreased"),
        stake_amount: amount,
        delegate: None,
    });

    env.storage().persistent().set(
        &DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()),
        &vouch_history,
    );

    // Issue #371: Emit event for stake decrease
    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("decreased")),
        (voucher, borrower, amount),
    );

    Ok(())
}

/// Issue #600: Withdraw a vouch completely and return the stake to the voucher.
///
/// Enforces a minimum lock period of 7 days from the vouch timestamp to prevent
/// flash-loan-style attacks where an attacker stakes, borrows, then immediately withdraws.
pub fn withdraw_vouch(env: Env, voucher: Address, borrower: Address) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    // Only allow withdraw before a loan is active.
    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoActiveLoan)?;

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher)
        .ok_or(ContractError::UnauthorizedCaller)? as u32;

    let vouch_rec = vouches.get(idx).unwrap();

    // Issue #600: Enforce minimum lock period (7 days) before withdrawal is allowed.
    // This prevents flash-loan attacks: stake → borrow → immediately withdraw.
    // The lock only applies when there is an active loan (no loan = no attack vector).
    let now = env.ledger().timestamp();
    if has_active_loan(&env, &borrower)
        && now < vouch_rec.vouch_timestamp + crate::types::MIN_VOUCH_LOCK_PERIOD
    {
        return Err(ContractError::VouchTooRecent);
    }

    let stake = vouch_rec.stake;
    let token_addr = vouch_rec.token.clone();
    vouches.remove(idx);

    if vouches.is_empty() {
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::Vouches(borrower.clone()), &vouches);
    }

    let token_client = require_allowed_token(&env, &token_addr)?;
    token_client.transfer(&env.current_contract_address(), &voucher, &stake);

    // Issue #534: Record withdrawal in modification history
    let timestamp = env.ledger().timestamp();
    let mut vouch_history: Vec<VouchHistoryEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::VouchHistory(borrower.clone(), voucher.clone(), token_addr.clone()))
        .unwrap_or(Vec::new(&env));

    vouch_history.push_back(VouchHistoryEntry {
        timestamp,
        modification_type: soroban_sdk::String::from_slice(&env, "withdrawn"),
        stake_amount: stake,
        delegate: None,
    });

    env.storage().persistent().set(
        &DataKey::VouchHistory(borrower.clone(), voucher.clone(), token_addr.clone()),
        &vouch_history,
    );

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("withdrawn")),
        (voucher, borrower, stake),
    );

    Ok(())
}

/// Issue #600/#537: Request a vouch withdrawal with a timelock.
///
/// Records a pending withdrawal request. The actual withdrawal can be executed
/// after `WITHDRAWAL_TIMELOCK_DELAY` seconds via `execute_vouch_withdrawal`.
pub fn request_vouch_withdrawal(
    env: Env,
    voucher: Address,
    borrower: Address,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoVouchesForBorrower)?;

    let _idx = vouches
        .iter()
        .position(|v| v.voucher == voucher && v.token == token)
        .ok_or(ContractError::VoucherNotFound)?;

    let now = env.ledger().timestamp();
    env.storage().persistent().set(
        &DataKey::PendingWithdrawal(voucher.clone(), borrower.clone()),
        &crate::types::WithdrawalRequest {
            voucher: voucher.clone(),
            borrower: borrower.clone(),
            token: token.clone(),
            requested_at: now,
        },
    );

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("wdraw_req")),
        (voucher, borrower, token, now),
    );

    Ok(())
}

/// Issue #600/#537: Execute a pending vouch withdrawal after the timelock expires.
pub fn execute_vouch_withdrawal(
    env: Env,
    voucher: Address,
    borrower: Address,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    let request: crate::types::WithdrawalRequest = env
        .storage()
        .persistent()
        .get(&DataKey::PendingWithdrawal(voucher.clone(), borrower.clone()))
        .ok_or(ContractError::TimelockNotFound)?;

    let now = env.ledger().timestamp();
    if now < request.requested_at + crate::types::WITHDRAWAL_TIMELOCK_DELAY {
        return Err(ContractError::TimelockNotReady);
    }

    // Remove the pending request
    env.storage()
        .persistent()
        .remove(&DataKey::PendingWithdrawal(voucher.clone(), borrower.clone()));

    // Now perform the actual withdrawal (reuse withdraw_vouch logic)
    withdraw_vouch(env, voucher, borrower)
}

pub fn transfer_vouch(
    env: Env,
    from: Address,
    to: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    from.require_auth();
    require_not_paused(&env)?;

    if from == to {
        return Ok(());
    }

    // Only allow transfer before a loan is active (consistent with withdraw_vouch).
    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoActiveLoan)?;

    let from_idx = vouches
        .iter()
        .position(|v| v.voucher == from)
        .ok_or(ContractError::UnauthorizedCaller)? as u32;

    let from_record = vouches.get(from_idx).unwrap();
    let stake_to_transfer = from_record.stake;

    if let Some(to_idx) = vouches.iter().position(|v| v.voucher == to) {
        // Merge into existing record for 'to'
        let mut to_record = vouches.get(to_idx as u32).unwrap();
        to_record.stake += stake_to_transfer;
        vouches.set(to_idx as u32, to_record);
        vouches.remove(from_idx);
    } else {
        // Transfer ownership to 'to'
        let mut updated_record = from_record;
        updated_record.voucher = to.clone();
        vouches.set(from_idx, updated_record);
    }

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    // Update voucher histories
    // 1. Remove borrower from 'from' history
    let mut from_history: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherHistory(from.clone()))
        .unwrap_or(Vec::new(&env));
    if let Some(h_idx) = from_history.iter().position(|b| b == borrower) {
        from_history.remove(h_idx as u32);
        env.storage()
            .persistent()
            .set(&DataKey::VoucherHistory(from.clone()), &from_history);
    }

    // 2. Add borrower to 'to' history if not already there
    let mut to_history: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherHistory(to.clone()))
        .unwrap_or(Vec::new(&env));
    if !to_history.iter().any(|b| b == borrower) {
        to_history.push_back(borrower.clone());
        env.storage()
            .persistent()
            .set(&DataKey::VoucherHistory(to.clone()), &to_history);
    }

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("transfer")),
        (from, to, borrower, stake_to_transfer),
    );

    Ok(())
}

pub fn vouch_exists(env: Env, voucher: Address, borrower: Address) -> bool {
    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower))
        .unwrap_or(Vec::new(&env));
    vouches.iter().any(|v| v.voucher == voucher)
}

pub fn total_vouched(env: Env, borrower: Address) -> Result<i128, ContractError> {
    let vouches = env
        .storage()
        .persistent()
        .get::<DataKey, Vec<VouchRecord>>(&DataKey::Vouches(borrower))
        .unwrap_or(Vec::new(&env));

    let mut total: i128 = 0;
    for vouch in vouches.iter() {
        total = total
            .checked_add(vouch.stake)
            .ok_or(ContractError::StakeOverflow)?;
    }

    Ok(total)
}

pub fn voucher_history(env: Env, voucher: Address) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::VoucherHistory(voucher))
        .unwrap_or(Vec::new(&env))
}

/// Issue #532: Delegate vouch management to another address.
/// Only the original voucher can call this.
pub fn delegate_vouch(
    env: Env,
    voucher: Address,
    borrower: Address,
    delegate: Address,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    if voucher == delegate {
        return Err(ContractError::InvalidAmount);
    }

    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoVouchesForBorrower)?;

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher && v.token == token)
        .ok_or(ContractError::VoucherNotFound)? as u32;

    let mut vouch_rec = vouches.get(idx).unwrap();
    let stake_amount = vouch_rec.stake;
    vouch_rec.delegate = Some(delegate.clone());
    vouches.set(idx, vouch_rec);

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    // Record delegation in history
    let mut history: Vec<VouchHistoryEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()))
        .unwrap_or(Vec::new(&env));

    history.push_back(VouchHistoryEntry {
        timestamp: env.ledger().timestamp(),
        modification_type: soroban_sdk::String::from_slice(&env, "delegated"),
        stake_amount,
        delegate: Some(delegate.clone()),
    });

    env.storage().persistent().set(
        &DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()),
        &history,
    );

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("delegated")),
        (voucher, borrower, delegate),
    );

    Ok(())
}

/// Issue #532: Revoke delegation of a vouch.
/// Only the original voucher can call this.
pub fn revoke_delegation(
    env: Env,
    voucher: Address,
    borrower: Address,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoVouchesForBorrower)?;

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher && v.token == token)
        .ok_or(ContractError::VoucherNotFound)? as u32;

    let mut vouch_rec = vouches.get(idx).unwrap();
    vouch_rec.delegate = None;
    vouches.set(idx, vouch_rec);

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("revoked")),
        (voucher, borrower),
    );

    Ok(())
}

/// Issue #533: Set expiry timestamp for a vouch.
/// Only the original voucher can call this.
pub fn set_vouch_expiry(
    env: Env,
    voucher: Address,
    borrower: Address,
    expiry_timestamp: u64,
    token: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    let now = env.ledger().timestamp();
    if expiry_timestamp <= now {
        return Err(ContractError::InvalidAmount);
    }

    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoVouchesForBorrower)?;

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher && v.token == token)
        .ok_or(ContractError::VoucherNotFound)? as u32;

    let mut vouch_rec = vouches.get(idx).unwrap();
    vouch_rec.expiry_timestamp = Some(expiry_timestamp);
    vouches.set(idx, vouch_rec);

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    env.events().publish(
        (symbol_short!("vouch"), symbol_short!("expiry")),
        (voucher, borrower, expiry_timestamp),
    );

    Ok(())
}

/// Issue #534: Get vouch modification history for auditing.
pub fn get_vouch_history(
    env: Env,
    borrower: Address,
    voucher: Address,
    token: Address,
) -> Vec<VouchHistoryEntry> {
    env.storage()
        .persistent()
        .get(&DataKey::VouchHistory(borrower, voucher, token))
        .unwrap_or(Vec::new(&env))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn create_test_token(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_test_admin(env: &Env) -> Address {
        Address::generate(env)
    }

    #[test]
    fn test_total_vouched_overflow() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let borrower = Address::generate(&env);

        // Create vouches that would overflow when summed
        let mut vouches = Vec::new(&env);

        // Add two vouches with very large stakes that would overflow i128::MAX
        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);

        vouches.push_back(VouchRecord {
            voucher: voucher1,
            stake: i128::MAX - 1000,
            vouch_timestamp: 0,
            token: token.clone(),
        });

        vouches.push_back(VouchRecord {
            voucher: voucher2,
            stake: 2000, // This would cause overflow when added to the first stake
            vouch_timestamp: 0,
            token: token.clone(),
        });

        // Store the vouches directly in contract storage
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Vouches(borrower.clone()), &vouches);
        });

        // Test that total_vouched returns StakeOverflow error
        let result = client.try_total_vouched(&borrower);
        assert_eq!(result, Err(Ok(ContractError::StakeOverflow)));
    }

    #[test]
    fn test_total_vouched_no_overflow() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let borrower = Address::generate(&env);

        // Create vouches with normal stakes that won't overflow
        let mut vouches = Vec::new(&env);

        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);

        vouches.push_back(VouchRecord {
            voucher: voucher1,
            stake: 1_000_000,
            vouch_timestamp: 0,
            token: token.clone(),
        });

        vouches.push_back(VouchRecord {
            voucher: voucher2,
            stake: 2_500_000,
            vouch_timestamp: 0,
            token: token.clone(),
        });

        // Store the vouches directly in contract storage
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Vouches(borrower.clone()), &vouches);
        });

        // Test that total_vouched returns correct sum
        let result = client.total_vouched(&borrower);
        assert_eq!(result, 3_500_000);
    }

    #[test]
    #[should_panic(expected = "DuplicateVouch")]
    fn test_duplicate_vouch_from_same_voucher_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        // First vouch should succeed
        client.vouch(&voucher, &borrower, &1000, &token);

        // Second vouch from same voucher for same borrower should panic with DuplicateVouch
        client.vouch(&voucher, &borrower, &2000, &token);
    }

    #[test]
    fn test_vouch_blacklisted_borrower() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        let stake = 1_000_000;

        // Blacklist the borrower
        client.blacklist(&Vec::from_array(&env, [admin]), &borrower);

        // Attempt to vouch for blacklisted borrower should fail
        let result = client.try_vouch(&voucher, &borrower, &stake, &token);
        assert_eq!(result, Err(Ok(ContractError::Blacklisted)));
    }

    /// Issue #375: Whitelist enforcement in do_vouch
    #[test]
    fn test_vouch_whitelisted_voucher_allowed() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        let stake = 1_000_000;

        // Enable whitelist
        client.set_whitelist_enabled(&admins, &true);

        // Whitelist the voucher
        client.whitelist_voucher(&admins, &voucher);

        // Vouch should succeed
        let result = client.try_vouch(&voucher, &borrower, &stake, &token);
        assert!(result.is_ok());
    }

    /// Issue #375: Non-whitelisted voucher rejected when whitelist enabled
    #[test]
    fn test_vouch_non_whitelisted_voucher_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        let stake = 1_000_000;

        // Enable whitelist
        client.set_whitelist_enabled(&admins, &true);

        // Try to vouch without being whitelisted
        let result = client.try_vouch(&voucher, &borrower, &stake, &token);
        assert_eq!(result, Err(Ok(ContractError::VoucherNotWhitelisted)));
    }

    /// Issue #375: Whitelist disabled by default (opt-in)
    #[test]
    fn test_vouch_whitelist_disabled_by_default() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        let stake = 1_000_000;

        // Whitelist is disabled by default, so any voucher can vouch
        let result = client.try_vouch(&voucher, &borrower, &stake, &token);
        assert!(result.is_ok());
    }

    /// Issue #442: decrease_stake() must reject self-vouch (voucher == borrower)
    #[test]
    fn test_decrease_stake_self_vouch_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = create_test_admin(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = create_test_token(&env);

        client.initialize(&deployer, &admins, &1, &token);

        let user = Address::generate(&env);

        let result = client.try_decrease_stake(&user, &user, &1_000);
        assert_eq!(result, Err(Ok(ContractError::SelfVouchNotAllowed)));
    }
}
