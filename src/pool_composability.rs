/// Lending Pool Composability Module (Issue #1187)
/// Enables lending pools to integrate with external DeFi protocols,
/// supporting yield farming and cross-protocol asset management.

use crate::errors::ContractError;
use crate::types::DataKey;
use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Symbol, Vec};

/// External pool interface for composability
#[derive(Clone, Debug)]
#[contracttype]
pub struct ExternalPoolInterface {
    /// Unique identifier for the external pool
    pub pool_id: u64,
    /// Name/identifier of the external protocol
    pub protocol_name: String,
    /// Address of the external pool contract
    pub pool_contract: Address,
    /// Type of yield strategy (farming, staking, etc)
    pub strategy_type: String,
    /// Whether this pool is currently active
    pub is_active: bool,
    /// Timestamp when pool was registered
    pub registered_at: u64,
    /// Issue #1469: timestamp of the pool's last reported activity
    /// (a yield earning or a portfolio snapshot touching this pool).
    /// Used to detect pools that have stopped reporting.
    pub last_updated: u64,
}

/// Issue #1469: an external pool paired with whether it is currently stale
/// (has not reported within the configured freshness window).
#[derive(Clone, Debug)]
#[contracttype]
pub struct ExternalPoolStatus {
    pub pool: ExternalPoolInterface,
    pub stale: bool,
}

/// Deposit record to external pool
#[derive(Clone, Debug)]
#[contracttype]
pub struct ExternalPoolDeposit {
    /// Deposit identifier
    pub deposit_id: u64,
    /// Internal pool that made the deposit
    pub internal_pool_id: u64,
    /// External pool receiving the deposit
    pub external_pool_id: u64,
    /// Amount deposited
    pub amount: i128,
    /// Timestamp of deposit
    pub deposit_time: u64,
    /// Yield earned so far
    pub yield_earned: i128,
}

/// Yield earning record
#[derive(Clone, Debug)]
#[contracttype]
pub struct YieldEarning {
    /// Deposit this yield is from
    pub deposit_id: u64,
    /// Amount of yield earned
    pub amount: i128,
    /// Timestamp of earning
    pub earned_at: u64,
    /// APY at time of earning (in basis points)
    pub apy_bps: u32,
}

/// Portfolio allocation across pools
#[derive(Clone, Debug)]
#[contracttype]
pub struct PoolAllocation {
    /// Pool identifier
    pub pool_id: u64,
    /// Allocated amount
    pub amount: i128,
    /// Percentage of total portfolio (in basis points)
    pub allocation_percentage_bps: u32,
    /// Type of pool (internal/external)
    pub pool_type: String,
}

/// Portfolio composition snapshot
#[derive(Clone, Debug)]
#[contracttype]
pub struct PortfolioSnapshot {
    /// Timestamp of snapshot
    pub timestamp: u64,
    /// Total portfolio value
    pub total_value: i128,
    /// Allocations across pools
    pub allocations: Vec<PoolAllocation>,
}

const EXTERNAL_POOLS_KEY: Symbol = symbol_short!("ext_pls");
const EXTERNAL_DEPOSITS_KEY: Symbol = symbol_short!("ext_dps");
const YIELD_EARNINGS_KEY: Symbol = symbol_short!("yld_ern");
const PORTFOLIO_SNAPSHOTS_KEY: Symbol = symbol_short!("prt_snp");
const NEXT_POOL_ID_KEY: Symbol = symbol_short!("nxt_pid");
const NEXT_DEPOSIT_ID_KEY: Symbol = symbol_short!("nxt_did");
const FRESHNESS_WINDOW_KEY: Symbol = symbol_short!("frsh_wnd");

/// Issue #1469: default freshness window (7 days) used when no configurable
/// value has been set — an external pool that hasn't reported (via
/// `record_yield_earning` or `create_portfolio_snapshot`) within this window
/// is treated as stale and excluded from aggregate TVL/APY figures.
const DEFAULT_FRESHNESS_WINDOW_SECS: u64 = 7 * 24 * 60 * 60;

