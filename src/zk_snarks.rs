//! **Commitment-based Self-Attestation with On-Chain Verification**
//!
//! This module provides **commitment-based self-attestation** with on-chain input verification,
//! NOT a zero-knowledge proof system. Proofs are deterministic hashes of claimed values bound
//! to the caller's identity, token, and amount. Verification succeeds if the hash matches and
//! the claimed values satisfy on-chain constraints (balance, blacklist, eligibility, vouch count).
//!
//! **Key Properties:**
//! - **Binding:** Proofs are deterministically derived from the voucher/borrower identity, token, amount, and boolean flags.
//! - **Verifiable:** On-chain code checks that the proof hash is correctly formed and claimed values satisfy constraints.
//! - **Commitment-based:** A separate `ConfidentialCommitment` can be stored and later opened against the amount/blinding factor
//!   to prove that the self-attested values match the committed values (preventing commitment-proofs mismatch attacks).
//! - **Not Private:** The claimed values (balance_ok, blacklisted, eligibility_ok, sufficient_vouches) are boolean
//!   assertions verified at call time; they do not hide the underlying amounts or identity of the voucher/borrower.
//!
//! **Current Limitations:**
//! - This is NOT a real zk-SNARK system and provides no cryptographic privacy. The voucher's balance, blacklist status,
//!   borrower's eligibility, and vouch count are revealed to anyone who observes the on-chain call. See
//!   `docs/threat-model.md` ("Confidentiality Model") for the full disclaimer — do not rely on this module for
//!   privacy guarantees.
//! - `reveal_commitment()` checks that a stored `ConfidentialCommitment` matches an amount/blinding factor supplied
//!   at settlement time. `reveal_vouch_commitment()` / `reveal_loan_commitment()` in `lib.rs` wire this into the
//!   settlement path for `vouch_confidential()` / `request_loan_confidential()`, and guard against replaying an
//!   already-revealed commitment.
//!
//! **Integration Notes:**
//! - `vouch_confidential()` and `request_loan_confidential()` now accept explicit amount and compute boolean flags from
//!   on-chain state (balance, blacklist, eligibility, vouch count).
//! - These functions call the regular `vouch()` and `request_loan()` with the real amounts, ensuring the loan/vouch
//!   is recorded with the caller-supplied values.

use crate::errors::ContractError;
use crate::types::{
    ConfidentialCommitment, DataKey, ZkProof, ZkProofRecord, PROOF_TYPE_LOAN_REQUEST,
    PROOF_TYPE_VOUCH,
};
use soroban_sdk::{Address, Bytes, BytesN, Env, Vec};
use sha3::{Digest, Sha3_256};

const MAX_CONFIDENTIAL_STAKE: i128 = 1_000_000_000;

fn hash_bytes(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(payload);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..]);
    out
}

fn hash_to_bytesn(env: &Env, payload: &[u8]) -> BytesN<32> {
    let digest = hash_bytes(payload);
    BytesN::from_array(env, &digest)
}

fn hash_address(addr: &Address) -> [u8; 32] {
    let addr_str = addr.to_string();
    let mut buf = [0u8; 64];
    let len = addr_str.len() as usize;
    addr_str.to_bytes().copy_into_slice(&mut buf[..len]);
    hash_bytes(&buf[..len])
}

fn hash_i128(value: i128) -> [u8; 32] {
    let mut payload = [0u8; 16];
    payload.copy_from_slice(&value.to_be_bytes());
    hash_bytes(&payload)
}

fn hash_bool(value: bool) -> [u8; 32] {
    hash_bytes(&[if value { 1u8 } else { 0u8 }])
}

fn bound_hash_bytes(env: &Env, proof_type: u32, voucher: &Address, borrower: &Address, token: &Address, stake_amount: i128) -> BytesN<32> {
    let _payload = [0u8; 0];
    let mut hasher = Sha3_256::new();
    hasher.update(&proof_type.to_be_bytes());
    hasher.update(&hash_address(voucher));
    hasher.update(&hash_address(borrower));
    hasher.update(&hash_address(token));
    hasher.update(&hash_i128(stake_amount));
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..]);
    BytesN::from_array(env, &out)
}

