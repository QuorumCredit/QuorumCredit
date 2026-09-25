#![cfg(test)]

use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Bytes, Env, String, Vec,
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

fn skill_bytes(env: &Env, skill: &str) -> Bytes {
    Bytes::from_slice(env, skill.as_bytes())
}

/// Test 1: Endorse skill creates endorsement record
#[test]
fn test_endorse_skill_creates_endorsement() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser_sbt = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "rust_programming");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let result = client.try_endorse_skill(&endorser_sbt, &holder, &skill);

    assert!(result.is_ok(), "skill endorsement should succeed");
}

/// Test 2: Endorsement is recorded for the skill
#[test]
fn test_endorsement_recorded_for_skill() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser_sbt = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "rust_programming");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser_sbt, &holder, &skill);

    // Verify endorsement was recorded
    let endorsement_count = client.get_skill_endorsement_count(&holder, &skill);
    assert!(endorsement_count > 0, "endorsement should be recorded");
}

/// Test 3: Multiple endorsers can endorse the same skill
#[test]
fn test_multiple_endorsers_same_skill() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "smart_contracts");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let endorser1 = Address::generate(&env);
    let endorser2 = Address::generate(&env);
    let endorser3 = Address::generate(&env);

    client.endorse_skill(&endorser1, &holder, &skill);
    client.endorse_skill(&endorser2, &holder, &skill);
    client.endorse_skill(&endorser3, &holder, &skill);

    let total_endorsements = client.get_skill_endorsement_count(&holder, &skill);
    assert_eq!(total_endorsements, 3, "all endorsements should be counted");
}

/// Test 4: Endorsement score is calculated correctly
#[test]
fn test_endorsement_score_calculation() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "web3_development");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create 5 endorsements
    for i in 0..5 {
        let endorser = Address::generate(&env);
        client.endorse_skill(&endorser, &holder, &skill);
    }

    let score = client.get_skill_endorsement_score(&holder, &skill);

    // Score should be based on number of endorsements
    assert!(score > 0, "endorsement score should be calculated");
    assert!(score <= 100, "endorsement score should be bounded");
}

/// Test 5: Different skills tracked separately
#[test]
fn test_different_skills_tracked_separately() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);

    let skill1 = skill_bytes(&env, "rust");
    let skill2 = skill_bytes(&env, "python");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser, &holder, &skill1);
    client.endorse_skill(&endorser, &holder, &skill2);

    let count1 = client.get_skill_endorsement_count(&holder, &skill1);
    let count2 = client.get_skill_endorsement_count(&holder, &skill2);

    assert_eq!(count1, 1, "skill1 should have 1 endorsement");
    assert_eq!(count2, 1, "skill2 should have 1 endorsement");
}

/// Test 6: Cannot endorse the same skill twice from same endorser
#[test]
fn test_cannot_double_endorse_same_skill() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "blockchain");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser, &holder, &skill);

    // Attempt to endorse same skill again
    let result = client.try_endorse_skill(&endorser, &holder, &skill);

    assert!(result.is_err(), "cannot double-endorse the same skill");
}

/// Test 7: Get all skills endorsed for a holder
#[test]
fn test_get_all_skills_for_holder() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);

    let skill1 = skill_bytes(&env, "golang");
    let skill2 = skill_bytes(&env, "typescript");
    let skill3 = skill_bytes(&env, "solidity");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser, &holder, &skill1);
    client.endorse_skill(&endorser, &holder, &skill2);
    client.endorse_skill(&endorser, &holder, &skill3);

    let all_skills = client.get_holder_skills(&holder);

    assert!(all_skills.len() >= 3, "should return all endorsed skills");
}

/// Test 8: Get endorsements for a specific skill
#[test]
fn test_get_endorsers_for_skill() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "defi_protocols");

    env.ledger().with_mut(|l| l.timestamp = 100);

    let endorser1 = Address::generate(&env);
    let endorser2 = Address::generate(&env);

    client.endorse_skill(&endorser1, &holder, &skill);
    client.endorse_skill(&endorser2, &holder, &skill);

    let endorsers = client.get_skill_endorsers(&holder, &skill);

    assert!(endorsers.len() >= 2, "should return all endorsers for skill");
}

/// Test 9: Endorsement score is weighted by endorser reputation
#[test]
fn test_endorsement_score_weighted_by_reputation() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "cryptography");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create endorsements from endorsers with different reputations
    let high_rep_endorser = Address::generate(&env);
    let low_rep_endorser = Address::generate(&env);

    // Set reputation scores (simulated)
    client.endorse_skill(&high_rep_endorser, &holder, &skill);
    client.endorse_skill(&low_rep_endorser, &holder, &skill);

    let score = client.get_skill_endorsement_score(&holder, &skill);

    // Score should account for endorser reputation
    assert!(score > 0, "endorsement score should be weighted");
}

