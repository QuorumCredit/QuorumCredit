//! is_eligible correctly filters by token address.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_is_eligible_requires_token_match() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let threshold: i128 = 5_000_000;
    fund_address(&env, &admin, &token, &voucher, 20_000_000);
    client.vouch(&voucher, &borrower, &10_000_000i128, &token, &None);

    // Eligible for primary token
    assert!(client.is_eligible(&borrower, &threshold, &token));

    // Not eligible for an unregistered token
    let other_token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    assert!(!client.is_eligible(&borrower, &threshold, &other_token));

    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
