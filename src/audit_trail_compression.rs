//! Issue #1795: Credential Audit Trail Compression.
//!
//! Audit entries for a credential live in a bounded "hot" trail. Entries older
//! than a cutoff can be moved into numbered archives, which keeps the hot trail
//! small and cheap to read and write.
//!
//! Each archive stores its entries in a compact packed binary encoding (a
//! big-endian timestamp and length prefix per entry). The encoding is not
//! entropy-coded, so the savings come from moving entries out of the hot trail
//! rather than from shrinking the bytes. Every archive carries a SHA-256 digest
//! bound to its id and entry count. Reads recompute that digest, so a tampered
//! or corrupted archive is rejected instead of returned.

use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env, String, Vec};

use crate::errors::ContractError;
use crate::types::DataKey;

/// Default audit trail retention period in seconds (90 days).
pub const DEFAULT_RETENTION_PERIOD_SECS: u64 = 90 * 24 * 60 * 60;

/// Minimum retention period in seconds (7 days).
pub const MIN_RETENTION_PERIOD_SECS: u64 = 7 * 24 * 60 * 60;

/// Maximum retention period in seconds (5 years).
pub const MAX_RETENTION_PERIOD_SECS: u64 = 5 * 365 * 24 * 60 * 60;

/// Maximum size of a single audit event, in bytes.
pub const MAX_AUDIT_EVENT_BYTES: u32 = 256;

/// Maximum number of hot entries before the trail must be compressed. The hot
/// trail is stored as one vector, so every append rewrites it. Keeping this
/// cap small keeps each append inside the per-transaction budget.
pub const MAX_HOT_AUDIT_ENTRIES: u32 = 200;

/// Maximum entries packed into one archive. Bounds the size of each archive
/// entry so it stays well within Soroban's per-entry limits.
pub const MAX_ENTRIES_PER_ARCHIVE: u32 = 50;

/// Compression history entries kept per credential.
const MAX_COMPRESSION_LOG_ENTRIES: u32 = 100;

/// Bytes of header per packed entry: 8-byte timestamp + 4-byte length.
const ENTRY_HEADER_BYTES: u32 = 12;

/// One audit event in the hot trail.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialAuditEntry {
    pub timestamp: u64,
    pub event: String,
}

/// A compressed, hash-protected block of audit entries.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompressedAuditArchive {
    pub credential_id: Address,
    pub archive_id: u32,
    pub created_at: u64,
    pub entry_count: u32,
    pub oldest_event_timestamp: u64,
    pub newest_event_timestamp: u64,
    /// SHA-256 over (archive_id, entry_count, payload).
    pub data_hash: BytesN<32>,
    /// Packed binary encoding of the archived entries.
    pub payload: Bytes,
}

/// Retention policy and archive counter for a credential.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionConfig {
    pub retention_period_secs: u64,
    pub last_compression_at: u64,
    pub total_archives: u32,
}

/// Append one event to the hot audit trail. Returns the hot trail length.
pub fn append_credential_audit_entry(
    env: &Env,
    credential_id: &Address,
    event: String,
) -> Result<u32, ContractError> {
    validate_audit_event(&event)?;

    let trail_key = DataKey::CredentialAuditTrail(credential_id.clone());
    let mut trail: Vec<CredentialAuditEntry> = env
        .storage()
        .persistent()
        .get(&trail_key)
        .unwrap_or(Vec::new(env));

    if trail.len() >= MAX_HOT_AUDIT_ENTRIES {
        return Err(ContractError::AuditTrailFull);
    }

    trail.push_back(CredentialAuditEntry {
        timestamp: env.ledger().timestamp(),
        event,
    });
    let len = trail.len();
    env.storage().persistent().set(&trail_key, &trail);
    Ok(len)
}

/// Reject events that are empty or larger than `MAX_AUDIT_EVENT_BYTES`.
pub fn validate_audit_event(event: &String) -> Result<(), ContractError> {
    if event.is_empty() || event.len() > MAX_AUDIT_EVENT_BYTES {
        return Err(ContractError::InvalidAuditEvent);
    }
    Ok(())
}

/// Number of entries currently in a credential's hot trail.
pub fn hot_entry_count(env: &Env, credential_id: &Address) -> u32 {
    env.storage()
        .persistent()
        .get::<_, Vec<CredentialAuditEntry>>(&DataKey::CredentialAuditTrail(credential_id.clone()))
        .map(|trail| trail.len())
        .unwrap_or(0)
}

