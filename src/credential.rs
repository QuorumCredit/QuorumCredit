/// Credential management module with multi-signature support
use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec, Symbol, symbol_short};
use crate::errors::ContractError;

/// Represents a single credential with multi-signature endorsement support
#[contracttype]
pub struct Credential {
    pub id: BytesN<32>,
    pub holder: Address,
    pub issuer: Address,
    pub created_at: u64,
    pub expires_at: u64,
    pub metadata: String,
    pub metadata_hash: BytesN<32>,
    pub signers: Vec<Address>,
    pub threshold: u32,
    pub signatures: Vec<Signature>,
}

/// Represents a single signature endorsement
#[contracttype]
pub struct Signature {
    pub signer: Address,
    pub signed_at: u64,
    pub signature_data: BytesN<64>,
}

/// Represents endorser tracking information
#[contracttype]
pub struct EndorserRecord {
    pub endorser: Address,
    pub credentials_endorsed: u32,
    pub first_endorsed_at: u64,
    pub last_endorsed_at: u64,
    pub total_value_endorsed: i128,
}

/// Creates a multi-signed credential
pub fn create_multi_signed_credential(
    env: &Env,
    holder: Address,
    issuer: Address,
    signers: Vec<Address>,
    threshold: u32,
    metadata: String,
    expires_at: u64,
) -> Result<Credential, ContractError> {
    holder.require_auth();

    if signers.len() < threshold as usize {
        return Err(ContractError::InvalidThreshold);
    }

    if threshold == 0 {
        return Err(ContractError::InvalidThreshold);
    }

    let now = env.ledger().timestamp();
    let credential_id = generate_credential_id(env, &holder, &issuer, now);

    let metadata_hash = compute_metadata_hash(env, &metadata);

    let credential = Credential {
        id: credential_id,
        holder,
        issuer,
        created_at: now,
        expires_at,
        metadata,
        metadata_hash,
        signers,
        threshold,
        signatures: Vec::new(env),
    };

    Ok(credential)
}

/// Verify threshold signatures for a credential
pub fn verify_threshold_signatures(
    env: &Env,
    credential: &Credential,
    signatures: Vec<Signature>,
) -> Result<bool, ContractError> {
    if signatures.len() < credential.threshold as usize {
        return Err(ContractError::InsufficientSignatures);
    }

    let mut valid_count: u32 = 0;

    for i in 0..signatures.len() {
        let sig = &signatures.get(i).ok_or(ContractError::SignatureError)?;

        // Verify signer is in the authorized signers list
        let is_authorized = credential.signers.iter().any(|signer| signer == &sig.signer);
        if !is_authorized {
            return Err(ContractError::UnauthorizedSigner);
        }

        // In production, would verify the actual signature using the public key
        // For now, we just verify the signer is authorized
        valid_count += 1;
    }

    if valid_count >= credential.threshold {
        Ok(true)
    } else {
        Err(ContractError::InsufficientSignatures)
    }
}

/// Track endorser history
pub fn track_endorser(
    env: &Env,
    endorser: Address,
    value: i128,
) -> EndorserRecord {
    let now = env.ledger().timestamp();

    EndorserRecord {
        endorser,
        credentials_endorsed: 1,
        first_endorsed_at: now,
        last_endorsed_at: now,
        total_value_endorsed: value,
    }
}

/// Generate a unique credential ID
fn generate_credential_id(
    env: &Env,
    holder: &Address,
    issuer: &Address,
    timestamp: u64,
) -> BytesN<32> {
    let holder_bytes = holder.to_xdr(env);
    let issuer_bytes = issuer.to_xdr(env);
    let ts_bytes = timestamp.to_le_bytes().to_vec(env);

    // Combine and hash to create unique ID
    let mut combined = holder_bytes.clone();
    combined.append(&mut issuer_bytes.clone());
    combined.append(&mut ts_bytes.clone());

    // Use env's host hash for deterministic ID generation
    env.crypto().sha256(&combined)
}

/// Compute metadata hash for tamper detection
fn compute_metadata_hash(env: &Env, metadata: &String) -> BytesN<32> {
    env.crypto().sha256(&metadata.to_bytes(env))
}

/// Increment endorser stats
pub fn increment_endorser_stats(
    env: &Env,
    record: &mut EndorserRecord,
    value: i128,
) {
    record.credentials_endorsed += 1;
    record.last_endorsed_at = env.ledger().timestamp();
    record.total_value_endorsed += value;
}
