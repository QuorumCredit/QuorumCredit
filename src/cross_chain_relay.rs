#![allow(unused_variables, dead_code)]
use crate::errors::ContractError;
use crate::types::{RelayEvent, RelayAttestation};
use soroban_sdk::{Address, BytesN, Bytes, Env, Vec};

pub fn set_relay_key(env: Env, admin_signers: Vec<Address>, source_chain: u32, public_key: BytesN<32>) -> Result<(), ContractError> {
    Ok(())
}
pub fn relay_emit(env: Env, admin_signers: Vec<Address>, dest_chain: u32, event_type: soroban_sdk::Symbol, payload: Bytes) -> Result<u64, ContractError> {
    Ok(0)
}
pub fn relay_attestation_message(env: &Env, event: &RelayEvent, nonce: u64, timestamp: u64) -> Bytes {
    Bytes::new(env)
}
pub fn relay_message(env: Env, event: RelayEvent, attestation: RelayAttestation) -> Result<(), ContractError> {
    Ok(())
}
pub fn acknowledge_relay(env: Env, admin_signers: Vec<Address>, dest_chain: u32, up_to_seq: u64) -> Result<(), ContractError> {
    Ok(())
}
pub fn get_outbound_event(env: Env, dest_chain: u32, seq: u64) -> Option<RelayEvent> {
    None
}
pub fn latest_outbound_seq(env: Env, dest_chain: u32) -> u64 {
    0
}
pub fn last_acknowledged_seq(env: Env, dest_chain: u32) -> u64 {
    0
}
pub fn is_relay_processed(env: Env, source_chain: u32, seq: u64) -> bool {
    false
}
pub fn is_relay_nonce_used(env: Env, source_chain: u32, nonce: u64) -> bool {
    false
}
