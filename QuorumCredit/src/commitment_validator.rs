use soroban_sdk::{Bytes, BytesN, Env};

pub fn create_commitment(env: &Env, value: Bytes, nonce: BytesN<32>) -> BytesN<32> {
    let mut data = value;
    let nonce_bytes: Bytes = nonce.into();
    data.append(&nonce_bytes);
    env.crypto().sha256(&data)
}

pub fn verify_commitment(env: &Env, commitment: BytesN<32>, value: Bytes, nonce: BytesN<32>) -> bool {
    let computed = create_commitment(env, value, nonce);
    commitment == computed
}
