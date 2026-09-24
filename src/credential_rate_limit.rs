/// Credential verification rate limiting module
/// Issue #1603: Add Credential Verification Rate Limiting per Holder

use soroban_sdk::{contracttype, Address, Env, Vec, Map};
use crate::errors::ContractError;

/// Represents rate limit configuration for a holder
#[contracttype]
pub struct VerificationRateLimit {
    pub holder: Address,
    pub limit: u32,
    pub window_seconds: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Represents a verification attempt record
#[contracttype]
pub struct VerificationAttempt {
    pub timestamp: u64,
    pub holder: Address,
    pub credential_id: [u8; 32],
    pub success: bool,
}

/// Represents the token bucket state for rate limiting
#[contracttype]
pub struct TokenBucket {
    pub holder: Address,
    pub tokens: u32,
    pub capacity: u32,
    pub refill_rate: u32,  // tokens per second
    pub last_refill_time: u64,
}

/// Set verification rate limit for a holder
pub fn set_verification_rate_limit(
    env: &Env,
    admin: &Address,
    holder: Address,
    limit: u32,
) -> Result<VerificationRateLimit, ContractError> {
    admin.require_auth();

    if limit == 0 {
        return Err(ContractError::InvalidRateLimit);
    }

    let now = env.ledger().timestamp();
    let window_seconds = 3600; // 1 hour default window

    let rate_limit = VerificationRateLimit {
        holder,
        limit,
        window_seconds,
        created_at: now,
        updated_at: now,
    };

    Ok(rate_limit)
}

/// Check if a holder has exceeded their rate limit
pub fn check_rate_limit(
    env: &Env,
    bucket: &mut TokenBucket,
    holder: &Address,
) -> Result<bool, ContractError> {
    let now = env.ledger().timestamp();

    // Refill tokens based on time elapsed
    let time_elapsed = now.saturating_sub(bucket.last_refill_time);
    let tokens_to_add = (bucket.refill_rate as u64).saturating_mul(time_elapsed) as u32;

    bucket.tokens = std::cmp::min(
        bucket.capacity,
        bucket.tokens.saturating_add(tokens_to_add)
    );

    bucket.last_refill_time = now;

    // Check if we have tokens available
    if bucket.tokens > 0 {
        bucket.tokens = bucket.tokens.saturating_sub(1);
        Ok(true)
    } else {
        Err(ContractError::CredentialRateLimitExceeded)
    }
}

/// Initialize token bucket for a holder
pub fn initialize_token_bucket(
    env: &Env,
    holder: Address,
    capacity: u32,
) -> TokenBucket {
    let now = env.ledger().timestamp();

    TokenBucket {
        holder,
        tokens: capacity,
        capacity,
        refill_rate: capacity / 3600, // refill capacity over 1 hour
        last_refill_time: now,
    }
}

/// Reset rate limit for a holder
pub fn reset_rate_limit(
    env: &Env,
    bucket: &mut TokenBucket,
    capacity: u32,
) {
    let now = env.ledger().timestamp();

    bucket.tokens = capacity;
    bucket.capacity = capacity;
    bucket.refill_rate = capacity / 3600;
    bucket.last_refill_time = now;
}

/// Get current token count for a holder
pub fn get_token_count(
    env: &Env,
    bucket: &TokenBucket,
) -> u32 {
    let now = env.ledger().timestamp();
    let time_elapsed = now.saturating_sub(bucket.last_refill_time);
    let tokens_to_add = (bucket.refill_rate as u64).saturating_mul(time_elapsed) as u32;

    std::cmp::min(
        bucket.capacity,
        bucket.tokens.saturating_add(tokens_to_add)
    )
}

/// Log a verification attempt
pub fn log_verification_attempt(
    env: &Env,
    holder: Address,
    credential_id: [u8; 32],
    success: bool,
) -> VerificationAttempt {
    let now = env.ledger().timestamp();

    VerificationAttempt {
        timestamp: now,
        holder,
        credential_id,
        success,
    }
}
