#[cfg(test)]
mod credit_score_tests {
    use crate::{
        CreditScore, CreditScoreConfig, CreditTier, QuorumCreditContract, QuorumCreditContractClient,
    };
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup_credit_score() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin1.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &2, &token);

        (env, client, admin1, admin2, deployer)
    }

    #[test]
    fn test_update_credit_score_new_user() {
        let (env, client, admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&env);

        // Update credit score for new user
        client.update_credit_score(&borrower).unwrap();

        let credit_score: CreditScore = client
            .get_credit_score(borrower)
            .unwrap()
            .try_into()
            .unwrap();

        // New user should have a neutral score around 500
        assert!(credit_score.score >= 400 && credit_score.score <= 600);
        assert_eq!(credit_score.total_loans, 0);
        assert_eq!(credit_score.successful_repayments, 0);
        assert_eq!(credit_score.defaults, 0);
    }

    #[test]
    fn test_get_credit_score_not_found() {
        let (_env, client, _admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&_env);

        // Should return None for user without credit score
        let credit_score = client.get_credit_score(borrower);
        assert!(credit_score.is_none());
    }

    #[test]
    fn test_set_credit_score_config() {
        let (_env, client, admin1, admin2, _deployer) = setup_credit_score();

        let config = CreditScoreConfig {
            enabled: true,
            factors: crate::types::CreditFactors {
                repayment_history_weight: 5000,
                loan_count_weight: 2000,
                account_age_weight: 1000,
                vouching_weight: 1000,
                timeliness_weight: 1000,
            },
            poor_rewards: crate::types::DEFAULT_POOR_REWARDS,
            fair_rewards: crate::types::DEFAULT_FAIR_REWARDS,
            good_rewards: crate::types::DEFAULT_GOOD_REWARDS,
            very_good_rewards: crate::types::DEFAULT_VERY_GOOD_REWARDS,
            excellent_rewards: crate::types::DEFAULT_EXCELLENT_REWARDS,
        };

        client
            .set_credit_score_config(
                &Vec::from_array(&_env, [admin1.clone(), admin2.clone()]),
                &config,
            )
            .unwrap();

        let retrieved_config = client.get_credit_score_config_view();
        assert_eq!(retrieved_config.factors.repayment_history_weight, 5000);
    }

    #[test]
    fn test_set_credit_score_config_invalid_weights() {
        let (env, client, admin1, admin2, _deployer) = setup_credit_score();

        let config = CreditScoreConfig {
            enabled: true,
            factors: crate::types::CreditFactors {
                repayment_history_weight: 3000,
                loan_count_weight: 2000,
                account_age_weight: 1000,
                vouching_weight: 1000,
                timeliness_weight: 1000,
            }, // Total = 8000, should be 10000
            poor_rewards: crate::types::DEFAULT_POOR_REWARDS,
            fair_rewards: crate::types::DEFAULT_FAIR_REWARDS,
            good_rewards: crate::types::DEFAULT_GOOD_REWARDS,
            very_good_rewards: crate::types::DEFAULT_VERY_GOOD_REWARDS,
            excellent_rewards: crate::types::DEFAULT_EXCELLENT_REWARDS,
        };

        let result = client.try_set_credit_score_config(
            &Vec::from_array(&env, [admin1.clone(), admin2.clone()]),
            &config,
        );

        assert_eq!(result, Err(Ok(crate::ContractError::InvalidCreditConfig)));
    }

    #[test]
    fn test_get_tier_rewards() {
        let (_env, client, _admin1, _admin2, _deployer) = setup_credit_score();

        let poor_rewards = client.get_tier_rewards(CreditTier::Poor);
        assert_eq!(poor_rewards.yield_bonus_bps, 0);
        assert_eq!(poor_rewards.max_loan_multiplier, 100);

        let excellent_rewards = client.get_tier_rewards(CreditTier::Excellent);
        assert_eq!(excellent_rewards.yield_bonus_bps, 200);
        assert_eq!(excellent_rewards.max_loan_multiplier, 200);
    }

    #[test]
    fn test_credit_tier_calculation() {
        // Test tier boundaries
        let poor_tier = crate::credit_score::calculate_tier(349);
        assert_eq!(poor_tier, CreditTier::Poor);

        let fair_tier = crate::credit_score::calculate_tier(350);
        assert_eq!(fair_tier, CreditTier::Fair);

        let good_tier = crate::credit_score::calculate_tier(550);
        assert_eq!(good_tier, CreditTier::Good);

        let very_good_tier = crate::credit_score::calculate_tier(700);
        assert_eq!(very_good_tier, CreditTier::VeryGood);

        let excellent_tier = crate::credit_score::calculate_tier(850);
        assert_eq!(excellent_tier, CreditTier::Excellent);
    }

    #[test]
    fn test_repayment_history_score() {
        // Perfect repayment history
        let score = crate::credit_score::calculate_repayment_history_score(10, 10, 0);
        assert_eq!(score, 1000);

        // Mixed history
        let score = crate::credit_score::calculate_repayment_history_score(8, 10, 2);
        assert!(score < 1000 && score > 0);

        // All defaults
        let score = crate::credit_score::calculate_repayment_history_score(0, 10, 10);
        assert!(score < 500);

        // New user
        let score = crate::credit_score::calculate_repayment_history_score(0, 0, 0);
        assert_eq!(score, 500);
    }

    #[test]
    fn test_loan_count_score() {
        let score = crate::credit_score::calculate_loan_count_score(0);
        assert_eq!(score, 0);

        let score = crate::credit_score::calculate_loan_count_score(5);
        assert!(score > 0 && score < 1000);

        let score = crate::credit_score::calculate_loan_count_score(10);
        assert_eq!(score, 1000);

        let score = crate::credit_score::calculate_loan_count_score(20);
        assert_eq!(score, 1000); // Capped at 1000
    }

    #[test]
    fn test_account_age_score() {
        let score = crate::credit_score::calculate_account_age_score(0);
        assert_eq!(score, 0);

        let one_year = 365 * 24 * 60 * 60;
        let score = crate::credit_score::calculate_account_age_score(one_year);
        assert_eq!(score, 1000);

        let score = crate::credit_score::calculate_account_age_score(one_year * 2);
        assert_eq!(score, 1000); // Capped at 1000
    }

    #[test]
    fn test_vouching_score() {
        let score = crate::credit_score::calculate_vouching_score(0);
        assert_eq!(score, 0);

        let score = crate::credit_score::calculate_vouching_score(10);
        assert!(score > 0 && score < 1000);

        let score = crate::credit_score::calculate_vouching_score(20);
        assert_eq!(score, 1000);

        let score = crate::credit_score::calculate_vouching_score(30);
        assert_eq!(score, 1000); // Capped at 1000
    }

    #[test]
    fn test_timeliness_score() {
        // Early repayment (7 days early)
        let early = 7 * 24 * 60 * 60;
        let score = crate::credit_score::calculate_timeliness_score(early as i64);
        assert_eq!(score, 1000);

        // On-time repayment
        let score = crate::credit_score::calculate_timeliness_score(0);
        assert!(score > 400 && score < 600);

        // Late repayment (7 days late)
        let late = -7 * 24 * 60 * 60;
        let score = crate::credit_score::calculate_timeliness_score(late);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_apply_tier_rewards_to_yield() {
        let (env, client, _admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&env);

        // Update credit score
        client.update_credit_score(&borrower).unwrap();

        let base_yield = 500; // 5%
        let adjusted_yield = crate::credit_score::apply_tier_rewards_to_yield(
            &env,
            &borrower,
            base_yield,
        );

        // Should be at least base yield
        assert!(adjusted_yield >= base_yield);
    }

    #[test]
    fn test_apply_tier_rewards_to_max_loan() {
        let (env, client, _admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&env);

        // Update credit score
        client.update_credit_score(&borrower).unwrap();

        let base_max_loan = 100_000_000; // 10 XLM
        let adjusted_max = crate::credit_score::apply_tier_rewards_to_max_loan(
            &env,
            &borrower,
            base_max_loan,
        );

        // Should be at least base amount
        assert!(adjusted_max >= base_max_loan);
    }

    #[test]
    fn test_apply_tier_rewards_to_min_stake() {
        let (env, client, _admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&env);

        // Update credit score
        client.update_credit_score(&borrower).unwrap();

        let base_min_stake = 10_000_000; // 1 XLM
        let adjusted_min = crate::credit_score::apply_tier_rewards_to_min_stake(
            &env,
            &borrower,
            base_min_stake,
        );

        // Should be at most base amount (reduction applied)
        assert!(adjusted_min <= base_min_stake);
    }

    #[test]
    fn test_apply_tier_rewards_to_duration() {
        let (env, client, _admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&env);

        // Update credit score
        client.update_credit_score(&borrower).unwrap();

        let base_duration = 30 * 24 * 60 * 60; // 30 days
        let adjusted_duration = crate::credit_score::apply_tier_rewards_to_duration(
            &env,
            &borrower,
            base_duration,
        );

        // Should be at least base duration
        assert!(adjusted_duration >= base_duration);
    }

    #[test]
    fn test_apply_tier_rewards_to_fee() {
        let (env, client, _admin1, _admin2, _deployer) = setup_credit_score();
        let borrower = Address::generate(&env);

        // Update credit score
        client.update_credit_score(&borrower).unwrap();

        let base_fee = 500; // 5%
        let adjusted_fee = crate::credit_score::apply_tier_rewards_to_fee(&env, &borrower, base_fee);

        // Should be at most base fee (discount applied)
        assert!(adjusted_fee <= base_fee);
    }

    #[test]
    fn test_default_credit_score_config() {
        let config = crate::types::DEFAULT_CREDIT_SCORE_CONFIG;
        assert!(config.enabled);

        let total_weight = config.factors.repayment_history_weight
            + config.factors.loan_count_weight
            + config.factors.account_age_weight
            + config.factors.vouching_weight
            + config.factors.timeliness_weight;

        assert_eq!(total_weight, 10000);
    }

    #[test]
    fn test_tier_rewards_progression() {
        let poor = crate::types::DEFAULT_POOR_REWARDS;
        let fair = crate::types::DEFAULT_FAIR_REWARDS;
        let good = crate::types::DEFAULT_GOOD_REWARDS;
        let very_good = crate::types::DEFAULT_VERY_GOOD_REWARDS;
        let excellent = crate::types::DEFAULT_EXCELLENT_REWARDS;

        // Rewards should increase with tier
        assert!(fair.yield_bonus_bps > poor.yield_bonus_bps);
        assert!(good.yield_bonus_bps > fair.yield_bonus_bps);
        assert!(very_good.yield_bonus_bps > good.yield_bonus_bps);
        assert!(excellent.yield_bonus_bps > very_good.yield_bonus_bps);

        assert!(fair.max_loan_multiplier > poor.max_loan_multiplier);
        assert!(good.max_loan_multiplier > fair.max_loan_multiplier);
        assert!(very_good.max_loan_multiplier > good.max_loan_multiplier);
        assert!(excellent.max_loan_multiplier > very_good.max_loan_multiplier);
    }
}
