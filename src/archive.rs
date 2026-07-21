#![allow(unused_variables)]
use crate::errors::ContractError;
use crate::types::{ArchivedLoanRecord, DataKey, VouchHistoryEntry};
use soroban_sdk::{Address, Env, Vec};

pub fn get_archive_count(env: &Env) -> u64 {
    env.storage().instance().get::<_, u64>(&DataKey::ArchiveCounter).unwrap_or(0)
}

pub fn get_archived_loan(env: &Env, archive_id: u64) -> Option<ArchivedLoanRecord> {
    env.storage().persistent().get(&DataKey::ArchivedLoan(archive_id))
}

pub fn archive_vouch_history(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
    max_active_entries: u32,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_archived_vouch_history(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
    batch_id: u32,
) -> Vec<VouchHistoryEntry> {
    Vec::new(env)
}
