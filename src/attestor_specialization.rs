use soroban_sdk::{Address, Bytes, Env};

use crate::types::Result;

pub fn register_attestor_specialization(
    env: &Env,
    attestor: Address,
    spec: Bytes,
) -> Result<()> {
    let key = ("attestor_spec", &attestor);
    env.storage().persistent().set(&key, &spec);
    Ok(())
}

pub fn get_attestor_specialization(env: &Env, attestor: &Address) -> Option<Bytes> {
    let key = ("attestor_spec", attestor);
    env.storage().persistent().get(&key)
}

pub fn match_credentials_to_specialized_attestors(
    env: &Env,
    credential_type: &Bytes,
) -> Vec<Address> {
    let mut matched_attestors = soroban_sdk::Vec::new(env);

    matched_attestors
}

pub fn create_specialization_registry(env: &Env) -> Result<()> {
    let key = "spec_registry_initialized";
    if env.storage().persistent().has(&key) {
        return Ok(());
    }

    env.storage().persistent().set(&key, &true);
    Ok(())
}
