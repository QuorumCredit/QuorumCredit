//! Issues #1603 / #1797: Credential verification rate limiting per holder.
//!
//! Each holder has a token bucket. A verification attempt spends one token.
//! Tokens refill continuously at `limit` per `window_secs`, so a holder whose
//! bucket is empty earns back the full quota one window after it was drained.
//! That caps how quickly anyone can brute-force verification against a holder.
//!
//! Holders who have never configured a limit use the protocol defaults.

use soroban_sdk::{contracttype, Address, Env};

use crate::errors::ContractError;
use crate::types::DataKey;

/// Attempts allowed per window when a holder has not configured a limit.
pub const DEFAULT_VERIFICATION_LIMIT: u32 = 10;
/// Largest configurable per-window limit.
pub const MAX_VERIFICATION_LIMIT: u32 = 1_000;
/// Default refill window: one hour.
pub const DEFAULT_VERIFICATION_WINDOW_SECS: u64 = 60 * 60;
/// Shortest configurable window: one minute.
pub const MIN_VERIFICATION_WINDOW_SECS: u64 = 60;
/// Longest configurable window: seven days.
pub const MAX_VERIFICATION_WINDOW_SECS: u64 = 7 * 24 * 60 * 60;

/// A holder's configured rate limit.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationRateLimit {
    pub holder: Address,
    /// Bucket capacity, and the number of attempts refilled per window.
    pub limit: u32,
    /// Refill window in seconds. A drained bucket is full again after one window.
    pub window_secs: u64,
    pub updated_at: u64,
}

/// A holder's bucket: the token count as of `last_refill_at`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenBucket {
    pub tokens: u32,
    pub last_refill_at: u64,
}

/// Check a per-window limit is within bounds. Used by the setters and by batch
/// validation, so an invalid limit is rejected before anything is written.
pub fn validate_verification_limit(limit: u32) -> Result<(), ContractError> {
    if limit == 0 || limit > MAX_VERIFICATION_LIMIT {
        return Err(ContractError::InvalidRateLimit);
    }
    Ok(())
}

/// Read a holder's limit, or the protocol default if it has none.
pub fn get_verification_rate_limit(env: &Env, holder: &Address) -> VerificationRateLimit {
    env.storage()
        .persistent()
        .get(&DataKey::VerificationRateLimit(holder.clone()))
        .unwrap_or(VerificationRateLimit {
            holder: holder.clone(),
            limit: DEFAULT_VERIFICATION_LIMIT,
            window_secs: DEFAULT_VERIFICATION_WINDOW_SECS,
            updated_at: 0,
        })
}

/// Set a holder's per-window limit. A new limit starts with a full bucket.
/// The caller is responsible for authorizing the holder.
pub fn set_verification_rate_limit(
    env: &Env,
    holder: &Address,
    limit: u32,
) -> Result<VerificationRateLimit, ContractError> {
    validate_verification_limit(limit)?;

    let now = env.ledger().timestamp();
    let mut config = get_verification_rate_limit(env, holder);
    config.limit = limit;
    config.updated_at = now;
    env.storage()
        .persistent()
        .set(&DataKey::VerificationRateLimit(holder.clone()), &config);

    let bucket = TokenBucket {
        tokens: limit,
        last_refill_at: now,
    };
    env.storage()
        .persistent()
        .set(&DataKey::VerificationBucket(holder.clone()), &bucket);
    Ok(config)
}

/// Set a holder's refill window. Lengthening the window slows refill; shortening
/// it speeds refill. The bucket is left as it is.
pub fn set_verification_window(
    env: &Env,
    holder: &Address,
    window_secs: u64,
) -> Result<VerificationRateLimit, ContractError> {
    if !(MIN_VERIFICATION_WINDOW_SECS..=MAX_VERIFICATION_WINDOW_SECS).contains(&window_secs) {
        return Err(ContractError::InvalidRateLimitWindow);
    }

    let mut config = get_verification_rate_limit(env, holder);
    config.window_secs = window_secs;
    config.updated_at = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&DataKey::VerificationRateLimit(holder.clone()), &config);
    Ok(config)
}

/// Spend one verification attempt for `holder`. Returns the tokens left.
///
/// No auth is required here, because verifiers call this on a holder's
/// behalf. The per-holder cap is what limits total attempts.
pub fn consume_verification_attempt(env: &Env, holder: &Address) -> Result<u32, ContractError> {
    let config = get_verification_rate_limit(env, holder);
    let now = env.ledger().timestamp();
    let mut bucket = refill(load_bucket(env, holder, config.limit, now), &config, now);

    if bucket.tokens == 0 {
        return Err(ContractError::CredentialRateLimitExceeded);
    }
    bucket.tokens -= 1;
    env.storage()
        .persistent()
        .set(&DataKey::VerificationBucket(holder.clone()), &bucket);
    Ok(bucket.tokens)
}

/// Tokens a holder can spend right now, without spending any.
pub fn get_available_attempts(env: &Env, holder: &Address) -> u32 {
    let config = get_verification_rate_limit(env, holder);
    let now = env.ledger().timestamp();
    refill(load_bucket(env, holder, config.limit, now), &config, now).tokens
}

/// The stored bucket, or a full one if the holder has never been limited.
fn load_bucket(env: &Env, holder: &Address, limit: u32, now: u64) -> TokenBucket {
    env.storage()
        .persistent()
        .get(&DataKey::VerificationBucket(holder.clone()))
        .unwrap_or(TokenBucket {
            tokens: limit,
            last_refill_at: now,
        })
}

/// Add the tokens earned since the last refill, capped at the bucket capacity.
///
/// The refill clock advances only by the time those whole tokens took. Any
/// fraction of a token keeps accruing and is not lost to rounding. Once the
/// bucket is full, the clock moves to `now` so no backlog builds up past capacity.
fn refill(bucket: TokenBucket, config: &VerificationRateLimit, now: u64) -> TokenBucket {
    let limit = config.limit as u128;
    let window = config.window_secs as u128;
    let elapsed = now.saturating_sub(bucket.last_refill_at) as u128;
    let earned = elapsed * limit / window;

    let total = bucket.tokens as u128 + earned;
    if total >= limit {
        return TokenBucket {
            tokens: config.limit,
            last_refill_at: now,
        };
    }
    let used_secs = earned * window / limit;
    TokenBucket {
        tokens: total as u32,
        last_refill_at: bucket.last_refill_at + used_secs as u64,
    }
}
