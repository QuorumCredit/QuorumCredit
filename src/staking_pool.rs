/// Issue #1238: Staking Pool with Yield Farming
///
/// Excess protocol capital can be staked into yield-bearing pools.  Yield is
/// sourced from the lending yield reserve and distributed proportionally to
/// stakers using a "yield-per-token" accumulator (the standard Synthetix/
/// MasterChef pattern), which avoids O(n) iterations over all stakers.
///
/// ## Accounting model
///
/// Each pool tracks `yield_per_token_scaled` — the cumulative yield earned per
/// 1 stroop staked, multiplied by `YIELD_PER_TOKEN_PRECISION` (10^12) to keep
/// sub-stroop precision in integer arithmetic.
///
/// A staker's pending reward at any point is:
///   pending = (pool.yield_per_token_scaled - position.yield_snapshot_scaled)
///             * position.amount / YIELD_PER_TOKEN_PRECISION
///
/// On every stake / unstake / claim the snapshot is updated to the current
/// pool accumulator, so historical yield is "banked" into `pending_rewards`.
///
/// ## Withdrawal queue
///
/// Unstaking is intentionally delayed by `STAKING_UNSTAKE_DELAY_SECS` (24 h)
/// to prevent bank-run dynamics during high-stress periods.  A staker calls
/// `queue_unstake` to register intent; after the delay they call
/// `process_unstake` to receive their tokens back.
use soroban_sdk::{symbol_short, token, Address, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::{require_admin_approval, require_allowed_token, require_not_paused};
use crate::types::{
    DataKey, StakerPosition, StakingPool, StakingPoolStatus, DEFAULT_STAKING_POOL_APY_BPS,
    PERSISTENT_TTL_TARGET_LEDGERS, PERSISTENT_TTL_THRESHOLD_LEDGERS, STAKING_UNSTAKE_DELAY_SECS,
    YIELD_PER_TOKEN_PRECISION,
};

// ── helpers ────────────────────────────────────────────────────────────────────

fn load_pool(env: &Env, pool_id: u64) -> Result<StakingPool, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::StakingPool(pool_id))
        .ok_or(ContractError::StakingPoolNotFound)
}

fn save_pool(env: &Env, pool: &StakingPool) {
    env.storage()
        .persistent()
        .set(&DataKey::StakingPool(pool.pool_id), pool);
    env.storage().persistent().extend_ttl(
        &DataKey::StakingPool(pool.pool_id),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

fn next_pool_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::StakingPoolCounter)
        .unwrap_or(0u64);
    let next = id + 1;
    env.storage()
        .persistent()
        .set(&DataKey::StakingPoolCounter, &next);
    next
}

fn load_position(env: &Env, pool_id: u64, staker: &Address) -> StakerPosition {
    env.storage()
        .persistent()
        .get(&DataKey::StakingPoolStake(pool_id, staker.clone()))
        .unwrap_or(StakerPosition {
            staker: staker.clone(),
            amount: 0,
            yield_snapshot_scaled: 0,
            pending_rewards: 0,
            last_action_timestamp: 0,
            pending_unstake: false,
            queued_unstake_amount: 0,
        })
}

