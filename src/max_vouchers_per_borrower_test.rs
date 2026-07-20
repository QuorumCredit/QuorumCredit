//! Max vouchers per borrower enforcement tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_invariants_hold_with_many_vouchers() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);

    // Add up to 5 vouchers (well within default max of 50)
    for _ in 0..5u32 {
        let v = Address::generate(&env);
        fund_address(&env, &admin, &token, &v, 5_000_000);
        client.vouch(&v, &borrower, &1_000_000i128, &token, &None);
    }
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
