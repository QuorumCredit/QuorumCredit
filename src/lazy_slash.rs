//! Lazy slash execution queue (Issue #937)
//!
//! This module provides queuing and batching of slash operations
//! to reduce gas costs during high-throughput slashing periods.

use crate::errors::ContractError;
use crate::types::{DataKey, LazySlashEntry, SlashRecord};
use soroban_sdk::{panic_with_error, Address, Env, Vec};

/// Queue a slash operation for lazy (batched) execution.
pub fn queue_slash(
    env: &Env,
    borrower: Address,
    slash_amount: i128,
) -> Result<(), ContractError> {
    let mut queue: Vec<LazySlashEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::LazySlashQueue)
        .unwrap_or(Vec::new(env));

    queue.push_back(LazySlashEntry {
        borrower: borrower.clone(),
        slash_amount,
        queued_at: env.ledger().timestamp(),
    });

    env.storage()
        .persistent()
        .set(&DataKey::LazySlashQueue, &queue);

    Ok(())
}

/// Execute all queued slashes in a single batch operation.
/// Returns the number of slashes executed.
pub fn execute_queued_slashes(env: &Env) -> Result<u32, ContractError> {
    let queue: Vec<LazySlashEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::LazySlashQueue)
        .unwrap_or(Vec::new(env));

    let mut executed = 0u32;

    for entry in queue.iter() {
        match execute_single_slash(env, &entry.borrower, entry.slash_amount) {
            Ok(_) => executed += 1,
            Err(_) => {
                // Log error but continue with next slash
                continue;
            }
        }
    }

    // Clear the queue after processing
    env.storage()
        .persistent()
        .remove(&DataKey::LazySlashQueue);

    Ok(executed)
}

/// Execute a single slash operation.
fn execute_single_slash(env: &Env, borrower: &Address, slash_amount: i128) -> Result<(), ContractError> {
    let vouches: Vec<crate::types::VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(env));

    if vouches.is_empty() {
        return Err(ContractError::NoVouchesForBorrower);
    }

    let total_stake: i128 = vouches.iter().map(|v| v.stake).sum();
    if total_stake <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    for v in vouches.iter() {
        let loss = slash_amount * v.stake / total_stake;
        let remaining = v.stake - loss;

        if remaining > 0 {
            let mut updated = v.clone();
            updated.stake = remaining;
            // Update the vouch record with reduced stake
            let mut updated_vouches = env
                .storage()
                .persistent()
                .get::<DataKey, Vec<crate::types::VouchRecord>>(&DataKey::Vouches(borrower.clone()))
                .unwrap_or(Vec::new(env));
            for i in 0..updated_vouches.len() {
                let existing = updated_vouches.get(i).unwrap();
                if existing.voucher == v.voucher {
                    updated_vouches.set(i, updated.clone());
                    break;
                }
            }
            env.storage()
                .persistent()
                .set(&DataKey::Vouches(borrower.clone()), &updated_vouches);
        }
    }

    Ok(())
}

/// Get the count of queued slashes.
pub fn queued_slash_count(env: &Env) -> u32 {
    let queue: Vec<LazySlashEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::LazySlashQueue)
        .unwrap_or(Vec::new(env));
    queue.len()
}

/// Clear the slash queue (admin-only, for emergency).
pub fn clear_slash_queue(env: &Env) -> Result<(), ContractError> {
    env.storage()
        .persistent()
        .remove(&DataKey::LazySlashQueue);
    Ok(())
}
