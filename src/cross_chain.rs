#![allow(unused_variables, dead_code)]
use crate::errors::ContractError;
use crate::types::{BridgeRecord, CrossChainLoanMetadata};
use soroban_sdk::{Address, Bytes, BytesN, Env, Vec};

pub fn register_bridge(env: Env, admin_signers: Vec<Address>, chain_id: u32, chain_name: soroban_sdk::String, bridge_address: Address) -> Result<(), ContractError> {
    vouch::register_bridge(env, admin_signers, chain_id, chain_name, bridge_address)
}

pub fn remove_bridge(env: Env, admin_signers: Vec<Address>, chain_id: u32) -> Result<(), ContractError> {
    vouch::remove_bridge(env, admin_signers, chain_id)
}

pub fn get_bridges(env: Env) -> Vec<BridgeRecord> {
    vouch::get_bridges(env)
}

pub fn set_bridge_public_key(env: Env, admin_signers: Vec<Address>, origin_chain: u32, public_key: BytesN<32>) -> Result<(), ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn validate_bridge_attestation(env: Env, metadata: CrossChainLoanMetadata, attestation: BytesN<64>) -> Result<(), ContractError> {
    Err(ContractError::InvalidProof)
}

pub fn verify_bridge_message(env: Env, metadata: CrossChainLoanMetadata, attestation: BytesN<64>) -> Result<Bytes, ContractError> {
    Err(ContractError::InvalidProof)
}

pub fn bridge_attestation_message(env: &Env, metadata: &CrossChainLoanMetadata, nonce: u64, timestamp: u64) -> Bytes {
    Bytes::new(env)
}

pub fn mirror_loan_to_chain(env: Env, metadata: CrossChainLoanMetadata, attestation: BytesN<64>) -> Result<(), ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn query_reputation_cross_chain(env: Env, borrower: Address) -> Option<u32> {
    None
}

pub fn query_mirrored_loan(env: Env, origin_chain: u32, loan_id: u64) -> Option<u64> {
    None
}

pub fn is_bridge_nonce_used(env: Env, origin_chain: u32, nonce: u64) -> bool {
    false
}

use crate::vouch;
