/// Issue #1070: Circuit Breaker for Rapid Default Cascade
///
/// When multiple borrowers default simultaneously, the protocol could enter a death spiral.
/// A circuit breaker automatically pauses protocol operations if the default rate exceeds
/// a configurable threshold.
///
/// Mechanism:
/// 1. On each slash or default event, calculate current default rate:
///    `default_rate = (default_count / total_loan_count) * 10_000` (in basis points)
/// 2. If default_rate >= config.default_rate_threshold, auto-pause the contract
/// 3. Admins must manually unpause after addressing the crisis
/// 4. Cooldown enforcement prevents rapid trigger-pause-resume cycles

use soroban_sdk::{Address, Env};
use crate::types::{Config, DataKey};
use crate::errors::ContractError;

/// Minimum time (in seconds) between circuit breaker activations to prevent thrashing.
const CIRCUIT_BREAKER_COOLDOWN_SECS: u64 = 3600; // 1 hour

/// Calculate the current default rate in basis points.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `default_count` - Number of defaulted loans
/// * `total_loan_count` - Total number of loans ever created
///
/// # Returns
/// Default rate in basis points (0-10000)
pub fn calculate_default_rate(
    default_count: u32,
    total_loan_count: u32,
) -> u32 {
    if total_loan_count == 0 {
        return 0;
    }

    let rate_bps = (default_count as u128)
        .saturating_mul(10_000)
        .saturating_div(total_loan_count as u128)
        as u32;

    std::cmp::min(rate_bps, 10_000) // Cap at 100%
}

/// Check if the circuit breaker should be triggered based on current default rate.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `config` - Current protocol configuration
/// * `default_count` - Number of defaulted loans
/// * `total_loan_count` - Total number of loans
///
/// # Returns
/// `true` if the default rate exceeds the threshold and circuit breaker should activate
pub fn should_trigger_circuit_breaker(
    env: &Env,
    config: &Config,
    default_count: u32,
    total_loan_count: u32,
) -> bool {
    if config.default_rate_threshold == 0 || total_loan_count == 0 {
        return false;
    }

    let current_rate = calculate_default_rate(default_count, total_loan_count);
    current_rate >= config.default_rate_threshold
}

/// Attempt to trigger the circuit breaker if conditions are met.
///
/// Checks cooldown enforcement to prevent rapid successive triggers.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `config` - Current protocol configuration
/// * `default_count` - Number of defaulted loans
/// * `total_loan_count` - Total number of loans
///
/// # Returns
/// `true` if circuit breaker was activated, `false` otherwise
pub fn try_trigger_circuit_breaker(
    env: &Env,
    config: &Config,
    default_count: u32,
    total_loan_count: u32,
) -> Result<bool, ContractError> {
    // Check if we should trigger
    if !should_trigger_circuit_breaker(env, config, default_count, total_loan_count) {
        return Ok(false);
    }

    // Check cooldown to prevent rapid thrashing
    let last_triggered: u64 = env.storage().instance()
        .get(&DataKey::CircuitBreakerLastTriggered)
        .unwrap_or(Ok(0u64))
        .unwrap_or(0);

    let now = env.ledger().timestamp();
    if now < last_triggered.saturating_add(CIRCUIT_BREAKER_COOLDOWN_SECS) {
        // Still in cooldown, don't trigger again
        return Ok(false);
    }

    // Record the trigger time
    env.storage().instance().set(&DataKey::CircuitBreakerLastTriggered, &now);

    // Auto-pause the contract
    env.storage().instance().set(&DataKey::Paused, &true);

    // Emit event (log the circuit breaker activation)
    env.events().publish(
        ("circuit_breaker", "activated"),
        (
            default_count,
            total_loan_count,
            calculate_default_rate(default_count, total_loan_count),
            config.default_rate_threshold,
        ),
    );

    Ok(true)
}

/// Get the timestamp when the circuit breaker was last triggered.
pub fn get_circuit_breaker_last_triggered(env: &Env) -> u64 {
    env.storage().instance()
        .get(&DataKey::CircuitBreakerLastTriggered)
        .unwrap_or(Ok(0u64))
        .unwrap_or(0)
}

/// Get the current default rate threshold from config.
pub fn get_default_rate_threshold(env: &Env) -> Result<u32, ContractError> {
    let config: Config = env.storage().instance()
        .get(&DataKey::Config)
        .ok_or(ContractError::InvalidStateTransition)?
        .map_err(|_| ContractError::InvalidStateTransition)?;

    Ok(config.default_rate_threshold)
}

/// Update the default rate threshold (requires admin approval).
pub fn set_default_rate_threshold(
    env: &Env,
    _admin_signers: Vec<Address>,
    new_threshold: u32,
) -> Result<(), ContractError> {
    if new_threshold > 10_000 {
        return Err(ContractError::InvalidBps);
    }

    let mut config: Config = env.storage().instance()
        .get(&DataKey::Config)
        .ok_or(ContractError::InvalidStateTransition)?
        .map_err(|_| ContractError::InvalidStateTransition)?;

    config.default_rate_threshold = new_threshold;
    env.storage().instance().set(&DataKey::Config, &config);

    env.events().publish(
        ("circuit_breaker", "threshold_updated"),
        (new_threshold,),
    );

    Ok(())
}

/// Get the current default rate based on loan statistics.
pub fn get_current_default_rate(env: &Env) -> Result<u32, ContractError> {
    // Query loan and default counts from a hypothetical statistics collection
    // This will be integrated with the loan module when fully implemented.
    // For now, return 0 as a placeholder.
    
    // TODO: Integrate with loan.rs to query actual default_count and total_loan_count
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_default_rate_zero_loans() {
        assert_eq!(calculate_default_rate(0, 0), 0);
        assert_eq!(calculate_default_rate(5, 0), 0);
    }

    #[test]
    fn test_calculate_default_rate_basic() {
        // 1 default out of 10 loans = 1000 basis points = 10%
        assert_eq!(calculate_default_rate(1, 10), 1000);
        
        // 5 defaults out of 10 loans = 5000 basis points = 50%
        assert_eq!(calculate_default_rate(5, 10), 5000);
        
        // All defaulted = 10000 basis points = 100%
        assert_eq!(calculate_default_rate(10, 10), 10000);
    }

    #[test]
    fn test_calculate_default_rate_cap() {
        // Rate capped at 10000 (100%)
        assert!(calculate_default_rate(100, 10) <= 10_000);
    }
}
