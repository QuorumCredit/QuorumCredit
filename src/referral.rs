/// Issue #1247: Referral Rewards Program
///
/// Drives user acquisition by rewarding referrers when a referred borrower
/// completes their first loan repayment.
///
/// ## Referral flow
///
/// 1. Existing user calls `generate_referral_code` — a unique, deterministic
///    code is derived from the referrer's address and stored on-chain.
/// 2. New borrower calls `register_referral(borrower, referrer)` (in loan.rs)
///    before or during their first loan request.
/// 3. When the referred borrower's first loan is fully repaid, the repayment
///    path calls `distribute_referral_reward` which pays the referrer
///    `referral_bonus_bps / 10_000 * interest_earned` in tokens.
/// 4. A leaderboard (`get_referral_leaderboard`) returns the top referrers
///    sorted by conversion count and total rewards earned.
///
/// ## Reward calculation
///
/// Per the issue spec: "10% of referrer's first interest."
/// The reward is expressed in the same token as the loan.  The caller
/// (repayment path) passes in `first_loan_interest` — the yield earned by
/// the vouchers on that repayment — and the referral bonus is:
///
///   reward = first_loan_interest * referral_bonus_bps / 10_000
///
/// The default `referral_bonus_bps` is 1000 (10%).  Admins can update it via
/// `set_referral_bonus_bps` (already wired in lib.rs).
///
/// The reward is sourced from the yield reserve.  If the reserve cannot cover
/// it, the reward is silently skipped (borrower repayment is never blocked).
use soroban_sdk::{symbol_short, token, Address, Bytes, BytesN, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::{require_not_paused};
use crate::types::{DataKey, ReferralStats, DEFAULT_REFERRAL_BONUS_BPS, BPS_DENOMINATOR};

// Issue #1247 specifies "10% of referrer's first interest".
// DEFAULT_REFERRAL_BONUS_BPS is defined in types.rs. The constant there is
// 100 bps (1%) as a conservative default; admins can raise it to 1000 (10%)
// via set_referral_bonus_bps. The distribute_referral_reward function uses
// whatever the current on-chain value is.

// ── helpers ────────────────────────────────────────────────────────────────────

/// Derive a deterministic referral code from the referrer's address.
///
/// The code is the first 8 bytes of SHA-256(address_bytes) encoded as hex.
/// On Soroban we use `env.crypto().sha256` for this.
fn derive_code_hash(env: &Env, referrer: &Address) -> BytesN<32> {
    // Encode the address as bytes then hash.
    let mut addr_bytes = Bytes::new(env);
    referrer.to_xdr(env).iter().for_each(|b| addr_bytes.push_back(b));
    env.crypto().sha256(&addr_bytes).to_bytes()
}

fn load_referral_stats(env: &Env, referrer: &Address) -> ReferralStats {
    env.storage()
        .persistent()
        .get(&DataKey::ReferralRewardsEarned(referrer.clone()))
        .unwrap_or(ReferralStats {
            referrer: referrer.clone(),
            conversion_count: 0,
            total_rewards_earned: 0,
            last_conversion_at: 0,
        })
}

fn save_referral_stats(env: &Env, stats: &ReferralStats) {
    env.storage()
        .persistent()
        .set(&DataKey::ReferralRewardsEarned(stats.referrer.clone()), stats);
}

// ── public entry-points ────────────────────────────────────────────────────────

/// Issue #1247: Generate (or retrieve) a unique referral code for the caller.
///
/// The code is a `BytesN<32>` SHA-256 hash of the referrer's XDR-encoded
/// address. It is stored in two indices:
///   - `DataKey::ReferralCode(referrer)` → code hash (lookup by owner)
///   - `DataKey::ReferralCodeOwner(code)` → referrer address (reverse lookup)
///
/// Returns the code hash. Idempotent — calling again returns the same code.
pub fn generate_referral_code(
    env: Env,
    referrer: Address,
) -> Result<BytesN<32>, ContractError> {
    referrer.require_auth();
    require_not_paused(&env)?;

    // Check if a code already exists for this referrer.
    if let Some(existing) = env
        .storage()
        .persistent()
        .get::<DataKey, BytesN<32>>(&DataKey::ReferralCode(referrer.clone()))
    {
        return Ok(existing);
    }

    let code = derive_code_hash(&env, &referrer);

    env.storage()
        .persistent()
        .set(&DataKey::ReferralCode(referrer.clone()), &code);
    env.storage()
        .persistent()
        .set(&DataKey::ReferralCodeOwner(code.clone()), &referrer);

    env.events().publish(
        (symbol_short!("referral"), symbol_short!("code")),
        (referrer, code.clone()),
    );

    Ok(code)
}

/// Issue #1247: Look up the referrer who owns a given referral code.
///
/// Returns `None` when the code is not registered.
pub fn get_referrer_by_code(env: Env, code: BytesN<32>) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::ReferralCodeOwner(code))
}

