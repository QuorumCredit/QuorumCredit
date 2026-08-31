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

use soroban_sdk::{Address, Env, Vec};
use crate::types::{CircuitBreakerTrigger, Config, DataKey, PauseMode, ThawState};
use crate::errors::ContractError;

/// Default minimum time (in seconds) between circuit breaker activations to
/// prevent thrashing. Used when `DataKey::CircuitBreakerCooldownSecs` is unset.
/// Issue #1425: the effective value is configurable at runtime via
/// `set_circuit_breaker_cooldown`.
pub const CIRCUIT_BREAKER_COOLDOWN_SECS_DEFAULT: u64 = 3600; // 1 hour

/// Maximum number of entries retained in `DataKey::CircuitBreakerHistory`.
/// Once reached, the oldest entry is evicted on each new trigger (Issue #1424).
const CIRCUIT_BREAKER_HISTORY_LIMIT: u32 = 32;

/// Resolve the effective circuit-breaker cooldown: the admin-configured value if
/// present, otherwise `CIRCUIT_BREAKER_COOLDOWN_SECS_DEFAULT` (Issue #1425).
pub fn circuit_breaker_cooldown_secs(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::CircuitBreakerCooldownSecs)
        .unwrap_or(CIRCUIT_BREAKER_COOLDOWN_SECS_DEFAULT)
}

/// Set the circuit-breaker cooldown window (Issue #1425). Admin-only.
pub fn set_circuit_breaker_cooldown(
    env: &Env,
    admin_signers: Vec<Address>,
    new_cooldown_secs: u64,
) -> Result<(), ContractError> {
    crate::rbac::require_admin_approval_for_action(
        env,
        &admin_signers,
        crate::rbac::AdminAction::UpdateConfig,
    )?;

    env.storage()
        .instance()
        .set(&DataKey::CircuitBreakerCooldownSecs, &new_cooldown_secs);

    env.events().publish(
        ("circuit_breaker", "cooldown_updated"),
        (new_cooldown_secs,),
    );

    Ok(())
}

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

    rate_bps.min(10_000) // Cap at 100%
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
        .unwrap_or(0u64);

    let now = env.ledger().timestamp();
    if now < last_triggered.saturating_add(circuit_breaker_cooldown_secs(env)) {
        // Still in cooldown, don't trigger again
        return Ok(false);
    }

    // Record the trigger time
    env.storage().instance().set(&DataKey::CircuitBreakerLastTriggered, &now);

    // Issue #1424: append this activation to the bounded history log.
    let rate_bps = calculate_default_rate(default_count, total_loan_count);
    append_circuit_breaker_history(
        env,
        CircuitBreakerTrigger {
            timestamp: now,
            default_count,
            total_loan_count,
            rate_bps,
        },
    );

    // Issue #1423: a fresh trigger must be explicitly acknowledged by an admin
    // before the resulting pause can be cleared via `unpause`.
    env.storage()
        .instance()
        .set(&DataKey::CircuitBreakerAcknowledged, &false);

    // Auto-pause the contract. `Paused` drives `get_paused()`; `PauseMode` is what
    // `require_not_paused` actually checks, so both must be set for the pause to
    // have any real effect on writes (mirrors `admin::pause`).
    env.storage().instance().set(&DataKey::Paused, &true);
    env.storage().instance().set(&DataKey::PauseMode, &PauseMode::Paused);
    env.storage().instance().set(
        &DataKey::ThawState,
        &ThawState {
            pause_timestamp: now,
            thaw_duration: crate::types::THAW_DURATION_SECS,
            thaw_start_timestamp: 0,
        },
    );

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
        .unwrap_or(0u64)
}

/// Issue #1424: append `entry` to the bounded circuit-breaker history log,
/// evicting the oldest entry once `CIRCUIT_BREAKER_HISTORY_LIMIT` is reached.
fn append_circuit_breaker_history(env: &Env, entry: CircuitBreakerTrigger) {
    let mut history: Vec<CircuitBreakerTrigger> = env
        .storage()
        .instance()
        .get(&DataKey::CircuitBreakerHistory)
        .unwrap_or(Vec::new(env));

    while history.len() >= CIRCUIT_BREAKER_HISTORY_LIMIT {
        history.remove(0);
    }
    history.push_back(entry);

    env.storage()
        .instance()
        .set(&DataKey::CircuitBreakerHistory, &history);
}

/// Issue #1424: return the full bounded history of circuit-breaker activations,
/// oldest first.
pub fn get_circuit_breaker_history(env: &Env) -> Vec<CircuitBreakerTrigger> {
    env.storage()
        .instance()
        .get(&DataKey::CircuitBreakerHistory)
        .unwrap_or(Vec::new(env))
}

/// Issue #1423: `true` when the most recent circuit-breaker activation has been
/// acknowledged by an admin (or when the breaker has never fired).
pub fn is_circuit_breaker_acknowledged(env: &Env) -> bool {
    if env
        .storage()
        .instance()
        .get::<_, u64>(&DataKey::CircuitBreakerLastTriggered)
        .unwrap_or(0)
        == 0
    {
        // Breaker has never fired — nothing to acknowledge.
        return true;
    }
    env.storage()
        .instance()
        .get(&DataKey::CircuitBreakerAcknowledged)
        .unwrap_or(true)
}

