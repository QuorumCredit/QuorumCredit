use crate::types::DataKey;
use soroban_sdk::Env;

pub fn secure_delete_persistent(env: &Env, key: &DataKey) {
    env.storage().persistent().remove(key);
}

pub fn secure_delete_instance(env: &Env, key: &DataKey) {
    env.storage().instance().remove(key);
}

pub fn secure_delete_temporary(env: &Env, key: &DataKey) {
    env.storage().temporary().remove(key);
}
