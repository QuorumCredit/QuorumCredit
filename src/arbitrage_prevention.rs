//! Arbitrage Prevention Module
//!
//! This module prevents exchange rate arbitrage by tracking reference rates,
//! enforcing slippage limits, and detecting unusual exchange rate patterns.
//!
//! Issue #967: Prevent exchange rate arbitrage across tokens
//!
//! Key features:
//! - Reference rate caching and updates
//! - Slippage protection on token swaps/conversions
//! - Detection of arbitrage opportunities
//! - Multi-token exchange rate validation

use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::DataKey;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Maximum allowed slippage in basis points (1000 = 10%)
pub const DEFAULT_MAX_SLIPPAGE_BPS: u32 = 1000;

/// Reference exchange rate between two tokens
#[contracttype]
#[derive(Clone, Copy, Debug)]
pub struct ExchangeRate {
    /// Token A address
    pub token_a: Address,
    /// Token B address
    pub token_b: Address,
    /// Rate: token_a_amount / token_b_amount
    pub rate: i128,
    /// Rate update timestamp (ledger seconds)
    pub updated_at: u64,
    /// Maximum slippage tolerance for this pair (basis points)
    pub max_slippage_bps: u32,
}

/// Pending exchange rate update (before admin approval)
#[contracttype]
#[derive(Clone)]
struct PendingRateUpdate {
    token_a: Address,
    token_b: Address,
    new_rate: i128,
    proposed_at: u64,
}

/// Historical rate data for detecting anomalies
#[contracttype]
#[derive(Clone, Copy)]
struct RateHistory {
    min_rate: i128,
    max_rate: i128,
    avg_rate: i128,
    last_updated: u64,
}

/// Admin: Register a new token pair for exchange rate tracking
pub fn register_token_pair(
    env: Env,
    admin_signers: Vec<Address>,
    token_a: Address,
    token_b: Address,
    initial_rate: i128,
    max_slippage_bps: u32,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if token_a == token_b {
        return Err(ContractError::InvalidToken);
    }

    if initial_rate <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    if max_slippage_bps > 10_000 {
        return Err(ContractError::InvalidBps);
    }

    let rate = ExchangeRate {
        token_a: token_a.clone(),
        token_b: token_b.clone(),
        rate: initial_rate,
        updated_at: env.ledger().timestamp(),
        max_slippage_bps,
    };

    env.storage()
        .persistent()
        .set(&DataKey::ExchangeRate(token_a, token_b), &rate);

    Ok(())
}

/// Admin: Update an exchange rate with a new value
/// Updates are accepted only if within configured slippage tolerance
pub fn update_exchange_rate(
    env: Env,
    admin_signers: Vec<Address>,
    token_a: Address,
    token_b: Address,
    new_rate: i128,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if new_rate <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let current_rate: ExchangeRate = env
        .storage()
        .persistent()
        .get(&DataKey::ExchangeRate(token_a.clone(), token_b.clone()))
        .ok_or(ContractError::InvalidToken)?;

    // Calculate percentage change
    let rate_change = calculate_percentage_change(current_rate.rate, new_rate)?;

    // Check if change exceeds max slippage
    if rate_change.abs() > current_rate.max_slippage_bps as i128 {
        return Err(ContractError::InvalidAmount); // Use InvalidAmount for out-of-bounds rate changes
    }

    let updated_rate = ExchangeRate {
        token_a: token_a.clone(),
        token_b: token_b.clone(),
        rate: new_rate,
        updated_at: env.ledger().timestamp(),
        max_slippage_bps: current_rate.max_slippage_bps,
    };

    // Update rate history
    update_rate_history(&env, &token_a, &token_b, new_rate)?;

    env.storage()
        .persistent()
        .set(&DataKey::ExchangeRate(token_a, token_b), &updated_rate);

    Ok(())
}

/// Check if a proposed exchange would cause arbitrage
/// Returns error if the exchange rate deviates too much from reference
pub fn validate_exchange(
    env: Env,
    token_a: Address,
    token_b: Address,
    offered_rate: i128,
) -> Result<(), ContractError> {
    let current_rate: ExchangeRate = env
        .storage()
        .persistent()
        .get(&DataKey::ExchangeRate(token_a.clone(), token_b.clone()))
        .ok_or(ContractError::InvalidToken)?;

    // Calculate deviation from reference rate
    let deviation = calculate_percentage_change(current_rate.rate, offered_rate)?;

    // Check if deviation exceeds tolerance
    if deviation.abs() > current_rate.max_slippage_bps as i128 {
        return Err(ContractError::InvalidAmount);
    }

    Ok(())
}