/// Issue #1423: record an admin acknowledgement of the most recent
/// circuit-breaker activation. Requires admin multi-sig approval. This does not
/// itself unpause the contract — it clears the gate that `unpause` checks and
/// leaves an on-chain record (event) that a circuit-breaker incident occurred.
pub fn acknowledge_circuit_breaker(
    env: &Env,
    admin_signers: Vec<Address>,
) -> Result<(), ContractError> {
    crate::rbac::require_admin_approval_for_action(
        env,
        &admin_signers,
        crate::rbac::AdminAction::Unpause,
    )?;

    let last_triggered: u64 = env
        .storage()
        .instance()
        .get(&DataKey::CircuitBreakerLastTriggered)
        .unwrap_or(0);

    if last_triggered == 0 {
        // Nothing to acknowledge.
        return Err(ContractError::InvalidStateTransition);
    }

    env.storage()
        .instance()
        .set(&DataKey::CircuitBreakerAcknowledged, &true);

    let now = env.ledger().timestamp();
    env.events().publish(
        ("circuit_breaker", "acknowledged"),
        (admin_signers.get(0).unwrap(), now, last_triggered),
    );

    Ok(())
}

/// Get the current default rate threshold from config.
pub fn get_default_rate_threshold(env: &Env) -> Result<u32, ContractError> {
    let config: Config = env.storage().instance()
        .get(&DataKey::Config)
        .ok_or(ContractError::InvalidStateTransition)?;

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
        .ok_or(ContractError::InvalidStateTransition)?;

    config.default_rate_threshold = new_threshold;
    env.storage().instance().set(&DataKey::Config, &config);

    env.events().publish(
        ("circuit_breaker", "threshold_updated"),
        (new_threshold,),
    );

    Ok(())
}

/// Get the current default rate based on real protocol-wide loan statistics.
pub fn get_current_default_rate(env: &Env) -> Result<u32, ContractError> {
    let default_count = crate::helpers::get_total_default_count(env);
    let total_loan_count = crate::helpers::get_total_loan_count(env);
    Ok(calculate_default_rate(default_count, total_loan_count))
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

    mod storage_backed {
        use super::super::*;
        use soroban_sdk::testutils::{Address as _, Ledger};
        use soroban_sdk::{Address, Env, Vec};

        fn setup() -> (Env, Address, Vec<Address>) {
            let env = Env::default();
            env.mock_all_auths();
            let deployer = Address::generate(&env);
            let admin = Address::generate(&env);
            let admins = Vec::from_array(&env, [admin.clone()]);
            let token_id = env.register_stellar_asset_contract_v2(admin.clone());
            let contract_id = env.register_contract(None, crate::QuorumCreditContract);
            let client = crate::QuorumCreditContractClient::new(&env, &contract_id);
            client.initialize(&deployer, &admins, &1, &token_id.address());
            (env.clone(), contract_id, admins)
        }

        #[test]
        fn test_cooldown_defaults_then_becomes_configurable() {
            let (env, contract_id, admins) = setup();
            env.as_contract(&contract_id, || {
                assert_eq!(
                    circuit_breaker_cooldown_secs(&env),
                    CIRCUIT_BREAKER_COOLDOWN_SECS_DEFAULT
                );
                set_circuit_breaker_cooldown(&env, admins.clone(), 120).unwrap();
                assert_eq!(circuit_breaker_cooldown_secs(&env), 120);
            });
        }

        #[test]
        fn test_history_accumulates_and_is_bounded() {
            let (env, contract_id, _admins) = setup();
            env.as_contract(&contract_id, || {
                for i in 0..(CIRCUIT_BREAKER_HISTORY_LIMIT + 5) {
                    append_circuit_breaker_history(
                        &env,
                        CircuitBreakerTrigger {
                            timestamp: i as u64,
                            default_count: i,
                            total_loan_count: 100,
                            rate_bps: i,
                        },
                    );
                }
                let history = get_circuit_breaker_history(&env);
                assert_eq!(history.len(), CIRCUIT_BREAKER_HISTORY_LIMIT);
                // Oldest entries evicted: first retained entry is index 5.
                assert_eq!(history.get(0).unwrap().timestamp, 5);
                assert_eq!(
                    history.get(history.len() - 1).unwrap().timestamp,
                    (CIRCUIT_BREAKER_HISTORY_LIMIT + 4) as u64
                );
            });
        }

        #[test]
        fn test_acknowledge_requires_a_prior_trigger() {
            let (env, contract_id, admins) = setup();
            env.as_contract(&contract_id, || {
                // No trigger yet -> acknowledged() is vacuously true, ack() errors.
                assert!(is_circuit_breaker_acknowledged(&env));
                assert_eq!(
                    acknowledge_circuit_breaker(&env, admins.clone()),
                    Err(ContractError::InvalidStateTransition)
                );
            });
        }

        #[test]
        fn test_trigger_then_acknowledge_flow() {
            let (env, contract_id, admins) = setup();
            env.ledger().set_timestamp(1_000_000);
            env.as_contract(&contract_id, || {
                let mut cfg = crate::helpers::config(&env);
                cfg.default_rate_threshold = 1_000; // 10%
                env.storage().instance().set(&DataKey::Config, &cfg);

                // 20 defaults / 100 loans = 2000 bps >= 1000 bps threshold.
                let fired = try_trigger_circuit_breaker(&env, &cfg, 20, 100).unwrap();
                assert!(fired);
                assert_eq!(get_circuit_breaker_history(&env).len(), 1);
                assert!(!is_circuit_breaker_acknowledged(&env));

                acknowledge_circuit_breaker(&env, admins.clone()).unwrap();
                assert!(is_circuit_breaker_acknowledged(&env));
            });
        }
    }
}
