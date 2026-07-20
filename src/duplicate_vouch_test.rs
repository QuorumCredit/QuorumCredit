//! Duplicate vouch protection tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_duplicate_vouch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);

    client.vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // Same (voucher, borrower, token) tuple must be rejected
    let result = client.try_vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    assert!(result.is_err(), "duplicate vouch must be rejected");
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
