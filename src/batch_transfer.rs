/// Stub batch_transfer module — performs transfers immediately.
/// A future implementation may batch these for gas efficiency.
use crate::errors::ContractError;
use soroban_sdk::{Address, Env};

/// Queue (and immediately execute) a token transfer.
pub fn queue_transfer(env: &Env, to: Address, amount: i128, token: Address) {
    if amount > 0 {
        let token_client = soroban_sdk::token::Client::new(env, &token);
        token_client.transfer(&env.current_contract_address(), &to, &amount);
    }
}

/// Flush all queued transfers — no-op since transfers are executed immediately.
pub fn flush_transfers(_env: &Env) -> Result<(), ContractError> {
    Ok(())
}
