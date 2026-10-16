//! Issue #1795: unit tests for credential audit trail compression.

use crate::audit_trail_compression::*;
use crate::errors::ContractError;
use crate::types::DataKey;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

#[contract]
pub struct Dummy;

#[contractimpl]
impl Dummy {
    pub fn noop() {}
}

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(Dummy, ());
    let credential = Address::generate(&env);
    (env, contract_id, credential)
}

fn at(env: &Env, t: u64) {
    env.ledger().set_timestamp(t);
}

#[test]
fn round_trip_compress_and_read_archive() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        at(&env, 100);
        append_credential_audit_entry(&env, &cred, String::from_str(&env, "issued")).unwrap();
        at(&env, 200);
        append_credential_audit_entry(&env, &cred, String::from_str(&env, "endorsed")).unwrap();
        at(&env, 300);
        let hot =
            append_credential_audit_entry(&env, &cred, String::from_str(&env, "renewed")).unwrap();
        assert_eq!(hot, 3);

        let archive_id = compress_audit_trail(&env, &cred, 250).unwrap();
        assert_eq!(archive_id, 0);

        let archive = get_compressed_archive(&env, &cred, 0).unwrap();
        assert_eq!(archive.entry_count, 2);
        assert_eq!(archive.oldest_event_timestamp, 100);
        assert_eq!(archive.newest_event_timestamp, 200);

        assert!(verify_archive_integrity(&env, &cred, 0).unwrap());
        let entries = get_archive_entries(&env, &cred, 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.get(0).unwrap().timestamp, 100);
        assert_eq!(
            entries.get(0).unwrap().event,
            String::from_str(&env, "issued")
        );
        assert_eq!(
            entries.get(1).unwrap().event,
            String::from_str(&env, "endorsed")
        );

        // Only the newer entry remains hot, so the next append reports length 2.
        let hot = append_credential_audit_entry(&env, &cred, String::from_str(&env, "x")).unwrap();
        assert_eq!(hot, 3 - 2 + 1);

        let history = get_compression_history(&env, &cred);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), (300, 0, 2));
    });
}

#[test]
fn tampered_archive_is_rejected() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        at(&env, 10);
        append_credential_audit_entry(&env, &cred, String::from_str(&env, "original")).unwrap();
        compress_audit_trail(&env, &cred, 100).unwrap();

        let key = DataKey::CompressedAuditArchive(cred.clone(), 0);
        let mut archive: CompressedAuditArchive = env.storage().persistent().get(&key).unwrap();
        let first = archive.payload.get(0).unwrap();
        archive.payload.set(0, first ^ 0xff);
        env.storage().persistent().set(&key, &archive);

        assert!(!verify_archive_integrity(&env, &cred, 0).unwrap());
        assert_eq!(
            get_archive_entries(&env, &cred, 0),
            Err(ContractError::ArchiveIntegrityFailed)
        );
    });
}

#[test]
fn compress_with_nothing_to_move_errors() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        at(&env, 500);
        append_credential_audit_entry(&env, &cred, String::from_str(&env, "recent")).unwrap();
        assert_eq!(
            compress_audit_trail(&env, &cred, 100),
            Err(ContractError::NoEventsToCompress)
        );
    });
}

#[test]
fn rejects_empty_and_oversized_events() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        assert_eq!(
            append_credential_audit_entry(&env, &cred, String::from_str(&env, "")),
            Err(ContractError::InvalidAuditEvent)
        );
        let big = [b'a'; (MAX_AUDIT_EVENT_BYTES + 1) as usize];
        let big = String::from_bytes(&env, &big);
        assert_eq!(
            append_credential_audit_entry(&env, &cred, big),
            Err(ContractError::InvalidAuditEvent)
        );
    });
}

#[test]
fn hot_trail_rejects_appends_when_full() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        at(&env, 1);
        // Each append is its own transaction on-chain, so give every call a
        // fresh budget rather than letting the test accumulate them.
        for _ in 0..MAX_HOT_AUDIT_ENTRIES {
            env.cost_estimate().budget().reset_default();
            append_credential_audit_entry(&env, &cred, String::from_str(&env, "e")).unwrap();
        }
        env.cost_estimate().budget().reset_default();
        assert_eq!(
            append_credential_audit_entry(&env, &cred, String::from_str(&env, "e")),
            Err(ContractError::AuditTrailFull)
        );
    });
}

#[test]
fn archives_are_capped_and_remainder_stays_hot() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        for t in 1..=(MAX_ENTRIES_PER_ARCHIVE as u64 + 10) {
            at(&env, t);
            append_credential_audit_entry(&env, &cred, String::from_str(&env, "e")).unwrap();
        }
        at(&env, 10_000);
        let first = compress_audit_trail(&env, &cred, 10_000).unwrap();
        assert_eq!(first, 0);
        assert_eq!(
            get_compressed_archive(&env, &cred, 0).unwrap().entry_count,
            MAX_ENTRIES_PER_ARCHIVE
        );

        let second = compress_audit_trail(&env, &cred, 10_000).unwrap();
        assert_eq!(second, 1);
        assert_eq!(
            get_compressed_archive(&env, &cred, 1).unwrap().entry_count,
            10
        );
    });
}

#[test]
fn retention_period_is_bounded_and_drives_expired_compression() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        assert_eq!(
            set_retention_config(&env, &cred, MIN_RETENTION_PERIOD_SECS - 1),
            Err(ContractError::InvalidRetentionPeriod)
        );
        assert_eq!(
            set_retention_config(&env, &cred, MAX_RETENTION_PERIOD_SECS + 1),
            Err(ContractError::InvalidRetentionPeriod)
        );
        set_retention_config(&env, &cred, MIN_RETENTION_PERIOD_SECS).unwrap();
        assert_eq!(
            get_retention_config(&env, &cred).retention_period_secs,
            MIN_RETENTION_PERIOD_SECS
        );

        let day = 24 * 60 * 60;
        at(&env, day);
        append_credential_audit_entry(&env, &cred, String::from_str(&env, "old")).unwrap();
        at(&env, 30 * day);
        append_credential_audit_entry(&env, &cred, String::from_str(&env, "new")).unwrap();

        // Cutoff = 30d - 7d = 23d, so only the day-1 entry is archived.
        let archive_id = compress_expired_audit_entries(&env, &cred).unwrap();
        assert_eq!(archive_id, 0);
        assert_eq!(
            get_compressed_archive(&env, &cred, 0).unwrap().entry_count,
            1
        );
    });
}

#[test]
fn missing_archive_reports_not_found() {
    let (env, cid, cred) = setup();
    env.as_contract(&cid, || {
        assert_eq!(
            get_compressed_archive(&env, &cred, 7),
            Err(ContractError::ArchiveNotFound)
        );
        assert_eq!(
            get_archive_entries(&env, &cred, 7),
            Err(ContractError::ArchiveNotFound)
        );
    });
}
