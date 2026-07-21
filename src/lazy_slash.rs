/// Stub lazy_slash module - implementation pending.
/// Provides lazy/deferred slash execution helpers used by governance.rs.
use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use soroban_sdk::{Address, Env, Vec};

/// Queue a slash operation for deferred batch execution.
pub fn queue_slash(_env: &Env, _borrower: Address, _amount: i128) -> Result<(), ContractError> {
    // Stub: no-op
    Ok(())
}

/// Execute all queued slash operations.
/// Returns the number of slashes executed.
pub fn execute_queued_slashes(_env: &Env) -> Result<u32, ContractError> {
    // Stub: nothing queued
    Ok(0)
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