/// Move hot entries with `timestamp < older_than` into a new archive.
///
/// At most `MAX_ENTRIES_PER_ARCHIVE` of the oldest qualifying entries move per
/// call. Any remainder stays hot until a later call. Returns the new archive id.
pub fn compress_audit_trail(
    env: &Env,
    credential_id: &Address,
    older_than: u64,
) -> Result<u32, ContractError> {
    let trail_key = DataKey::CredentialAuditTrail(credential_id.clone());
    let trail: Vec<CredentialAuditEntry> = env
        .storage()
        .persistent()
        .get(&trail_key)
        .unwrap_or(Vec::new(env));

    // Entries are appended in ledger order, so the prefix is the oldest data.
    let mut archived: Vec<CredentialAuditEntry> = Vec::new(env);
    let mut remaining: Vec<CredentialAuditEntry> = Vec::new(env);
    for entry in trail.iter() {
        if entry.timestamp < older_than && archived.len() < MAX_ENTRIES_PER_ARCHIVE {
            archived.push_back(entry);
        } else {
            remaining.push_back(entry);
        }
    }

    if archived.is_empty() {
        return Err(ContractError::NoEventsToCompress);
    }

    let config_key = DataKey::AuditTrailCompressionConfig(credential_id.clone());
    let mut config: RetentionConfig = env
        .storage()
        .persistent()
        .get(&config_key)
        .unwrap_or(default_retention_config());

    let archive_id = config.total_archives;
    let entry_count = archived.len();
    let payload = encode_entries(env, &archived)?;
    let now = env.ledger().timestamp();

    let archive = CompressedAuditArchive {
        credential_id: credential_id.clone(),
        archive_id,
        created_at: now,
        entry_count,
        oldest_event_timestamp: archived.get(0).map(|e| e.timestamp).unwrap_or(0),
        newest_event_timestamp: archived
            .get(entry_count - 1)
            .map(|e| e.timestamp)
            .unwrap_or(0),
        data_hash: archive_digest(env, archive_id, entry_count, &payload),
        payload,
    };
    env.storage().persistent().set(
        &DataKey::CompressedAuditArchive(credential_id.clone(), archive_id),
        &archive,
    );

    config.total_archives = archive_id + 1;
    config.last_compression_at = now;
    env.storage().persistent().set(&config_key, &config);

    env.storage().persistent().set(&trail_key, &remaining);

    let log_key = DataKey::AuditTrailCompressionLog(credential_id.clone());
    let mut log: Vec<(u64, u32, u32)> = env
        .storage()
        .persistent()
        .get(&log_key)
        .unwrap_or(Vec::new(env));
    if log.len() >= MAX_COMPRESSION_LOG_ENTRIES {
        log.remove(0);
    }
    log.push_back((now, archive_id, entry_count));
    env.storage().persistent().set(&log_key, &log);

    Ok(archive_id)
}

/// Compress entries older than `now - retention_period_secs` for a credential.
pub fn compress_expired_audit_entries(
    env: &Env,
    credential_id: &Address,
) -> Result<u32, ContractError> {
    let retention = get_retention_config(env, credential_id).retention_period_secs;
    let cutoff = env.ledger().timestamp().saturating_sub(retention);
    compress_audit_trail(env, credential_id, cutoff)
}

/// Read archive metadata by id.
pub fn get_compressed_archive(
    env: &Env,
    credential_id: &Address,
    archive_id: u32,
) -> Result<CompressedAuditArchive, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::CompressedAuditArchive(
            credential_id.clone(),
            archive_id,
        ))
        .ok_or(ContractError::ArchiveNotFound)
}

/// Check that an archive's stored payload still matches its digest.
pub fn verify_archive_integrity(
    env: &Env,
    credential_id: &Address,
    archive_id: u32,
) -> Result<bool, ContractError> {
    let archive = get_compressed_archive(env, credential_id, archive_id)?;
    let expected = archive_digest(
        env,
        archive.archive_id,
        archive.entry_count,
        &archive.payload,
    );
    Ok(expected == archive.data_hash)
}

/// Decode an archive's entries. Fails with `ArchiveIntegrityFailed` if the
/// payload does not match its digest, so tampered data is never returned.
pub fn get_archive_entries(
    env: &Env,
    credential_id: &Address,
    archive_id: u32,
) -> Result<Vec<CredentialAuditEntry>, ContractError> {
    if !verify_archive_integrity(env, credential_id, archive_id)? {
        return Err(ContractError::ArchiveIntegrityFailed);
    }
    let archive = get_compressed_archive(env, credential_id, archive_id)?;
    decode_entries(env, &archive.payload, archive.entry_count)
}

