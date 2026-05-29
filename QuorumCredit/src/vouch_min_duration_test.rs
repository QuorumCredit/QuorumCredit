#[cfg(test)]
mod vouch_min_duration_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::{Address as _, Ledger as _}, token::StellarAssetClient, Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        QuorumCreditContractClient::new(env, &contract_id).initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token_id,
        );
        (contract_id, token_id, admin)
    }

    /// #640: get/set min_vouch_duration works.
    #[test]
    fn test_set_get_min_vouch_duration() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        assert_eq!(client.get_min_vouch_duration(), 0);
        client.set_min_vouch_duration(&Vec::from_array(&env, [admin]), &86400);
        assert_eq!(client.get_min_vouch_duration(), 86400);
    }

    /// #640: withdraw_vouch before min duration is rejected.
    #[test]
    fn test_withdraw_vouch_before_min_duration_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Set 1-day minimum duration
        client.set_min_vouch_duration(&Vec::from_array(&env, [admin.clone()]), &86400);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&voucher, &1_000_000);
        client.vouch(&voucher, &borrower, &500_000, &token_id);

        // Try to withdraw immediately — should fail
        let result = client.try_withdraw_vouch(&voucher, &borrower);
        assert_eq!(result, Err(Ok(ContractError::VouchTooYoungToWithdraw)));
    }

    /// #640: withdraw_vouch after min duration succeeds.
    #[test]
    fn test_withdraw_vouch_after_min_duration_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.set_min_vouch_duration(&Vec::from_array(&env, [admin.clone()]), &86400);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&voucher, &1_000_000);
        client.vouch(&voucher, &borrower, &500_000, &token_id);

        // Advance time past the minimum duration
        env.ledger().with_mut(|l| l.timestamp += 86401);

        let result = client.try_withdraw_vouch(&voucher, &borrower);
        assert!(result.is_ok());
    }

    /// #640: decrease_stake before min duration is rejected.
    #[test]
    fn test_decrease_stake_before_min_duration_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.set_min_vouch_duration(&Vec::from_array(&env, [admin.clone()]), &3600);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&voucher, &1_000_000);
        client.vouch(&voucher, &borrower, &500_000, &token_id);

        let result = client.try_decrease_stake(&voucher, &borrower, &100_000);
        assert_eq!(result, Err(Ok(ContractError::VouchTooYoungToWithdraw)));
    }

    /// #640: min_duration=0 means no restriction.
    #[test]
    fn test_min_duration_zero_no_restriction() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&voucher, &1_000_000);
        client.vouch(&voucher, &borrower, &500_000, &token_id);

        // Withdraw immediately with no duration restriction
        let result = client.try_withdraw_vouch(&voucher, &borrower);
        assert!(result.is_ok());
    }
}
