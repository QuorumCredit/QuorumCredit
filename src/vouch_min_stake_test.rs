//! Min stake enforcement tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_vouch_below_min_stake_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

    // Set a minimum stake of 100_000 stroops
    client.set_min_stake(&admins, &100_000i128);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);

    // Try to vouch with 50 stroops (below default min)
    let result = client.try_vouch(&voucher, &borrower, &50i128, &token, &None);
    assert!(result.is_err(), "sub-minimum stake must be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
