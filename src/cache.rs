//! Caching layer for read-heavy endpoints (Issue #724, #934)
//!
//! This module provides utilities for caching frequently accessed data
//! to reduce storage reads and improve performance.

use crate::types::{
    CachedConfigRecord, CachedLoanRecord, CachedVouchesRecord, CacheKey, CachedYieldRecord,
    Config, DataKey, LoanRecord, VouchRecord, CACHE_TTL_SECS, YIELD_CACHE_TTL_SECS,
};
use soroban_sdk::{Address, Env, Vec};

/// Check if a cached record is still valid (not expired).
pub fn is_cache_valid(cached_at: u64, current_time: u64) -> bool {
    current_time.saturating_sub(cached_at) < CACHE_TTL_SECS
}

/// Get a cached loan record if it exists and is valid.
pub fn get_cached_loan(env: &Env, loan_id: u64) -> Option<LoanRecord> {
    let cache_key = CacheKey::LoanCache(loan_id);
    if let Some(cached) = env.storage().persistent().get::<CacheKey, CachedLoanRecord>(&cache_key) {
        let current_time = env.ledger().timestamp();
        if is_cache_valid(cached.cached_at, current_time) {
            return Some(cached.data);
        } else {
            // Invalidate expired cache
            env.storage().persistent().remove(&cache_key);
        }
    }
    None
}

/// Set a cached loan record.
pub fn set_cached_loan(env: &Env, loan_id: u64, loan: LoanRecord) {
    let cache_key = CacheKey::LoanCache(loan_id);
    let cached = CachedLoanRecord {
        data: loan,
        cached_at: env.ledger().timestamp(),
    };
    env.storage().persistent().set(&cache_key, &cached);
}

/// Invalidate a cached loan record.
pub fn invalidate_loan_cache(env: &Env, loan_id: u64) {
    let cache_key = CacheKey::LoanCache(loan_id);
    env.storage().persistent().remove(&cache_key);
}

/// Get cached vouches if they exist and are valid.
pub fn get_cached_vouches(env: &Env, borrower: &Address) -> Option<Vec<VouchRecord>> {
    let cache_key = CacheKey::VouchesCache(borrower.clone());
    if let Some(cached) = env
        .storage()
        .persistent()
        .get::<CacheKey, CachedVouchesRecord>(&cache_key)
    {
        let current_time = env.ledger().timestamp();
        if is_cache_valid(cached.cached_at, current_time) {
            return Some(cached.data);
        } else {
            env.storage().persistent().remove(&cache_key);
        }
    }
    None
}

/// Set cached vouches.
pub fn set_cached_vouches(env: &Env, borrower: &Address, vouches: Vec<VouchRecord>) {
    let cache_key = CacheKey::VouchesCache(borrower.clone());
    let cached = CachedVouchesRecord {
        data: vouches,
        cached_at: env.ledger().timestamp(),
    };
    env.storage().persistent().set(&cache_key, &cached);
}

/// Invalidate cached vouches.
pub fn invalidate_vouches_cache(env: &Env, borrower: &Address) {
    let cache_key = CacheKey::VouchesCache(borrower.clone());
    env.storage().persistent().remove(&cache_key);
}

/// Get cached config if it exists and is valid.
pub fn get_cached_config(env: &Env) -> Option<Config> {
    let cache_key = CacheKey::ConfigCache;
    if let Some(cached) = env
        .storage()
        .persistent()
        .get::<CacheKey, CachedConfigRecord>(&cache_key)
    {
        let current_time = env.ledger().timestamp();
        if is_cache_valid(cached.cached_at, current_time) {
            return Some(cached.data);
        } else {
            env.storage().persistent().remove(&cache_key);
        }
    }
    None
}

/// Set cached config.
pub fn set_cached_config(env: &Env, config: Config) {
    let cache_key = CacheKey::ConfigCache;
    let cached = CachedConfigRecord {
        data: config,
        cached_at: env.ledger().timestamp(),
    };
    env.storage().persistent().set(&cache_key, &cached);
}

/// Invalidate cached config.
pub fn invalidate_config_cache(env: &Env) {
    let cache_key = CacheKey::ConfigCache;
    env.storage().persistent().remove(&cache_key);
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}

// ── Issue #934: Yield Calculation Caching ────────────────────────────────────

/// Get a cached yield bps for a (borrower, voucher) pair if valid.
///
/// Returns `None` if the cache is missing, expired, or the base yield_bps from
/// the current config differs from the one recorded at cache time (stale config).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    current_base_yield_bps: i128,
) -> Option<i128> {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    if let Some(cached) = env.storage().persistent().get::<DataKey, CachedYieldRecord>(&key) {
        let current_time = env.ledger().timestamp();
        if current_time.saturating_sub(cached.cached_at) < YIELD_CACHE_TTL_SECS
            && cached.base_yield_bps == current_base_yield_bps
        {
            return Some(cached.yield_bps);
        } else {
            env.storage().persistent().remove(&key);
        }
    }
    None
}

/// Store a computed yield bps for a (borrower, voucher) pair.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = DataKey::YieldCache(borrower.clone(), voucher.clone());
    let record = CachedYieldRecord {
        yield_bps,
        cached_at: env.ledger().timestamp(),
        base_yield_bps,
    };
    env.storage().persistent().set(&key, &record);
}

/// Invalidate the cached yield for a (borrower, voucher) pair.
/// Call this when stake, reputation, or config changes affect the yield rate.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::YieldCache(borrower.clone(), voucher.clone()));
}
