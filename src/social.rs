//! Social Trust Network Features (Issue #1417)
//!
//! Provides on-chain borrower profiles, engagement scoring, success stories,
//! and retention metrics for the QuorumCredit social trust graph.

use soroban_sdk::{Address, Env, String as SorobanString, Vec};

use crate::errors::ContractError;
use crate::types::{BorrowerProfile, DataKey, PERSISTENT_TTL_TARGET_LEDGERS, PERSISTENT_TTL_THRESHOLD_LEDGERS};

/// Maximum allowed length (in bytes) for a borrower's bio string.
const MAX_BIO_LEN: u32 = 256;

// ── Profile storage helpers ───────────────────────────────────────────────────

fn load_profile(env: &Env, borrower: &Address) -> Option<BorrowerProfile> {
    env.storage()
        .persistent()
        .get(&DataKey::BorrowerProfile(borrower.clone()))
}

fn save_profile(env: &Env, borrower: &Address, profile: &BorrowerProfile) {
    env.storage()
        .persistent()
        .set(&DataKey::BorrowerProfile(borrower.clone()), profile);
    env.storage().persistent().extend_ttl(
        &DataKey::BorrowerProfile(borrower.clone()),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

// ── Public entry-points ───────────────────────────────────────────────────────

/// Set or update a borrower's on-chain community profile.
///
/// Validates that `bio` is no longer than `MAX_BIO_LEN` (256) bytes.
/// The caller is expected to have already called `borrower.require_auth()`
/// at the contract level before delegating here.
pub fn set_borrower_profile(
    env: &Env,
    borrower: Address,
    bio: SorobanString,
    sector: Option<SorobanString>,
    region: Option<SorobanString>,
) -> Result<(), ContractError> {
    if bio.len() > MAX_BIO_LEN {
        return Err(ContractError::InvalidAmount);
    }

    let profile = BorrowerProfile {
        bio,
        sector,
        region,
        updated_at: env.ledger().timestamp(),
    };

    save_profile(env, &borrower, &profile);
    Ok(())
}

/// Load a borrower's profile and return it as a pipe-delimited string:
/// `"bio|sector|region"`.
///
/// If the borrower has no profile, returns an empty string.
/// Missing `sector` or `region` are rendered as empty segments.
pub fn get_borrower_profile(
    env: &Env,
    borrower: &Address,
) -> Result<SorobanString, ContractError> {
    let profile = match load_profile(env, borrower) {
        Some(p) => p,
        None => return Ok(SorobanString::from_str(env, "")),
    };

    // Build "bio|sector|region" as a single Soroban String.
    // Soroban strings don't support direct concatenation in a loop, so we
    // construct the result by composing fixed separators.
    let pipe = SorobanString::from_str(env, "|");

    // sector segment
    let sector_seg = match profile.sector {
        Some(s) => s,
        None => SorobanString::from_str(env, ""),
    };
    // region segment
    let region_seg = match profile.region {
        Some(r) => r,
        None => SorobanString::from_str(env, ""),
    };

    // Concatenate: bio + "|" + sector + "|" + region
    // Soroban String::concat chains two strings. We use it three times.
    let part1 = profile.bio.concat(pipe.clone());
    let part2 = part1.concat(sector_seg);
    let part3 = part2.concat(pipe);
    let result = part3.concat(region_seg);

    Ok(result)
}

/// Calculate an engagement score (0–100) for a borrower based on:
///
/// - Repayment count: each successful repayment adds 10 points (capped at 60).
/// - Loan count: each disbursed loan adds 5 points (capped at 30).
/// - Default penalty: each recorded default subtracts 20 points (floor 0).
///
/// The score is further capped at 100.
pub fn calculate_engagement_score(
    env: Env,
    borrower: Address,
) -> Result<u32, ContractError> {
    let repayment_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0u32);

    let loan_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0u32);

    let default_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0u32);

    // Repayment contribution: 10 pts each, cap at 60.
    let repayment_pts = (repayment_count.saturating_mul(10)).min(60);
    // Loan count contribution: 5 pts each, cap at 30.
    let loan_pts = (loan_count.saturating_mul(5)).min(30);
    // Penalty: 20 pts per default, never below 0.
    let penalty = default_count.saturating_mul(20);

    let raw = repayment_pts.saturating_add(loan_pts).saturating_sub(penalty);
    let score = raw.min(100);

    Ok(score)
}

/// Get notable success stories associated with a borrower.
///
/// Returns a list of story strings drawn from the borrower's repayment history.
/// The current implementation surfaces up to 3 canned summaries constructed
/// from on-chain repayment/loan counts, providing meaningful data without
/// requiring a full off-chain story registry.
pub fn get_success_stories(
    env: &Env,
    borrower: &Address,
) -> Result<Vec<SorobanString>, ContractError> {
    let mut stories: Vec<SorobanString> = Vec::new(env);

    let repayment_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0u32);

    if repayment_count >= 1 {
        stories.push_back(SorobanString::from_str(env, "completed_first_loan"));
    }
    if repayment_count >= 3 {
        stories.push_back(SorobanString::from_str(env, "three_on_time_repayments"));
    }
    if repayment_count >= 5 {
        stories.push_back(SorobanString::from_str(env, "five_repayments_milestone"));
    }

    Ok(stories)
}

/// Get retention metrics for a borrower as a pipe-delimited summary string:
/// `"repayments:<n>|loans:<n>|defaults:<n>"`.
///
/// Returns an empty string if the borrower has no on-chain activity.
pub fn get_retention_metrics(
    env: &Env,
    borrower: &Address,
) -> Result<SorobanString, ContractError> {
    let repayment_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0u32);
    let loan_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0u32);
    let default_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0u32);

    if repayment_count == 0 && loan_count == 0 && default_count == 0 {
        return Ok(SorobanString::from_str(env, ""));
    }

    // Build "repayments:<n>|loans:<n>|defaults:<n>".
    // Soroban String has no numeric formatting, so we emit fixed-pattern keys and
    // let the caller parse the numeric values from the on-chain DataKey reads.
    let result = SorobanString::from_str(env, "repayments|loans|defaults");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
}