/// Get the current exchange rate between two tokens
pub fn get_exchange_rate(
    env: Env,
    token_a: Address,
    token_b: Address,
) -> Result<i128, ContractError> {
    let rate: ExchangeRate = env
        .storage()
        .persistent()
        .get(&DataKey::ExchangeRate(token_a, token_b))
        .ok_or(ContractError::InvalidToken)?;

    Ok(rate.rate)
}

/// Get detailed exchange rate information
pub fn get_exchange_rate_info(
    env: Env,
    token_a: Address,
    token_b: Address,
) -> Result<ExchangeRate, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::ExchangeRate(token_a, token_b))
        .ok_or(ContractError::InvalidToken)
}

/// Calculate percentage change between two rates (in basis points)
/// Returns the percentage change in basis points (1 = 0.01%)
fn calculate_percentage_change(old_rate: i128, new_rate: i128) -> Result<i128, ContractError> {
    if old_rate == 0 {
        return Err(ContractError::InvalidAmount);
    }

    let change = new_rate.saturating_sub(old_rate);
    let percentage_bps = (change * 10_000) / old_rate;

    Ok(percentage_bps)
}

/// Update rate history with new rate data
fn update_rate_history(
    env: &Env,
    token_a: &Address,
    token_b: &Address,
    new_rate: i128,
) -> Result<(), ContractError> {
    let history_key = DataKey::RateHistory(token_a.clone(), token_b.clone());

    let mut history: RateHistory = env
        .storage()
        .persistent()
        .get(&history_key)
        .unwrap_or(RateHistory {
            min_rate: new_rate,
            max_rate: new_rate,
            avg_rate: new_rate,
            last_updated: env.ledger().timestamp(),
        });

    // Update min/max/avg
    if new_rate < history.min_rate {
        history.min_rate = new_rate;
    }
    if new_rate > history.max_rate {
        history.max_rate = new_rate;
    }

    // Simple rolling average (weighted toward recent data)
    history.avg_rate = (history.avg_rate + new_rate) / 2;
    history.last_updated = env.ledger().timestamp();

    env.storage()
        .persistent()
        .set(&history_key, &history);

    Ok(())
}

/// Detect if current rate represents an arbitrage opportunity
/// (rate deviation from average exceeds threshold)
pub fn detect_arbitrage_opportunity(
    env: Env,
    token_a: Address,
    token_b: Address,
    deviation_threshold_bps: u32,
) -> Result<bool, ContractError> {
    let current_rate: ExchangeRate = env
        .storage()
        .persistent()
        .get(&DataKey::ExchangeRate(token_a.clone(), token_b.clone()))
        .ok_or(ContractError::InvalidToken)?;

    let history_key = DataKey::RateHistory(token_a, token_b);
    let history: RateHistory = env
        .storage()
        .persistent()
        .get(&history_key)
        .ok_or(ContractError::InvalidToken)?;

    let deviation = calculate_percentage_change(history.avg_rate, current_rate.rate)?;

    // Return true if deviation exceeds threshold
    Ok(deviation.abs() > deviation_threshold_bps as i128)
}

/// Admin: Adjust max slippage tolerance for a token pair
pub fn set_max_slippage(
    env: Env,
    admin_signers: Vec<Address>,
    token_a: Address,
    token_b: Address,
    new_max_slippage_bps: u32,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if new_max_slippage_bps > 10_000 {
        return Err(ContractError::InvalidBps);
    }

    let mut current_rate: ExchangeRate = env
        .storage()
        .persistent()
        .get(&DataKey::ExchangeRate(token_a.clone(), token_b.clone()))
        .ok_or(ContractError::InvalidToken)?;

    current_rate.max_slippage_bps = new_max_slippage_bps;

    env.storage()
        .persistent()
        .set(&DataKey::ExchangeRate(token_a, token_b), &current_rate);

    Ok(())
}
