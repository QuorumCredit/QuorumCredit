#[cfg(test)]
mod voucher_stake_limit_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Address, Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token_id,
        );
        let voucher = Address::generate(env);
        StellarAssetClient::new(env, &token_id).mint(&voucher, &10_000_000);
        (contract_id, token_id, admin, voucher, Address::generate(env))
    }

    /// vouch() with stake exactly at the limit must succeed.
    #[test]
    fn test_vouch_at_limit_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.set_voucher_stake_limit(
            &Vec::from_array(&env, [admin]),
            &voucher,
            &borrower,
            &1_000_000,
        );

        let result = client.try_vouch(&voucher, &borrower, &1_000_000, &token_id);
        assert!(result.is_ok());
    }

    /// vouch() with stake above the limit must be rejected.
    #[test]
    fn test_vouch_above_limit_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.set_voucher_stake_limit(
            &Vec::from_array(&env, [admin]),
            &voucher,
            &borrower,
            &1_000_000,
        );

        let result = client.try_vouch(&voucher, &borrower, &1_000_001, &token_id);
        assert_eq!(result, Err(Ok(ContractError::StakeLimitExceeded)));
    }

    /// increase_stake() that would push total above the limit must be rejected.
    #[test]
    fn test_increase_stake_above_limit_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Set limit to 1_500_000
        client.set_voucher_stake_limit(
            &Vec::from_array(&env, [admin]),
            &voucher,
            &borrower,
            &1_500_000,
        );

        // Initial vouch of 1_000_000 — within limit
        client.vouch(&voucher, &borrower, &1_000_000, &token_id);

        // Increase by 500_001 would reach 1_500_001 — over limit
        let result = client.try_increase_stake(&voucher, &borrower, &500_001);
        assert_eq!(result, Err(Ok(ContractError::StakeLimitExceeded)));
    }

    /// increase_stake() that stays at the limit must succeed.
    #[test]
    fn test_increase_stake_at_limit_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.set_voucher_stake_limit(
            &Vec::from_array(&env, [admin]),
            &voucher,
            &borrower,
            &1_500_000,
        );

        client.vouch(&voucher, &borrower, &1_000_000, &token_id);

        // Increase by exactly 500_000 — reaches limit exactly
        let result = client.try_increase_stake(&voucher, &borrower, &500_000);
        assert!(result.is_ok());
    }

    /// No limit set — vouch() with any positive amount must succeed.
    #[test]
    fn test_vouch_no_limit_set_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let result = client.try_vouch(&voucher, &borrower, &5_000_000, &token_id);
        assert!(result.is_ok());
    }

    /// get_voucher_stake_limit returns None when no limit is set.
    #[test]
    fn test_get_voucher_stake_limit_none_when_unset() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, _admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let limit = client.get_voucher_stake_limit(&voucher, &borrower);
        assert!(limit.is_none());
    }

    /// get_voucher_stake_limit returns the set value.
    #[test]
    fn test_get_voucher_stake_limit_returns_set_value() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, admin, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.set_voucher_stake_limit(
            &Vec::from_array(&env, [admin]),
            &voucher,
            &borrower,
            &2_000_000,
        );

        assert_eq!(client.get_voucher_stake_limit(&voucher, &borrower), Some(2_000_000));
    }
}
