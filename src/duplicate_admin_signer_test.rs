//! Issue #2045 — Duplicate admin signers must not count toward the approval threshold.
//!
//! `helpers::require_admin_approval` and `rbac::require_admin_approval_with_permission`
//! previously checked `len() >= admin_threshold` and then verified each signer
//! individually, so passing the same admin N times satisfied an N-of-M threshold.
//! These tests check that:
//!   * a 2-of-N threshold rejects `[A, A]` (one key listed twice), and
//!   * the same threshold accepts `[A, B]` (two distinct admins).

#![cfg(test)]

use crate::rbac::require_admin_approval_with_permission;
use crate::types::AdminPermission;
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

/// Deploys the contract with two admins and an admin threshold of 2.
fn setup_two_of_two(env: &Env) -> (Address, Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin_a = Address::generate(env);
    let admin_b = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin_a.clone())
        .address();
    let contract_id = env.register_contract(None, QuorumCreditContract);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(
        &deployer,
        &Vec::from_array(env, [admin_a.clone(), admin_b.clone()]),
        &2u32,
        &token,
    );

    (contract_id, admin_a, admin_b)
}

#[test]
#[should_panic(expected = "duplicate admin signer")]
fn duplicate_signer_fails_two_of_n_threshold_via_entrypoint() {
    let env = Env::default();
    let (contract_id, admin_a, _admin_b) = setup_two_of_two(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // One admin repeated twice must not satisfy a 2-of-N threshold.
    let signers = Vec::from_array(&env, [admin_a.clone(), admin_a.clone()]);
    client.set_min_stake(&signers, &1_000);
}

#[test]
fn distinct_signers_pass_two_of_n_threshold_via_entrypoint() {
    let env = Env::default();
    let (contract_id, admin_a, admin_b) = setup_two_of_two(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let signers = Vec::from_array(&env, [admin_a, admin_b]);
    client.set_min_stake(&signers, &1_000);
    assert_eq!(client.get_min_stake(), 1_000);
}

#[test]
fn rbac_rejects_duplicate_signers_with_typed_error() {
    let env = Env::default();
    let (contract_id, admin_a, admin_b) = setup_two_of_two(&env);

    let duplicated = Vec::from_array(&env, [admin_a.clone(), admin_a.clone()]);
    let duplicate_result = env.as_contract(&contract_id, || {
        require_admin_approval_with_permission(
            &env,
            &duplicated,
            AdminPermission::UpdateConfig,
        )
    });
    assert_eq!(duplicate_result, Err(ContractError::DuplicateApprovalSigner));

    let distinct = Vec::from_array(&env, [admin_a, admin_b]);
    let distinct_result = env.as_contract(&contract_id, || {
        require_admin_approval_with_permission(&env, &distinct, AdminPermission::UpdateConfig)
    });
    assert_eq!(distinct_result, Ok(()));
}
