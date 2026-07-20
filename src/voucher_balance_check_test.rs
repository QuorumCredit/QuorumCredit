//! Voucher balance check: voucher must hold enough tokens to cover stake.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_vouch_rejected_when_voucher_has_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Voucher has zero balance
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);

    let result = client.try_vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    assert!(result.is_err(), "vouch with no balance must be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
