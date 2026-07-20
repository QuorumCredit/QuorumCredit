//! get_loan returns None when no loan exists.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_get_loan_returns_none_for_unknown_borrower() {
    let env = Env::default();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let unknown = Address::generate(&env);
    let result = client.get_loan(&unknown);
    assert!(result.is_none(), "expected None for borrower with no loan");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
