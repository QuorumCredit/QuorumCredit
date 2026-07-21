//! Cross-chain bridge attestation (Issue #14 / #968 / #85).
//!
//! Bridge registration and the "is this chain active" check live in `vouch.rs`
//! (used directly by `vouch_cross_chain`). This module adds the layer on top:
//! verifying that a cross-chain event actually happened, via a real Ed25519
//! signature from a bridge-specific attestor key, with nonce-replay protection,
//! a freshness window, and a minimum-confirmations claim.
//!
//! See `docs/cross-chain-trust-model.md` for what this contract can and cannot
//! verify on its own.

use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::{DataKey, LoanStatus};
use crate::vouch;
use soroban_sdk::{contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

/// An attestation older than this (relative to the ledger clock) is rejected as stale.
pub const BRIDGE_ATTESTATION_MAX_AGE_SECS: u64 = 10 * 60;
/// An attestation timestamped further than this into the future is rejected — guards
/// against a misconfigured or compromised attestor clock.
pub const BRIDGE_ATTESTATION_MAX_SKEW_SECS: u64 = 60;
/// Minimum origin-chain confirmations an attestor must claim before its attestation
/// is accepted. This is a claim the attestor signs, not something this contract can
/// verify independently — see the trust-model doc.
pub const MIN_BRIDGE_CONFIRMATIONS: u32 = 12;

/// A snapshot of loan/reputation state on another chain, to be mirrored here.
#[contracttype]
#[derive(Clone)]
pub struct CrossChainLoanMetadata {
    pub origin_chain: u32,
    pub loan_id: u64,
    pub borrower: Address,
    pub amount: i128,
    pub status: LoanStatus,
    pub reputation_score: u32,
}

/// A signed attestation from a bridge's registered Ed25519 key, covering one
/// `CrossChainLoanMetadata` payload.
#[contracttype]
#[derive(Clone)]
pub struct BridgeAttestation {
    pub nonce: u64,
    pub timestamp: u64,
    /// Confirmations the attestor claims the origin chain had reached for this event.
    pub confirmations: u32,
    pub signature: BytesN<64>,
}

/// A borrower's reputation as known locally, merged with the latest snapshot
/// mirrored in from another chain (if any).
#[contracttype]
#[derive(Clone)]
pub struct UnifiedReputation {
    pub native_score: Option<u32>,
    pub mirrored_score: Option<u32>,
    pub mirrored_from_chain: Option<u32>,
    pub mirrored_at: Option<u64>,
}

/// Internal record of the last cross-chain reputation snapshot applied for a borrower,
/// keyed by borrower so a stale/older attestation can't clobber a newer one.
#[contracttype]
#[derive(Clone)]
struct CrossChainReputationRecord {
    score: u32,
    chain: u32,
    updated_at: u64,
}

/// Admin: configure or rotate the Ed25519 key trusted to sign attestations from
/// `origin_chain`. The chain must already be registered via `register_bridge`.
pub fn set_bridge_public_key(
    env: Env,
    admin_signers: Vec<Address>,
    origin_chain: u32,
    public_key: BytesN<32>,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    vouch::validate_bridge(&env, origin_chain)?;

    env.storage()
        .persistent()
        .set(&DataKey::BridgePublicKey(origin_chain), &public_key);
    Ok(())
}

/// Canonical bytes an origin-chain attestor key must sign for this payload.
pub fn bridge_attestation_message(
    env: &Env,
    metadata: &CrossChainLoanMetadata,
    nonce: u64,
    timestamp: u64,
    confirmations: u32,
) -> Bytes {
    let payload = (metadata.clone(), nonce, timestamp, confirmations);
    let encoded = payload.to_xdr(env);
    env.crypto().sha256(&encoded).into()
}

pub fn is_bridge_nonce_used(env: Env, origin_chain: u32, nonce: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::BridgeNonceUsed(origin_chain, nonce))
        .unwrap_or(false)
}

