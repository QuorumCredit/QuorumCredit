/// Stub cache module - implementation pending.
/// Provides yield caching helpers used by loan.rs.
use soroban_sdk::{Address, Env};

/// Get cached yield for a (borrower, voucher) pair. Returns None (cache miss).
pub fn get_cached_yield(
    _env: &Env,
    _borrower: &Address,
    _voucher: &Address,
    _base_yield_bps: i128,
) -> Option<i128> {
    // Stub: always a cache miss
    None
}

/// Cache the yield for a (borrower, voucher) pair.
pub fn set_cached_yield(
    _env: &Env,
    _borrower: &Address,
    _voucher: &Address,
    _yield_bps: i128,
    _base_yield_bps: i128,
) {
    // Stub: no-op
}
