use crate::errors::ContractError;
use crate::types::{Config, DataKey, LoanRecord};
use soroban_sdk::{token, Address, Env};

pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        Err(ContractError::ContractPaused)
    } else {
        Ok(())
    }
}

/// Returns `Err(InsufficientFunds)` if `amount` is not strictly positive (≤ 0).
/// Use this for all numeric inputs that must be > 0 (stakes, loan amounts, thresholds).
/// All such amounts are denominated in stroops (1 XLM = 10,000,000 stroops).
pub fn require_positive_amount(_env: &Env, amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InsufficientFunds);
    }
    Ok(())
}

pub fn config(env: &Env) -> Config {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .expect("not initialized")
}

pub fn has_active_loan(env: &Env, borrower: &Address) -> bool {
    matches!(get_active_loan_record(env, borrower), Ok(loan) if loan.status == crate::types::LoanStatus::Active)
}

pub fn get_active_loan_record(env: &Env, borrower: &Address) -> Result<LoanRecord, ContractError> {
    let loan_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveLoan(borrower.clone()))
        .ok_or(ContractError::NoActiveLoan)?;
    env.storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)
}

/// Returns a token client for `addr` after verifying it is an allowed token
/// (either the primary protocol token or in `Config.allowed_tokens`).
pub fn require_allowed_token<'a>(
    env: &'a Env,
    addr: &Address,
) -> Result<token::Client<'a>, ContractError> {
    let cfg = config(env);
    if *addr == cfg.token || cfg.allowed_tokens.iter().any(|t| t == *addr) {
        Ok(token::Client::new(env, addr))
    } else {
        Err(ContractError::InvalidToken)
    }
}
