/// Queue-based lazy slash execution module.
/// Provides admin-gated batch slash operations to reduce on-chain execution costs.
use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::{DataKey, LazySlashEntry};
use soroban_sdk::{Address, Env, Vec};

/// Maximum number of queued slashes to prevent unbounded storage growth.
const MAX_SLASH_QUEUE_SIZE: u32 = 1000;

/// Maximum number of slashes to execute in a single batch call.
const MAX_SLASHES_PER_BATCH: u32 = 50;

/// Queue a slash operation for deferred batch execution.
/// The slash is appended to the persistent queue and will be executed
/// when `execute_queued_slashes` is called.
///
/// # Errors
/// - `ContractError::InvalidAmount` if amount <= 0
/// - `ContractError::QueueFull` if the queue has reached MAX_SLASH_QUEUE_SIZE
pub fn queue_slash(env: &Env, borrower: Address, amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut queue: Vec<LazySlashEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::LazySlashQueue)
        .unwrap_or(Vec::new(env));

    if queue.len() >= MAX_SLASH_QUEUE_SIZE {
        return Err(ContractError::QueueFull);
    }

    let now = env.ledger().timestamp();
    let entry = LazySlashEntry {
        borrower,
        amount,
        queued_at: now,
    };

    queue.push_back(entry);

    env.storage()
        .persistent()
        .set(&DataKey::LazySlashQueue, &queue);

    Ok(())
}

/// Execute all queued slash operations (up to MAX_SLASHES_PER_BATCH).
/// Returns the number of slashes successfully executed.
///
/// For each queued entry, this function:
/// 1. Loads the entry from the queue
/// 2. Calls the existing slash execution logic from governance.rs
/// 3. Removes the executed entry from the queue
/// 4. Continues to the next entry
///
/// If a slash execution fails (e.g., NoActiveLoan), that entry is skipped
/// and removed from the queue to prevent blocking subsequent slashes.
///
/// # Returns
/// The number of slashes executed successfully.
pub fn execute_queued_slashes(env: &Env) -> Result<u32, ContractError> {
    let queue: Vec<LazySlashEntry> = env
        .storage()
        .persistent()
        .get(&DataKey::LazySlashQueue)
        .unwrap_or(Vec::new(env));

    if queue.is_empty() {
        return Ok(0);
    }

    let batch_size = queue.len().min(MAX_SLASHES_PER_BATCH);
    let mut executed_count = 0u32;
    let mut remaining_queue: Vec<LazySlashEntry> = Vec::new(env);

    for i in 0..queue.len() {
        let entry = queue.get(i).unwrap();

        if i < batch_size {
            // Attempt to execute this slash
            // Use the existing execute_slash logic from governance.rs
            // Note: execute_slash is not public, so we need to use execute_pending_slash pattern
            // or expose a way to execute a slash directly.
            // For now, we'll call the internal execute_slash via the public execute_pending_slash approach.
            // Since execute_slash is private, we'll need to create a pending slash record and execute it.
            
            // Actually, looking at the code, we should directly call the slash logic.
            // Let's use the approach of calling governance::execute_slash indirectly.
            // The best approach is to use the same mechanism as execute_pending_slash.
            
            match crate::governance::execute_slash_for_lazy_queue(env, &entry.borrower) {
                Ok(()) => {
                    executed_count += 1;
                }
                Err(_) => {
                    // Skip this entry if slash fails (e.g., borrower has no active loan)
                    // This prevents one failed slash from blocking the entire queue
                }
            }
        } else {
            // Keep entries beyond batch size in the queue
            remaining_queue.push_back(entry);
        }
    }

    // Update the queue with remaining entries
    if remaining_queue.is_empty() {
        env.storage().persistent().remove(&DataKey::LazySlashQueue);
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::LazySlashQueue, &remaining_queue);
    }

    Ok(executed_count)
}

/// Admin-gated wrapper: queue a slash via governance call.
pub fn queue_slash_gov(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
    slash_amount: i128,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    queue_slash(&env, borrower, slash_amount)
}

/// Admin-gated wrapper: execute all queued slashes via governance call.
pub fn execute_queued_slashes_gov(
    env: Env,
    admin_signers: Vec<Address>,
) -> Result<u32, ContractError> {
    require_admin_approval(&env, &admin_signers);
    execute_queued_slashes(&env)
}
