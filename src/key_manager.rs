use crate::types::DataKey;
use soroban_sdk::{Bytes, BytesN, Env};

pub fn derive_key(env: &Env, seed: Bytes, context: Bytes) -> BytesN<32> {
    let mut data = seed;
    data.append(&context);
    env.crypto().sha256(&data)
}

pub fn store_key(env: &Env, key_id: BytesN<32>, key: BytesN<32>) {
    env.storage()
        .persistent()
        .set(&DataKey::ManagedKey(key_id), &key);
}

pub fn get_key(env: &Env, key_id: BytesN<32>) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::ManagedKey(key_id))
}

pub fn rotate_key(env: &Env, key_id: BytesN<32>, new_seed: Bytes, context: Bytes) -> BytesN<32> {
    let new_key = derive_key(env, new_seed, context);
    store_key(env, key_id, new_key.clone());
    new_key
}

pub fn delete_key(env: &Env, key_id: BytesN<32>) {
    env.storage()
        .persistent()
        .remove(&DataKey::ManagedKey(key_id));
}
