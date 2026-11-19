//! Issue #2099: the public quorum-slice wrappers in `lib.rs` must pass the
//! arguments their module functions take. One test per wrapper drives the
//! contract through the generated client, so an arity or type mismatch fails here.
#![cfg(test)]

use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, Env};

/// Registers and initializes the contract with one admin. Returns (contract id, admin).
fn initialized(env: &Env) -> (Address, Address) {
    env.mock_all_auths();
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &vec![env, admin.clone()], &1u32, &token);
    (contract_id, admin)
}

#[test]
fn migrate_credential_slice_wrapper_records_new_slice() {
    let env = Env::default();
    let (contract_id, admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let old_slice = Address::generate(&env);
    let new_slice = Address::generate(&env);

    client.migrate_credential_slice(&vec![&env, admin], &borrower, &old_slice, &new_slice);

    assert_eq!(client.get_credential_current_slice(&borrower), Some(new_slice));
}

#[test]
fn implement_slice_migration_protocol_wrapper_records_new_slice() {
    let env = Env::default();
    let (contract_id, admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let old_slice = Address::generate(&env);
    let new_slice = Address::generate(&env);

    client.implement_slice_migration_protocol(&vec![&env, admin], &borrower, &old_slice, &new_slice);

    assert_eq!(client.get_credential_current_slice(&borrower), Some(new_slice));
}

#[test]
fn get_credential_current_slice_wrapper_is_none_before_migration() {
    let env = Env::default();
    let (contract_id, _admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);

    assert_eq!(client.get_credential_current_slice(&borrower), None);
}

#[test]
fn get_migration_history_count_wrapper_counts_migrations() {
    let env = Env::default();
    let (contract_id, admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let old_slice = Address::generate(&env);
    let new_slice = Address::generate(&env);

    assert_eq!(client.get_migration_history_count(&borrower), 0u64);
    client.migrate_credential_slice(&vec![&env, admin], &borrower, &old_slice, &new_slice);
    assert_eq!(client.get_migration_history_count(&borrower), 1u64);
}

#[test]
fn is_rebalancing_enabled_wrapper_is_false_by_default() {
    let env = Env::default();
    let (contract_id, _admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let slice_id = Address::generate(&env);

    assert!(!client.is_rebalancing_enabled(&slice_id));
}

#[test]
fn enable_dynamic_rebalancing_wrapper_enables_slice() {
    let env = Env::default();
    let (contract_id, admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let slice_id = Address::generate(&env);
    let params = Bytes::new(&env, &[1, 2, 3]);

    client.enable_dynamic_rebalancing(&slice_id, &vec![&env, admin], &params);

    assert!(client.is_rebalancing_enabled(&slice_id));
}

#[test]
fn disable_dynamic_rebalancing_wrapper_disables_slice() {
    let env = Env::default();
    let (contract_id, admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let slice_id = Address::generate(&env);
    let params = Bytes::new(&env, &[1, 2, 3]);
    let signers = vec![&env, admin];

    client.enable_dynamic_rebalancing(&slice_id, &signers, &params);
    client.disable_dynamic_rebalancing(&slice_id, &signers);

    assert!(!client.is_rebalancing_enabled(&slice_id));
}

#[test]
fn execute_rebalancing_wrapper_increments_event_count() {
    let env = Env::default();
    let (contract_id, admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let slice_id = Address::generate(&env);
    let params = Bytes::new(&env, &[1, 2, 3]);
    let signers = vec![&env, admin];

    client.enable_dynamic_rebalancing(&slice_id, &signers, &params);
    assert_eq!(client.get_rebalancing_event_count(&slice_id), 0u64);

    client.execute_rebalancing(&slice_id, &signers);
    assert_eq!(client.get_rebalancing_event_count(&slice_id), 1u64);
}

#[test]
fn get_rebalancing_event_count_wrapper_is_zero_before_execution() {
    let env = Env::default();
    let (contract_id, _admin) = initialized(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let slice_id = Address::generate(&env);

    assert_eq!(client.get_rebalancing_event_count(&slice_id), 0u64);
}