fn save_position(env: &Env, pool_id: u64, position: &StakerPosition) {
    env.storage().persistent().set(
        &DataKey::StakingPoolStake(pool_id, position.staker.clone()),
        position,
    );
    env.storage().persistent().extend_ttl(
        &DataKey::StakingPoolStake(pool_id, position.staker.clone()),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

/// Harvest a staker's accrued rewards into `pending_rewards` and update snapshot.
fn harvest(pool: &StakingPool, position: &mut StakerPosition) {
    if position.amount > 0 {
        let accrued = (pool.yield_per_token_scaled - position.yield_snapshot_scaled)
            .checked_mul(position.amount)
            .unwrap_or(0)
            / YIELD_PER_TOKEN_PRECISION;
        position.pending_rewards = position.pending_rewards.saturating_add(accrued);
    }
    position.yield_snapshot_scaled = pool.yield_per_token_scaled;
}

// ── public entry-points ────────────────────────────────────────────────────────

/// Issue #1238: Create a new staking pool for the given token.
///
/// Requires admin approval. The pool starts in `Active` status with zero
/// staked balance and the default APY.
pub fn create_staking_pool(
    env: Env,
    admin_signers: Vec<Address>,
    token: Address,
) -> Result<u64, ContractError> {
    require_not_paused(&env)?;
    require_admin_approval(&env, &admin_signers);
    require_allowed_token(&env, &token)?;

    let pool_id = next_pool_id(&env);
    let now = env.ledger().timestamp();

    let pool = StakingPool {
        pool_id,
        token,
        total_staked: 0,
        yield_per_token_scaled: 0,
        current_apy_bps: DEFAULT_STAKING_POOL_APY_BPS,
        total_yield_distributed: 0,
        last_yield_timestamp: now,
        status: StakingPoolStatus::Active,
        created_at: now,
    };

    save_pool(&env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("created")),
        (pool_id, pool.token.clone()),
    );

    Ok(pool_id)
}

/// Issue #1238: Stake `amount` stroops into the pool.
///
/// Tokens are transferred from the staker to the contract.  Any previously
/// accrued yield is harvested into `pending_rewards` before the new deposit
/// is recorded.
///
/// Returns the staker's updated staked balance.
pub fn stake_capital(
    env: Env,
    pool_id: u64,
    staker: Address,
    amount: i128,
) -> Result<i128, ContractError> {
    staker.require_auth();
    require_not_paused(&env)?;

    if amount <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    let mut pool = load_pool(&env, pool_id)?;
    if pool.status != StakingPoolStatus::Active {
        return Err(ContractError::StakingPoolNotActive);
    }

    // Harvest pending rewards before modifying the position.
    let mut position = load_position(&env, pool_id, &staker);
    harvest(&pool, &mut position);

    // Transfer tokens from staker into the contract.
    let token_client = token::Client::new(&env, &pool.token);
    token_client.transfer(&staker, &env.current_contract_address(), &amount);

    // Update position and pool totals.
    position.amount = position.amount.checked_add(amount).ok_or(ContractError::StakeOverflow)?;
    position.last_action_timestamp = env.ledger().timestamp();

    pool.total_staked = pool.total_staked.saturating_add(amount);

    save_position(&env, pool_id, &position);
    save_pool(&env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("stake")),
        (pool_id, staker, amount, position.amount),
    );

    Ok(position.amount)
}

/// Issue #1238: Queue a full or partial unstake for `amount` stroops.
///
/// The funds are locked for `STAKING_UNSTAKE_DELAY_SECS` before
/// `process_unstake` can release them.  Only one pending unstake per
/// staker per pool is allowed at a time.
pub fn queue_unstake(
    env: Env,
    pool_id: u64,
    staker: Address,
    amount: i128,
) -> Result<u64, ContractError> {
    staker.require_auth();
    require_not_paused(&env)?;

    if amount <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    let pool = load_pool(&env, pool_id)?;
    if pool.status == StakingPoolStatus::Closed {
        return Err(ContractError::StakingPoolNotActive);
    }

    let mut position = load_position(&env, pool_id, &staker);
    harvest(&pool, &mut position);

    if amount > position.amount {
        return Err(ContractError::InsufficientFunds);
    }
    if position.pending_unstake {
        return Err(ContractError::WithdrawalAlreadyQueued);
    }

    position.pending_unstake = true;
    position.queued_unstake_amount = amount;
    position.last_action_timestamp = env.ledger().timestamp();

    save_position(&env, pool_id, &position);

    let available_at = env.ledger().timestamp() + STAKING_UNSTAKE_DELAY_SECS;

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("unstakeQ")),
        (pool_id, staker, amount, available_at),
    );

    Ok(available_at)
}

/// Issue #1238: Process a queued unstake after the delay has elapsed.
///
/// Transfers the queued amount (plus any pending yield) back to the staker.
pub fn process_unstake(
    env: Env,
    pool_id: u64,
    staker: Address,
) -> Result<i128, ContractError> {
    staker.require_auth();
    require_not_paused(&env)?;

    let mut pool = load_pool(&env, pool_id)?;
    let mut position = load_position(&env, pool_id, &staker);

    if !position.pending_unstake {
        return Err(ContractError::WithdrawalNotQueued);
    }

    // Enforce the 24-hour delay.
    let elapsed = env.ledger().timestamp().saturating_sub(position.last_action_timestamp);
    if elapsed < STAKING_UNSTAKE_DELAY_SECS {
        return Err(ContractError::TimelockNotReady);
    }

    harvest(&pool, &mut position);

    let unstake_amount = position.queued_unstake_amount;
    let rewards = position.pending_rewards;
    let total_return = unstake_amount.saturating_add(rewards);

    // Update position.
    position.amount = position.amount.saturating_sub(unstake_amount);
    position.pending_rewards = 0;
    position.pending_unstake = false;
    position.queued_unstake_amount = 0;
    position.last_action_timestamp = env.ledger().timestamp();

    // Update pool totals.
    pool.total_staked = pool.total_staked.saturating_sub(unstake_amount);

    // Transfer principal + rewards back to staker.
    let token_client = token::Client::new(&env, &pool.token);
    if total_return > 0 {
        token_client.transfer(&env.current_contract_address(), &staker, &total_return);
    }

    save_position(&env, pool_id, &position);
    save_pool(&env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("unstaked")),
        (pool_id, staker, unstake_amount, rewards),
    );

    Ok(total_return)
}

