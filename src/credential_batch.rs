//! Issue #1796: Credential Transactional Batching.
//!
//! A batch groups related credential operations so they succeed or fail
//! together. Creating a batch records the operations and returns an id.
//! Executing it runs in two phases:
//!
//! 1. **Validate.** Authorize every holder touched by the batch, check every
//!    operation's input, and check that each hot audit trail has room for all
//!    of its pending appends. Nothing is written in this phase.
//! 2. **Apply.** Perform the writes. Phase 1 has already ruled out every
//!    failure this phase could hit, so no partial state is left behind.
//!
//! Soroban also reverts all storage writes made by an invocation that returns
//! an error. The two-phase split means a failed batch never relies on that
//! revert alone: it never starts writing.

use soroban_sdk::{contracttype, Address, Env, Map, String, Vec};

use crate::audit_trail_compression::{
    append_credential_audit_entry, hot_entry_count, validate_audit_event, MAX_HOT_AUDIT_ENTRIES,
};
use crate::credential_rate_limit::{set_verification_rate_limit, validate_verification_limit};
use crate::errors::ContractError;
use crate::types::DataKey;

/// Maximum operations in one batch.
pub const MAX_BATCH_OPERATIONS: u32 = 20;

/// A single credential operation that can be batched.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchOperation {
    /// Append an event to a credential holder's hot audit trail.
    AppendAuditEntry(Address, String),
    /// Set a holder's verification attempts per window (#1797).
    SetVerificationRateLimit(Address, u32),
}

/// Lifecycle state of a batch.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchStatus {
    Pending,
    Executed,
}

/// A recorded batch and its status.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialBatch {
    pub batch_id: u64,
    pub operations: Vec<BatchOperation>,
    pub status: BatchStatus,
    pub created_at: u64,
    /// Ledger timestamp of execution, or 0 while pending.
    pub executed_at: u64,
}

/// Record a batch and return its id. Input is checked here so a bad batch is
/// rejected at creation rather than at execution.
pub fn create_batch_operation(
    env: &Env,
    operations: Vec<BatchOperation>,
) -> Result<u64, ContractError> {
    if operations.is_empty() {
        return Err(ContractError::EmptyBatch);
    }
    if operations.len() > MAX_BATCH_OPERATIONS {
        return Err(ContractError::BatchTooLarge);
    }
    for op in operations.iter() {
        match op {
            BatchOperation::AppendAuditEntry(_, event) => validate_audit_event(&event)?,
            BatchOperation::SetVerificationRateLimit(_, limit) => {
                validate_verification_limit(limit)?
            }
        }
    }

    let counter_key = DataKey::CredentialBatchCounter;
    let batch_id: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64) + 1;
    env.storage().instance().set(&counter_key, &batch_id);

    let batch = CredentialBatch {
        batch_id,
        operations,
        status: BatchStatus::Pending,
        created_at: env.ledger().timestamp(),
        executed_at: 0,
    };
    env.storage()
        .persistent()
        .set(&DataKey::CredentialBatch(batch_id), &batch);
    Ok(batch_id)
}

/// Execute a pending batch atomically. See the module docs for the two phases.
pub fn execute_batch_operation(env: &Env, batch_id: u64) -> Result<(), ContractError> {
    let key = DataKey::CredentialBatch(batch_id);
    let mut batch: CredentialBatch = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::BatchNotFound)?;
    if batch.status == BatchStatus::Executed {
        return Err(ContractError::BatchAlreadyExecuted);
    }

    // Phase 1: validate everything without writing.
    check_batch(env, &batch.operations)?;

    // Phase 2: apply. Phase 1 covers every failure path below.
    for op in batch.operations.iter() {
        match op {
            BatchOperation::AppendAuditEntry(holder, event) => {
                append_credential_audit_entry(env, &holder, event)?;
            }
            BatchOperation::SetVerificationRateLimit(holder, limit) => {
                set_verification_rate_limit(env, &holder, limit)?;
            }
        }
    }

    batch.status = BatchStatus::Executed;
    batch.executed_at = env.ledger().timestamp();
    env.storage().persistent().set(&key, &batch);
    Ok(())
}

/// Read a batch record, including its status.
pub fn get_batch_operation(env: &Env, batch_id: u64) -> Result<CredentialBatch, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::CredentialBatch(batch_id))
        .ok_or(ContractError::BatchNotFound)
}

/// Read only a batch's status.
pub fn get_batch_status(env: &Env, batch_id: u64) -> Result<BatchStatus, ContractError> {
    Ok(get_batch_operation(env, batch_id)?.status)
}

/// Phase 1: authorize holders, validate inputs, and check trail capacity,
/// including all appends that share a holder within this batch.
fn check_batch(env: &Env, operations: &Vec<BatchOperation>) -> Result<(), ContractError> {
    let mut pending: Map<Address, u32> = Map::new(env);
    for op in operations.iter() {
        match op {
            BatchOperation::AppendAuditEntry(holder, event) => {
                holder.require_auth();
                validate_audit_event(&event)?;
                let so_far = pending.get(holder.clone()).unwrap_or(0);
                pending.set(holder, so_far + 1);
            }
            BatchOperation::SetVerificationRateLimit(holder, limit) => {
                holder.require_auth();
                validate_verification_limit(limit)?;
            }
        }
    }

    for (holder, added) in pending.iter() {
        if hot_entry_count(env, &holder) + added > MAX_HOT_AUDIT_ENTRIES {
            return Err(ContractError::AuditTrailFull);
        }
    }
    Ok(())
}
