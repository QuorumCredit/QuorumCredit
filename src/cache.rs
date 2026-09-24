//! Caches the yield rate computed by `loan::vouch_yield_bps` for a `(borrower, voucher)`
//! pair, so repeated reads (e.g. multiple loan-yield calculations in the same settlement)
//! don't re-derive the age/reputation/diversification bonuses from scratch every time.
//!
//! **Caching contract — what invalidates an entry:**
//! - **Rate change:** the cache key includes `base_yield_bps`. If the protocol's configured
//!   yield rate changes, the caller passes a new `base_yield_bps` and the old entry simply
//!   misses (it's never explicitly deleted, just superseded).
//! - **Stake change:** `vouch::increase_stake`, `vouch::decrease_stake`, and
//!   `vouch::withdraw_vouch` call `invalidate_yield_cache` for the affected
//!   `(borrower, voucher)` pair, since a stake change is the most common reason the
//!   downstream yield distribution needs to be recomputed.
//! - **TTL:** entries older than `CACHE_TTL_SECS` are treated as a miss regardless of the
//!   above, bounding staleness from inputs this cache does not explicitly track (e.g. the
//!   vouch-age bonus, which changes as time passes).
use soroban_sdk::{contracttype, Address, Env};

/// Cache entries older than this are treated as a miss.
const CACHE_TTL_SECS: u64 = 3600;

#[contracttype]
#[derive(Clone)]
struct CachedYield {
    yield_bps: i128,
    base_yield_bps: i128,
    cached_at: u64,
}

#[contracttype]
enum CacheKey {
    /// (borrower, voucher) → CachedYield
    YieldEntry(Address, Address),
}

/// Get the cached yield (in bps) for a `(borrower, voucher)` pair, or `None` on a cache miss
/// (never cached, stale rate, or expired TTL).
pub fn get_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    base_yield_bps: i128,
) -> Option<i128> {
    let key = CacheKey::YieldEntry(borrower.clone(), voucher.clone());
    let entry: CachedYield = env.storage().temporary().get(&key)?;

    if entry.base_yield_bps != base_yield_bps {
        return None;
    }
    if env.ledger().timestamp().saturating_sub(entry.cached_at) > CACHE_TTL_SECS {
        return None;
    }

    Some(entry.yield_bps)
}

/// Cache the yield (in bps) for a `(borrower, voucher)` pair, keyed also by the
/// `base_yield_bps` it was computed with.
pub fn set_cached_yield(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    yield_bps: i128,
    base_yield_bps: i128,
) {
    let key = CacheKey::YieldEntry(borrower.clone(), voucher.clone());
    let entry = CachedYield {
        yield_bps,
        base_yield_bps,
        cached_at: env.ledger().timestamp(),
    };
    env.storage().temporary().set(&key, &entry);
    let ttl_ledgers = (CACHE_TTL_SECS / 5) as u32; // ~5s/ledger
    env.storage()
        .temporary()
        .extend_ttl(&key, ttl_ledgers, ttl_ledgers);
}

/// Invalidate the cached yield for a `(borrower, voucher)` pair. Called whenever the
/// underlying vouch's stake changes.
pub fn invalidate_yield_cache(env: &Env, borrower: &Address, voucher: &Address) {
    let key = CacheKey::YieldEntry(borrower.clone(), voucher.clone());
    env.storage().temporary().remove(&key);
}