fn freshness_window(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::Custom(FRESHNESS_WINDOW_KEY.into()))
        .unwrap_or(DEFAULT_FRESHNESS_WINDOW_SECS)
}

/// Configure how long an external pool may go without reporting before it is
/// considered stale.
pub fn set_freshness_window(env: &Env, seconds: u64) -> Result<(), ContractError> {
    if seconds == 0 {
        return Err(ContractError::InvalidAmount);
    }
    env.storage()
        .instance()
        .set(&DataKey::Custom(FRESHNESS_WINDOW_KEY.into()), &seconds);
    Ok(())
}

pub fn get_freshness_window(env: &Env) -> u64 {
    freshness_window(env)
}

fn is_pool_stale(env: &Env, pool: &ExternalPoolInterface) -> bool {
    let now = env.ledger().timestamp();
    now.saturating_sub(pool.last_updated) > freshness_window(env)
}

/// Record that an external pool has just reported activity, resetting its
/// staleness clock.
fn touch_pool_last_updated(env: &Env, pool_id: u64, now: u64) {
    let mut pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));

    for i in 0..pools.len() {
        if pools.get(i).unwrap().pool_id == pool_id {
            let mut pool = pools.get(i).unwrap();
            pool.last_updated = now;
            pools.set(i, pool);
            break;
        }
    }

    env.storage()
        .persistent()
        .set(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()), &pools);
}

/// Register an external pool for composability
pub fn register_external_pool(
    env: &Env,
    protocol_name: String,
    pool_contract: Address,
    strategy_type: String,
) -> Result<ExternalPoolInterface, ContractError> {
    if protocol_name.is_empty() || strategy_type.is_empty() {
        return Err(ContractError::InvalidAmount);
    }

    let now = env.ledger().timestamp();

    // Get next pool ID
    let next_pool_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::Custom(NEXT_POOL_ID_KEY.into()))
        .unwrap_or(1u64);

    let pool_interface = ExternalPoolInterface {
        pool_id: next_pool_id,
        protocol_name,
        pool_contract,
        strategy_type,
        is_active: true,
        registered_at: now,
        last_updated: now,
    };

    // Store pool interface
    let mut pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));
    pools.push_back(pool_interface.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()), &pools);

    // Increment pool ID
    env.storage()
        .instance()
        .set(&DataKey::Custom(NEXT_POOL_ID_KEY.into()), &(next_pool_id + 1));

    Ok(pool_interface)
}

/// Deposit assets to an external pool for yield farming
pub fn deposit_to_external_pool(
    env: &Env,
    internal_pool_id: u64,
    external_pool_id: u64,
    amount: i128,
) -> Result<ExternalPoolDeposit, ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let now = env.ledger().timestamp();

    // Get next deposit ID
    let next_deposit_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::Custom(NEXT_DEPOSIT_ID_KEY.into()))
        .unwrap_or(1u64);

    let deposit = ExternalPoolDeposit {
        deposit_id: next_deposit_id,
        internal_pool_id,
        external_pool_id,
        amount,
        deposit_time: now,
        yield_earned: 0,
    };

    // Store deposit record
    let mut deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));
    deposits.push_back(deposit.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()), &deposits);

    // Increment deposit ID
    env.storage()
        .instance()
        .set(&DataKey::Custom(NEXT_DEPOSIT_ID_KEY.into()), &(next_deposit_id + 1));

    Ok(deposit)
}