/// Issue #1247: Distribute a referral reward to the referrer of `borrower`.
///
/// Called internally from the repayment path on the **first** repayment of a
/// referred borrower.  Looks up the referrer, computes the reward from
/// `first_loan_interest`, deducts from the yield reserve, and pays out.
///
/// This function is fire-and-forget from the caller's perspective: if the
/// reserve is insufficient or the borrower has no referrer, it is a no-op —
/// the caller's repayment logic is never blocked.
pub fn distribute_referral_reward(
    env: &Env,
    borrower: &Address,
    token: &Address,
    first_loan_interest: i128,
) {
    if first_loan_interest <= 0 {
        return;
    }

    // Look up the referrer.
    let referrer: Address = match env
        .storage()
        .persistent()
        .get(&DataKey::ReferredBy(borrower.clone()))
    {
        Some(r) => r,
        None => return,
    };

    // Compute reward.
    let bonus_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ReferralBonusBps)
        .unwrap_or(DEFAULT_REFERRAL_BONUS_BPS);

    let reward = first_loan_interest * bonus_bps as i128 / BPS_DENOMINATOR;
    if reward <= 0 {
        return;
    }

    // Deduct from yield reserve — silently skip if insufficient.
    let reserve: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::YieldReserve)
        .unwrap_or(0);
    if reserve < reward {
        return;
    }
    env.storage()
        .persistent()
        .set(&DataKey::YieldReserve, &(reserve - reward));

    // Pay the referrer.
    let token_client = token::Client::new(env, token);
    token_client.transfer(&env.current_contract_address(), &referrer, &reward);

    // Update referrer's stats.
    let mut stats = load_referral_stats(env, &referrer);
    stats.conversion_count = stats.conversion_count.saturating_add(1);
    stats.total_rewards_earned = stats.total_rewards_earned.saturating_add(reward);
    stats.last_conversion_at = env.ledger().timestamp();
    save_referral_stats(env, &stats);

    // Update global referral count for the referrer.
    let count: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ReferralCount(referrer.clone()))
        .unwrap_or(0u64);
    env.storage()
        .persistent()
        .set(&DataKey::ReferralCount(referrer.clone()), &(count + 1));

    env.events().publish(
        (symbol_short!("referral"), symbol_short!("reward")),
        (referrer, borrower.clone(), reward),
    );
}

/// Issue #1247: Get the referral stats for a single referrer.
pub fn get_referral_stats(env: Env, referrer: Address) -> ReferralStats {
    load_referral_stats(&env, &referrer)
}

/// Issue #1247: Get the referral leaderboard.
///
/// `referrers` is the list of addresses to include in the leaderboard.
/// Returns each referrer's `ReferralStats`, sorted descending by
/// `conversion_count` then `total_rewards_earned`.
///
/// On-chain sorting is O(n²) but the leaderboard is query-only (read path)
/// and is expected to be called with a small curated list rather than all
/// possible referrers.
pub fn get_referral_leaderboard(env: Env, referrers: Vec<Address>) -> Vec<ReferralStats> {
    let mut entries: soroban_sdk::Vec<ReferralStats> = soroban_sdk::Vec::new(&env);

    for referrer in referrers.iter() {
        entries.push_back(load_referral_stats(&env, &referrer));
    }

    // Insertion sort (descending conversion_count, then descending rewards).
    let len = entries.len();
    for i in 1..len {
        let key = entries.get(i).unwrap();
        let mut j = i;
        while j > 0 {
            let prev = entries.get(j - 1).unwrap();
            let should_swap = prev.conversion_count < key.conversion_count
                || (prev.conversion_count == key.conversion_count
                    && prev.total_rewards_earned < key.total_rewards_earned);
            if should_swap {
                entries.set(j, prev);
                j -= 1;
            } else {
                break;
            }
        }
        entries.set(j, key);
    }

    entries
}
