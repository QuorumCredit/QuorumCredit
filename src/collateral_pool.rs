#![allow(unused_variables)]
use crate::errors::ContractError;
use crate::types::CollateralPool;
use soroban_sdk::{Address, Env, Vec};

pub fn create_pool(env: Env, creator: Address, token: Address, initial_stake: i128) -> Result<u64, ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn join_pool(env: Env, voucher: Address, pool_id: u64, stake: i128) -> Result<(), ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn join_pool_cross_chain(env: Env, voucher: Address, pool_id: u64, stake: i128, chain_id: u32) -> Result<(), ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn leave_pool(env: Env, voucher: Address, pool_id: u64) -> Result<(), ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn assign_pool_to_borrower(env: Env, admin_signers: Vec<Address>, pool_id: u64, borrower: Address) -> Result<(), ContractError> {
    Err(ContractError::InvalidAmount)
}

pub fn get_pool(env: Env, pool_id: u64) -> Result<CollateralPool, ContractError> {
    Err(ContractError::CollateralPoolNotFound)
}

pub fn get_pool_total_stake(env: Env, pool_id: u64) -> Result<i128, ContractError> {
    Ok(0)
}

pub fn get_pool_chain_stake(env: Env, pool_id: u64, chain_id: u32) -> Result<i128, ContractError> {
    Ok(0)
}