fn build_vouch_public_inputs(
    env: &Env,
    voucher: &Address,
    borrower: &Address,
    token: &Address,
    stake_amount: i128,
    balance_ok: bool,
    blacklisted: bool,
) -> Vec<BytesN<32>> {
    let mut inputs = Vec::new(env);
    inputs.push_back(bound_hash_bytes(env, PROOF_TYPE_VOUCH, voucher, borrower, token, stake_amount));
    inputs.push_back(hash_to_bytesn(env, &hash_i128(stake_amount)));
    inputs.push_back(hash_to_bytesn(env, &hash_bool(balance_ok)));
    inputs.push_back(hash_to_bytesn(env, &hash_bool(blacklisted)));
    inputs
}

fn build_loan_public_inputs(
    env: &Env,
    borrower: &Address,
    token: &Address,
    amount: i128,
    threshold: i128,
    eligibility_ok: bool,
    sufficient_vouches: bool,
) -> Vec<BytesN<32>> {
    let mut inputs = Vec::new(env);
    inputs.push_back(bound_hash_bytes(env, PROOF_TYPE_LOAN_REQUEST, borrower, &borrower, token, amount));
    inputs.push_back(hash_to_bytesn(env, &hash_i128(amount)));
    inputs.push_back(hash_to_bytesn(env, &hash_i128(threshold)));
    inputs.push_back(hash_to_bytesn(env, &hash_bool(eligibility_ok)));
    inputs.push_back(hash_to_bytesn(env, &hash_bool(sufficient_vouches)));
    inputs
}

fn proof_digest(env: &Env, proof: &ZkProof) -> BytesN<32> {
    let mut hasher = Sha3_256::new();
    hasher.update(&proof.proof_type.to_be_bytes());
    for input in proof.public_inputs.iter() {
        hasher.update(&input.to_array());
    }
    // NOTE: proof_bytes.len() is intentionally NOT hashed here because
    // proof_bytes IS the output of this function — including its length
    // would make create() and verify() produce different digests.
    let mut proof_bytes = [0u8; 32];
    proof_bytes.copy_from_slice(&hasher.finalize()[..]);
    BytesN::from_array(env, &proof_bytes)
}

pub fn create_vouch_proof(
    env: &Env,
    voucher: &Address,
    borrower: &Address,
    token: &Address,
    stake_amount: i128,
    balance_ok: bool,
    blacklisted: bool,
) -> ZkProof {
    let public_inputs = build_vouch_public_inputs(env, voucher, borrower, token, stake_amount, balance_ok, blacklisted);
    let digest = proof_digest(env, &ZkProof {
        proof_bytes: Bytes::new(env),
        public_inputs: public_inputs.clone(),
        proof_type: PROOF_TYPE_VOUCH,
    });
    ZkProof {
        proof_bytes: Bytes::from_array(env, &digest.to_array()),
        public_inputs,
        proof_type: PROOF_TYPE_VOUCH,
    }
}

pub fn create_loan_proof(
    env: &Env,
    borrower: &Address,
    token: &Address,
    amount: i128,
    threshold: i128,
    eligibility_ok: bool,
    sufficient_vouches: bool,
) -> ZkProof {
    let public_inputs = build_loan_public_inputs(env, borrower, token, amount, threshold, eligibility_ok, sufficient_vouches);
    let digest = proof_digest(env, &ZkProof {
        proof_bytes: Bytes::new(env),
        public_inputs: public_inputs.clone(),
        proof_type: PROOF_TYPE_LOAN_REQUEST,
    });
    ZkProof {
        proof_bytes: Bytes::from_array(env, &digest.to_array()),
        public_inputs,
        proof_type: PROOF_TYPE_LOAN_REQUEST,
    }
}

