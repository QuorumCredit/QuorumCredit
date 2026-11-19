use soroban_sdk::{symbol_short, Address, Bytes, Env};

use crate::errors::ContractError;

/// Breach reason recorded when a metric falls below its SLA threshold.
const SLA_BREACH_REASON: &[u8] = b"sla_below_threshold";

pub fn register_performance_contract(
    env: &Env,
    attestor: Address,
    contract: Bytes,
) -> Result<(), ContractError> {
    attestor.require_auth();
    let key = ("perf_contract", &attestor);
    env.storage().persistent().set(&key, &contract);
    Ok(())
}

pub fn get_performance_contract(env: &Env, attestor: &Address) -> Option<Bytes> {
    let key = ("perf_contract", attestor);
    env.storage().persistent().get(&key)
}

/// Store the SLA threshold for `attestor`. The threshold is the minimum
/// acceptable value of the attestor's performance metric: a metric strictly
/// below it is a breach.
pub fn set_performance_sla(
    env: &Env,
    attestor: Address,
    min_metric: u64,
) -> Result<(), ContractError> {
    attestor.require_auth();
    let key = ("perf_sla", &attestor);
    env.storage().persistent().set(&key, &min_metric);
    Ok(())
}

pub fn get_performance_sla(env: &Env, attestor: &Address) -> Option<u64> {
    let key = ("perf_sla", attestor);
    env.storage().persistent().get(&key)
}

/// Mark `attestor` as tracked and evaluate its current metric against its SLA.
///
/// If both an SLA and a metric exist and the metric is below the SLA, a breach
/// is recorded and a `perf_breach` event is emitted. Each call that finds the
/// metric below the SLA records one breach, so repeated calls while the metric
/// stays below the threshold count repeatedly.
pub fn track_performance_against_slas(
    env: &Env,
    attestor: &Address,
) -> Result<(), ContractError> {
    attestor.require_auth();
    let key = ("perf_tracked", attestor);
    env.storage().persistent().set(&key, &true);

    if let (Some(sla), Some(metric)) = (
        get_performance_sla(env, attestor),
        get_performance_metric(env, attestor),
    ) {
        if metric < sla {
            let reason = Bytes::from_slice(env, SLA_BREACH_REASON);
            write_breach(env, attestor, reason);
            env.events().publish(
                (symbol_short!("perf_brch"), attestor.clone()),
                (metric, sla),
            );
        }
    }
    Ok(())
}

pub fn get_performance_tracking_status(env: &Env, attestor: &Address) -> bool {
    let key = ("perf_tracked", attestor);
    env.storage().persistent().get(&key).unwrap_or(false)
}

pub fn record_performance_metric(
    env: &Env,
    attestor: &Address,
    metric_value: u64,
) -> Result<(), ContractError> {
    attestor.require_auth();
    let key = ("perf_metric", attestor);
    env.storage().persistent().set(&key, &metric_value);
    Ok(())
}

pub fn get_performance_metric(env: &Env, attestor: &Address) -> Option<u64> {
    let key = ("perf_metric", attestor);
    env.storage().persistent().get(&key)
}

pub fn implement_breach_notifications(
    env: &Env,
    attestor: &Address,
    breach_reason: Bytes,
) -> Result<(), ContractError> {
    attestor.require_auth();
    write_breach(env, attestor, breach_reason);
    Ok(())
}

fn write_breach(env: &Env, attestor: &Address, breach_reason: Bytes) {
    let key = ("perf_breach", attestor);
    env.storage().persistent().set(&key, &breach_reason);

    let count_key = ("perf_breach_count", attestor);
    let count: u64 = env.storage().persistent().get(&count_key).unwrap_or(0);
    env.storage().persistent().set(&count_key, &(count + 1));
}

pub fn get_breach_notification(env: &Env, attestor: &Address) -> Option<Bytes> {
    let key = ("perf_breach", attestor);
    env.storage().persistent().get(&key)
}

pub fn get_breach_count(env: &Env, attestor: &Address) -> u64 {
    let key = ("perf_breach_count", attestor);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn clear_breach_notification(env: &Env, attestor: &Address) -> Result<(), ContractError> {
    attestor.require_auth();
    let key = ("perf_breach", attestor);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().remove(&key);
    }
    Ok(())
}
