use soroban_sdk::{Address, Env, String, Vec};
use crate::errors::ContractError;
use crate::types::{LoanSyndication, SyndicationMember, SyndicationConfig, SyndicationRole};

pub fn create_syndication(
    _env: Env,
    _creator: Address,
    _loan_purpose: String,
    _token_address: Address,
    _total_amount: i128,
) -> Result<u64, ContractError> {
    Ok(0)
}

pub fn join_syndication(
    _env: Env,
    _syndication_id: u64,
    _member: Address,
    _role: SyndicationRole,
    _share_bps: u32,
    _collateral: i128,
    _vouch_stake: i128,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn approve_syndication(
    _env: Env,
    _syndication_id: u64,
    _member: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn leave_syndication(
    _env: Env,
    _syndication_id: u64,
    _member: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn cancel_syndication(
    _env: Env,
    _syndication_id: u64,
    _caller: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn request_syndication_loan(
    _env: Env,
    _syndication_id: u64,
    _lead_borrower: Address,
) -> Result<u64, ContractError> {
    Ok(0)
}

pub fn repay_syndication_loan(
    _env: Env,
    _syndication_id: u64,
    _repayer: Address,
    _amount: i128,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn handle_syndication_default(
    _env: Env,
    _syndication_id: u64,
    _caller: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_syndication(
    _env: Env,
    _syndication_id: u64,
) -> Option<LoanSyndication> {
    None
}

pub fn get_syndication_member(
    _env: Env,
    _syndication_id: u64,
    _member: Address,
) -> Option<SyndicationMember> {
    None
}

pub fn get_syndication_config_view(
    _env: Env,
) -> SyndicationConfig {
    crate::types::DEFAULT_SYNDICATION_CONFIG
}

pub fn set_syndication_config(
    _env: Env,
    _admin_signers: Vec<Address>,
    _config: SyndicationConfig,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_syndication_count(
    _env: Env,
) -> u64 {
    0
}
