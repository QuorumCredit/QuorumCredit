//! Issue #1422: Fraud Score Detection Logic
//!
//! Assigns each voucher a fraud score derived from their on-chain vouch/slash
//! history and flags them once the score crosses an admin-configured threshold.
//! The score is recomputed (not blindly incremented) from `VoucherStats` every
//! time `update_fraud_score` runs, so it stays consistent with the authoritative
//! slash accounting even if an event is missed.
//!
//! Scoring model (all values in "points", capped at 10 000):
//!   * `SCORE_PER_SLASH` points for every slash recorded against the voucher —
//!     captures repeated defaults on backed borrowers.
//!   * A ratio component: `slashed / (slashed + successful)` scaled to
//!     `RATIO_SCALE` points — captures a voucher whose vouches disproportionately
//!     end in a slash (circular / collusive vouching, rapid vouch cycling).

use soroban_sdk::{Address, Env, Vec};
use crate::errors::ContractError;
use crate::types::{DataKey, FraudScoreConfig, VoucherFraudScore, VoucherStats};

/// Points added per slash event recorded against the voucher.
const SCORE_PER_SLASH: u32 = 250;
/// Maximum points contributed by the slashed-to-total ratio component.
const RATIO_SCALE: u32 = 5_000;
/// Hard cap on any fraud score.
const SCORE_CAP: u32 = 10_000;

fn voucher_stats(env: &Env, voucher: &Address) -> VoucherStats {
    env.storage()
        .persistent()
        .get(&DataKey::VoucherStats(voucher.clone()))
        .unwrap_or(VoucherStats {
            successful_vouches: 0,
            total_vouches_slashed: 0,
            total_yield_earned: 0,
            total_slashed: 0,
        })
}

/// Pure scoring function: derive a fraud score from a voucher's stats.
pub fn score_from_stats(stats: &VoucherStats) -> u32 {
    let slashed = stats.total_vouches_slashed;
    let successful = stats.successful_vouches;

    let slash_component = slashed.saturating_mul(SCORE_PER_SLASH);

    let total = slashed.saturating_add(successful);
    let ratio_component = if total == 0 {
        0
    } else {
        ((slashed as u64)
            .saturating_mul(RATIO_SCALE as u64)
            / total as u64) as u32
    };

    slash_component
        .saturating_add(ratio_component)
        .min(SCORE_CAP)
}

/// Recompute and persist the fraud score for `voucher`, emitting a `flagged`
/// event when the score meets the configured threshold and scoring is enabled.
pub fn update_fraud_score(
    env: Env,
    voucher: Address,
) -> Result<(), ContractError> {
    let stats = voucher_stats(&env, &voucher);
    let score = score_from_stats(&stats);

    env.storage().persistent().set(
        &DataKey::VoucherFraudScore(voucher.clone()),
        &VoucherFraudScore { score },
    );

    let config = get_fraud_score_config_view(env.clone());
    if config.enabled && config.threshold > 0 && score >= config.threshold {
        env.events().publish(
            ("fraud_score", "flagged"),
            (voucher.clone(), score, config.threshold),
        );
    }

    env.events().publish(
        ("fraud_score", "updated"),
        (voucher, score),
    );

    Ok(())
}

/// Read the stored fraud score for `voucher`, if one has been computed.
pub fn get_fraud_score(
    env: Env,
    voucher: Address,
) -> Option<VoucherFraudScore> {
    env.storage()
        .persistent()
        .get(&DataKey::VoucherFraudScore(voucher))
}

/// `true` when scoring is enabled and the voucher's stored score meets the
/// configured threshold.
pub fn is_flagged(env: &Env, voucher: &Address) -> bool {
    let config = get_fraud_score_config_view(env.clone());
    if !config.enabled || config.threshold == 0 {
        return false;
    }
    env.storage()
        .persistent()
        .get::<_, VoucherFraudScore>(&DataKey::VoucherFraudScore(voucher.clone()))
        .map(|s| s.score >= config.threshold)
        .unwrap_or(false)
}

/// Persist the fraud-score configuration. Requires admin multi-sig approval.
pub fn set_fraud_score_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: FraudScoreConfig,
) -> Result<(), ContractError> {
    crate::rbac::require_admin_approval_for_action(
        &env,
        &admin_signers,
        crate::rbac::AdminAction::UpdateConfig,
    )?;

    env.storage()
        .instance()
        .set(&DataKey::FraudScoreConfig, &config);

    env.events().publish(
        ("fraud_score", "config_updated"),
        (config.threshold, config.enabled),
    );

    Ok(())
}

/// Read the current fraud-score configuration, falling back to a disabled
/// zero-threshold configuration when none has been set.
pub fn get_fraud_score_config_view(
    env: Env,
) -> FraudScoreConfig {
    env.storage()
        .instance()
        .get(&DataKey::FraudScoreConfig)
        .unwrap_or(FraudScoreConfig {
            threshold: 0,
            enabled: false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(successful: u32, slashed: u32) -> VoucherStats {
        VoucherStats {
            successful_vouches: successful,
            total_vouches_slashed: slashed,
            total_yield_earned: 0,
            total_slashed: 0,
        }
    }

    #[test]
    fn test_clean_voucher_scores_zero() {
        assert_eq!(score_from_stats(&stats(10, 0)), 0);
        assert_eq!(score_from_stats(&stats(0, 0)), 0);
    }

    #[test]
    fn test_score_accumulates_with_slashes() {
        let one = score_from_stats(&stats(0, 1));
        let two = score_from_stats(&stats(0, 2));
        let three = score_from_stats(&stats(0, 3));
        assert!(one < two && two < three, "score must grow with slash count");
        // 1 slash, 0 successful: 250 + 5000 (ratio = 1/1) = 5250
        assert_eq!(one, 5_250);
    }

    #[test]
    fn test_ratio_component_rewards_mostly_successful_vouchers() {
        // 1 slash out of 100 total → small ratio component.
        let mostly_good = score_from_stats(&stats(99, 1));
        // 1 slash out of 1 total → full ratio component.
        let all_bad = score_from_stats(&stats(0, 1));
        assert!(mostly_good < all_bad);
    }

    #[test]
    fn test_score_is_capped() {
        assert_eq!(score_from_stats(&stats(0, 1_000)), SCORE_CAP);
    }

    #[test]
    fn test_threshold_breach_detection() {
        let cfg = FraudScoreConfig { threshold: 5_000, enabled: true };
        // 1 slash → 5250 ≥ 5000 → breach.
        assert!(score_from_stats(&stats(0, 1)) >= cfg.threshold);
        // clean voucher → no breach.
        assert!(score_from_stats(&stats(5, 0)) < cfg.threshold);
    }
}
