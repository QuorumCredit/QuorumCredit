//! Oracle credit score integration.
//!
//! Allows a trusted oracle contract (e.g. a Stellar-compatible price/data feed)
//! to push an external credit score for a borrower. The score is stored on-chain
//! and used at loan-request time to adjust the effective yield rate:
//!
//! - Score 0–299  → yield += 100 bps (higher risk premium)
//! - Score 300–699 → no adjustment (neutral)
//! - Score 700–1000 → yield -= 50 bps (lower risk premium, capped at 0)
//!
//! The oracle address is set once by an admin via `set_oracle()`.
//! Only the registered oracle may call `update_credit_score_from_oracle()`.

use crate::errors::ContractError;
use crate::helpers::{config, require_not_paused};
use crate::types::{DataKey, ExternalCreditScore};
use soroban_sdk::{Address, Env};

/// Maximum valid credit score value.
pub const MAX_CREDIT_SCORE: u32 = 1000;

/// Yield adjustment (bps) added for low-score borrowers (score < 300).
pub const LOW_SCORE_YIELD_PREMIUM_BPS: i128 = 100;

/// Yield adjustment (bps) subtracted for high-score borrowers (score >= 700).
pub const HIGH_SCORE_YIELD_DISCOUNT_BPS: i128 = 50;

/// Register the oracle contract address. Admin-only; can be updated.
pub fn set_oracle(env: Env, admin_signers: soroban_sdk::Vec<Address>, oracle: Address) -> Result<(), ContractError> {
    require_not_paused(&env)?;
    let cfg = config(&env);

    // Require at least admin_threshold signers
    let mut approved: u32 = 0;
    for signer in admin_signers.iter() {
        if cfg.admins.iter().any(|a| a == signer) {
            signer.require_auth();
            approved += 1;
        }
    }
    if approved < cfg.admin_threshold {
        return Err(ContractError::UnauthorizedCaller);
    }

    env.storage().instance().set(&DataKey::OracleAddress, &oracle);
    Ok(())
}

/// Called by the registered oracle to push a credit score for a borrower.
///
/// `oracle` must match the address registered via `set_oracle()`.
/// `score` must be in range 0–1000.
pub fn update_credit_score_from_oracle(
    env: Env,
    oracle: Address,
    borrower: Address,
    score: u32,
) -> Result<(), ContractError> {
    oracle.require_auth();
    require_not_paused(&env)?;

    // Verify caller is the registered oracle
    let registered: Address = env
        .storage()
        .instance()
        .get(&DataKey::OracleAddress)
        .ok_or(ContractError::OracleUnauthorized)?;

    if oracle != registered {
        return Err(ContractError::OracleUnauthorized);
    }

    if score > MAX_CREDIT_SCORE {
        return Err(ContractError::InvalidCreditScore);
    }

    env.storage().persistent().set(
        &DataKey::ExternalCreditScore(borrower),
        &ExternalCreditScore {
            score,
            updated_at: env.ledger().timestamp(),
            oracle,
        },
    );

    Ok(())
}

/// Returns the stored external credit score for a borrower, or `None` if not set.
pub fn get_external_credit_score(env: Env, borrower: Address) -> Option<ExternalCreditScore> {
    env.storage()
        .persistent()
        .get(&DataKey::ExternalCreditScore(borrower))
}

/// Compute the yield-rate adjustment (in bps) based on the borrower's external credit score.
/// Returns a signed delta to add to the base yield_bps.
/// Positive = higher yield (risk premium), negative = lower yield (discount).
pub fn credit_score_yield_adjustment(env: &Env, borrower: &Address) -> i128 {
    let record: Option<ExternalCreditScore> = env
        .storage()
        .persistent()
        .get(&DataKey::ExternalCreditScore(borrower.clone()));

    match record {
        None => 0, // no oracle data → no adjustment
        Some(cs) => {
            if cs.score < 300 {
                LOW_SCORE_YIELD_PREMIUM_BPS // riskier borrower → higher yield
            } else if cs.score >= 700 {
                -HIGH_SCORE_YIELD_DISCOUNT_BPS // trusted borrower → lower yield
            } else {
                0 // neutral band
            }
        }
    }
}
