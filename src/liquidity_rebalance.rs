/// Issue #88: Liquidity Rebalancing — auto-rebalance collateral pools.
///
/// Moves excess stake from over-funded inactive pools to under-funded ones so
/// that dormant capital is put to work without manual admin intervention.
use soroban_sdk::{symbol_short, Address, Env, Vec};

use crate::{
    errors::ContractError,
    helpers::{require_admin_approval, require_not_paused},
    types::{CollateralPool, DataKey},
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn load_pool(env: &Env, pool_id: u64) -> Result<CollateralPool, ContractError> {
    env.storage()
        .persistent()
        .get::<DataKey, CollateralPool>(&DataKey::CollateralPool(pool_id))
        .ok_or(ContractError::NotPoolMember)
}

fn save_pool(env: &Env, pool: &CollateralPool) {
    env.storage()
        .persistent()
        .set(&DataKey::CollateralPool(pool.pool_id), pool);
}

fn pool_total_stake(pool: &CollateralPool) -> i128 {
    pool.stakes.iter().sum()
}

/// Deduct `amount` proportionally from the front of `pool.stakes`.
/// Removes members whose stake reaches zero.
fn deduct_from_pool(pool: &mut CollateralPool, amount: i128) {
    let mut remaining = amount;
    for i in 0..pool.stakes.len() {
        if remaining == 0 {
            break;
        }
        let s = pool.stakes.get(i).unwrap();
        let deduct = if s <= remaining { s } else { remaining };
        pool.stakes.set(i, s - deduct);
        remaining -= deduct;
    }
    // Remove zero-stake members.
    let mut i = 0u32;
    while i < pool.members.len() {
        if pool.stakes.get(i).unwrap() == 0 {
            pool.members.remove(i);
            pool.stakes.remove(i);
        } else {
            i += 1;
        }
    }
}

/// Credit `amount` to `tgt` pool, attributed to the contract address.
fn credit_pool(env: &Env, tgt: &mut CollateralPool, amount: i128) {
    let contract_addr = env.current_contract_address();
    for i in 0..tgt.members.len() {
        if tgt.members.get(i).unwrap() == contract_addr {
            let existing = tgt.stakes.get(i).unwrap();
            tgt.stakes.set(i, existing + amount);
            return;
        }
    }
    tgt.members.push_back(contract_addr);
    tgt.stakes.push_back(amount);
}

// ── public interface ──────────────────────────────────────────────────────────

/// Rebalance liquidity between two inactive collateral pools.
///
/// Moves `amount` stroops of stake from `source_pool_id` to `target_pool_id`.
/// Both pools must share the same token and must not be active (i.e. no loan
/// is currently outstanding against them).
///
/// No real token transfer is needed — the contract already holds all staked
/// tokens.  Only the accounting records in `CollateralPool.stakes` are updated.
///
/// Emits `pool/rebalanc` with `(source_pool_id, target_pool_id, amount)`.
pub fn rebalance_pools(
    env: Env,
    admin_signers: Vec<Address>,
    source_pool_id: u64,
    target_pool_id: u64,
    amount: i128,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    require_not_paused(&env)?;

    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    if source_pool_id == target_pool_id {
        return Err(ContractError::InvalidAmount);
    }

    let mut src = load_pool(&env, source_pool_id)?;
    let mut tgt = load_pool(&env, target_pool_id)?;

    if src.active || tgt.active {
        return Err(ContractError::CollateralPoolActive);
    }
    if src.token != tgt.token {
        return Err(ContractError::InvalidToken);
    }
    if pool_total_stake(&src) < amount {
        return Err(ContractError::InsufficientFunds);
    }

    deduct_from_pool(&mut src, amount);
    credit_pool(&env, &mut tgt, amount);

    save_pool(&env, &src);
    save_pool(&env, &tgt);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("rebalanc")),
        (source_pool_id, target_pool_id, amount),
    );

    Ok(())
}

/// Return the total stake (in stroops) held in a collateral pool.
pub fn get_pool_liquidity(env: Env, pool_id: u64) -> Result<i128, ContractError> {
    let pool = load_pool(&env, pool_id)?;
    Ok(pool_total_stake(&pool))
}

/// Auto-rebalance: scan all collateral pools and move surplus stake from pools
/// above `target_stake` to pools below it.
///
/// Only inactive pools are considered.  Pools with a different token than a
/// potential counterpart are skipped.  Returns the number of transfers made.
pub fn auto_rebalance_pools(
    env: Env,
    admin_signers: Vec<Address>,
    target_stake: i128,
) -> Result<u32, ContractError> {
    require_admin_approval(&env, &admin_signers);
    require_not_paused(&env)?;

    if target_stake <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let pool_count: u64 = env
        .storage()
        .instance()
        .get(&DataKey::CollateralPoolCounter)
        .unwrap_or(0);

    let mut transfers: u32 = 0;

    // Two-pointer scan: find over-funded / under-funded pairs.
    let mut oi: u64 = 1; // over-funded cursor
    let mut ui: u64 = 1; // under-funded cursor

    // Advance cursors until we either find an over- or under-funded pool.
    while oi <= pool_count && ui <= pool_count {
        // Find next over-funded inactive pool.
        while oi <= pool_count {
            if let Some(p) = env
                .storage()
                .persistent()
                .get::<DataKey, CollateralPool>(&DataKey::CollateralPool(oi))
            {
                if !p.active && pool_total_stake(&p) > target_stake {
                    break;
                }
            }
            oi += 1;
        }

        // Find next under-funded inactive pool.
        while ui <= pool_count {
            if let Some(p) = env
                .storage()
                .persistent()
                .get::<DataKey, CollateralPool>(&DataKey::CollateralPool(ui))
            {
                if !p.active && pool_total_stake(&p) < target_stake {
                    break;
                }
            }
            ui += 1;
        }

        if oi > pool_count || ui > pool_count || oi == ui {
            break;
        }

        let mut src = load_pool(&env, oi)?;
        let mut tgt = load_pool(&env, ui)?;

        // Skip incompatible token pairs.
        if src.token != tgt.token {
            oi += 1;
            continue;
        }

        let surplus = pool_total_stake(&src).saturating_sub(target_stake);
        let deficit = target_stake.saturating_sub(pool_total_stake(&tgt));
        let move_amount = surplus.min(deficit);

        if move_amount > 0 {
            deduct_from_pool(&mut src, move_amount);
            credit_pool(&env, &mut tgt, move_amount);
            save_pool(&env, &src);
            save_pool(&env, &tgt);

            env.events().publish(
                (symbol_short!("pool"), symbol_short!("rebalanc")),
                (oi, ui, move_amount),
            );

            transfers += 1;
        }

        // Advance whichever cursor is now at target.
        if pool_total_stake(&src) <= target_stake {
            oi += 1;
        }
        if pool_total_stake(&tgt) >= target_stake {
            ui += 1;
        }
    }

    Ok(transfers)
}
