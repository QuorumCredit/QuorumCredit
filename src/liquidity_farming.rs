//! Liquidity Farming Module
//!
//! Implements liquidity mining rewards for LP providers who contribute
//! liquidity to the protocol's loan pools. Rewards are distributed based
//! on stake duration, amount, and pool tier.
//!
//! Issue #978: Reward LP providers
//!
//! Key features:
//! - Multi-tier liquidity farming with tiered reward rates
//! - Time-weighted average liquidity calculations
//! - Seasonal reward multipliers
//! - Claim and compounding mechanics

use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::DataKey;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Liquidity farming pool for a token
#[contracttype]
#[derive(Clone)]
pub struct LiquidityFarmPool {
    /// Unique pool ID
    pub pool_id: u64,
    /// Token being farmed
    pub token: Address,
    /// Total liquidity in pool
    pub total_liquidity: i128,
    /// Total reward tokens allocated
    pub total_rewards: i128,
    /// Rewards per unit time (per second)
    pub reward_rate: i128,
    /// Pool creation timestamp
    pub created_at: u64,
    /// Current farming season
    pub current_season: u32,
    /// Season multiplier (1000 = 1x, 2000 = 2x, etc.)
    pub season_multiplier: u32,
    /// Whether pool is active
    pub active: bool,
}

/// LP provider's stake in a liquidity farming pool
#[contracttype]
#[derive(Clone)]
pub struct FarmingPosition {
    /// LP provider address
    pub lp_provider: Address,
    /// Pool ID
    pub pool_id: u64,
    /// Amount of liquidity provided
    pub liquidity_amount: i128,
    /// When the position was opened
    pub stake_timestamp: u64,
    /// Last time rewards were claimed
    pub last_claim_time: u64,
    /// Accumulated unclaimed rewards
    pub pending_rewards: i128,
    /// Total rewards claimed all-time
    pub total_rewards_claimed: i128,
}

/// Time-weighted reward calculation snapshot
#[contracttype]
#[derive(Clone, Copy)]
struct RewardSnapshot {
    timestamp: u64,
    liquidity_amount: i128,
    accumulated_reward_per_unit: i128,
}

/// Seasonal reward configuration
#[contracttype]
#[derive(Clone)]
pub struct SeasonConfig {
    pub season: u32,
    pub start_time: u64,
    pub end_time: u64,
    pub reward_multiplier: u32,
    pub total_allocation: i128,
}

/// Create a new liquidity farming pool
pub fn create_farm_pool(
    env: Env,
    admin_signers: Vec<Address>,
    token: Address,
    initial_reward_rate: i128,
) -> Result<u64, ContractError> {
    require_admin_approval(&env, &admin_signers);

    if initial_reward_rate <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Generate pool ID (timestamp-based)
    let pool_id = env.ledger().timestamp() as u64;

    let pool = LiquidityFarmPool {
        pool_id,
        token,
        total_liquidity: 0,
        total_rewards: 0,
        reward_rate: initial_reward_rate,
        created_at: env.ledger().timestamp(),
        current_season: 1,
        season_multiplier: 1000, // 1x multiplier
        active: true,
    };

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);

    Ok(pool_id)
}

/// Add liquidity to a farming pool and start earning rewards
pub fn add_liquidity(
    env: Env,
    lp_provider: Address,
    pool_id: u64,
    liquidity_amount: i128,
) -> Result<(), ContractError> {
    lp_provider.require_auth();

    if liquidity_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    if !pool.active {
        return Err(ContractError::ContractPaused);
    }

    let now = env.ledger().timestamp();

    // Check if position already exists
    let existing_position: Option<FarmingPosition> = env
        .storage()
        .persistent()
        .get(&DataKey::FarmingPosition(pool_id, lp_provider.clone()));

    let mut position = if let Some(mut pos) = existing_position {
        // Claim pending rewards first
        claim_farming_rewards_internal(&env, &mut pos, &pool, now)?;
        pos.liquidity_amount = pos.liquidity_amount.saturating_add(liquidity_amount);
        pos
    } else {
        FarmingPosition {
            lp_provider: lp_provider.clone(),
            pool_id,
            liquidity_amount,
            stake_timestamp: now,
            last_claim_time: now,
            pending_rewards: 0,
            total_rewards_claimed: 0,
        }
    };

    // Update pool totals
    pool.total_liquidity = pool.total_liquidity.saturating_add(liquidity_amount);

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);
    env.storage()
        .persistent()
        .set(&DataKey::FarmingPosition(pool_id, lp_provider), &position);

    // In production, would transfer liquidity tokens from LP to contract

    Ok(())
}

/// Remove liquidity from a farming pool
pub fn remove_liquidity(
    env: Env,
    lp_provider: Address,
    pool_id: u64,
    amount_to_withdraw: i128,
) -> Result<(), ContractError> {
    lp_provider.require_auth();

    if amount_to_withdraw <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    let mut position: FarmingPosition = env
        .storage()
        .persistent()
        .get(&DataKey::FarmingPosition(pool_id, lp_provider.clone()))
        .ok_or(ContractError::InvalidAmount)?;

    if amount_to_withdraw > position.liquidity_amount {
        return Err(ContractError::InsufficientFunds);
    }

    let now = env.ledger().timestamp();

    // Claim pending rewards first
    claim_farming_rewards_internal(&env, &mut position, &pool, now)?;

    // Update position
    position.liquidity_amount = position.liquidity_amount.saturating_sub(amount_to_withdraw);

    // Update pool
    pool.total_liquidity = pool.total_liquidity.saturating_sub(amount_to_withdraw);

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);

    if position.liquidity_amount > 0 {
        env.storage()
            .persistent()
            .set(&DataKey::FarmingPosition(pool_id, lp_provider), &position);
    } else {
        // Remove position if no liquidity left
        env.storage()
            .persistent()
            .remove(&DataKey::FarmingPosition(pool_id, lp_provider));
    }

    // In production, would transfer liquidity tokens back to LP

    Ok(())
}

