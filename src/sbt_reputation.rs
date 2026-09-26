//! # SBT Reputation System (Issue #1738)
//!
//! Implements a per-holder reputation score for Soulbound Token (SBT) holders.
//! Scores are updated via signed delta adjustments, tracked over time in a
//! bounded history ring buffer, and decay passively over time so that stale
//! scores never grant perpetual benefits.
//!
//! ## Score semantics
//!
//! - Scores are stored as `i64` internally to allow negative deltas, but the
//!   *effective* score exposed to external callers is clamped to `[0, i64::MAX]`.
//! - The history ring buffer holds the last [`SBT_REPUTATION_HISTORY_LIMIT`]
//!   entries so that off-chain UIs can plot a voucher's trust trajectory.
//! - Decay is applied lazily on every read and write: whole months elapsed
//!   since `last_updated` are multiplied by [`DEFAULT_REPUTATION_SCORE_DECAY_BPS`]
//!   (100 bps = 1 % per month) and subtracted from the current score.
//!
//! ## Access control
//!
//! `update_holder_reputation` requires an admin-threshold approval —
//! reputation writes have protocol-wide impact and must not be callable by
//! arbitrary addresses.

#![allow(unused)]

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::{require_admin_approval, require_not_paused};
use crate::types::{
    DataKey, DEFAULT_REPUTATION_SCORE_DECAY_BPS, PERSISTENT_TTL_TARGET_LEDGERS,
    PERSISTENT_TTL_THRESHOLD_LEDGERS,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Seconds in one calendar month (≈ 30 days).
const SECS_PER_MONTH: u64 = 30 * 24 * 60 * 60;

/// Maximum number of historical reputation entries retained per holder.
pub const SBT_REPUTATION_HISTORY_LIMIT: u32 = 50;

/// Maximum absolute delta that may be applied in a single call (prevents
/// catastrophic one-shot score changes).
pub const MAX_SCORE_DELTA_ABS: i32 = 1_000;

// ── Data Structures ───────────────────────────────────────────────────────────

/// A single snapshot in a holder's reputation history.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReputationHistoryEntry {
    /// Ledger timestamp when this snapshot was recorded.
    pub timestamp: u64,
    /// Score value at the time of recording (after applying delta).
    pub score: i64,
    /// The delta that caused this entry (positive = reward, negative = penalty).
    pub delta: i32,
}

/// The live reputation state stored for each SBT holder.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SbtReputationRecord {
    /// The SBT holder address.
    pub holder: Address,
    /// Current reputation score (clamped ≥ 0 externally).
    pub score: i64,
    /// Ledger timestamp of the last score update (used for decay).
    pub last_updated: u64,
    /// Bounded ring buffer of historical snapshots.
    pub history: Vec<ReputationHistoryEntry>,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn load_record(env: &Env, holder: &Address) -> SbtReputationRecord {
    env.storage()
        .persistent()
        .get(&DataKey::SbtReputation(holder.clone()))
        .unwrap_or(SbtReputationRecord {
            holder: holder.clone(),
            score: 0,
            last_updated: env.ledger().timestamp(),
            history: Vec::new(env),
        })
}

fn save_record(env: &Env, record: &SbtReputationRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::SbtReputation(record.holder.clone()), record);
    env.storage().persistent().extend_ttl(
        &DataKey::SbtReputation(record.holder.clone()),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

// ── Decay logic ───────────────────────────────────────────────────────────────

/// Apply passive score decay for whole months elapsed since `last_updated`.
/// Returns the decayed score (floor 0).
fn apply_decay(score: i64, last_updated: u64, now: u64) -> i64 {
    if now <= last_updated {
        return score;
    }
    let months_elapsed = (now - last_updated) / SECS_PER_MONTH;
    if months_elapsed == 0 {
        return score;
    }
    // Decay = score * decay_bps_per_month * months_elapsed / 10_000
    let decay_bps = DEFAULT_REPUTATION_SCORE_DECAY_BPS as i64;
    let total_decay_bps = decay_bps.saturating_mul(months_elapsed as i64);
    let decay_amount = score.saturating_mul(total_decay_bps) / 10_000;
    score.saturating_sub(decay_amount).max(0)
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Issue #1738 — Update the reputation score for an SBT holder.
///
/// `score_delta` is an additive increment: positive values reward the holder,
/// negative values penalise them.  The resulting score is clamped to ≥ 0.
///
/// Requires admin-threshold approval.  Emits a `sbt_rep/update` event.
///
/// # Errors
/// - [`ContractError::ContractPaused`] — contract is paused.
/// - [`ContractError::InvalidAmount`] — `|score_delta| > MAX_SCORE_DELTA_ABS`.
pub fn update_holder_reputation(
    env: &Env,
    admin_signers: Vec<Address>,
    holder: Address,
    score_delta: i32,
) -> Result<(), ContractError> {
    require_not_paused(env)?;
    require_admin_approval(env, &admin_signers);

    if score_delta.abs() > MAX_SCORE_DELTA_ABS {
        return Err(ContractError::InvalidAmount);
    }

    let now = env.ledger().timestamp();
    let mut record = load_record(env, &holder);

    // Apply passive decay first
    record.score = apply_decay(record.score, record.last_updated, now);

    // Apply delta
    let new_score = record.score.saturating_add(score_delta as i64).max(0);
    record.score = new_score;
    record.last_updated = now;

    // Append history entry (trim to SBT_REPUTATION_HISTORY_LIMIT)
    let entry = ReputationHistoryEntry {
        timestamp: now,
        score: new_score,
        delta: score_delta,
    };
    record.history.push_back(entry);
    while record.history.len() > SBT_REPUTATION_HISTORY_LIMIT {
        record.history.pop_front();
    }

    save_record(env, &record);

    env.events().publish(
        (symbol_short!("sbt_rep"), symbol_short!("update")),
        (holder, new_score, score_delta),
    );

    Ok(())
}

/// Issue #1738 — Return the current (decay-adjusted) reputation score for a
/// holder.  Applies passive decay at read time without mutating storage.
pub fn get_holder_reputation(env: &Env, holder: Address) -> i64 {
    let record = load_record(env, &holder);
    let now = env.ledger().timestamp();
    apply_decay(record.score, record.last_updated, now).max(0)
}

/// Issue #1738 — Return the bounded reputation history for a holder.
/// Each entry records the timestamp, post-delta score, and the delta that
/// caused it.  The list is ordered oldest-first.
pub fn get_reputation_history(env: &Env, holder: Address) -> Vec<ReputationHistoryEntry> {
    load_record(env, &holder).history
}
