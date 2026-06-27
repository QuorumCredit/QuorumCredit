use crate::types::{DataKey, IpfsArchiveReference};
use soroban_sdk::{Env, String};

/// Register an IPFS archive for a completed loan.
/// The loan data should be serialized and uploaded to IPFS separately.
/// This function stores the IPFS hash for future reference.
pub fn register_loan_ipfs_archive(
    env: &Env,
    archive_id: u64,
    ipfs_hash: String,
) -> Result<(), crate::ContractError> {
    let reference = IpfsArchiveReference {
        ipfs_hash: ipfs_hash.clone(),
        archived_at: env.ledger().timestamp(),
        archive_type: String::from_str(env, "loan"),
    };

    env.storage().persistent().set(
        &DataKey::IpfsLoanArchive(archive_id),
        &reference,
    );

    // Mark archive as IPFS-backed
    mark_archive_ipfs_backed(env, archive_id)?;

    Ok(())
}

/// Retrieve the IPFS hash for an archived loan.
pub fn get_loan_ipfs_archive(env: &Env, archive_id: u64) -> Option<IpfsArchiveReference> {
    env.storage()
        .persistent()
        .get(&DataKey::IpfsLoanArchive(archive_id))
}

/// Register an IPFS archive for vouch history batch.
/// Stores the IPFS content hash for later retrieval.
pub fn register_vouch_history_ipfs_archive(
    env: &Env,
    archive_id: u64,
    ipfs_hash: String,
) -> Result<(), crate::ContractError> {
    let reference = IpfsArchiveReference {
        ipfs_hash: ipfs_hash.clone(),
        archived_at: env.ledger().timestamp(),
        archive_type: String::from_str(env, "vouch_history"),
    };

    env.storage().persistent().set(
        &DataKey::IpfsVouchHistoryArchive(archive_id),
        &reference,
    );

    // Mark archive as IPFS-backed
    mark_archive_ipfs_backed(env, archive_id)?;

    Ok(())
}

/// Retrieve the IPFS hash for archived vouch history.
pub fn get_vouch_history_ipfs_archive(env: &Env, archive_id: u64) -> Option<IpfsArchiveReference> {
    env.storage()
        .persistent()
        .get(&DataKey::IpfsVouchHistoryArchive(archive_id))
}

/// Get the total count of IPFS archives for loans.
pub fn get_loan_ipfs_archive_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::IpfsArchiveCounter)
        .unwrap_or(0)
}

/// Increment the IPFS archive counter.
pub fn increment_ipfs_archive_counter(env: &Env) -> Result<u64, crate::ContractError> {
    let count: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::IpfsArchiveCounter)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(crate::ContractError::StakeOverflow)?;

    env.storage()
        .persistent()
        .set(&DataKey::IpfsArchiveCounter, &count);

    Ok(count)
}

/// Mark an archive as having been backed up to IPFS.
/// This serves as a progress indicator for multi-step archival processes.
pub fn mark_archive_ipfs_backed(
    env: &Env,
    archive_id: u64,
) -> Result<(), crate::ContractError> {
    env.storage()
        .persistent()
        .set(&DataKey::IpfsBackedArchive(archive_id), &true);

    Ok(())
}

/// Check if an archive has been backed up to IPFS.
pub fn is_archive_ipfs_backed(env: &Env, archive_id: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::IpfsBackedArchive(archive_id))
        .unwrap_or(false)
}

/// Verify the integrity of an archived loan by comparing with IPFS hash.
/// In a real implementation, this would fetch from IPFS and verify.
/// For now, it serves as a placeholder for off-chain verification.
pub fn verify_loan_archive_integrity(
    env: &Env,
    archive_id: u64,
    _expected_ipfs_hash: String,
) -> Result<bool, crate::ContractError> {
    // Check if the archive exists
    let _archived_loan = crate::archive::get_archived_loan(env, archive_id)
        .ok_or(crate::ContractError::InvalidStateTransition)?;

    // Check if IPFS reference exists
    let ipfs_ref = get_loan_ipfs_archive(env, archive_id)
        .ok_or(crate::ContractError::InvalidStateTransition)?;

    // In a production system, this would fetch from IPFS and verify
    // For now, we just verify that the hash is stored
    Ok(!ipfs_ref.ipfs_hash.is_empty())
}
