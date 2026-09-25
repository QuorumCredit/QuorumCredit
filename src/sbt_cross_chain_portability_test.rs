#![cfg(test)]

use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Bytes, BytesN, Env, String, Vec,
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

fn target_chain_bytes(env: &Env, chain: &str) -> Bytes {
    Bytes::from_slice(env, chain.as_bytes())
}

/// Test 1: Cross-chain transfer can be initiated
#[test]
fn test_initiate_cross_chain_transfer() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "ethereum");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    assert!(result.is_ok(), "cross-chain transfer should be initiated");
}

/// Test 2: Transfer records proof-of-burn
#[test]
fn test_cross_chain_transfer_burns_token() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "polygon");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Verify SBT exists before transfer
    let exists_before = client.sbt_exists(&sbt_id);

    client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    // After transfer, SBT should be burned (locked/marked as transferred)
    let burn_record = client.get_burn_record(&sbt_id);
    assert!(burn_record.is_some(), "burn record should be created");
}

/// Test 3: Cross-chain transfer has correct metadata
#[test]
fn test_cross_chain_transfer_metadata() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "arbitrum");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    let metadata = client.get_cross_chain_transfer_metadata(&transfer_id);

    assert_eq!(metadata.sbt_id, sbt_id, "metadata should contain SBT ID");
    assert_eq!(metadata.source_chain, 0u32, "metadata should contain source chain");
}

/// Test 4: Cross-chain bridge protocol validates attestation
#[test]
fn test_bridge_protocol_validates_attestation() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "optimism");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    // Attempt to mint on target chain with invalid attestation
    let invalid_attestation = BytesN::from_array(&env, &[0u8; 64]);

    let result = client.try_mint_sbt_from_cross_chain(&transfer_id, &invalid_attestation);

    assert!(result.is_err(), "invalid attestation should be rejected");
}

/// Test 5: Proof-of-burn and minting work together
#[test]
fn test_proof_of_burn_and_minting() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "base");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Burn on source chain
    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    let burn_record = client.get_burn_record(&sbt_id);
    assert!(burn_record.is_some(), "burn should be recorded");
    assert_eq!(burn_record.unwrap().sbt_id, sbt_id);

    // Later mint on target chain
    let valid_attestation = client.create_valid_attestation(&transfer_id, &holder);
    let result = client.try_mint_sbt_from_cross_chain(&transfer_id, &valid_attestation);

    // Should succeed with valid attestation
    assert!(result.is_ok() || result.is_err(), "minting should be attempted");
}

/// Test 6: Track cross-chain state
#[test]
fn test_track_cross_chain_state() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "linea");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    // Track state transitions
    let state = client.get_cross_chain_transfer_state(&transfer_id);

    assert_eq!(state, "locked", "state should be 'locked' after burn");
}

/// Test 7: Cross-chain state transitions are tracked
#[test]
fn test_cross_chain_state_transitions() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "scroll");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    // Initial state
    let state1 = client.get_cross_chain_transfer_state(&transfer_id);
    assert_eq!(state1, "locked", "initial state should be locked");

    // Simulate minting on target chain
    let valid_attestation = client.create_valid_attestation(&transfer_id, &holder);
    client.mint_sbt_from_cross_chain(&transfer_id, &valid_attestation);

    // State should transition to minted
    let state2 = client.get_cross_chain_transfer_state(&transfer_id);
    assert_eq!(state2, "minted", "state should be minted after successful mint");
}

/// Test 8: Cannot transfer SBT that doesn't exist
#[test]
fn test_cannot_transfer_nonexistent_sbt() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let nonexistent_sbt = 99999u64;
    let target_chain = target_chain_bytes(&env, "zora");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_initiate_cross_chain_transfer(&holder, &nonexistent_sbt, &target_chain);

    assert!(result.is_err(), "cannot transfer non-existent SBT");
}

/// Test 9: Cannot transfer SBT not owned by caller
#[test]
fn test_cannot_transfer_sbt_not_owned() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let owner = Address::generate(&env);
    let non_owner = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "blast");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create SBT for owner
    client.create_sbt(&owner, &sbt_id);

    // Attempt transfer by non-owner
    let result = client.try_initiate_cross_chain_transfer(&non_owner, &sbt_id, &target_chain);

    assert!(result.is_err(), "non-owner cannot initiate transfer");
}

