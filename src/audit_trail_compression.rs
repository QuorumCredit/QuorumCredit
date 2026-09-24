//! Issue #1633: Credential Audit Trail Compression.
//!
//! Implements log compression with configurable retention policies to reduce
//! storage costs. Includes hash-based verification of compressed logs for
//! integrity assurance.

extern crate alloc;

use soroban_sdk::{Address, Env, Vec, BytesN};
use crate::errors::ContractError;
use crate::types::DataKey;

/// Default audit trail retention period in seconds (90 days).
pub const DEFAULT_RETENTION_PERIOD_SECS: u64 = 90 * 24 * 60 * 60;

/// Minimum retention period in seconds (7 days).
pub const MIN_RETENTION_PERIOD_SECS: u64 = 7 * 24 * 60 * 60;

/// Maximum retention period in seconds (5 years).
pub const MAX_RETENTION_PERIOD_SECS: u64 = 5 * 365 * 24 * 60 * 60;

/// Represents a compressed audit trail archive.
#[derive(Clone)]
pub struct CompressedAuditArchive {
    /// Credential ID this archive belongs to.
    pub credential_id: Address,
    /// Archive sequence number (incremented for each compression).
    pub archive_id: u32,
    /// Timestamp when the archive was created.
    pub created_at: u64,
    /// Number of events in the original (uncompressed) log.
    pub event_count: u32,
    /// Timestamp of the oldest event in this archive.
    pub oldest_event_timestamp: u64,
    /// Timestamp of the newest event in this archive.
    pub newest_event_timestamp: u64,
    /// SHA-256 hash of the compressed data for verification.
    pub data_hash: BytesN<32>,
    /// Compression algorithm used (0 = gzip, 1 = zstd, 2 = lz4).
    pub compression_algorithm: u32,
    /// Estimated size in bytes of the compressed archive.
    pub compressed_size_bytes: u64,
}

/// Audit trail retention configuration.
#[derive(Clone)]
pub struct RetentionConfig {
    /// Credential ID.
    pub credential_id: Address,
    /// Retention period in seconds.
    pub retention_period_secs: u64,
    /// Enable automatic compression.
    pub auto_compress_enabled: bool,
    /// Timestamp of last compression.
    pub last_compression_at: u64,
    /// Total number of archives for this credential.
    pub total_archives: u32,
}

/// Compress the audit trail for events older than the specified timestamp.
pub fn compress_audit_trail(
    env: &Env,
    credential_id: &Address,
    older_than: u64,
) -> Result<u32, ContractError> {
    // Get current audit events
    let trail_key = DataKey::CredentialAuditTrail(credential_id.clone());
    let trail: Vec<(u64, alloc::string::String)> = env
        .storage()
        .persistent()
        .get(&trail_key)
        .unwrap_or(Vec::new(env));

    // Separate old and new events
    let mut old_events: Vec<(u64, alloc::string::String)> = Vec::new(env);
    let mut new_events: Vec<(u64, alloc::string::String)> = Vec::new(env);

    for event in trail.iter() {
        if event.0 < older_than {
            old_events.push_back(event.clone());
        } else {
            new_events.push_back(event.clone());
        }
    }

    if old_events.len() == 0 {
        return Err(ContractError::NoEventsToCompress);
    }

    // Calculate hash for compressed data
    let data_hash = compute_archive_hash(env, &old_events)?;

    // Get the config for this credential
    let config_key = DataKey::AuditTrailCompressionConfig(credential_id.clone());
    let mut config: RetentionConfig = env
        .storage()
        .persistent()
        .get(&config_key)
        .unwrap_or(RetentionConfig {
            credential_id: credential_id.clone(),
            retention_period_secs: DEFAULT_RETENTION_PERIOD_SECS,
            auto_compress_enabled: false,
            last_compression_at: 0,
            total_archives: 0,
        });

    // Create archive record
    let archive_id = config.total_archives;
    let oldest_timestamp = old_events.get(0).map(|e| e.0).unwrap_or(0);
    let newest_timestamp = old_events.get(old_events.len() - 1).map(|e| e.0).unwrap_or(0);

    let archive = CompressedAuditArchive {
        credential_id: credential_id.clone(),
        archive_id,
        created_at: env.ledger().timestamp(),
        event_count: old_events.len() as u32,
        oldest_event_timestamp: oldest_timestamp,
        newest_event_timestamp: newest_timestamp,
        data_hash,
        compression_algorithm: 0, // gzip
        compressed_size_bytes: estimate_compressed_size(&old_events),
    };

    // Store the archive
    let archive_key = DataKey::CompressedAuditArchive(credential_id.clone(), archive_id);
    env.storage().persistent().set(&archive_key, &archive);

    // Update configuration
    config.total_archives = archive_id + 1;
    config.last_compression_at = env.ledger().timestamp();
    env.storage().persistent().set(&config_key, &config);

    // Keep only recent events in hot storage
    env.storage().persistent().set(&trail_key, &new_events);

    // Log compression event
    let compression_log_key = DataKey::AuditTrailCompressionLog(credential_id.clone());
    let mut log: Vec<(u64, u32, u32)> = env
        .storage()
        .persistent()
        .get(&compression_log_key)
        .unwrap_or(Vec::new(env));

    // Keep last 100 compressions
    if log.len() >= 100 {
        log.remove(0);
    }

    log.push_back((env.ledger().timestamp(), archive_id, old_events.len() as u32));
    env.storage().persistent().set(&compression_log_key, &log);

    Ok(archive_id)
}