/// Set the retention period used by `compress_expired_audit_entries`.
pub fn set_retention_config(
    env: &Env,
    credential_id: &Address,
    retention_period_secs: u64,
) -> Result<(), ContractError> {
    if !(MIN_RETENTION_PERIOD_SECS..=MAX_RETENTION_PERIOD_SECS).contains(&retention_period_secs) {
        return Err(ContractError::InvalidRetentionPeriod);
    }

    let config_key = DataKey::AuditTrailCompressionConfig(credential_id.clone());
    let mut config: RetentionConfig = env
        .storage()
        .persistent()
        .get(&config_key)
        .unwrap_or(default_retention_config());
    config.retention_period_secs = retention_period_secs;
    env.storage().persistent().set(&config_key, &config);
    Ok(())
}

/// Get the retention policy for a credential, or the default if unset.
pub fn get_retention_config(env: &Env, credential_id: &Address) -> RetentionConfig {
    env.storage()
        .persistent()
        .get(&DataKey::AuditTrailCompressionConfig(credential_id.clone()))
        .unwrap_or(default_retention_config())
}

/// Get the compression history for a credential as (timestamp, archive_id, entry_count).
pub fn get_compression_history(env: &Env, credential_id: &Address) -> Vec<(u64, u32, u32)> {
    env.storage()
        .persistent()
        .get(&DataKey::AuditTrailCompressionLog(credential_id.clone()))
        .unwrap_or(Vec::new(env))
}

fn default_retention_config() -> RetentionConfig {
    RetentionConfig {
        retention_period_secs: DEFAULT_RETENTION_PERIOD_SECS,
        last_compression_at: 0,
        total_archives: 0,
    }
}

/// SHA-256 over (archive_id, entry_count, payload). Binding the id and count
/// stops a valid payload from being replayed under a different archive.
fn archive_digest(env: &Env, archive_id: u32, entry_count: u32, payload: &Bytes) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.extend_from_array(&archive_id.to_be_bytes());
    preimage.extend_from_array(&entry_count.to_be_bytes());
    preimage.append(payload);
    env.crypto().sha256(&preimage).into()
}

/// Pack entries as: [u64 timestamp BE][u32 len BE][event bytes] per entry.
fn encode_entries(env: &Env, entries: &Vec<CredentialAuditEntry>) -> Result<Bytes, ContractError> {
    let mut out = Bytes::new(env);
    for entry in entries.iter() {
        let len = entry.event.len();
        if len == 0 || len > MAX_AUDIT_EVENT_BYTES {
            return Err(ContractError::InvalidAuditEvent);
        }
        out.extend_from_array(&entry.timestamp.to_be_bytes());
        out.extend_from_array(&len.to_be_bytes());

        let mut buf = [0u8; MAX_AUDIT_EVENT_BYTES as usize];
        entry.event.copy_into_slice(&mut buf[..len as usize]);
        out.append(&Bytes::from_slice(env, &buf[..len as usize]));
    }
    Ok(out)
}

/// Inverse of `encode_entries`. Every read is bounds-checked, and the decoded
/// count must match `expected_count`, so a malformed payload cannot panic.
fn decode_entries(
    env: &Env,
    payload: &Bytes,
    expected_count: u32,
) -> Result<Vec<CredentialAuditEntry>, ContractError> {
    let total = payload.len();
    let mut entries: Vec<CredentialAuditEntry> = Vec::new(env);
    let mut pos: u32 = 0;

    while pos < total {
        if total - pos < ENTRY_HEADER_BYTES {
            return Err(ContractError::ArchiveCorrupted);
        }
        let timestamp = u64::from_be_bytes(read_array::<8>(payload, pos));
        let len = u32::from_be_bytes(read_array::<4>(payload, pos + 8));
        pos += ENTRY_HEADER_BYTES;

        if len == 0 || len > MAX_AUDIT_EVENT_BYTES || total - pos < len {
            return Err(ContractError::ArchiveCorrupted);
        }
        let mut buf = [0u8; MAX_AUDIT_EVENT_BYTES as usize];
        payload
            .slice(pos..pos + len)
            .copy_into_slice(&mut buf[..len as usize]);
        entries.push_back(CredentialAuditEntry {
            timestamp,
            event: String::from_bytes(env, &buf[..len as usize]),
        });
        pos += len;
    }

    if entries.len() != expected_count {
        return Err(ContractError::ArchiveCorrupted);
    }
    Ok(entries)
}

/// Copy `N` bytes starting at `pos`. The caller guarantees `pos + N <= len`.
fn read_array<const N: usize>(payload: &Bytes, pos: u32) -> [u8; N] {
    let mut out = [0u8; N];
    payload.slice(pos..pos + N as u32).copy_into_slice(&mut out);
    out
}
