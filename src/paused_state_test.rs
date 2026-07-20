//! Paused state tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_invariants_hold_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

    client.pause(&admins);
    // Invariants must still hold even in paused state
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();

    client.unpause(&admins);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_vouch_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

    client.pause(&admins);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 10_000_000);
    let result = client.try_vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    assert!(result.is_err(), "vouch while paused must fail");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