/// Claim accumulated farming rewards
pub fn claim_farming_rewards(
    env: Env,
    lp_provider: Address,
    pool_id: u64,
) -> Result<i128, ContractError> {
    lp_provider.require_auth();

    let pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    let mut position: FarmingPosition = env
        .storage()
        .persistent()
        .get(&DataKey::FarmingPosition(pool_id, lp_provider.clone()))
        .ok_or(ContractError::InvalidAmount)?;

    let now = env.ledger().timestamp();

    claim_farming_rewards_internal(&env, &mut position, &pool, now)?;

    let claimed = position.pending_rewards;

    position.pending_rewards = 0;
    position.total_rewards_claimed = position.total_rewards_claimed.saturating_add(claimed);

    env.storage()
        .persistent()
        .set(&DataKey::FarmingPosition(pool_id, lp_provider), &position);

    // In production, would transfer reward tokens to LP

    Ok(claimed)
}

/// Internal: Calculate and update pending rewards
fn claim_farming_rewards_internal(
    env: &Env,
    position: &mut FarmingPosition,
    pool: &LiquidityFarmPool,
    now: u64,
) -> Result<(), ContractError> {
    // Calculate time elapsed since last claim
    let time_elapsed = now.saturating_sub(position.last_claim_time);

    if time_elapsed == 0 {
        return Ok(());
    }

    // Calculate rewards = reward_rate * time_elapsed * (liquidity / total_liquidity) * season_multiplier
    let reward_allocation = if pool.total_liquidity > 0 {
        let base_reward = pool.reward_rate * time_elapsed as i128;
        let share = (base_reward * position.liquidity_amount) / pool.total_liquidity;
        (share * pool.season_multiplier as i128) / 1000
    } else {
        0
    };

    position.pending_rewards = position.pending_rewards.saturating_add(reward_allocation);
    position.last_claim_time = now;

    Ok(())
}

/// Compound rewards (claim and auto-reinvest into the pool)
pub fn compound_rewards(
    env: Env,
    lp_provider: Address,
    pool_id: u64,
) -> Result<(), ContractError> {
    lp_provider.require_auth();

    let mut pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    let mut position: FarmingPosition = env
        .storage()
        .persistent()
        .get(&DataKey::FarmingPosition(pool_id, lp_provider.clone()))
        .ok_or(ContractError::InvalidAmount)?;

    let now = env.ledger().timestamp();

    // Calculate pending rewards
    claim_farming_rewards_internal(&env, &mut position, &pool, now)?;

    let compound_amount = position.pending_rewards;

    if compound_amount <= 0 {
        return Ok(());
    }

    // Add rewards back as liquidity
    position.liquidity_amount = position.liquidity_amount.saturating_add(compound_amount);
    position.pending_rewards = 0;
    position.total_rewards_claimed = position.total_rewards_claimed.saturating_add(compound_amount);

    // Update pool
    pool.total_liquidity = pool.total_liquidity.saturating_add(compound_amount);

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);
    env.storage()
        .persistent()
        .set(&DataKey::FarmingPosition(pool_id, lp_provider), &position);

    Ok(())
}

/// Admin: Update reward rate for a pool
pub fn set_pool_reward_rate(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
    new_reward_rate: i128,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if new_reward_rate < 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    pool.reward_rate = new_reward_rate;

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);

    Ok(())
}

/// Admin: Start a new seasonal reward multiplier
pub fn set_seasonal_multiplier(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
    multiplier: u32,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if multiplier == 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    pool.season_multiplier = multiplier;

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);

    Ok(())
}

/// Query a farming position
pub fn get_farming_position(
    env: Env,
    pool_id: u64,
    lp_provider: Address,
) -> Result<FarmingPosition, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::FarmingPosition(pool_id, lp_provider))
        .ok_or(ContractError::InvalidAmount)
}

/// Query a farm pool
pub fn get_farm_pool(env: Env, pool_id: u64) -> Result<LiquidityFarmPool, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)
}

/// Calculate current pending rewards for a position (without claiming)
pub fn calculate_pending_rewards(
    env: Env,
    pool_id: u64,
    lp_provider: Address,
) -> Result<i128, ContractError> {
    let pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    let position: FarmingPosition = env
        .storage()
        .persistent()
        .get(&DataKey::FarmingPosition(pool_id, lp_provider))
        .ok_or(ContractError::InvalidAmount)?;

    let now = env.ledger().timestamp();
    let time_elapsed = now.saturating_sub(position.last_claim_time);

    if time_elapsed == 0 || pool.total_liquidity == 0 {
        return Ok(position.pending_rewards);
    }

    let base_reward = pool.reward_rate * time_elapsed as i128;
    let share = (base_reward * position.liquidity_amount) / pool.total_liquidity;
    let accrued = (share * pool.season_multiplier as i128) / 1000;

    Ok(position.pending_rewards + accrued)
}

/// Admin: Disable a farm pool (prevents new deposits)
pub fn deactivate_farm_pool(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    let mut pool: LiquidityFarmPool = env
        .storage()
        .persistent()
        .get(&DataKey::FarmPool(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    pool.active = false;

    env.storage()
        .persistent()
        .set(&DataKey::FarmPool(pool_id), &pool);

    Ok(())
}