pub fn commit_amount(env: &Env, amount: i128, blinding: &[u8]) -> Result<ConfidentialCommitment, ContractError> {
    let mut hasher = Sha3_256::new();
    hasher.update(&amount.to_be_bytes());
    hasher.update(blinding);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..]);
    Ok(ConfidentialCommitment {
        commitment: BytesN::from_array(env, &out),
    })
}

/// Blinding factors longer than this are rejected by `reveal_commitment_bytes` rather than
/// silently truncated.
const MAX_BLINDING_LEN: usize = 128;

/// Verify that a previously-stored `ConfidentialCommitment` opens to `amount`/`blinding`.
///
/// Recomputes the SHA3-256 digest over `amount` and `blinding` the same way `commit_amount`
/// does, and compares it against `commitment.commitment`. Returns `CommitmentMismatch` if the
/// recomputed digest doesn't match — i.e. the caller is trying to settle with different values
/// than they committed to.
pub fn reveal_commitment(
    env: &Env,
    commitment: &ConfidentialCommitment,
    amount: i128,
    blinding: &[u8],
) -> Result<(), ContractError> {
    let recomputed = commit_amount(env, amount, blinding)?;
    if recomputed.commitment != commitment.commitment {
        return Err(ContractError::CommitmentMismatch);
    }
    Ok(())
}

/// `reveal_commitment` taking the blinding factor as a contract-call `Bytes` argument
/// (used by the `reveal_vouch_commitment` / `reveal_loan_commitment` entry points).
pub fn reveal_commitment_bytes(
    env: &Env,
    commitment: &ConfidentialCommitment,
    amount: i128,
    blinding: &Bytes,
) -> Result<(), ContractError> {
    let len = blinding.len() as usize;
    if len > MAX_BLINDING_LEN {
        return Err(ContractError::InvalidProof);
    }
    let mut buf = [0u8; MAX_BLINDING_LEN];
    blinding.copy_into_slice(&mut buf[..len]);
    reveal_commitment(env, commitment, amount, &buf[..len])
}

/// Reads the `i`-th public input, rejecting a proof that is too short instead of panicking.
fn public_input_at(proof: &ZkProof, i: u32) -> Result<BytesN<32>, ContractError> {
    proof.public_inputs.get(i).ok_or(ContractError::InvalidProof)
}

pub fn verify_vouch_proof(
    env: &Env,
    proof: &ZkProof,
    voucher: &Address,
    borrower: &Address,
    token: &Address,
    stake_amount: i128,
    balance_ok: bool,
    blacklisted: bool,
) -> Result<(), ContractError> {
    if proof.proof_type != PROOF_TYPE_VOUCH {
        return Err(ContractError::InvalidProof);
    }
    if proof.public_inputs.len() != 4 {
        return Err(ContractError::InvalidProof);
    }

    let expected_context = bound_hash_bytes(env, PROOF_TYPE_VOUCH, voucher, borrower, token, stake_amount);
    let expected_amount = hash_to_bytesn(env, &hash_i128(stake_amount));
    let expected_balance = hash_to_bytesn(env, &hash_bool(balance_ok));
    let expected_blacklist = hash_to_bytesn(env, &hash_bool(blacklisted));

    if public_input_at(proof, 0)? != expected_context
        || public_input_at(proof, 1)? != expected_amount
        || public_input_at(proof, 2)? != expected_balance
        || public_input_at(proof, 3)? != expected_blacklist
    {
        return Err(ContractError::InvalidProof);
    }

    if stake_amount <= 0 || stake_amount > MAX_CONFIDENTIAL_STAKE {
        return Err(ContractError::InvalidAmount);
    }

    let expected_proof_bytes = proof_digest(env, proof);
    if proof.proof_bytes != soroban_sdk::Bytes::from(&expected_proof_bytes) {
        return Err(ContractError::InvalidProof);
    }

    Ok(())
}

