//! Zero-stake vouch rejection test.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_zero_stake_vouch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 10_000_000);

    let result = client.try_vouch(&voucher, &borrower, &0i128, &token, &None);
    assert!(result.is_err(), "zero-stake vouch must be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
