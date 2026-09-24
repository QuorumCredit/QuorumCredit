//! Batches token transfers so multiple transfers to the same `(token, recipient)` pair are
//! aggregated into a single on-chain transfer instead of one transfer per `queue_transfer`
//! call.
//!
//! **Contract:** `queue_transfer` only records the amount owed; no token movement happens
//! until `flush_transfers` is called. `flush_transfers` executes exactly one `transfer` per
//! distinct `(token, recipient)` pair that has a nonzero queued amount, then clears the
//! queue.
use crate::errors::ContractError;
use soroban_sdk::{contracttype, Address, Env, Vec};

#[contracttype]
enum BatchTransferKey {
    /// (token, recipient) → i128 amount pending transfer.
    Pending(Address, Address),
    /// Vec<(token, recipient)> of keys with a pending balance, for iteration on flush.
    PendingKeys,
}

/// Queue a token transfer, aggregating it with any other pending transfer to the same
/// `(token, recipient)` pair. No token movement happens until `flush_transfers` is called.
pub fn queue_transfer(env: &Env, to: Address, amount: i128, token: Address) {
    if amount <= 0 {
        return;
    }

    let pending_key = BatchTransferKey::Pending(token.clone(), to.clone());
    let existing: i128 = env.storage().temporary().get(&pending_key).unwrap_or(0);

    if existing == 0 {
        let mut keys: Vec<(Address, Address)> = env
            .storage()
            .temporary()
            .get(&BatchTransferKey::PendingKeys)
            .unwrap_or(Vec::new(env));
        keys.push_back((token.clone(), to.clone()));
        env.storage()
            .temporary()
            .set(&BatchTransferKey::PendingKeys, &keys);
    }

    env.storage()
        .temporary()
        .set(&pending_key, &(existing + amount));
}

/// Execute every queued transfer — one `transfer` per distinct `(token, recipient)` pair —
/// and clear the queue.
pub fn flush_transfers(env: &Env) -> Result<(), ContractError> {
    let keys: Vec<(Address, Address)> = env
        .storage()
        .temporary()
        .get(&BatchTransferKey::PendingKeys)
        .unwrap_or(Vec::new(env));

    for (token, to) in keys.iter() {
        let pending_key = BatchTransferKey::Pending(token.clone(), to.clone());
        let amount: i128 = env.storage().temporary().get(&pending_key).unwrap_or(0);
        if amount > 0 {
            let token_client = soroban_sdk::token::Client::new(env, &token);
            token_client.transfer(&env.current_contract_address(), &to, &amount);
        }
        env.storage().temporary().remove(&pending_key);
    }

    env.storage().temporary().remove(&BatchTransferKey::PendingKeys);
    Ok(())
}
