//! Issue #1908 — Admin entrypoints must not panic on an empty signer vector.
//!
//! Admin functions previously indexed `admin_signers.get(0).unwrap()` after the
//! approval check. With a valid threshold (>= 1) an empty vector is rejected by
//! the approval check with `UnauthorizedCaller`. The unchecked index was only
//! reachable when the threshold is 0, and then it panicked with no error variant.
//! These tests cover both behaviours:
//!   * threshold >= 1: an empty signer vector returns a typed error from the
//!     approval check, never a generic panic.
//!   * threshold == 0 (set directly in storage, since the public API forbids it):
//!     each affected function returns `EmptySignerList` instead of panicking on
//!     `unwrap()`.

#![cfg(test)]

use crate::admin;
use crate::types::{Config, DataKey};
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Deploys and initializes the contract with a single admin and threshold 1.
fn setup(env: &Env) -> Address {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, QuorumCreditContract);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(
        &deployer,
        &Vec::from_array(env, [admin]),
        &1u32,
        &token,
    );

    contract_id
}

/// Forces `admin_threshold` to 0 in storage so an empty signer vector passes the
/// approval loop and reaches the indexing code path under test.
fn force_zero_threshold(env: &Env, contract_id: &Address) {
    env.as_contract(contract_id, || {
        let mut cfg: Config = crate::helpers::config(env);
        cfg.admin_threshold = 0;
        env.storage().instance().set(&DataKey::Config, &cfg);
    });
}

// ── Threshold >= 1: empty signers are rejected by the approval check ──────────

#[test]
fn test_empty_signers_rejected_by_approval_when_threshold_is_valid() {
    let env = Env::default();
    let contract_id = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    assert_eq!(
        client.try_set_protocol_fee(&empty, &100u32),
        Err(Ok(ContractError::UnauthorizedCaller)),
    );
    assert_eq!(
        client.try_pause(&empty),
        Err(Ok(ContractError::UnauthorizedCaller)),
    );
}

// ── Threshold == 0: the indexing path must return a typed error ───────────────

#[test]
fn test_set_protocol_fee_empty_signers_returns_empty_signer_list() {
    let env = Env::default();
    let contract_id = setup(&env);
    force_zero_threshold(&env, &contract_id);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    assert_eq!(
        client.try_set_protocol_fee(&empty, &100u32),
        Err(Ok(ContractError::EmptySignerList)),
    );
}

#[test]
fn test_pause_empty_signers_returns_empty_signer_list() {
    let env = Env::default();
    let contract_id = setup(&env);
    force_zero_threshold(&env, &contract_id);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    assert_eq!(
        client.try_pause(&empty),
        Err(Ok(ContractError::EmptySignerList)),
    );
}

// The remaining affected functions are not exposed on the contract client, so
// they are called directly inside the contract context. The helper aborts with
// the typed contract error, which surfaces as a panic in the test host.

#[test]
#[should_panic(expected = "Error(Contract, #243)")]
fn test_set_whitelist_enabled_empty_signers_returns_empty_signer_list() {
    let env = Env::default();
    let contract_id = setup(&env);
    force_zero_threshold(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    env.as_contract(&contract_id, || {
        admin::set_whitelist_enabled(env.clone(), empty, true);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #243)")]
fn test_begin_thaw_empty_signers_returns_empty_signer_list() {
    let env = Env::default();
    let contract_id = setup(&env);
    force_zero_threshold(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    env.as_contract(&contract_id, || {
        admin::begin_thaw(env.clone(), empty);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #243)")]
fn test_unpause_empty_signers_returns_empty_signer_list() {
    let env = Env::default();
    let contract_id = setup(&env);
    force_zero_threshold(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    env.as_contract(&contract_id, || {
        admin::unpause(env.clone(), empty);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #243)")]
fn test_pause_with_thaw_empty_signers_returns_empty_signer_list() {
    let env = Env::default();
    let contract_id = setup(&env);
    force_zero_threshold(&env, &contract_id);
    let empty: Vec<Address> = Vec::new(&env);

    env.as_contract(&contract_id, || {
        admin::pause_with_thaw(env.clone(), empty, 60);
    });
}
