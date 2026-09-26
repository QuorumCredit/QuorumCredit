//! # SBT Staking and Rewards (Issue #1740)
//!
//! SBT holders can lock ("stake") their credential for a fixed duration to
//! earn XLM rewards.  Longer lock periods earn proportionally higher rewards
//! via a linear bonus multiplier.
//!
//! ## Reward formula
//!
//! ```text
//! base_reward  = amount * SBT_BASE_REWARD_BPS / 10_000
//! lock_bonus   = amount * lock_duration_secs * SBT_LOCK_BONUS_BPS_PER_SEC / 10_000
//! total_reward = base_reward + lock_bonus
//! ```
//!
//! `SBT_LOCK_BONUS_BPS_PER_SEC` is intentionally tiny (≈ 1 bps per day) so
//! that the bonus stays in a sensible range even for multi-year locks.
//!
//! ## Lifecycle
//!
//! ```text
//! stake_sbt(sbt_id, amount, duration)  ─►  SbtStakeRecord { unlocks_at, reward, … }
//!         │
//!         └─ (wait for lock to expire)
//!                 │
//!                 ▼
//!       claim_sbt_reward(sbt_id)   ─►  transfers (amount + reward) to holder
//! ```
//!
//! Only one active stake per `(holder, sbt_id)` is allowed at a time.
//! Restaking after claiming is permitted.
//!
//! ## Access control
//!
//! The caller must be the `holder` recorded against the SBT stake.  Rewards
//! are disbursed from the yield reserve — the admin is responsible for
//! keeping the reserve funded.

#![allow(unused)]

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::{require_not_paused, token_client};
use crate::types::{DataKey, PERSISTENT_TTL_TARGET_LEDGERS, PERSISTENT_TTL_THRESHOLD_LEDGERS};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Base reward rate in basis points applied to the staked amount (50 bps = 0.5%).
pub const SBT_BASE_REWARD_BPS: i128 = 50;

/// Per-second lock duration bonus in basis points (scaled × 10^9 for precision).
/// Value: 1_157 ≈ 1 bps per day (1_000_000 / 86_400 ≈ 11.57 per second,
/// further divided by 10^4 denomination → 1_157 × 10^{-9} bps/s).
pub const SBT_LOCK_BONUS_BPS_PER_SEC_SCALED: i128 = 1_157; // × 10^{-9} bps/s

/// Scale factor for the per-second bonus to preserve integer precision.
pub const SBT_LOCK_BONUS_SCALE: i128 = 1_000_000_000;

/// Minimum lock duration: 1 day.
pub const SBT_MIN_LOCK_DURATION_SECS: u64 = 24 * 60 * 60;

/// Maximum lock duration: 2 years.
pub const SBT_MAX_LOCK_DURATION_SECS: u64 = 2 * 365 * 24 * 60 * 60;

// ── Data Structures ───────────────────────────────────────────────────────────

/// A single active SBT stake record.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SbtStakeRecord {
    /// Unique identifier for this stake (same as sbt_id for simplicity).
    pub sbt_id: u64,
    /// The staking holder.
    pub holder: Address,
    /// Amount staked in stroops.
    pub amount: i128,
    /// Lock duration requested in seconds.
    pub lock_duration_secs: u64,
    /// Ledger timestamp when the stake was created.
    pub staked_at: u64,
    /// Ledger timestamp when the lock expires and rewards become claimable.
    pub unlocks_at: u64,
    /// Pre-computed reward in stroops (locked in at stake time).
    pub reward: i128,
    /// Whether the stake has been claimed.
    pub claimed: bool,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn load_stake(env: &Env, sbt_id: u64, holder: &Address) -> Option<SbtStakeRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::SbtStake(sbt_id, holder.clone()))
}

