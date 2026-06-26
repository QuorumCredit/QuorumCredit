//! # Issue #867: Cross-Collateral Vouch Pools
//!
//! A shared collateral pool lets multiple vouchers deposit stake into a single
//! pool that backs one borrower.  This decouples individual vouch records from
//! the pool's aggregate collateral, enabling:
//!
//! - **Risk sharing** — pool members share the slash proportionally.
//! - **Flexible composition** — vouchers join/leave the pool independently
//!   (while no active loan exists).
//! - **Single-threshold eligibility** — the borrower checks the pool's total
//!   stake rather than individual vouch records.

use crate::errors::ContractError;
use crate::helpers::{require_admin_approval, require_not_paused, require_not_thawing};
use crate::types::{CollateralPool, DataKey};
use soroban_sdk::{symbol_short, token, Address, Env, Vec};

// ── Internal helpers ──────────────────────────────────────────────────────────

fn next_pool_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::CollateralPoolCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .expect("pool ID overflow");
    env.storage()
        .persistent()
        .set(&DataKey::CollateralPoolCounter, &id);
    id
}

fn load_pool(env: &Env, pool_id: u64) -> Result<CollateralPool, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::CollateralPool(pool_id))
        .ok_or(ContractError::CollateralPoolNotFound)
}

fn save_pool(env: &Env, pool: &CollateralPool) {
    env.storage()
        .persistent()
        .set(&DataKey::CollateralPool(pool.pool_id), pool);
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Create a new empty cross-collateral pool for `token`.
/// The caller becomes the first member with `initial_stake` deposited.
///
/// Returns the new pool ID.
pub fn create_pool(
    env: Env,
    creator: Address,
    token: Address,
    initial_stake: i128,
) -> Result<u64, ContractError> {
    creator.require_auth();
    require_not_thawing(&env)?;

    if initial_stake <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let token_client = crate::helpers::require_allowed_token(&env, &token)?;
    let contract = env.current_contract_address();

    // Transfer initial stake into the contract.
    let before = token_client.balance(&contract);
    token_client.transfer(&creator, &contract, &initial_stake);
    let after = token_client.balance(&contract);
    let received = after
        .checked_sub(before)
        .ok_or(ContractError::StakeOverflow)?;
    if received != initial_stake {
        return Err(ContractError::InsufficientFunds);
    }

    let pool_id = next_pool_id(&env);
    let now = env.ledger().timestamp();

    let mut members: Vec<Address> = Vec::new(&env);
    members.push_back(creator.clone());

    let mut stakes: Vec<i128> = Vec::new(&env);
    stakes.push_back(initial_stake);

    let pool = CollateralPool {
        pool_id,
        members,
        stakes,
        token: token.clone(),
        borrower: None,
        active: false,
        created_at: now,
    };

    save_pool(&env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("created")),
        (creator, pool_id, token, initial_stake),
    );

    Ok(pool_id)
}

/// Join an existing pool by contributing `stake` stroops.
/// The pool must not currently be active (i.e., no borrower assigned).
pub fn join_pool(
    env: Env,
    voucher: Address,
    pool_id: u64,
    stake: i128,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    if stake <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut pool = load_pool(&env, pool_id)?;

    if pool.active {
        return Err(ContractError::CollateralPoolActive);
    }

    // Prevent duplicate membership.
    for m in pool.members.iter() {
        if m == voucher {
            return Err(ContractError::DuplicateVouch);
        }
    }

    let token_client = crate::helpers::require_allowed_token(&env, &pool.token)?;
    let contract = env.current_contract_address();

    let before = token_client.balance(&contract);
    token_client.transfer(&voucher, &contract, &stake);
    let after = token_client.balance(&contract);
    let received = after
        .checked_sub(before)
        .ok_or(ContractError::StakeOverflow)?;
    if received != stake {
        return Err(ContractError::InsufficientFunds);
    }

    pool.members.push_back(voucher.clone());
    pool.stakes.push_back(stake);
    save_pool(&env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("join")),
        (voucher, pool_id, stake),
    );

    Ok(())
}

/// Leave a pool, withdrawing the caller's stake.
/// Only allowed when the pool has no active borrower.
pub fn leave_pool(env: Env, voucher: Address, pool_id: u64) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    let mut pool = load_pool(&env, pool_id)?;

    if pool.active {
        return Err(ContractError::CollateralPoolActive);
    }

    let idx = pool
        .members
        .iter()
        .position(|m| m == voucher)
        .ok_or(ContractError::NotPoolMember)? as u32;

    let stake = pool.stakes.get(idx).unwrap();

    pool.members.remove(idx);
    pool.stakes.remove(idx);
    save_pool(&env, &pool);

    let token_client = crate::helpers::require_allowed_token(&env, &pool.token)?;
    token_client.transfer(&env.current_contract_address(), &voucher, &stake);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("leave")),
        (voucher, pool_id, stake),
    );

    Ok(())
}

/// Assign a borrower to a pool and mark it active.
/// Only admin can link a pool to a borrower (protects against griefing).
/// Once active the pool's collateral is locked until the loan resolves.
pub fn assign_pool_to_borrower(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
    borrower: Address,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    require_not_paused(&env)?;

    let mut pool = load_pool(&env, pool_id)?;

    if pool.active {
        return Err(ContractError::CollateralPoolActive);
    }

    pool.borrower = Some(borrower.clone());
    pool.active = true;
    save_pool(&env, &pool);

    env.storage()
        .persistent()
        .set(&DataKey::BorrowerPool(borrower.clone(), pool_id), &true);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("assign")),
        (pool_id, borrower),
    );

    Ok(())
}

/// Return the total stake held in a pool, in stroops.
pub fn get_pool_total_stake(env: Env, pool_id: u64) -> Result<i128, ContractError> {
    let pool = load_pool(&env, pool_id)?;
    let total: i128 = pool.stakes.iter().sum();
    Ok(total)
}

/// Read a pool record.
pub fn get_pool(env: Env, pool_id: u64) -> Result<CollateralPool, ContractError> {
    load_pool(&env, pool_id)
}

/// Release pool collateral after a loan is repaid or slashed.
/// Called internally by the loan module.  Distributes each member's stake back
/// proportionally (or zeroes it out on slash, handled by the caller).
pub fn release_pool_collateral(env: &Env, pool_id: u64) -> Result<(), ContractError> {
    let mut pool = load_pool(env, pool_id)?;

    if !pool.active {
        return Err(ContractError::CollateralPoolNotFound);
    }

    let token_client = token::Client::new(env, &pool.token);
    let contract = env.current_contract_address();

    for i in 0..pool.members.len() {
        let member = pool.members.get(i).unwrap();
        let stake = pool.stakes.get(i).unwrap();
        if stake > 0 {
            token_client.transfer(&contract, &member, &stake);
        }
    }

    // Clear the pool's active state.
    pool.active = false;
    pool.borrower = None;
    // Zero out all stakes since funds have been returned.
    for i in 0..pool.stakes.len() {
        pool.stakes.set(i, 0);
    }
    save_pool(env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("released")),
        pool_id,
    );

    Ok(())
}
