/// Stub cooldown_bypass module - implementation pending.
/// Provides cooldown bypass check for vouching.
use soroban_sdk::{Address, Env};

/// Check if a voucher has a cooldown bypass for a given borrower.
pub fn has_cooldown_bypass(_env: &Env, _voucher: &Address, _borrower: &Address) -> bool {
    // Stub: no bypass
    false
}
