use soroban_sdk::{Address, Env, Vec};
use crate::errors::ContractError;
use crate::types::{ArchivedLoanRecord, VouchHistoryEntry};

pub fn get_archive_count(_env: &Env) -> u64 {
    0
}

pub fn get_archived_loan(_env: &Env, _archive_id: u64) -> Option<ArchivedLoanRecord> {
    None
}

pub fn archive_vouch_history(
    _env: &Env,
    _borrower: &Address,
    _voucher: &Address,
    _token: &Address,
    _max_active_entries: u32,
) -> Result<(), ContractError> {
    Ok(())
}

pub fn get_archived_vouch_history(
    env: &Env,
    _borrower: &Address,
    _voucher: &Address,
    _token: &Address,
    _batch_id: u32,
) -> Vec<VouchHistoryEntry> {
    Vec::new(env)
}
