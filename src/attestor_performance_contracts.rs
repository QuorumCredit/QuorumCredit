use soroban_sdk::{Address, Bytes, Env};

use crate::types::Result;

pub fn register_performance_contract(
    env: &Env,
    attestor: Address,
    contract: Bytes,
) -> Result<()> {
    let key = ("perf_contract", &attestor);
    env.storage().persistent().set(&key, &contract);
    Ok(())
}

pub fn get_performance_contract(env: &Env, attestor: &Address) -> Option<Bytes> {
    let key = ("perf_contract", attestor);
    env.storage().persistent().get(&key)
}

pub fn track_performance_against_slas(
    env: &Env,
    attestor: &Address,
) -> Result<()> {
    let key = ("perf_tracked", attestor);
    env.storage().persistent().set(&key, &true);
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
) -> Result<()> {
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
) -> Result<()> {
    let key = ("perf_breach", attestor);
    env.storage().persistent().set(&key, &breach_reason);

    let count_key = ("perf_breach_count", attestor);
    let count: u64 = env.storage().persistent().get(&count_key).unwrap_or(0);
    env.storage().persistent().set(&count_key, &(count + 1));

    Ok(())
}

pub fn get_breach_notification(env: &Env, attestor: &Address) -> Option<Bytes> {
    let key = ("perf_breach", attestor);
    env.storage().persistent().get(&key)
}

pub fn get_breach_count(env: &Env, attestor: &Address) -> u64 {
    let key = ("perf_breach_count", attestor);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn clear_breach_notification(env: &Env, attestor: &Address) -> Result<()> {
    let key = ("perf_breach", attestor);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().remove(&key);
    }
    Ok(())
}
