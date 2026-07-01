//! Integration tests for the Dynamic Minimum Stake Calculation feature (issue #1057).
//!
//! These tests verify the full lifecycle of the dynamic min stake:
//! - The `get_dynamic_min_stake` contract function
//! - Credit-tier-based reductions via `apply_tier_rewards_to_min_stake`
//! - The governance `SetMinStake` action flowing through to `get_dynamic_min_stake`
//! - The unit helper `credit_score::apply_tier_rewards_to_min_stake`

#[cfg(test)]
mod dynamic_min_stake_tests {
    use crate::{
        ContractError, CreditTier, QuorumCreditContract, QuorumCreditContractClient,
    };
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    // ── helpers ───────────────────────────────────────────────────────────────

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &100_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 120);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    // ── unit: apply_tier_rewards_to_min_stake ─────────────────────────────────

    /// Calling the helper with a borrower that has no credit score record always
    /// returns the base value unchanged.
    #[test]
    fn test_unit_no_credit_score_returns_base() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let base: i128 = 10_000_000;

        let result = crate::credit_score::apply_tier_rewards_to_min_stake(
            &s.env, &borrower, base,
        );
        assert_eq!(result, base);
    }

    /// The helper should never return a negative value.
    #[test]
    fn test_unit_result_is_non_negative() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        // Record a credit score so a reduction may be applied.
        s.client.update_credit_score(&borrower).unwrap();

        let result = crate::credit_score::apply_tier_rewards_to_min_stake(
            &s.env, &borrower, 1,
        );
        assert!(result >= 0, "result must be non-negative, got {result}");
    }

    /// The helper should never return a value larger than the base.
    #[test]
    fn test_unit_result_never_exceeds_base() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let base: i128 = 50_000;
        s.client.update_credit_score(&borrower).unwrap();

        let result = crate::credit_score::apply_tier_rewards_to_min_stake(
            &s.env, &borrower, base,
        );
        assert!(
            result <= base,
            "result ({result}) must not exceed base ({base})"
        );
    }

    /// With base = 0 the helper always returns 0.
    #[test]
    fn test_unit_zero_base_stays_zero() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        s.client.update_credit_score(&borrower).unwrap();

        let result =
            crate::credit_score::apply_tier_rewards_to_min_stake(&s.env, &borrower, 0);
        assert_eq!(result, 0);
    }

    // ── contract: get_dynamic_min_stake ───────────────────────────────────────

    /// Returns 0 when no min_stake has been configured.
    #[test]
    fn test_contract_no_min_stake_set_returns_zero() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        assert_eq!(s.client.get_dynamic_min_stake(&borrower), 0);
    }

    /// Returns the base min_stake when no credit score exists for the borrower.
    #[test]
    fn test_contract_returns_base_when_no_credit_score() {
        let s = setup();
        let base: i128 = 5_000_000;
        s.client.set_min_stake(&admins(&s), &base);

        let borrower = Address::generate(&s.env);
        assert_eq!(s.client.get_dynamic_min_stake(&borrower), base);
    }

    /// After setting a new base min_stake the dynamic value updates accordingly.
    #[test]
    fn test_contract_reflects_updated_min_stake() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        s.client.set_min_stake(&admins(&s), &1_000);
        assert_eq!(s.client.get_dynamic_min_stake(&borrower), 1_000);

        s.client.set_min_stake(&admins(&s), &2_500);
        assert_eq!(s.client.get_dynamic_min_stake(&borrower), 2_500);
    }

    /// Dynamic value is always <= base, never > base.
    #[test]
    fn test_contract_dynamic_never_greater_than_base() {
        let s = setup();
        let base: i128 = 10_000_000;
        s.client.set_min_stake(&admins(&s), &base);

        let borrower = Address::generate(&s.env);
        s.client.update_credit_score(&borrower).unwrap();

        let dynamic = s.client.get_dynamic_min_stake(&borrower);
        assert!(
            dynamic <= base,
            "dynamic ({dynamic}) must be <= base ({base})"
        );
    }

    /// Dynamic value is always >= 0.
    #[test]
    fn test_contract_dynamic_is_non_negative() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &10_000_000);

        let borrower = Address::generate(&s.env);
        s.client.update_credit_score(&borrower).unwrap();

        let dynamic = s.client.get_dynamic_min_stake(&borrower);
        assert!(dynamic >= 0, "dynamic min_stake must be non-negative");
    }

    /// Calling get_dynamic_min_stake multiple times without state changes returns
    /// the same value (idempotent).
    #[test]
    fn test_contract_idempotent() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &3_000);
        let borrower = Address::generate(&s.env);

        let a = s.client.get_dynamic_min_stake(&borrower);
        let b = s.client.get_dynamic_min_stake(&borrower);
        assert_eq!(a, b);
    }

    /// Setting min_stake to 0 disables enforcement; dynamic must also return 0.
    #[test]
    fn test_contract_zero_base_disables_minimum() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &0);

        let borrower = Address::generate(&s.env);
        s.client.update_credit_score(&borrower).unwrap();
        assert_eq!(s.client.get_dynamic_min_stake(&borrower), 0);
    }

    // ── integration: vouch enforcement ───────────────────────────────────────

    /// A vouch above the dynamic min_stake for a borrower with no credit score
    /// (dynamic == base) is accepted.
    #[test]
    fn test_vouch_above_dynamic_min_accepted() {
        let s = setup();
        let base: i128 = 1_000;
        s.client.set_min_stake(&admins(&s), &base);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &(base + 500));

        let res = s.client.try_vouch(&voucher, &borrower, &(base + 500), &s.token);
        assert!(res.is_ok(), "vouch above dynamic min should succeed");
    }

    /// A vouch strictly below the dynamic min_stake is rejected with MinStakeNotMet.
    #[test]
    fn test_vouch_below_dynamic_min_rejected() {
        let s = setup();
        let base: i128 = 2_000;
        s.client.set_min_stake(&admins(&s), &base);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &(base - 1));

        let err = s
            .client
            .try_vouch(&voucher, &borrower, &(base - 1), &s.token)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::MinStakeNotMet);
    }

    /// When a credit score exists the dynamic min is <= base; a voucher with exactly
    /// the dynamic min should succeed.
    #[test]
    fn test_vouch_at_dynamic_min_with_credit_score() {
        let s = setup();
        let base: i128 = 10_000_000; // 1 XLM
        s.client.set_min_stake(&admins(&s), &base);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        s.client.update_credit_score(&borrower).unwrap();

        let dynamic = s.client.get_dynamic_min_stake(&borrower);
        // Mint exactly the dynamic minimum.
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &dynamic.max(1));

        // Vouch with the dynamic minimum (or 1 if dynamic == 0).
        let stake = dynamic.max(1);
        let res = s.client.try_vouch(&voucher, &borrower, &stake, &s.token);
        assert!(
            res.is_ok(),
            "vouch at dynamic min_stake should succeed; dynamic={dynamic}"
        );
    }

    // ── tier reduction constants ──────────────────────────────────────────────

    /// Verify the default TierRewards min_stake_reduction_bps constants are in range.
    #[test]
    fn test_default_tier_rewards_reduction_bps_in_range() {
        use crate::types::{
            DEFAULT_FAIR_REWARDS, DEFAULT_GOOD_REWARDS, DEFAULT_VERY_GOOD_REWARDS,
            DEFAULT_EXCELLENT_REWARDS,
        };
        for (tier, bps) in [
            ("Fair", DEFAULT_FAIR_REWARDS.min_stake_reduction_bps),
            ("Good", DEFAULT_GOOD_REWARDS.min_stake_reduction_bps),
            ("VeryGood", DEFAULT_VERY_GOOD_REWARDS.min_stake_reduction_bps),
            ("Excellent", DEFAULT_EXCELLENT_REWARDS.min_stake_reduction_bps),
        ] {
            assert!(
                bps <= 10_000,
                "Tier {tier} min_stake_reduction_bps ({bps}) must be <= 10_000"
            );
        }
    }

    /// Excellent tier should have the highest (or equal) reduction compared to lower tiers.
    #[test]
    fn test_excellent_tier_has_highest_reduction() {
        use crate::types::{
            DEFAULT_GOOD_REWARDS, DEFAULT_VERY_GOOD_REWARDS, DEFAULT_EXCELLENT_REWARDS,
        };
        assert!(
            DEFAULT_EXCELLENT_REWARDS.min_stake_reduction_bps
                >= DEFAULT_VERY_GOOD_REWARDS.min_stake_reduction_bps,
            "Excellent tier must reduce min_stake by at least as much as VeryGood"
        );
        assert!(
            DEFAULT_VERY_GOOD_REWARDS.min_stake_reduction_bps
                >= DEFAULT_GOOD_REWARDS.min_stake_reduction_bps,
            "VeryGood tier must reduce min_stake by at least as much as Good"
        );
    }
}