/// Shared checks for both `validate_bridge_attestation` and `verify_bridge_message`:
/// bridge active, key configured, nonce unused, confirmations sufficient, timestamp
/// fresh and not from the future, and the signature itself verifies.
fn check_attestation(
    env: &Env,
    metadata: &CrossChainLoanMetadata,
    attestation: &BridgeAttestation,
) -> Result<(), ContractError> {
    vouch::validate_bridge(env, metadata.origin_chain)?;

    let public_key: BytesN<32> = env
        .storage()
        .persistent()
        .get(&DataKey::BridgePublicKey(metadata.origin_chain))
        .ok_or(ContractError::BridgeNotConfigured)?;

    if is_bridge_nonce_used(env.clone(), metadata.origin_chain, attestation.nonce) {
        return Err(ContractError::ReplayAttackDetected);
    }

    if attestation.confirmations < MIN_BRIDGE_CONFIRMATIONS {
        return Err(ContractError::InsufficientBridgeConfirmations);
    }

    let now = env.ledger().timestamp();
    if attestation.timestamp > now {
        if attestation.timestamp - now > BRIDGE_ATTESTATION_MAX_SKEW_SECS {
            return Err(ContractError::AttestationFromFuture);
        }
    } else if now - attestation.timestamp > BRIDGE_ATTESTATION_MAX_AGE_SECS {
        return Err(ContractError::AttestationExpired);
    }

    let message = bridge_attestation_message(
        env,
        metadata,
        attestation.nonce,
        attestation.timestamp,
        attestation.confirmations,
    );
    // Panics (aborting the transaction) if the signature does not verify.
    env.crypto()
        .ed25519_verify(&public_key, &message, &attestation.signature);

    Ok(())
}

/// Verify a bridge attestation and consume its nonce, so it cannot be replayed.
pub fn validate_bridge_attestation(
    env: Env,
    metadata: CrossChainLoanMetadata,
    attestation: BridgeAttestation,
) -> Result<(), ContractError> {
    check_attestation(&env, &metadata, &attestation)?;
    env.storage().persistent().set(
        &DataKey::BridgeNonceUsed(metadata.origin_chain, attestation.nonce),
        &true,
    );
    Ok(())
}

/// Issue #968/#85: Read-only integrity check — verifies signature, freshness,
/// confirmations and nonce without consuming any state. Safe to call multiple times.
pub fn verify_bridge_message(
    env: Env,
    metadata: CrossChainLoanMetadata,
    attestation: BridgeAttestation,
) -> Result<(), ContractError> {
    check_attestation(&env, &metadata, &attestation)
}

/// Accept a bridge-attested loan-completion event from `metadata.origin_chain` and
/// mirror it into local storage so it can inform this contract's own decisions.
pub fn mirror_loan_to_chain(
    env: Env,
    metadata: CrossChainLoanMetadata,
    attestation: BridgeAttestation,
) -> Result<(), ContractError> {
    validate_bridge_attestation(env.clone(), metadata.clone(), attestation.clone())?;

    let loan_key = DataKey::MirroredLoan(metadata.origin_chain, metadata.loan_id);
    if env.storage().persistent().has(&loan_key) {
        return Err(ContractError::ReputationAlreadySpent);
    }

    if let Some(existing) = env
        .storage()
        .persistent()
        .get::<DataKey, CrossChainReputationRecord>(&DataKey::CrossChainReputation(
            metadata.borrower.clone(),
        ))
    {
        if existing.chain == metadata.origin_chain && attestation.timestamp < existing.updated_at
        {
            return Err(ContractError::StaleBridgeAttestation);
        }
    }

    env.storage().persistent().set(&loan_key, &metadata);
    env.storage().persistent().set(
        &DataKey::CrossChainReputation(metadata.borrower.clone()),
        &CrossChainReputationRecord {
            score: metadata.reputation_score,
            chain: metadata.origin_chain,
            updated_at: attestation.timestamp,
        },
    );

    Ok(())
}

pub fn query_mirrored_loan(
    env: Env,
    origin_chain: u32,
    loan_id: u64,
) -> Option<CrossChainLoanMetadata> {
    env.storage()
        .persistent()
        .get(&DataKey::MirroredLoan(origin_chain, loan_id))
}

/// Merge the borrower's native credit score with the latest cross-chain snapshot
/// mirrored in for them, if any.
pub fn query_reputation_cross_chain(env: Env, borrower: Address) -> Option<UnifiedReputation> {
    let native_score = crate::credit_score::get_credit_score(env.clone(), borrower.clone())
        .map(|cs| cs.score);
    let mirrored: Option<CrossChainReputationRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainReputation(borrower));

    if native_score.is_none() && mirrored.is_none() {
        return None;
    }

    Some(UnifiedReputation {
        native_score,
        mirrored_score: mirrored.as_ref().map(|m| m.score),
        mirrored_from_chain: mirrored.as_ref().map(|m| m.chain),
        mirrored_at: mirrored.as_ref().map(|m| m.updated_at),
    })
}