/// Record yield earned from external pool
pub fn record_yield_earning(
    env: &Env,
    deposit_id: u64,
    amount: i128,
    apy_bps: u32,
) -> Result<YieldEarning, ContractError> {
    if amount <= 0 || apy_bps > 10_000 {
        return Err(ContractError::InvalidAmount);
    }

    let now = env.ledger().timestamp();

    let earning = YieldEarning {
        deposit_id,
        amount,
        earned_at: now,
        apy_bps,
    };

    // Store yield earning
    let mut earnings: Vec<YieldEarning> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(YIELD_EARNINGS_KEY.into()))
        .unwrap_or(Vec::new(env));
    earnings.push_back(earning.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Custom(YIELD_EARNINGS_KEY.into()), &earnings);

    // Update deposit's yield_earned
    let mut deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let mut touched_pool_id: Option<u64> = None;
    for i in 0..deposits.len() {
        if deposits.get(i).unwrap().deposit_id == deposit_id {
            let mut deposit = deposits.get(i).unwrap();
            deposit.yield_earned = deposit.yield_earned.saturating_add(amount);
            touched_pool_id = Some(deposit.external_pool_id);
            deposits.set(i, deposit);
            break;
        }
    }

    env.storage()
        .persistent()
        .set(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()), &deposits);

    // Issue #1469: a reported earning counts as the pool being "alive".
    if let Some(pool_id) = touched_pool_id {
        touch_pool_last_updated(env, pool_id, now);
    }

    Ok(earning)
}

/// Automatically claim all accumulated yields
pub fn claim_all_yields(env: &Env, internal_pool_id: u64) -> Result<i128, ContractError> {
    let deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let mut total_yields = 0i128;

    for deposit in deposits.iter() {
        if deposit.internal_pool_id == internal_pool_id {
            total_yields = total_yields.saturating_add(deposit.yield_earned);
        }
    }

    Ok(total_yields)
}

/// Get aggregated yield from all pools
pub fn get_aggregated_yield(env: &Env, internal_pool_id: u64) -> i128 {
    let deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));

    deposits
        .iter()
        .filter(|d| d.internal_pool_id == internal_pool_id)
        .fold(0i128, |acc, d| acc.saturating_add(d.yield_earned))
}

/// Create a portfolio composition snapshot
pub fn create_portfolio_snapshot(
    env: &Env,
    internal_pool_id: u64,
    total_value: i128,
) -> Result<PortfolioSnapshot, ContractError> {
    if total_value <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let now = env.ledger().timestamp();
    let deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let mut allocations = Vec::new(env);

    // Aggregate by external pool
    for deposit in deposits.iter() {
        if deposit.internal_pool_id == internal_pool_id {
            let allocation_percentage_bps =
                ((deposit.amount as u128 * 10_000) / (total_value as u128)) as u32;

            let allocation = PoolAllocation {
                pool_id: deposit.external_pool_id,
                amount: deposit.amount,
                allocation_percentage_bps,
                pool_type: String::from_slice(env, "external"),
            };

            allocations.push_back(allocation);

            // Issue #1469: a snapshot touching this pool counts as fresh activity.
            touch_pool_last_updated(env, deposit.external_pool_id, now);
        }
    }

    let snapshot = PortfolioSnapshot {
        timestamp: now,
        total_value,
        allocations,
    };

    // Store snapshot
    let mut snapshots: Vec<PortfolioSnapshot> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(PORTFOLIO_SNAPSHOTS_KEY.into()))
        .unwrap_or(Vec::new(env));
    snapshots.push_back(snapshot.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Custom(PORTFOLIO_SNAPSHOTS_KEY.into()), &snapshots);

    Ok(snapshot)
}

/// Get current portfolio allocation
pub fn get_portfolio_allocation(env: &Env, internal_pool_id: u64) -> Vec<PoolAllocation> {
    let deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let total: i128 = deposits
        .iter()
        .filter(|d| d.internal_pool_id == internal_pool_id)
        .map(|d| d.amount)
        .sum();

    if total == 0 {
        return Vec::new(env);
    }

    let mut allocations = Vec::new(env);
    for deposit in deposits.iter() {
        if deposit.internal_pool_id == internal_pool_id {
            let allocation_percentage_bps =
                ((deposit.amount as u128 * 10_000) / (total as u128)) as u32;

            let allocation = PoolAllocation {
                pool_id: deposit.external_pool_id,
                amount: deposit.amount,
                allocation_percentage_bps,
                pool_type: String::from_slice(env, "external"),
            };

            allocations.push_back(allocation);
        }
    }

    allocations
}

