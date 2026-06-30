#![cfg(test)]

use soroban_sdk::{Address, BytesN};
use crate::zk_snarks::*;
use crate::types::{ZkProof, ConfidentialCommitment, PROOF_TYPE_VOUCH, PROOF_TYPE_LOAN_REQUEST};

#[test]
fn test_commitment_creation() {
    let env = soroban_sdk::Env::default();
    let amount = 1000i128;
    let blinding = BytesN::from_array(&env, &[1u8; 32]);
    let context = BytesN::from_array(&env, &[2u8; 32]);
    
    let commitment = create_commitment(&env, amount, &blinding, &context);
    
    // Verify commitment is deterministic
    let commitment2 = create_commitment(&env, amount, &blinding, &context);
    assert_eq!(commitment.commitment, commitment2.commitment);
    
    // Verify different inputs produce different commitments
    let commitment3 = create_commitment(&env, 2000, &blinding, &context);
    assert_ne!(commitment.commitment, commitment3.commitment);
}

#[test]
fn test_commitment_verification() {
    let env = soroban_sdk::Env::default();
    let amount = 1000i128;
    let blinding = BytesN::from_array(&env, &[1u8; 32]);
    let context = BytesN::from_array(&env, &[2u8; 32]);
    
    let commitment = create_commitment(&env, amount, &blinding, &context);
    
    // Correct opening should verify
    assert!(verify_commitment_opening(&env, &commitment, amount, &context));
    
    // Wrong amount should not verify
    assert!(!verify_commitment_opening(&env, &commitment, 2000, &context));
    
    // Wrong context should not verify
    let wrong_context = BytesN::from_array(&env, &[3u8; 32]);
    assert!(!verify_commitment_opening(&env, &commitment, amount, &wrong_context));
}

#[test]
fn test_vk_hash_storage() {
    let env = soroban_sdk::Env::default();
    let proof_type = PROOF_TYPE_VOUCH;
    let vk_hash = BytesN::from_array(&env, &[5u8; 32]);
    
    // Set the verifying key hash
    set_vk_hash(&env, proof_type, vk_hash);
    
    // Retrieve the verifying key hash
    let retrieved = get_vk_hash(&env, proof_type);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), vk_hash);
    
    // Verify non-existent proof type returns None
    let non_existent = get_vk_hash(&env, 999);
    assert!(non_existent.is_none());
}

#[test]
fn test_proof_type_validation() {
    let env = soroban_sdk::Env::default();
    
    let valid_proof = ZkProof {
        proof_bytes: soroban_sdk::Bytes::from_array(&env, &[1u8; 64]),
        public_inputs: soroban_sdk::Vec::new(&env),
        proof_type: PROOF_TYPE_VOUCH,
    };
    
    let invalid_proof = ZkProof {
        proof_bytes: soroban_sdk::Bytes::from_array(&env, &[1u8; 64]),
        public_inputs: soroban_sdk::Vec::new(&env),
        proof_type: 999, // Invalid proof type
    };
    
    let public_params = get_vouch_public_params(&env);
    let expected_input = BytesN::from_array(&env, &[0u8; 32]);
    
    // Valid proof type should not return InvalidProofType error
    let result = verify_proof(&env, &valid_proof, &public_params, &expected_input);
    // Note: This will fail verification due to hash mismatch, but should not be InvalidProofType
    match result {
        Err(crate::ContractError::InvalidProofType) => panic!("Should not return InvalidProofType for valid proof type"),
        _ => {}, // Expected
    }
    
    // Invalid proof type should return InvalidProofType error
    let result = verify_proof(&env, &invalid_proof, &public_params, &expected_input);
    assert!(matches!(result, Err(crate::ContractError::InvalidProofType)));
}

#[test]
fn test_vouch_proof_verification_structure() {
    let env = soroban_sdk::Env::default();
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let expected_stake = 1000i128;
    
    let proof = ZkProof {
        proof_bytes: soroban_sdk::Bytes::from_array(&env, &[1u8; 64]),
        public_inputs: soroban_sdk::Vec::new(&env),
        proof_type: PROOF_TYPE_VOUCH,
    };
    
    // This will fail verification due to hash mismatch, but tests the structure
    let result = verify_vouch_proof(&env, &proof, &voucher, &borrower, expected_stake);
    // Should not panic, just return an error
    assert!(result.is_err());
}

#[test]
fn test_loan_proof_verification_structure() {
    let env = soroban_sdk::Env::default();
    let borrower = Address::generate(&env);
    let requested_amount = 5000i128;
    let total_stake = 10000i128;
    
    let proof = ZkProof {
        proof_bytes: soroban_sdk::Bytes::from_array(&env, &[1u8; 64]),
        public_inputs: soroban_sdk::Vec::new(&env),
        proof_type: PROOF_TYPE_LOAN_REQUEST,
    };
    
    // This will fail verification due to hash mismatch, but tests the structure
    let result = verify_loan_proof(&env, &proof, &borrower, requested_amount, total_stake);
    // Should not panic, just return an error
    assert!(result.is_err());
}

#[test]
fn test_confidential_commitment_storage() {
    let env = soroban_sdk::Env::default();
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    
    let commitment = ConfidentialCommitment {
        commitment: BytesN::from_array(&env, &[1u8; 32]),
        blinding: BytesN::from_array(&env, &[2u8; 32]),
    };
    
    // Store the commitment
    env.storage()
        .persistent()
        .set(&crate::types::DataKey::VouchCommitment(voucher.clone(), borrower.clone()), &commitment);
    
    // Retrieve the commitment
    let retrieved: Option<ConfidentialCommitment> = env
        .storage()
        .persistent()
        .get(&crate::types::DataKey::VouchCommitment(voucher, borrower));
    
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.commitment, commitment.commitment);
    assert_eq!(retrieved.blinding, commitment.blinding);
}
