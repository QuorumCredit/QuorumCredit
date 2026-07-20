//! Input validation edge-case tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_negative_stake_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let result = client.try_vouch(&voucher, &borrower, &(-1i128), &token, &None);
    assert!(result.is_err(), "negative stake must be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_negative_loan_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let result = client.try_request_loan(&borrower, &(-100i128), &10_000_000i128,
        &soroban_sdk::String::from_str(&env, "negative amount"), &token);
    assert!(result.is_err(), "negative loan amount must be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
