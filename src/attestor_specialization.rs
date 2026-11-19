use soroban_sdk::{contracttype, Address, Bytes, Env, Vec};

use crate::errors::ContractError;

/// Issue #2102: Typed attestor specialization.
///
/// `credential_type` is the credential category the attestor specializes in and
/// is the key used by [`match_credentials_to_specialized_attestors`]. `details`
/// holds free-form specialization metadata that is stored but not matched on.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestorSpecialization {
    pub credential_type: Bytes,
    pub details: Bytes,
}

const SPEC_INDEX_KEY: &str = "attestor_spec_index";

/// Store the specialization for `attestor`, replacing any previous one.
///
/// The attestor is also added to the registry index (once) so that matching can
/// enumerate every registered specialization.
pub fn register_attestor_specialization(
    env: &Env,
    attestor: Address,
    spec: AttestorSpecialization,
) -> Result<(), ContractError> {
    let key = ("attestor_spec", &attestor);
    env.storage().persistent().set(&key, &spec);

    let mut index = get_specialization_index(env);
    if !index.contains(&attestor) {
        index.push_back(attestor);
        env.storage().persistent().set(&SPEC_INDEX_KEY, &index);
    }
    Ok(())
}

pub fn get_attestor_specialization(env: &Env, attestor: &Address) -> Option<AttestorSpecialization> {
    let key = ("attestor_spec", attestor);
    env.storage().persistent().get(&key)
}

/// Return every registered attestor whose specialization `credential_type`
/// equals `credential_type`. Returns an empty vector when none match.
pub fn match_credentials_to_specialized_attestors(
    env: &Env,
    credential_type: &Bytes,
) -> Vec<Address> {
    let mut matched_attestors = Vec::new(env);

    for attestor in get_specialization_index(env).iter() {
        if let Some(spec) = get_attestor_specialization(env, &attestor) {
            if spec.credential_type == *credential_type {
                matched_attestors.push_back(attestor);
            }
        }
    }

    matched_attestors
}

fn get_specialization_index(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&SPEC_INDEX_KEY)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn create_specialization_registry(env: &Env) -> Result<(), ContractError> {
    let key = "spec_registry_initialized";
    if env.storage().persistent().has(&key) {
        return Ok(());
    }

    env.storage().persistent().set(&key, &true);
    Ok(())
}
