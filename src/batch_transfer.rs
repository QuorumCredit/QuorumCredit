//! Batch token transfer optimization (Issue #935)
//!
//! This module provides utilities for batching multiple token transfers
//! into single operations to reduce gas costs and storage writes.

use crate::types::{BatchTransfer, DataKey};
use soroban_sdk::{Address, Env, Map, Vec};

/// Queue a token transfer for batching.
pub fn queue_transfer(env: &Env, to: Address, amount: i128, token: Address) {
    let mut batch: Vec<BatchTransfer> = env
        .storage()
        .persistent()
        .get(&DataKey::PendingTransfers)
        .unwrap_or(Vec::new(env));

    batch.push_back(BatchTransfer { to, amount, token });
    env.storage()
        .persistent()
        .set(&DataKey::PendingTransfers, &batch);
}

/// Process all queued transfers, grouping by token for efficiency.
pub fn flush_transfers(env: &Env) -> Result<(), crate::errors::ContractError> {
    let batch: Vec<BatchTransfer> = env
        .storage()
        .persistent()
        .get(&DataKey::PendingTransfers)
        .unwrap_or(Vec::new(env));

    if batch.is_empty() {
        return Ok(());
    }

    // Group transfers by token for batch processing
    let mut by_token: Map<Address, Vec<(Address, i128)>> = Map::new(env);

    for transfer in batch.iter() {
        let key = transfer.token.clone();
        let mut entries = by_token
            .get(key.clone())
            .unwrap_or(Vec::new(env));
        entries.push_back((transfer.to.clone(), transfer.amount));
        by_token.set(key, entries);
    }

    // Execute transfers per token
    for (token_addr, transfers) in by_token.iter() {
        let token = crate::helpers::require_allowed_token(env, &token_addr)?;
        let contract_addr = env.current_contract_address();

        for (recipient, amount) in transfers.iter() {
            token.transfer(&contract_addr, &recipient, &amount);
        }
    }

    // Clear the batch
    env.storage()
        .persistent()
        .remove(&DataKey::PendingTransfers);

    Ok(())
}

/// Get pending transfers count (for monitoring).
pub fn pending_transfer_count(env: &Env) -> u32 {
    let batch: Vec<BatchTransfer> = env
        .storage()
        .persistent()
        .get(&DataKey::PendingTransfers)
        .unwrap_or(Vec::new(env));
    batch.len()
}
