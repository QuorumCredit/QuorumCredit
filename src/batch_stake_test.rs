//! Batch vouch (batch_stake) tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_batch_vouch_invariants_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 100_000_000);

    let b1 = Address::generate(&env);
    let b2 = Address::generate(&env);
    let b3 = Address::generate(&env);

    let mut borrowers: Vec<Address> = Vec::new(&env);
    borrowers.push_back(b1.clone());
    borrowers.push_back(b2.clone());
    borrowers.push_back(b3.clone());

    let mut stakes: Vec<i128> = Vec::new(&env);
    stakes.push_back(5_000_000i128);
    stakes.push_back(5_000_000i128);
    stakes.push_back(5_000_000i128);

    client.batch_vouch(&voucher, &borrowers, &stakes, &token, &None);
    verify_invariants(&env, &contract_id, &token, &[&b1, &b2, &b3]).unwrap();
}

#[test]
fn test_batch_vouch_length_mismatch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 100_000_000);

    let b1 = Address::generate(&env);
    let mut borrowers: Vec<Address> = Vec::new(&env);
    borrowers.push_back(b1.clone());

    let stakes: Vec<i128> = Vec::new(&env); // mismatched — 0 stakes for 1 borrower

    let result = client.try_batch_vouch(&voucher, &borrowers, &stakes, &token, &None);
    assert!(result.is_err(), "length mismatch must be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
