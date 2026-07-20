//! Initialization tests.
//! `verify_invariants` is called after every state-changing operation.
#![cfg(test)]
#![allow(unused_imports)]

use crate::invariants_test::{setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    Address, Env, Vec,
};

#[test]
fn test_invariants_hold_after_initialize() {
    let env = Env::default();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_double_initialize_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let deployer = Address::generate(&env);
    let admin = Address::generate(&env);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_id.address();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(&env, &token).mint(&contract_id, &100_000_000_000i128);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.initialize(&deployer, &admins, &1u32, &token);

    // Second call must fail
    let result = client.try_initialize(&deployer, &admins, &1u32, &token);
    assert!(result.is_err(), "expected AlreadyInitialized error");

    // Contract state must still be consistent
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_invariants_hold_with_multisig_admins() {
    let env = Env::default();
    env.mock_all_auths();
    let deployer = Address::generate(&env);
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let admins = soroban_sdk::Vec::from_array(&env, [admin1.clone(), admin2.clone(), admin3.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
    let token = token_id.address();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(&env, &token).mint(&contract_id, &100_000_000_000i128);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    // 2-of-3 multisig
    client.initialize(&deployer, &admins, &2u32, &token);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