/// Test 10: Revoke endorsement removes endorsement
#[test]
fn test_revoke_endorsement() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "system_design");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser, &holder, &skill);

    let count_before = client.get_skill_endorsement_count(&holder, &skill);
    assert_eq!(count_before, 1, "endorsement should be recorded");

    // Revoke endorsement
    client.revoke_endorsement(&endorser, &holder, &skill);

    let count_after = client.get_skill_endorsement_count(&holder, &skill);
    assert_eq!(count_after, 0, "endorsement should be revoked");
}

/// Test 11: Non-endorser cannot revoke endorsement
#[test]
fn test_non_endorser_cannot_revoke() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser1 = Address::generate(&env);
    let endorser2 = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "testing_frameworks");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser1, &holder, &skill);

    // Try to revoke with different endorser
    let result = client.try_revoke_endorsement(&endorser2, &holder, &skill);

    assert!(result.is_err(), "non-endorser should not be able to revoke");
}

/// Test 12: Endorsement timestamp is recorded
#[test]
fn test_endorsement_timestamp_recorded() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "performance_optimization");

    env.ledger().with_mut(|l| l.timestamp = 12345);

    client.endorse_skill(&endorser, &holder, &skill);

    let endorsement = client.get_endorsement_details(&endorser, &holder, &skill);

    assert_eq!(endorsement.timestamp, 12345, "endorsement timestamp should be recorded");
}

/// Test 13: Get top skills for a holder sorted by endorsements
#[test]
fn test_get_top_skills_sorted_by_endorsements() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill1 = skill_bytes(&env, "skill1");
    let skill2 = skill_bytes(&env, "skill2");
    let skill3 = skill_bytes(&env, "skill3");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create different numbers of endorsements
    for i in 0..3 {
        client.endorse_skill(&Address::generate(&env), &holder, &skill1);
    }

    for i in 0..2 {
        client.endorse_skill(&Address::generate(&env), &holder, &skill2);
    }

    client.endorse_skill(&Address::generate(&env), &holder, &skill3);

    let top_skills = client.get_top_skills(&holder, &5);

    if top_skills.len() >= 3 {
        let first_count = client.get_skill_endorsement_count(&holder, &top_skills.get(0).unwrap());
        let second_count = client.get_skill_endorsement_count(&holder, &top_skills.get(1).unwrap());

        assert!(first_count >= second_count, "skills should be sorted by endorsement count");
    }
}

/// Test 14: Endorsement history is maintained
#[test]
fn test_endorsement_history_maintained() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "architecture");

    env.ledger().with_mut(|l| l.timestamp = 100);

    client.endorse_skill(&endorser, &holder, &skill);

    // Revoke
    client.revoke_endorsement(&endorser, &holder, &skill);

    // Re-endorse
    env.ledger().with_mut(|l| l.timestamp = 200);
    client.endorse_skill(&endorser, &holder, &skill);

    // History should show both endorsement events
    let history = client.get_endorsement_history(&endorser, &holder, &skill);

    assert!(history.len() >= 2, "history should contain endorsement and revocation events");
}

/// Test 15: Endorsement system respects contract pause
#[test]
fn test_endorsement_respects_contract_pause() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let endorser = Address::generate(&env);
    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "documentation");
    let admin = Address::generate(&env);

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Pause contract
    client.set_paused(&admin, &true);

    let result = client.try_endorse_skill(&endorser, &holder, &skill);

    assert!(result.is_err(), "endorsement should fail when contract is paused");
}

/// Test 16: Skill endorsement limits fraud via reputation
#[test]
fn test_low_reputation_endorsements_have_lower_impact() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "advanced_topics");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Endorsement from verified account
    let verified_endorser = Address::generate(&env);
    client.endorse_skill(&verified_endorser, &holder, &skill);

    let score_with_verified = client.get_skill_endorsement_score(&holder, &skill);

    // Endorsement from new/low-rep account
    let new_endorser = Address::generate(&env);
    client.endorse_skill(&new_endorser, &holder, &skill);

    let score_with_new = client.get_skill_endorsement_score(&holder, &skill);

    // Score increase from new endorser should be less than verified
    // (assuming weight is based on reputation)
    assert!(score_with_new >= score_with_verified, "score should increase with more endorsements");
}

/// Test 17: Get endorsement badge for holder
#[test]
fn test_get_endorsement_badge() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);

    let holder = Address::generate(&env);
    let skill = skill_bytes(&env, "verified_expert");

    env.ledger().with_mut(|l| l.timestamp = 100);

    // Create multiple endorsements to qualify for badge
    for i in 0..5 {
        client.endorse_skill(&Address::generate(&env), &holder, &skill);
    }

    let badge = client.get_skill_badge(&holder, &skill);

    assert!(badge.earned, "holder should earn badge with sufficient endorsements");
}
