use crate::errors::ContractError;
use soroban_sdk::Address;

/// Verify that a caller has signed a request with their keypair.
/// This ensures the caller owns the address they claim.
pub fn verify_caller_signature(env: &soroban_sdk::Env, caller: &Address) -> Result<(), ContractError> {
    caller.require_auth();
    Ok(())
}
