use soroban_sdk::{contracttype, Address, Env};

use crate::errors::ContractError;

/// A single entry in a credential's migration history.
#[contracttype]
#[derive(Clone)]
pub struct MigrationHistoryEntry {
    pub old_slice: Address,
    pub new_slice: Address,
}

pub fn migrate_credential_slice(
    env: &Env,
    credential_id: &Address,
    old_slice: &Address,
    new_slice: &Address,
) -> Result<(), ContractError> {
    let key = ("credential_slice", credential_id);

    // If a current slice is recorded, it must match old_slice.
    if let Some(current) = env.storage().persistent().get::<_, Address>(&key) {
        if &current != old_slice {
            return Err(ContractError::InvalidStateTransition);
        }
    }

    env.storage().persistent().set(&key, new_slice);

    let history_key = ("migration_history", credential_id);
    let count: u64 = env.storage().persistent().get(&history_key).unwrap_or(0);

    let entry_key = ("migration_entry", credential_id, count);
    env.storage().persistent().set(
        &entry_key,
        &MigrationHistoryEntry {
            old_slice: old_slice.clone(),
            new_slice: new_slice.clone(),
        },
    );

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
) -> Result<(), ContractError> {
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
) -> Result<(), ContractError> {
    if !verify_slice_compatibility(env, credential_id, new_slice) {
        return Err(ContractError::InvalidStateTransition);
    }

    migrate_credential_slice(env, credential_id, old_slice, new_slice)
}
