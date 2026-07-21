use soroban_sdk::{Env, String};
use crate::errors::ContractError;
use crate::types::IpfsArchiveReference;

pub fn register_loan_ipfs_archive(
    _env: &Env,
    _archive_id: u64,
    _ipfs_hash: String,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_loan_ipfs_archive(_env: &Env, _archive_id: u64) -> Option<IpfsArchiveReference> {
    None
}

pub fn register_vouch_history_ipfs_archive(
    _env: &Env,
    _archive_id: u64,
    _ipfs_hash: String,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_vouch_history_ipfs_archive(_env: &Env, _archive_id: u64) -> Option<IpfsArchiveReference> {
    None
}

pub fn get_loan_ipfs_archive_count(_env: &Env) -> u64 {
    0
}

pub fn is_archive_ipfs_backed(_env: &Env, _archive_id: u64) -> bool {
    false
}

pub fn verify_loan_archive_integrity(
    _env: &Env,
    _archive_id: u64,
    _expected_ipfs_hash: String,
) -> Result<bool, ContractError> {
    Ok(false)
}
