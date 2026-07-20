//! Repay on non-existent loan must return an error.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_repay_nonexistent_loan_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    let result = client.try_repay(&unknown, &1_000_000i128);
    assert!(result.is_err(), "repay for non-existent loan must fail");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
