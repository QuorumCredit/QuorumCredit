use soroban_sdk::{Bytes, BytesN, Env};

pub fn generate_nonce(env: &Env) -> BytesN<32> {
    env.prng().gen::<BytesN<32>>()
}

pub fn generate_id(env: &Env) -> u64 {
    env.prng().gen::<u64>()
}

pub fn generate_bytes(env: &Env, len: u32) -> Bytes {
    env.prng().gen_len(len)
}
