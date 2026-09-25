use soroban_sdk::{Address, Env};

use crate::types::Result;

pub fn migrate_credential_slice(
    env: &Env,
    credential_id: &Address,
    old_slice: &Address,
    new_slice: &Address,
) -> Result<()> {
    let key = ("credential_slice", credential_id);
    env.storage().persistent().set(&key, new_slice);

    let history_key = ("migration_history", credential_id);
    let count: u64 = env.storage().persistent().get(&history_key).unwrap_or(0);
    env.storage()
        .persistent()
        .set(&history_key, &(count + 1));

    Ok(())
}

pub fn get_credential_current_slice(env: &Env, credential_id: &Address) -> Option<Address> {
    let key = ("credential_slice", credential_id);
    env.storage().persistent().get(&key)
}

pub fn get_migration_history_count(env: &Env, credential_id: &Address) -> u64 {
    let key = ("migration_history", credential_id);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn verify_slice_compatibility(
    env: &Env,
    credential_id: &Address,
    new_slice: &Address,
) -> bool {
    let key = ("slice_compat", new_slice);
    env.storage().persistent().get(&key).unwrap_or(true)
}

pub fn set_slice_compatibility(
    env: &Env,
    slice_id: &Address,
    compatible: bool,
) -> Result<()> {
    let key = ("slice_compat", slice_id);
    env.storage().persistent().set(&key, &compatible);
    Ok(())
}

pub fn maintain_migration_history(
    env: &Env,
    credential_id: &Address,
) -> u64 {
    get_migration_history_count(env, credential_id)
}

pub fn implement_slice_migration_protocol(
    env: &Env,
    credential_id: &Address,
    old_slice: &Address,
    new_slice: &Address,
) -> Result<()> {
    if !verify_slice_compatibility(env, credential_id, new_slice) {
        return Err(crate::types::Error::InvalidOperation);
    }

    migrate_credential_slice(env, credential_id, old_slice, new_slice)
}
