use soroban_sdk::{Address, Bytes, Env};

use crate::types::Result;

pub struct RebalancingConfig {
    pub target_weight: u32,
    pub rebalance_interval: u64,
    pub enabled: bool,
}

pub fn enable_dynamic_rebalancing(
    env: &Env,
    slice_id: &Address,
    parameters: Bytes,
) -> Result<()> {
    let key = ("rebalance_enabled", slice_id);
    env.storage().persistent().set(&key, &true);

    let key_params = ("rebalance_params", slice_id);
    env.storage().persistent().set(&key_params, &parameters);

    Ok(())
}

pub fn disable_dynamic_rebalancing(env: &Env, slice_id: &Address) -> Result<()> {
    let key = ("rebalance_enabled", slice_id);
    env.storage().persistent().set(&key, &false);
    Ok(())
}

pub fn is_rebalancing_enabled(env: &Env, slice_id: &Address) -> bool {
    let key = ("rebalance_enabled", slice_id);
    env.storage().persistent().get(&key).unwrap_or(false)
}

pub fn get_rebalancing_parameters(env: &Env, slice_id: &Address) -> Option<Bytes> {
    let key_params = ("rebalance_params", slice_id);
    env.storage().persistent().get(&key_params)
}

pub fn execute_rebalancing(env: &Env, slice_id: &Address) -> Result<()> {
    let key = ("rebalance_enabled", slice_id);
    if !env.storage().persistent().get::<_, bool>(&key).unwrap_or(false) {
        return Ok(());
    }

    let event_key = ("rebalance_event_count", slice_id);
    let count: u64 = env.storage().persistent().get(&event_key).unwrap_or(0);
    env.storage().persistent().set(&event_key, &(count + 1));

    Ok(())
}

pub fn get_rebalancing_event_count(env: &Env, slice_id: &Address) -> u64 {
    let event_key = ("rebalance_event_count", slice_id);
    env.storage().persistent().get(&event_key).unwrap_or(0)
}

pub fn implement_periodic_rebalancing(env: &Env, slice_id: &Address) -> Result<()> {
    execute_rebalancing(env, slice_id)
}
