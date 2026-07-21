use crate::zk_snarks;
use soroban_sdk::{testutils::Address as _, Address, Env};
use crate::types::{ConfidentialCommitment, ZkProof};

#[test]
fn forged_vouch_proof_is_rejected() {
    let env = Env::default();
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let token = Address::generate(&env);
    let proof = zk_snarks::create_vouch_proof(&env, &voucher, &borrower, &token, 500_000, true, false);

    let malformed = ZkProof {
        proof_bytes: proof.proof_bytes,
        public_inputs: proof.public_inputs,
        proof_type: 999,
    };

    assert!(zk_snarks::verify_vouch_proof(&env, &malformed, &voucher, &borrower, &token, 500_000, true, false).is_err());
}

#[test]
fn cross_binding_vouch_proof_is_rejected() {
    let env = Env::default();
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let other_borrower = Address::generate(&env);
    let token = Address::generate(&env);
    let proof = zk_snarks::create_vouch_proof(&env, &voucher, &borrower, &token, 250_000, true, false);

    assert!(zk_snarks::verify_vouch_proof(&env, &proof, &voucher, &other_borrower, &token, 250_000, true, false).is_err());
}

#[test]
fn commitment_does_not_reveal_amount_from_on_chain_state() {
    let env = Env::default();
    let commitment_a = zk_snarks::commit_amount(&env, 250_000, b"blind-a").unwrap();
    let commitment_b = zk_snarks::commit_amount(&env, 500_000, b"blind-b").unwrap();

    let on_chain_only = ConfidentialCommitment {
        commitment: commitment_a.commitment,
    };

    assert_ne!(on_chain_only.commitment, commitment_b.commitment);
    assert_eq!(on_chain_only.commitment.len(), 32);
}

#[test]
fn loan_proof_is_verified_for_the_correct_borrower_token_pair() {
    let env = Env::default();
    let borrower = Address::generate(&env);
    let token = Address::generate(&env);
    let proof = zk_snarks::create_loan_proof(&env, &borrower, &token, 1_000_000, 500_000, true, false);

    assert!(zk_snarks::verify_loan_proof(&env, &proof, &borrower, &token, 1_000_000, 500_000, true, false).is_ok());
    let other_borrower = Address::generate(&env);
    assert!(zk_snarks::verify_loan_proof(&env, &proof, &other_borrower, &token, 1_000_000, 500_000, true, false).is_err());
}