pub fn verify_loan_proof(
    env: &Env,
    proof: &ZkProof,
    borrower: &Address,
    token: &Address,
    amount: i128,
    threshold: i128,
    eligibility_ok: bool,
    sufficient_vouches: bool,
) -> Result<(), ContractError> {
    if proof.proof_type != PROOF_TYPE_LOAN_REQUEST {
        return Err(ContractError::InvalidProof);
    }
    if proof.public_inputs.len() != 5 {
        return Err(ContractError::InvalidProof);
    }

    let expected_context = bound_hash_bytes(env, PROOF_TYPE_LOAN_REQUEST, borrower, &borrower, token, amount);
    let expected_amount = hash_to_bytesn(env, &hash_i128(amount));
    let expected_threshold = hash_to_bytesn(env, &hash_i128(threshold));
    let expected_eligibility = hash_to_bytesn(env, &hash_bool(eligibility_ok));
    let expected_vouches = hash_to_bytesn(env, &hash_bool(sufficient_vouches));

    if public_input_at(proof, 0)? != expected_context
        || public_input_at(proof, 1)? != expected_amount
        || public_input_at(proof, 2)? != expected_threshold
        || public_input_at(proof, 3)? != expected_eligibility
        || public_input_at(proof, 4)? != expected_vouches
    {
        return Err(ContractError::InvalidProof);
    }

    if amount <= 0 || amount > MAX_CONFIDENTIAL_STAKE {
        return Err(ContractError::InvalidAmount);
    }
    if threshold <= 0 || threshold > MAX_CONFIDENTIAL_STAKE {
        return Err(ContractError::InvalidAmount);
    }

    let expected_proof_bytes = proof_digest(env, proof);
    if proof.proof_bytes != soroban_sdk::Bytes::from(&expected_proof_bytes) {
        return Err(ContractError::InvalidProof);
    }

    Ok(())
}

pub fn record_proof(
    env: &Env,
    proof: &ZkProof,
    operation_type: u32,
    submitter: &Address,
) -> Result<ZkProofRecord, ContractError> {
    let proof_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::ZkProofCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(ContractError::CounterOverflow)?;
    env.storage().instance().set(&DataKey::ZkProofCounter, &proof_id);
    Ok(ZkProofRecord {
        proof_id,
        proof: proof.clone(),
        operation_type,
        submitter: submitter.clone(),
        verified: true,
        submitted_at: env.ledger().timestamp(),
    })
}

