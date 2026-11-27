//! Issue #2167: SBT Insurance Coverage Tests
//!
//! Verifies:
//! - Premium collection on registration
//! - Reserve balance check against total committed coverage
//! - Holder verification against the SBT owner

#![cfg(test)]

use crate::errors::ContractError;
use crate::sbt_insurance::{
    approve_sbt_insurance_claim, cancel_sbt_insurance, file_sbt_insurance_claim,
    get_sbt_insurance, get_sbt_insurance_committed_coverage, register_sbt_insurance,
    set_sbt_owner,
};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, Bytes, Env, Vec,
};

fn setup() -> (Env, Address, Address, Address, QuorumCreditContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_id = env.register(QuorumCreditContract, ());

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.initialize(&admin, &admins, &1, &token);

    (env, admin, token, contract_id, client)
}

#[test]
fn test_register_sbt_insurance_refuses_non_owner() {
    let (env, admin, token, contract_id, _client) = setup();
    let real_owner = Address::generate(&env);
    let imposter = Address::generate(&env);

    // Fund contract reserves and imposter
    StellarAssetClient::new(&env, &token).mint(&contract_id, &100_000);
    StellarAssetClient::new(&env, &token).mint(&imposter, &10_000);

    let admins = Vec::from_array(&env, [admin.clone()]);

    env.as_contract(&contract_id, || {
        // Register real owner for SBT 1
        set_sbt_owner(&env, 1, &real_owner);

        // Imposter tries to register insurance for SBT 1
        let res = register_sbt_insurance(
            &env,
            admins.clone(),
            1,
            imposter.clone(),
            50_000,
            1_000,
        );
        assert_eq!(res, Err(ContractError::UnauthorizedCaller));

        // SBT with no owner record also fails
        let res_no_owner = register_sbt_insurance(
            &env,
            admins.clone(),
            999,
            imposter.clone(),
            50_000,
            1_000,
        );
        assert_eq!(res_no_owner, Err(ContractError::UnauthorizedCaller));
    });
}

#[test]
fn test_register_sbt_insurance_collects_premium_and_checks_reserves() {
    let (env, admin, token, contract_id, _client) = setup();
    let owner = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);

    let initial_contract_reserve = 100_000i128;
    let initial_owner_balance = 20_000i128;
    let premium = 2_000i128;
    let coverage = 80_000i128;

    StellarAssetClient::new(&env, &token).mint(&contract_id, &initial_contract_reserve);
    StellarAssetClient::new(&env, &token).mint(&owner, &initial_owner_balance);

    env.as_contract(&contract_id, || {
        set_sbt_owner(&env, 1, &owner);

        // Registration succeeds
        let res = register_sbt_insurance(
            &env,
            admins.clone(),
            1,
            owner.clone(),
            coverage,
            premium,
        );
        assert!(res.is_ok(), "registration should succeed");

        // Verify premium was collected (contract reserve increased by premium, owner balance decreased)
        let token_client = StellarAssetClient::new(&env, &token);
        assert_eq!(
            token_client.balance(&owner),
            initial_owner_balance - premium
        );
        assert_eq!(
            token_client.balance(&contract_id),
            initial_contract_reserve + premium
        );

        // Verify committed coverage is tracked
        assert_eq!(get_sbt_insurance_committed_coverage(&env), coverage);

        let policy = get_sbt_insurance(&env, 1).expect("policy should exist");
        assert_eq!(policy.coverage_amount, coverage);
        assert_eq!(policy.premium, premium);
    });
}

#[test]
fn test_register_sbt_insurance_refuses_when_coverage_exceeds_reserves() {
    let (env, admin, token, contract_id, _client) = setup();
    let owner = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);

    // Contract has small reserve: 5,000 stroops
    StellarAssetClient::new(&env, &token).mint(&contract_id, &5_000);
    StellarAssetClient::new(&env, &token).mint(&owner, &10_000);

    env.as_contract(&contract_id, || {
        set_sbt_owner(&env, 2, &owner);

        // Attempt coverage of 50_000 with premium of 1_000 (total reserve after premium = 6,000 < 50,000)
        let res = register_sbt_insurance(
            &env,
            admins.clone(),
            2,
            owner.clone(),
            50_000,
            1_000,
        );
        assert_eq!(res, Err(ContractError::InsufficientYieldReserve));

        // Committed coverage remains 0
        assert_eq!(get_sbt_insurance_committed_coverage(&env), 0);
    });
}

#[test]
fn test_cancellation_releases_committed_coverage() {
    let (env, admin, token, contract_id, _client) = setup();
    let owner = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);

    StellarAssetClient::new(&env, &token).mint(&contract_id, &100_000);
    StellarAssetClient::new(&env, &token).mint(&owner, &10_000);

    env.as_contract(&contract_id, || {
        set_sbt_owner(&env, 3, &owner);

        register_sbt_insurance(
            &env,
            admins.clone(),
            3,
            owner.clone(),
            40_000,
            1_000,
        )
        .expect("registration should succeed");

        assert_eq!(get_sbt_insurance_committed_coverage(&env), 40_000);

        // Cancel policy
        cancel_sbt_insurance(&env, admins.clone(), 3).expect("cancel should succeed");

        // Committed coverage is released
        assert_eq!(get_sbt_insurance_committed_coverage(&env), 0);
    });
}