/// Issue #1238: Claim accumulated yield rewards without unstaking.
///
/// Harvests pending rewards and transfers them to the staker from the yield
/// reserve.  The staked principal remains in the pool.
pub fn claim_yield(
    env: Env,
    pool_id: u64,
    staker: Address,
) -> Result<i128, ContractError> {
    staker.require_auth();
    require_not_paused(&env)?;

    let pool = load_pool(&env, pool_id)?;
    let mut position = load_position(&env, pool_id, &staker);

    harvest(&pool, &mut position);

    let rewards = position.pending_rewards;
    if rewards <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    position.pending_rewards = 0;
    position.last_action_timestamp = env.ledger().timestamp();

    let token_client = token::Client::new(&env, &pool.token);
    token_client.transfer(&env.current_contract_address(), &staker, &rewards);

    save_position(&env, pool_id, &position);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("yield")),
        (pool_id, staker, rewards),
    );

    Ok(rewards)
}

/// Issue #1238: Distribute yield from the lending yield reserve into the pool.
///
/// Called by admins (or automatically triggered on repayment events).
/// `yield_amount` stroops are taken from the yield reserve and spread
/// proportionally across all current stakers via the accumulator.
///
/// Updates `yield_per_token_scaled` and recalculates `current_apy_bps` based
/// on the elapsed time since the last distribution.
pub fn distribute_yield(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
    yield_amount: i128,
) -> Result<(), ContractError> {
    require_not_paused(&env)?;
    require_admin_approval(&env, &admin_signers);

    if yield_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut pool = load_pool(&env, pool_id)?;
    if pool.status != StakingPoolStatus::Active {
        return Err(ContractError::StakingPoolNotActive);
    }
    if pool.total_staked <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    // Deduct from the protocol yield reserve.
    let reserve: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::YieldReserve)
        .unwrap_or(0);
    if reserve < yield_amount {
        return Err(ContractError::InsufficientYieldReserve);
    }
    env.storage()
        .persistent()
        .set(&DataKey::YieldReserve, &(reserve - yield_amount));

    // Update accumulator.
    let delta = yield_amount
        .checked_mul(YIELD_PER_TOKEN_PRECISION)
        .unwrap_or(i128::MAX)
        / pool.total_staked;

    pool.yield_per_token_scaled = pool.yield_per_token_scaled.saturating_add(delta);
    pool.total_yield_distributed = pool.total_yield_distributed.saturating_add(yield_amount);

    // Recalculate APY: annualised yield / total staked, in basis points.
    let now = env.ledger().timestamp();
    let elapsed_secs = now.saturating_sub(pool.last_yield_timestamp);
    if elapsed_secs > 0 && pool.total_staked > 0 {
        // apy_bps = yield_amount * 10_000 * SECS_PER_YEAR / elapsed_secs / total_staked
        const SECS_PER_YEAR: i128 = 365 * 24 * 60 * 60;
        let annualised = yield_amount
            .saturating_mul(10_000)
            .saturating_mul(SECS_PER_YEAR)
            / (elapsed_secs as i128)
            / pool.total_staked;
        pool.current_apy_bps = annualised.min(u32::MAX as i128) as u32;
    }
    pool.last_yield_timestamp = now;

    save_pool(&env, &pool);

    env.events().publish(
        (symbol_short!("pool"), symbol_short!("yieldDist")),
        (pool_id, yield_amount, pool.yield_per_token_scaled),
    );

    Ok(())
}

/// Issue #1238: Get the current APY and pool stats for display.
pub fn get_staking_pool(env: Env, pool_id: u64) -> Result<StakingPool, ContractError> {
    load_pool(&env, pool_id)
}

/// Issue #1238: Get the staker's position in a pool (amount, pending rewards).
pub fn get_staker_position(
    env: Env,
    pool_id: u64,
    staker: Address,
) -> Result<StakerPosition, ContractError> {
    let pool = load_pool(&env, pool_id)?;
    let mut position = load_position(&env, pool_id, &staker);
    // Harvest to surface up-to-date pending_rewards without writing.
    harvest(&pool, &mut position);
    Ok(position)
}
