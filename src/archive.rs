use crate::types::{ArchivedLoanRecord, DataKey, LoanRecord, LoanStatus, VouchHistoryEntry};
use soroban_sdk::{Address, Env};

/// Archive a completed or slashed loan to reduce persistent storage bloat.
/// The loan is copied to archive storage and removed from active storage.
pub fn archive_loan(env: &Env, loan: &LoanRecord) -> Result<u64, crate::ContractError> {
    // Only archive terminal loans
    if loan.status != LoanStatus::Repaid && loan.status != LoanStatus::Defaulted {
        return Err(crate::ContractError::InvalidStateTransition);
    }

    // Get next archive ID
    let archive_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ArchiveCounter)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(crate::ContractError::StakeOverflow)?;

    // Create archived record
    let archived = ArchivedLoanRecord {
        archive_id,
        original_loan_id: loan.id,
        borrower: loan.borrower.clone(),
        amount: loan.amount,
        amount_repaid: loan.amount_repaid,
        total_yield: loan.total_yield,
        final_status: loan.status.clone(),
        created_at: loan.created_at,
        archived_at: env.ledger().timestamp(),
        loan_purpose: loan.loan_purpose.clone(),
        token_address: loan.token_address.clone(),
    };

    // Store in archive
    env.storage()
        .persistent()
        .set(&DataKey::ArchivedLoan(archive_id), &archived);

    // Update counter
    env.storage()
        .persistent()
        .set(&DataKey::ArchiveCounter, &archive_id);

    // Remove from active storage
    env.storage()
        .persistent()
        .remove(&DataKey::Loan(loan.id));

    Ok(archive_id)
}

/// Retrieve an archived loan by archive ID.
pub fn get_archived_loan(env: &Env, archive_id: u64) -> Option<ArchivedLoanRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::ArchivedLoan(archive_id))
}

/// Archive vouch history entries when history grows beyond threshold (e.g., > 100 entries).
/// Old entries are moved to archived storage, keeping recent ones in active storage.
pub fn archive_vouch_history(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
    max_active_entries: u32,
) -> Result<(), crate::ContractError> {
    let mut history: Vec<VouchHistoryEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::VouchHistory(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
        ))
        .unwrap_or(Vec::new(env));

    if history.len() <= max_active_entries as usize {
        return Ok(()); // No need to archive
    }

    // Calculate how many entries to archive
    let entries_to_archive = history.len() - (max_active_entries as usize / 2);

    // Create archive batch
    let mut archived_entries = Vec::new(env);
    for i in 0..entries_to_archive {
        archived_entries.push_back(history.get(i as u32).unwrap());
    }

    // Get next batch ID for this history
    let batch_id: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::ArchivedVouchHistory(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
            0u32,
        ))
        .map(|_: Vec<VouchHistoryEntry>| 1u32)
        .unwrap_or(0);

    // Store archived batch
    env.storage().persistent().set(
        &DataKey::ArchivedVouchHistory(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
            batch_id,
        ),
        &archived_entries,
    );

    // Keep only recent entries in active storage
    let mut recent_entries = Vec::new(env);
    for i in entries_to_archive..history.len() {
        recent_entries.push_back(history.get(i as u32).unwrap());
    }

    env.storage().persistent().set(
        &DataKey::VouchHistory(borrower.clone(), voucher.clone(), token.clone()),
        &recent_entries,
    );

    Ok(())
}

/// Retrieve archived vouch history for a borrower-voucher-token-batch tuple.
pub fn get_archived_vouch_history(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
    batch_id: u32,
) -> Vec<VouchHistoryEntry> {
    env.storage()
        .persistent()
        .get(&DataKey::ArchivedVouchHistory(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
            batch_id,
        ))
        .unwrap_or(Vec::new(env))
}

/// Get count of archived loans
pub fn get_archive_count(env: &Env) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::ArchiveCounter)
        .unwrap_or(0)
}