#[cfg(test)]
mod malformed_input_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Bytes};

    const STAKE: i128 = 500_000;

    struct Fixture {
        env: Env,
        voucher: Address,
        borrower: Address,
        token: Address,
    }

    fn fixture() -> Fixture {
        let env = Env::default();
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        let token = Address::generate(&env);
        Fixture { env, voucher, borrower, token }
    }

    /// Positive control: the unmodified proof must verify, so the negative
    /// cases below fail only because of the injected malformation.
    #[test]
    fn well_formed_vouch_proof_verifies() {
        let f = fixture();
        let proof = create_vouch_proof(&f.env, &f.voucher, &f.borrower, &f.token, STAKE, true, false);
        assert_eq!(
            verify_vouch_proof(&f.env, &proof, &f.voucher, &f.borrower, &f.token, STAKE, true, false),
            Ok(())
        );
    }

    /// Truncated proof bytes (0..32) and oversized proof bytes must be rejected
    /// with `InvalidProof`, never a panic.
    #[test]
    fn truncated_and_oversized_proof_bytes_are_rejected() {
        let f = fixture();
        let proof = create_vouch_proof(&f.env, &f.voucher, &f.borrower, &f.token, STAKE, true, false);

        let check = |bytes: Bytes, label: &str| {
            let candidate = ZkProof {
                proof_bytes: bytes,
                public_inputs: proof.public_inputs.clone(),
                proof_type: PROOF_TYPE_VOUCH,
            };
            assert_eq!(
                verify_vouch_proof(&f.env, &candidate, &f.voucher, &f.borrower, &f.token, STAKE, true, false),
                Err(ContractError::InvalidProof),
                "{label}"
            );
        };

        for n in 0..32u32 {
            check(proof.proof_bytes.slice(0..n), "truncated proof bytes should be rejected");
        }
        for n in [33u32, 64, 128, 1024] {
            let mut oversized = Bytes::new(&f.env);
            for _ in 0..n {
                oversized.push_back(0xAB);
            }
            check(oversized, "oversized proof bytes should be rejected");
        }
    }

    /// A vouch proof with fewer public inputs than required must be rejected
    /// before any index access, for every truncation length.
    #[test]
    fn truncated_vouch_public_inputs_are_rejected() {
        let f = fixture();
        let proof = create_vouch_proof(&f.env, &f.voucher, &f.borrower, &f.token, STAKE, true, false);

        for keep in 0..4u32 {
            let mut inputs = Vec::new(&f.env);
            for i in 0..keep {
                inputs.push_back(proof.public_inputs.get(i).unwrap());
            }
            let candidate = ZkProof {
                proof_bytes: proof.proof_bytes.clone(),
                public_inputs: inputs,
                proof_type: PROOF_TYPE_VOUCH,
            };
            assert_eq!(
                verify_vouch_proof(&f.env, &candidate, &f.voucher, &f.borrower, &f.token, STAKE, true, false),
                Err(ContractError::InvalidProof),
                "vouch proof with {keep} public inputs should be rejected"
            );
        }
    }

    /// Same as above for loan proofs, which require five public inputs.
    #[test]
    fn truncated_loan_public_inputs_are_rejected() {
        let f = fixture();
        let proof = create_loan_proof(&f.env, &f.borrower, &f.token, STAKE, STAKE, true, true);

        for keep in 0..5u32 {
            let mut inputs = Vec::new(&f.env);
            for i in 0..keep {
                inputs.push_back(proof.public_inputs.get(i).unwrap());
            }
            let candidate = ZkProof {
                proof_bytes: proof.proof_bytes.clone(),
                public_inputs: inputs,
                proof_type: PROOF_TYPE_LOAN_REQUEST,
            };
            assert_eq!(
                verify_loan_proof(&f.env, &candidate, &f.borrower, &f.token, STAKE, STAKE, true, true),
                Err(ContractError::InvalidProof),
                "loan proof with {keep} public inputs should be rejected"
            );
        }
    }

    /// Five public inputs with a wrong proof type must still be rejected
    /// without panicking.
    #[test]
    fn loan_proof_with_wrong_type_is_rejected() {
        let f = fixture();
        let proof = create_loan_proof(&f.env, &f.borrower, &f.token, STAKE, STAKE, true, true);
        let candidate = ZkProof {
            proof_bytes: proof.proof_bytes.clone(),
            public_inputs: proof.public_inputs.clone(),
            proof_type: PROOF_TYPE_VOUCH,
        };
        assert_eq!(
            verify_loan_proof(&f.env, &candidate, &f.borrower, &f.token, STAKE, STAKE, true, true),
            Err(ContractError::InvalidProof)
        );
    }

    #[test]
    fn record_proof_returns_record_with_incrementing_ids() {
        let f = fixture();
        let proof = create_vouch_proof(&f.env, &f.voucher, &f.borrower, &f.token, STAKE, true, false);
        f.env.as_contract(&f.voucher, || {
            let first = record_proof(&f.env, &proof, PROOF_TYPE_VOUCH, &f.voucher).unwrap();
            let second = record_proof(&f.env, &proof, PROOF_TYPE_VOUCH, &f.voucher).unwrap();
            assert_eq!(first.proof_id, 1);
            assert_eq!(second.proof_id, 2);
            assert!(first.verified);
        });
    }
}
