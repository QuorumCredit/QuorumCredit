#![cfg(test)]

use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, Vec,
};

fn setup(env: &Env) -> (QuorumCreditContractClient<'static>, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);

    let token_id = env.register_stellar_asset_contract_v2(admin);
    let contract_id = env.register_contract(None, QuorumCreditContract);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    (client, contract_id)
}

/// Test 1: SBT delegation with time bounds allows temporary authority transfer
#[test]
fn test_delegate_sbt_temporarily_with_valid_duration() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64; // 1 hour

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    assert!(result.is_ok(), "delegation should succeed with valid parameters");
}

/// Test 2: Delegated SBT authority is available to delegate
#[test]
fn test_delegated_sbt_can_be_used_by_delegate() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    // Verify delegation is active
    let is_delegated = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(is_delegated, "SBT should be delegated to the specified delegate");
}

/// Test 3: SBT delegation expires after specified duration
#[test]
fn test_sbt_delegation_expires_after_duration() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64; // 1 hour

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    // Verify delegation is active
    let is_active = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(is_active, "delegation should be active immediately");

    // Move time forward past expiry
    env.ledger().with_mut(|l| l.timestamp = 100 + 3601);

    // Verify delegation has expired
    let is_expired = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(!is_expired, "delegation should expire after duration");
}

/// Test 4: Multiple delegations can exist for same SBT
#[test]
fn test_multiple_delegations_same_sbt() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate1 = Address::generate(&env);
    let delegate2 = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Delegate to multiple addresses
    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate1, &duration);
    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate2, &duration);

    let is_delegated_1 = client.is_sbt_delegated_to(&delegate1, &sbt_id);
    let is_delegated_2 = client.is_sbt_delegated_to(&delegate2, &sbt_id);

    assert!(is_delegated_1 && is_delegated_2, "both delegations should exist");
}

/// Test 5: Delegation chain is tracked correctly
#[test]
fn test_delegation_chain_tracking() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let original_holder = Address::generate(&env);
    let primary_delegate = Address::generate(&env);
    let secondary_delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create delegation chain: original_holder -> primary_delegate -> secondary_delegate
    client.delegate_sbt_temporarily(&original_holder, &sbt_id, &primary_delegate, &duration);

    // Primary delegate re-delegates
    client.delegate_sbt_temporarily(&primary_delegate, &sbt_id, &secondary_delegate, &duration / 2);

    // Get delegation chain
    let delegation_chain = client.get_delegation_chain(&sbt_id);

    assert!(delegation_chain.len() >= 2, "delegation chain should track multiple levels");
}

/// Test 6: Holder can revoke delegation before expiry
#[test]
fn test_revoke_delegation_before_expiry() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    let is_delegated_before = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(is_delegated_before, "delegation should be active");

    // Revoke delegation
    client.revoke_sbt_delegation(&holder, &sbt_id, &delegate);

    let is_delegated_after = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(!is_delegated_after, "delegation should be revoked");
}

/// Test 7: Delegate cannot revoke delegation they don't own
#[test]
fn test_delegate_cannot_revoke_others_delegation() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate1 = Address::generate(&env);
    let delegate2 = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate1, &duration);

    // Attempt to revoke someone else's delegation
    let result = client.try_revoke_sbt_delegation(&delegate2, &sbt_id, &delegate1);

    assert!(result.is_err(), "non-holder should not be able to revoke delegation");
}

/// Test 8: Delegation duration must be positive
#[test]
fn test_delegation_duration_must_be_positive() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let zero_duration = 0u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &zero_duration);

    assert!(result.is_err(), "delegation with zero duration should be rejected");
}

/// Test 9: Delegation to self fails
#[test]
fn test_delegation_to_self_fails() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_delegate_sbt_temporarily(&holder, &sbt_id, &holder, &duration);

    assert!(result.is_err(), "cannot delegate SBT to self");
}

/// Test 10: Automatic delegation expiry is enforced
#[test]
fn test_automatic_delegation_expiry() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 7200u64; // 2 hours

    env.ledger().with_mut(|l| l.timestamp = 1000);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    // Delegation should be active at time 1000 + 7199
    env.ledger().with_mut(|l| l.timestamp = 1000 + 7199);
    let is_active_before = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(is_active_before, "delegation should still be active just before expiry");

    // Delegation should be expired at time 1000 + 7201
    env.ledger().with_mut(|l| l.timestamp = 1000 + 7201);
    let is_expired = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(!is_expired, "delegation should be expired after duration");
}

/// Test 11: Delegation cannot be renewed unless holder re-delegates
#[test]
fn test_expired_delegation_cannot_be_auto_renewed() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    // Move past expiry
    env.ledger().with_mut(|l| l.timestamp = 100 + 3601);

    // Delegation should be expired
    let is_expired = client.is_sbt_delegated_to(&delegate, &sbt_id);
    assert!(!is_expired, "delegation should be expired");

    // Attempt to use expired delegation should fail
    let result = client.try_use_delegated_sbt(&delegate, &sbt_id);
    assert!(result.is_err(), "cannot use expired delegation");
}

/// Test 12: Maximum delegation depth is enforced
#[test]
fn test_maximum_delegation_chain_depth() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;
    let max_chain_depth = 3u32; // Example max depth

    env.ledger().with_mut(|l| l.timestamp = 100);

    let mut current_holder = holder.clone();

    // Try to create a chain deeper than allowed
    for i in 0..(max_chain_depth + 2) {
        let delegate = Address::generate(&env);
        let result = if i == 0 {
            client.try_delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration)
        } else {
            client.try_delegate_sbt_temporarily(&current_holder, &sbt_id, &delegate, &duration)
        };

        if i >= max_chain_depth as usize {
            // Deep delegations should be rejected
            assert!(result.is_err(), "delegation chain depth should be limited");
            break;
        }

        current_holder = delegate;
    }
}

/// Test 13: Get all delegations for an SBT
#[test]
fn test_get_all_delegations_for_sbt() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    let delegate1 = Address::generate(&env);
    let delegate2 = Address::generate(&env);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate1, &duration);
    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate2, &duration);

    let all_delegations = client.get_all_delegations(&sbt_id);

    assert!(all_delegations.len() >= 2, "should retrieve all active delegations");
}

/// Test 14: Delegation respects contract pause
#[test]
fn test_delegation_respects_contract_pause() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;
    let admin = Address::generate(&env);

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Pause contract
    client.set_paused(&admin, &true);

    let result = client.try_delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    assert!(result.is_err(), "delegation should fail when contract is paused");
}

/// Test 15: Verify delegation metadata
#[test]
fn test_delegation_metadata_is_stored() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let delegate = Address::generate(&env);
    let sbt_id = 1u64;
    let duration = 3600u64;

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.delegate_sbt_temporarily(&holder, &sbt_id, &delegate, &duration);

    // Retrieve delegation metadata
    let metadata = client.get_delegation_metadata(&sbt_id, &delegate);

    assert_eq!(metadata.sbt_id, sbt_id, "metadata should contain correct SBT ID");
    assert_eq!(metadata.delegate, delegate, "metadata should contain correct delegate");
    assert_eq!(metadata.duration, duration, "metadata should contain correct duration");
}
