#![cfg(test)]

use crate::cross_chain::{BridgeAttestation, CrossChainLoanMetadata};
use crate::types::LoanStatus;
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup(env: &Env) -> (QuorumCreditContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuorumCreditContract, ());
    let client = QuorumCreditContractClient::new(env, &contract_id);

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();

    let admins = Vec::from_array(env, [admin.clone()]);
    client.initialize(&deployer, &admins, &1u32, &token);

    (client, admin)
}

fn sign(key: &SigningKey, message: &soroban_sdk::Bytes) -> [u8; 64] {
    let mut buf = [0u8; 32];
    message.copy_into_slice(&mut buf);
    key.sign(&buf).to_bytes()
}

/// Audit finding: `validate_bridge_attestation` must perform genuine Ed25519
/// signature verification against the registered attestor key, not a structural
/// check. This proves a tampered/forged signature is rejected, while a
/// genuinely-signed attestation over the identical payload is accepted.
#[test]
fn forged_bridge_attestation_is_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);

    let chain_id = 7u32;
    let bridge_address = Address::generate(&env);
    client.register_bridge(
        &admins,
        &chain_id,
        &String::from_str(&env, "test-chain"),
        &bridge_address,
    );

    let real_key = SigningKey::from_bytes(&[7u8; 32]);
    let real_public_key = BytesN::from_array(&env, &real_key.verifying_key().to_bytes());
    client.set_bridge_public_key(&admins, &chain_id, &real_public_key);

    let borrower = Address::generate(&env);
    let metadata = CrossChainLoanMetadata {
        origin_chain: chain_id,
        loan_id: 42,
        borrower: borrower.clone(),
        amount: 1_000_000,
        status: LoanStatus::Repaid,
        reputation_score: 800,
    };
    let nonce = 1u64;
    let timestamp = env.ledger().timestamp();
    let confirmations = 20u32;

    let message =
        client.bridge_attestation_message(&metadata, &nonce, &timestamp, &confirmations);

    // Forged: signed by a DIFFERENT key than the one registered for this bridge.
    let attacker_key = SigningKey::from_bytes(&[9u8; 32]);
    let forged_signature = BytesN::from_array(&env, &sign(&attacker_key, &message));
    let forged_attestation = BridgeAttestation {
        nonce,
        timestamp,
        confirmations,
        signature: forged_signature,
    };
    let forged_result =
        client.try_validate_bridge_attestation(&metadata, &forged_attestation);
    assert!(
        forged_result.is_err(),
        "an attestation signed by the wrong key must be rejected"
    );

    // Tampered: real key, but the signature is over a mutated payload (bit-flipped nonce).
    let tampered_message = client.bridge_attestation_message(
        &metadata,
        &(nonce + 1),
        &timestamp,
        &confirmations,
    );
    let tampered_signature = BytesN::from_array(&env, &sign(&real_key, &tampered_message));
    let tampered_attestation = BridgeAttestation {
        nonce,
        timestamp,
        confirmations,
        signature: tampered_signature,
    };
    let tampered_result =
        client.try_validate_bridge_attestation(&metadata, &tampered_attestation);
    assert!(
        tampered_result.is_err(),
        "a signature over a different payload than the one submitted must be rejected"
    );

    // Genuine: real key, real payload -> accepted.
    let real_signature = BytesN::from_array(&env, &sign(&real_key, &message));
    let real_attestation = BridgeAttestation {
        nonce,
        timestamp,
        confirmations,
        signature: real_signature,
    };
    client.validate_bridge_attestation(&metadata, &real_attestation);

    // The nonce is now consumed -- replaying the same genuine attestation must fail too.
    let replay_result = client.try_validate_bridge_attestation(&metadata, &real_attestation);
    assert!(replay_result.is_err(), "a consumed nonce must not be replayable");
}

/// Insufficient claimed confirmations must be rejected outright, even with a
/// valid signature -- this is the finality/confirmation-depth guard.
#[test]
fn insufficient_confirmations_is_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);

    let chain_id = 3u32;
    client.register_bridge(
        &admins,
        &chain_id,
        &String::from_str(&env, "low-conf-chain"),
        &Address::generate(&env),
    );
    let key = SigningKey::from_bytes(&[3u8; 32]);
    let public_key = BytesN::from_array(&env, &key.verifying_key().to_bytes());
    client.set_bridge_public_key(&admins, &chain_id, &public_key);

    let metadata = CrossChainLoanMetadata {
        origin_chain: chain_id,
        loan_id: 1,
        borrower: Address::generate(&env),
        amount: 500,
        status: LoanStatus::Repaid,
        reputation_score: 500,
    };
    let nonce = 1u64;
    let timestamp = env.ledger().timestamp();
    let low_confirmations = 1u32; // below MIN_BRIDGE_CONFIRMATIONS

    let message =
        client.bridge_attestation_message(&metadata, &nonce, &timestamp, &low_confirmations);
    let signature = BytesN::from_array(&env, &sign(&key, &message));
    let attestation = BridgeAttestation {
        nonce,
        timestamp,
        confirmations: low_confirmations,
        signature,
    };

    let result = client.try_validate_bridge_attestation(&metadata, &attestation);
    assert!(
        result.is_err(),
        "an attestation claiming too few confirmations must be rejected"
    );
}
