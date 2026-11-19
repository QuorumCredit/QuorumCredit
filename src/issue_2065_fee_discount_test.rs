//! Issue #2065: the tier fee discount must not overflow `u32` when the configured discount is large.

#![cfg(test)]

use crate::credit_score::apply_tier_rewards_to_fee;
use crate::types::{CreditScore, CreditTier, DataKey, DEFAULT_CREDIT_SCORE_CONFIG};
use crate::QuorumCreditContract;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_excellent_borrower(discount_bps: u32) -> (Env, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(QuorumCreditContract, ());
    let borrower = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut cfg = DEFAULT_CREDIT_SCORE_CONFIG;
        cfg.excellent_rewards.fee_discount_bps = discount_bps;
        env.storage().instance().set(&crate::types::DataKey::CreditScoreConfig, &cfg);

        let score = CreditScore {
            score: 950,
            tier: CreditTier::Excellent,
            last_updated: 0,
            last_decay_timestamp: 0,
            total_loans: 0,
            successful_repayments: 0,
            defaults: 0,
            total_borrowed: 0,
            total_repaid: 0,
            account_age: 0,
            voucher_count: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(borrower.clone()), &score);
    });

    (env, contract_id, borrower)
}

#[test]
fn test_fee_discount_maximum_configured_value_does_not_overflow() {
    // u32::MAX bps would overflow the old `base * discount_bps` u32 product.
    let (env, contract_id, borrower) = setup_excellent_borrower(u32::MAX);

    let fee = env.as_contract(&contract_id, || {
        apply_tier_rewards_to_fee(&env, &borrower, 1_000)
    });

    // Discount is capped at 100%, so the fee is fully discounted and never underflows.
    assert_eq!(fee, 0);
}

#[test]
fn test_fee_discount_large_base_fee_with_large_discount() {
    let (env, contract_id, borrower) = setup_excellent_borrower(9_999);

    let fee = env.as_contract(&contract_id, || {
        apply_tier_rewards_to_fee(&env, &borrower, u32::MAX)
    });

    // 9,999 bps of u32::MAX, subtracted from u32::MAX, leaves a value in range.
    let expected_discount = (u32::MAX as u64 * 9_999 / 10_000) as u32;
    assert_eq!(fee, u32::MAX - expected_discount);
}

#[test]
fn test_fee_discount_normal_configuration_is_unchanged() {
    // 1,000 bps (10%) on a 500 bps fee gives a 450 bps fee.
    let (env, contract_id, borrower) = setup_excellent_borrower(1_000);

    let fee = env.as_contract(&contract_id, || {
        apply_tier_rewards_to_fee(&env, &borrower, 500)
    });

    assert_eq!(fee, 450);
}