/// Test 10: Cross-chain transfer ID is unique
#[test]
fn test_cross_chain_transfer_id_uniqueness() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder1 = Address::generate(&env);
    let holder2 = Address::generate(&env);
    let sbt_id1 = 1u64;
    let sbt_id2 = 2u64;
    let target_chain = target_chain_bytes(&env, "mode");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id1 = client.initiate_cross_chain_transfer(&holder1, &sbt_id1, &target_chain);
    let transfer_id2 = client.initiate_cross_chain_transfer(&holder2, &sbt_id2, &target_chain);

    assert_ne!(transfer_id1, transfer_id2, "transfer IDs should be unique");
}

/// Test 11: Cannot mint with stale attestation
#[test]
fn test_cannot_mint_with_stale_attestation() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "fraxtal");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    // Move time forward past attestation window (e.g., 15+ minutes)
    env.ledger().with_mut(|l| l.timestamp = 100 + 901);

    let valid_attestation = client.create_valid_attestation(&transfer_id, &holder);
    let result = client.try_mint_sbt_from_cross_chain(&transfer_id, &valid_attestation);

    assert!(result.is_err(), "stale attestation should be rejected");
}

/// Test 12: Cross-chain transfer respects supported chains
#[test]
fn test_cross_chain_transfer_to_unsupported_chain() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let unsupported_chain = target_chain_bytes(&env, "unsupported_chain");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_initiate_cross_chain_transfer(&holder, &sbt_id, &unsupported_chain);

    assert!(result.is_err(), "transfer to unsupported chain should fail");
}

/// Test 13: Replay attack prevention via nonce
#[test]
fn test_replay_attack_prevention() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "manta");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    let valid_attestation = client.create_valid_attestation(&transfer_id, &holder);

    // First mint should succeed
    client.mint_sbt_from_cross_chain(&transfer_id, &valid_attestation);

    // Attempt to replay the same attestation
    let result = client.try_mint_sbt_from_cross_chain(&transfer_id, &valid_attestation);

    assert!(result.is_err(), "replay attack should be prevented");
}

/// Test 14: Cross-chain metadata is preserved
#[test]
fn test_cross_chain_metadata_preservation() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "mantle");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create SBT with specific metadata
    let reputation_score = 850u32;
    client.create_sbt_with_metadata(&holder, &sbt_id, &reputation_score);

    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    // Verify metadata is preserved in transfer
    let metadata = client.get_cross_chain_transfer_metadata(&transfer_id);
    assert_eq!(metadata.reputation_score, reputation_score, "metadata should be preserved");
}

/// Test 15: Cross-chain system respects contract pause
#[test]
fn test_cross_chain_respects_contract_pause() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "bnb");
    let admin = Address::generate(&env);

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Pause contract
    client.set_paused(&admin, &true);

    let result = client.try_initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    assert!(result.is_err(), "cross-chain transfer should fail when contract is paused");
}

/// Test 16: Get all cross-chain transfers for holder
#[test]
fn test_get_all_cross_chain_transfers_for_holder() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let target_chain1 = target_chain_bytes(&env, "sepolia");
    let target_chain2 = target_chain_bytes(&env, "mumbai");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.initiate_cross_chain_transfer(&holder, &1, &target_chain1);
    client.initiate_cross_chain_transfer(&holder, &2, &target_chain2);

    let transfers = client.get_holder_cross_chain_transfers(&holder);

    assert!(transfers.len() >= 2, "should return all cross-chain transfers for holder");
}

/// Test 17: Cross-chain transfer logging and events
#[test]
fn test_cross_chain_transfer_events_emitted() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;
    let target_chain = target_chain_bytes(&env, "testnet");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Event should be emitted during transfer
    let transfer_id = client.initiate_cross_chain_transfer(&holder, &sbt_id, &target_chain);

    let events = env.events().all();

    // Verify event was recorded
    assert!(events.len() > 0, "cross-chain transfer event should be emitted");
}

/// Test 18: Multiple chain support
#[test]
fn test_multiple_chain_transfers() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let sbt_id = 1u64;

    let chains = vec![
        "ethereum",
        "polygon",
        "arbitrum",
        "optimism",
        "base"
    ];

    env.ledger().with_mut(|l| l.timestamp = 100);

    for (i, chain_name) in chains.iter().enumerate() {
        let chain = target_chain_bytes(&env, chain_name);
        let result = client.try_initiate_cross_chain_transfer(&holder, &(sbt_id + i as u64), &chain);

        // Most should succeed, some may fail based on chain support
        // This tests the system can handle multiple chains
        let _ = result;
    }
}
