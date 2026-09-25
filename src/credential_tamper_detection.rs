/// Credential metadata tamper detection module
/// Issue #1605: Add Credential Metadata Tamper Detection

use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};
use crate::errors::ContractError;

/// Represents versioned metadata with hash history
#[contracttype]
pub struct VersionedMetadata {
    pub credential_id: BytesN<32>,
    pub current_version: u32,
    pub metadata: String,
    pub hash: BytesN<32>,
    pub version_history: Vec<MetadataVersion>,
    pub last_modified_at: u64,
    pub last_modified_by: Address,
}

/// Represents a single metadata version in the history
#[contracttype]
pub struct MetadataVersion {
    pub version: u32,
    pub metadata: String,
    pub hash: BytesN<32>,
    pub timestamp: u64,
    pub modified_by: Address,
}

/// Represents a tamper alert event
#[contracttype]
pub struct TamperAlert {
    pub credential_id: BytesN<32>,
    pub detected_at: u64,
    pub expected_hash: BytesN<32>,
    pub actual_hash: BytesN<32>,
    pub detector: Address,
}

/// Represents forensic metadata history
#[contracttype]
pub struct ForensicHistory {
    pub credential_id: BytesN<32>,
    pub versions: Vec<MetadataVersion>,
    pub tampering_events: Vec<TamperAlert>,
    pub created_at: u64,
    pub last_checked_at: u64,
}

/// Create versioned metadata for a credential
pub fn create_versioned_metadata(
    env: &Env,
    credential_id: BytesN<32>,
    metadata: String,
    issuer: Address,
) -> Result<VersionedMetadata, ContractError> {
    let now = env.ledger().timestamp();
    let hash = compute_metadata_hash(env, &metadata);

    let initial_version = MetadataVersion {
        version: 1,
        metadata: metadata.clone(),
        hash: hash.clone(),
        timestamp: now,
        modified_by: issuer.clone(),
    };

    let mut history = Vec::new(env);
    history.push_back(initial_version);

    let versioned = VersionedMetadata {
        credential_id,
        current_version: 1,
        metadata,
        hash,
        version_history: history,
        last_modified_at: now,
        last_modified_by: issuer,
    };

    Ok(versioned)
}

/// Detect metadata tampering by comparing hash
pub fn detect_metadata_tampering(
    env: &Env,
    versioned: &VersionedMetadata,
) -> Result<bool, ContractError> {
    let current_hash = compute_metadata_hash(env, &versioned.metadata);

    if current_hash != versioned.hash {
        return Ok(true);  // Tampering detected
    }

    Ok(false)  // No tampering
}

/// Update metadata and track version
pub fn update_metadata(
    env: &Env,
    versioned: &mut VersionedMetadata,
    new_metadata: String,
    modifier: Address,
) -> Result<(), ContractError> {
    let now = env.ledger().timestamp();
    let new_hash = compute_metadata_hash(env, &new_metadata);

    // Create new version entry
    let new_version = MetadataVersion {
        version: versioned.current_version + 1,
        metadata: new_metadata.clone(),
        hash: new_hash.clone(),
        timestamp: now,
        modified_by: modifier.clone(),
    };

    versioned.version_history.push_back(new_version);
    versioned.current_version += 1;
    versioned.metadata = new_metadata;
    versioned.hash = new_hash;
    versioned.last_modified_at = now;
    versioned.last_modified_by = modifier;

    Ok(())
}

/// Record a tamper alert event
pub fn record_tamper_alert(
    env: &Env,
    credential_id: BytesN<32>,
    expected_hash: BytesN<32>,
    actual_hash: BytesN<32>,
    detector: Address,
) -> TamperAlert {
    let now = env.ledger().timestamp();

    TamperAlert {
        credential_id,
        detected_at: now,
        expected_hash,
        actual_hash,
        detector,
    }
}

/// Get forensic metadata history
pub fn get_forensic_history(
    env: &Env,
    versioned: &VersionedMetadata,
    tampering_events: Vec<TamperAlert>,
) -> ForensicHistory {
    let now = env.ledger().timestamp();

    ForensicHistory {
        credential_id: versioned.credential_id.clone(),
        versions: versioned.version_history.clone(),
        tampering_events,
        created_at: versioned.last_modified_at,
        last_checked_at: now,
    }
}

/// Verify metadata hash integrity
pub fn verify_metadata_integrity(
    env: &Env,
    versioned: &VersionedMetadata,
) -> Result<bool, ContractError> {
    let current_hash = compute_metadata_hash(env, &versioned.metadata);

    if current_hash == versioned.hash {
        Ok(true)
    } else {
        Err(ContractError::MetadataHashMismatch)
    }
}

/// Compute metadata hash for integrity checking
fn compute_metadata_hash(env: &Env, metadata: &String) -> BytesN<32> {
    env.crypto().sha256(&metadata.to_bytes(env))
}

/// Get metadata version history page
pub fn get_version_history_page(
    env: &Env,
    versioned: &VersionedMetadata,
    start_version: u32,
    limit: u32,
) -> Result<Vec<MetadataVersion>, ContractError> {
    let mut result = Vec::new(env);

    for i in 0..versioned.version_history.len() {
        let version = versioned.version_history.get(i)
            .ok_or(ContractError::NotFound)?;

        if version.version >= start_version && version.version < start_version + limit {
            result.push_back(version);
        }
    }

    Ok(result)
}
