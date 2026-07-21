use soroban_sdk::{Address, Env, Vec};
use crate::errors::ContractError;
use crate::types::CollateralPool;

pub fn create_pool(
    _env: Env,
    _creator: Address,
    _token: Address,
    _initial_stake: i128,
) -> Result<u64, ContractError> {
    Ok(0)
}

pub fn join_pool(
    _env: Env,
    _voucher: Address,
    _pool_id: u64,
    _stake: i128,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn join_pool_cross_chain(
    _env: Env,
    _voucher: Address,
    _pool_id: u64,
    _stake: i128,
    _chain_id: u32,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn leave_pool(
    _env: Env,
    _voucher: Address,
    _pool_id: u64,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn assign_pool_to_borrower(
    _env: Env,
    _admin_signers: Vec<Address>,
    _pool_id: u64,
    _borrower: Address,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_pool(
    env: Env,
    _pool_id: u64,
) -> Result<CollateralPool, ContractError> {
    let dummy = env.current_contract_address();
    Ok(CollateralPool {
        pool_id: 0,
        members: Vec::new(&env),
        stakes: Vec::new(&env),
        chain_ids: Vec::new(&env),
        token: dummy,
        borrower: None,
        active: false,
        created_at: 0,
    })
}

pub fn get_pool_total_stake(
    _env: Env,
    _pool_id: u64,
) -> Result<i128, ContractError> {
    Ok(0)
}

pub fn get_pool_chain_stake(
    _env: Env,
    _pool_id: u64,
    _chain_id: u32,
) -> Result<i128, ContractError> {
    Ok(0)
}
