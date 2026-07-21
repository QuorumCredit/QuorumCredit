use soroban_sdk::{Address, Env, String, Vec, Bytes, BytesN};
use crate::errors::ContractError;
use crate::types::{CrossChainLoanMetadata, BridgeAttestation, BridgeRecord, UnifiedReputation};

pub fn register_bridge(
    _env: Env,
    _admin_signers: Vec<Address>,
    _chain_id: u32,
    _chain_name: String,
    _bridge_address: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn remove_bridge(
    _env: Env,
    _admin_signers: Vec<Address>,
    _chain_id: u32,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_bridges(env: Env) -> Vec<BridgeRecord> {
    Vec::new(&env)
}

pub fn set_bridge_public_key(
    _env: Env,
    _admin_signers: Vec<Address>,
    _origin_chain: u32,
    _public_key: BytesN<32>,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn validate_bridge_attestation(
    _env: Env,
    _metadata: CrossChainLoanMetadata,
    _attestation: BridgeAttestation,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn verify_bridge_message(
    _env: Env,
    _metadata: CrossChainLoanMetadata,
    _attestation: BridgeAttestation,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn bridge_attestation_message(
    env: &Env,
    _metadata: &CrossChainLoanMetadata,
    _nonce: u64,
    _timestamp: u64,
) -> Bytes {
    Bytes::new(env)
}

pub fn mirror_loan_to_chain(
    _env: Env,
    _metadata: CrossChainLoanMetadata,
    _attestation: BridgeAttestation,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn query_reputation_cross_chain(
    _env: Env,
    _borrower: Address,
) -> Option<UnifiedReputation> {
    None
}

pub fn query_mirrored_loan(
    _env: Env,
    _origin_chain: u32,
    _loan_id: u64,
) -> Option<CrossChainLoanMetadata> {
    None
}

pub fn is_bridge_nonce_used(
    _env: Env,
    _origin_chain: u32,
    _nonce: u64,
) -> bool {
    false
}