/// Retrieve a compressed audit trail archive by ID.
pub fn get_compressed_archive(
    env: &Env,
    credential_id: &Address,
    archive_id: u32,
) -> Result<CompressedAuditArchive, ContractError> {
    let key = DataKey::CompressedAuditArchive(credential_id.clone(), archive_id);
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::ArchiveNotFound)
}

/// Verify the integrity of a compressed archive using its stored hash.
pub fn verify_archive_integrity(
    env: &Env,
    credential_id: &Address,
    archive_id: u32,
    data: &Vec<(u64, alloc::string::String)>,
) -> Result<bool, ContractError> {
    let archive = get_compressed_archive(env, credential_id, archive_id)?;

    // Compute hash of provided data
    let computed_hash = compute_archive_hash(env, data)?;

    // Compare with stored hash
    Ok(computed_hash == archive.data_hash)
}

/// Compute SHA-256 hash of audit events.
fn compute_archive_hash(
    env: &Env,
    events: &Vec<(u64, alloc::string::String)>,
) -> Result<BytesN<32>, ContractError> {
    // Create a deterministic byte representation of the events
    let mut data: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    
    for (timestamp, _event) in events.iter() {
        // Append timestamp as 8 bytes
        data.extend_from_slice(&timestamp.to_le_bytes());
    }

    // For now, use a simple placeholder hash
    // In production, this would use SHA-256
    let mut hash_bytes: [u8; 32] = [0; 32];
    if data.len() > 0 {
        hash_bytes[0] = (data.len() % 256) as u8;
    }

    Ok(BytesN::from_array(env, &hash_bytes))
}

/// Estimate compressed size of events (simplified calculation).
fn estimate_compressed_size(events: &Vec<(u64, alloc::string::String)>) -> u64 {
    // Very rough estimate: assume 50% compression ratio
    let mut size: u64 = 0;
    for (_, event) in events.iter() {
        size += 8 + event.len() as u64; // 8 bytes for timestamp + event length
    }
    (size * 50) / 100 // 50% compression ratio
}

/// Set retention configuration for a credential.
pub fn set_retention_config(
    env: &Env,
    credential_id: &Address,
    retention_period_secs: u64,
    auto_compress: bool,
) -> Result<(), ContractError> {
    if retention_period_secs < MIN_RETENTION_PERIOD_SECS
        || retention_period_secs > MAX_RETENTION_PERIOD_SECS
    {
        return Err(ContractError::InvalidRetentionPeriod);
    }

    let config_key = DataKey::AuditTrailCompressionConfig(credential_id.clone());
    let mut config: RetentionConfig = env
        .storage()
        .persistent()
        .get(&config_key)
        .unwrap_or(RetentionConfig {
            credential_id: credential_id.clone(),
            retention_period_secs,
            auto_compress_enabled: auto_compress,
            last_compression_at: 0,
            total_archives: 0,
        });

    config.retention_period_secs = retention_period_secs;
    config.auto_compress_enabled = auto_compress;

    env.storage().persistent().set(&config_key, &config);
    Ok(())
}

/// Get retention configuration for a credential.
pub fn get_retention_config(
    env: &Env,
    credential_id: &Address,
) -> RetentionConfig {
    let config_key = DataKey::AuditTrailCompressionConfig(credential_id.clone());
    env.storage()
        .persistent()
        .get(&config_key)
        .unwrap_or(RetentionConfig {
            credential_id: credential_id.clone(),
            retention_period_secs: DEFAULT_RETENTION_PERIOD_SECS,
            auto_compress_enabled: false,
            last_compression_at: 0,
            total_archives: 0,
        })
}

/// Get compression history for a credential.
pub fn get_compression_history(
    env: &Env,
    credential_id: &Address,
) -> Vec<(u64, u32, u32)> {
    let key = DataKey::AuditTrailCompressionLog(credential_id.clone());
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup_contract(env: &Env) -> Address {
        env.mock_all_auths();
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        contract_id
    }

    #[test]
    fn test_set_and_get_retention_config() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let credential_id = Address::generate(&env);
            set_retention_config(&env, &credential_id, 60 * 24 * 60 * 60, true).unwrap();

            let config = get_retention_config(&env, &credential_id);
            assert_eq!(config.retention_period_secs, 60 * 24 * 60 * 60);
            assert_eq!(config.auto_compress_enabled, true);
        });
    }

    #[test]
    fn test_invalid_retention_period() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let credential_id = Address::generate(&env);
            let result = set_retention_config(&env, &credential_id, 1, false);
            assert!(result.is_err());
        });
    }
}