fn save_stake(env: &Env, record: &SbtStakeRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::SbtStake(record.sbt_id, record.holder.clone()), record);
    env.storage().persistent().extend_ttl(
        &DataKey::SbtStake(record.sbt_id, record.holder.clone()),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

// ── Reward calculation ────────────────────────────────────────────────────────

/// Calculate the total reward for staking `amount` for `duration_secs`.
///
/// ```text
/// base   = amount × SBT_BASE_REWARD_BPS / 10_000
/// bonus  = amount × duration_secs × SBT_LOCK_BONUS_BPS_PER_SEC_SCALED
///          / (10_000 × SBT_LOCK_BONUS_SCALE)
/// total  = base + bonus
/// ```
pub fn calculate_sbt_reward(amount: i128, duration_secs: u64) -> i128 {
    let base = amount * SBT_BASE_REWARD_BPS / 10_000;
    let bonus = amount
        .saturating_mul(duration_secs as i128)
        .saturating_mul(SBT_LOCK_BONUS_BPS_PER_SEC_SCALED)
        / (10_000 * SBT_LOCK_BONUS_SCALE);
    base.saturating_add(bonus)
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Issue #1740 — Stake an SBT for `duration` seconds to earn rewards.
///
/// Transfers `amount` tokens from `holder` to the contract and locks them
/// until `staked_at + duration`.  The reward is computed and locked in at
/// stake time.
///
/// # Errors
/// - [`ContractError::ContractPaused`]
/// - [`ContractError::InvalidAmount`] — amount ≤ 0, duration out of
///   `[SBT_MIN_LOCK_DURATION_SECS, SBT_MAX_LOCK_DURATION_SECS]`.
/// - [`ContractError::ActiveLoanExists`] — reused here to signal an existing
///   active stake for this `(sbt_id, holder)`.
pub fn stake_sbt(
    env: &Env,
    holder: Address,
    sbt_id: u64,
    amount: i128,
    duration: u64,
) -> Result<(), ContractError> {
    require_not_paused(env)?;
    holder.require_auth();

    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    if duration < SBT_MIN_LOCK_DURATION_SECS || duration > SBT_MAX_LOCK_DURATION_SECS {
        return Err(ContractError::InvalidAmount);
    }

    // Only one active stake per (sbt_id, holder)
    if let Some(existing) = load_stake(env, sbt_id, &holder) {
        if !existing.claimed {
            return Err(ContractError::ActiveLoanExists);
        }
    }

    // Transfer tokens from holder to contract
    let tok = token_client(env);
    tok.transfer(&holder, &env.current_contract_address(), &amount);

    let now = env.ledger().timestamp();
    let reward = calculate_sbt_reward(amount, duration);

    let record = SbtStakeRecord {
        sbt_id,
        holder: holder.clone(),
        amount,
        lock_duration_secs: duration,
        staked_at: now,
        unlocks_at: now + duration,
        reward,
        claimed: false,
    };

    save_stake(env, &record);

    env.events().publish(
        (symbol_short!("sbt_stk"), symbol_short!("stake")),
        (sbt_id, holder, amount, duration, reward),
    );

    Ok(())
}

/// Issue #1740 — Claim rewards for a matured SBT stake.
///
/// Returns `amount + reward` to `holder` once `unlocks_at` has passed.
///
/// # Errors
/// - [`ContractError::ContractPaused`]
/// - [`ContractError::NoActiveLoan`] — no stake found for this `(sbt_id, holder)`.
/// - [`ContractError::AlreadyRepaid`] — stake already claimed.
/// - [`ContractError::LoanPastDeadline`] — lock period has not elapsed yet.
pub fn claim_sbt_reward(env: &Env, holder: Address, sbt_id: u64) -> Result<(), ContractError> {
    require_not_paused(env)?;
    holder.require_auth();

    let mut record = load_stake(env, sbt_id, &holder).ok_or(ContractError::NoActiveLoan)?;

    if record.claimed {
        return Err(ContractError::AlreadyRepaid);
    }

    let now = env.ledger().timestamp();
    if now < record.unlocks_at {
        // Re-using LoanPastDeadline semantically inverted: lock not yet expired
        return Err(ContractError::LoanPastDeadline);
    }

    record.claimed = true;
    save_stake(env, &record);

    // Return principal + reward from yield reserve / contract balance
    let total_payout = record.amount.saturating_add(record.reward);
    let tok = token_client(env);
    tok.transfer(&env.current_contract_address(), &holder, &total_payout);

    env.events().publish(
        (symbol_short!("sbt_stk"), symbol_short!("claim")),
        (sbt_id, holder, total_payout),
    );

    Ok(())
}

/// Issue #1740 — Read the stake record for `(sbt_id, holder)`.
pub fn get_sbt_stake(env: &Env, holder: Address, sbt_id: u64) -> Option<SbtStakeRecord> {
    load_stake(env, sbt_id, &holder)
}
