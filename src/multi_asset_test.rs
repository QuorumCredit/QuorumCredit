//! Multi-asset vouch token tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};

#[test]
fn test_multi_asset_vouches_do_not_break_invariants() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Register a second allowed token
    let token2 = env.register_stellar_asset_contract_v2(admin.clone()).address();
    StellarAssetClient::new(&env, &token2).mint(&contract_id, &100_000_000_000i128);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);
    client.add_allowed_token(&admins, &token2);

    let borrower = Address::generate(&env);
    let v1 = Address::generate(&env);
    let v2 = Address::generate(&env);
    fund_address(&env, &admin, &token, &v1, 10_000_000);
    StellarAssetClient::new(&env, &token2).mint(&v2, &10_000_000i128);

    client.vouch(&v1, &borrower, &5_000_000i128, &token, &None);
    client.vouch(&v2, &borrower, &5_000_000i128, &token2, &None);

    // I1 should still hold — we only count primary-token stake in total_locked
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
