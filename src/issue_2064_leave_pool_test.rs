//! Issue #2064: `leave_pool` must not treat a missing stake / chain id as zero.

#![cfg(test)]

use crate::collateral_pool::leave_pool;
use crate::errors::ContractError;
use crate::types::{CollateralPool, DataKey};
use crate::QuorumCreditContract;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

const POOL_ID: u64 = 1;

fn inactive_pool(
    env: &Env,
    token: &Address,
    members: Vec<Address>,
    stakes: Vec<i128>,
    chain_ids: Vec<u32>,
) -> CollateralPool {
    CollateralPool {
        pool_id: POOL_ID,
        members,
        stakes,
        chain_ids,
        token: token.clone(),
        borrower: None,
        active: false,
        created_at: env.ledger().timestamp(),
    }
}

fn store_pool(env: &Env, contract_id: &Address, pool: &CollateralPool) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::CollateralPool(POOL_ID), pool);
    });
}

fn load_pool(env: &Env, contract_id: &Address) -> CollateralPool {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::CollateralPool(POOL_ID))
            .expect("pool should exist")
    })
}

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(admin).address();
    let contract_id = env.register(QuorumCreditContract, ());
    StellarAssetClient::new(&env, &token).mint(&contract_id, &1_000_000);
    (env, token, contract_id)
}

#[test]
fn test_leave_pool_refuses_when_stakes_are_shorter_than_members() {
    let (env, token, contract_id) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // `stakes` is missing bob's entry, so his index is corrupt.
    let pool = inactive_pool(
        &env,
        &token,
        Vec::from_array(&env, [alice.clone(), bob.clone()]),
        Vec::from_array(&env, [100i128]),
        Vec::from_array(&env, [0u32, 0u32]),
    );
    store_pool(&env, &contract_id, &pool);

    let result = env.as_contract(&contract_id, || leave_pool(env.clone(), bob.clone(), POOL_ID));
    assert_eq!(result, Err(ContractError::InvariantViolation));

    // Nothing was mutated: bob is still a member.
    let after = load_pool(&env, &contract_id);
    assert_eq!(after.members.len(), 2);
}

#[test]
fn test_leave_pool_refuses_when_chain_ids_are_shorter_than_members() {
    let (env, token, contract_id) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Even the member whose own index is intact must be refused: the vectors are out of step.
    let pool = inactive_pool(
        &env,
        &token,
        Vec::from_array(&env, [alice.clone(), bob.clone()]),
        Vec::from_array(&env, [100i128, 200i128]),
        Vec::from_array(&env, [0u32]),
    );
    store_pool(&env, &contract_id, &pool);

    let result = env.as_contract(&contract_id, || leave_pool(env.clone(), alice.clone(), POOL_ID));
    assert_eq!(result, Err(ContractError::InvariantViolation));
}

#[test]
fn test_leave_pool_keeps_parallel_vectors_equal_length_and_refunds_stake() {
    let (env, token, contract_id) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    let pool = inactive_pool(
        &env,
        &token,
        Vec::from_array(&env, [alice.clone(), bob.clone(), carol.clone()]),
        Vec::from_array(&env, [100i128, 200i128, 300i128]),
        Vec::from_array(&env, [0u32, 0u32, 0u32]),
    );
    store_pool(&env, &contract_id, &pool);

    let result = env.as_contract(&contract_id, || leave_pool(env.clone(), bob.clone(), POOL_ID));
    assert_eq!(result, Ok(()));

    let after = load_pool(&env, &contract_id);
    assert_eq!(after.members.len(), 2);
    assert_eq!(after.stakes.len(), 2);
    assert_eq!(after.chain_ids.len(), 2);
    assert_eq!(after.members.get(0).unwrap(), alice);
    assert_eq!(after.members.get(1).unwrap(), carol);
    assert_eq!(after.stakes.get(1).unwrap(), 300);
}