/// Get all active external pools, each flagged with whether it has gone
/// stale (Issue #1469: no reported activity within the freshness window).
pub fn get_active_pools(env: &Env) -> Vec<ExternalPoolStatus> {
    let pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let mut active_pools = Vec::new(env);
    for pool in pools.iter() {
        if pool.is_active {
            let stale = is_pool_stale(env, &pool);
            active_pools.push_back(ExternalPoolStatus { pool, stale });
        }
    }

    active_pools
}

/// Get external pool by ID
pub fn get_external_pool(env: &Env, pool_id: u64) -> Result<ExternalPoolInterface, ContractError> {
    let pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));

    pools
        .iter()
        .find(|p| p.pool_id == pool_id)
        .ok_or(ContractError::NotFound)
}

/// Deactivate an external pool
pub fn deactivate_pool(env: &Env, pool_id: u64) -> Result<(), ContractError> {
    let mut pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let mut found = false;
    for i in 0..pools.len() {
        if pools.get(i).unwrap().pool_id == pool_id {
            let mut pool = pools.get(i).unwrap();
            pool.is_active = false;
            pools.set(i, pool);
            found = true;
            break;
        }
    }

    if !found {
        return Err(ContractError::NotFound);
    }

    env.storage()
        .persistent()
        .set(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()), &pools);

    Ok(())
}

/// Look up whether a given external pool id is currently stale. A pool that
/// no longer exists is treated as stale (its figures should not be trusted).
fn pool_is_stale_by_id(env: &Env, pools: &Vec<ExternalPoolInterface>, external_pool_id: u64) -> bool {
    pools
        .iter()
        .find(|p| p.pool_id == external_pool_id)
        .map(|p| is_pool_stale(env, &p))
        .unwrap_or(true)
}

/// Get total value locked across all external pools. Issue #1469: deposits
/// tied to a pool that has gone stale (no recent report) are excluded so a
/// dead integration cannot keep inflating the aggregate TVL indefinitely.
pub fn get_total_external_tvl(env: &Env) -> i128 {
    let deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));
    let pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));

    deposits.iter().fold(0i128, |acc, d| {
        if pool_is_stale_by_id(env, &pools, d.external_pool_id) {
            acc
        } else {
            acc.saturating_add(d.amount)
        }
    })
}

/// Calculate weighted average APY for a pool. Issue #1469: deposits tied to
/// a stale external pool are excluded from both the weighting base and the
/// APY sum.
pub fn calculate_weighted_avg_apy(env: &Env, internal_pool_id: u64) -> Result<u32, ContractError> {
    let earnings: Vec<YieldEarning> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(YIELD_EARNINGS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let deposits: Vec<ExternalPoolDeposit> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_DEPOSITS_KEY.into()))
        .unwrap_or(Vec::new(env));
    let pools: Vec<ExternalPoolInterface> = env
        .storage()
        .persistent()
        .get(&DataKey::Custom(EXTERNAL_POOLS_KEY.into()))
        .unwrap_or(Vec::new(env));

    let is_fresh_deposit = |d: &ExternalPoolDeposit| -> bool {
        d.internal_pool_id == internal_pool_id && !pool_is_stale_by_id(env, &pools, d.external_pool_id)
    };

    let total_amount: i128 = deposits
        .iter()
        .filter(|d| is_fresh_deposit(d))
        .fold(0i128, |acc, d| acc.saturating_add(d.amount));

    if total_amount <= 0 {
        return Err(ContractError::NotFound);
    }

    let mut weighted_apy = 0u64;
    for deposit in deposits.iter() {
        if !is_fresh_deposit(&deposit) {
            continue;
        }
        let weight = ((deposit.amount as u128 * 10_000) / (total_amount as u128)) as u64;
        for earning in earnings.iter() {
            if earning.deposit_id == deposit.deposit_id {
                weighted_apy = weighted_apy.saturating_add((earning.apy_bps as u64 * weight) / 10_000);
            }
        }
    }

    Ok(weighted_apy as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_allocation_calculation() {
        // Test that allocation percentages are calculated correctly
        let amount = 500i128;
        let total = 1000i128;
        let allocation_percentage_bps = ((amount as u128 * 10_000) / (total as u128)) as u32;

        assert_eq!(allocation_percentage_bps, 5000); // 50%
    }
}
