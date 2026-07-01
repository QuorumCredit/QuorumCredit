#[cfg(test)]
mod ratio_enforcement_tests {
    use crate::{
        QuorumCreditContract, QuorumCreditContractClient,
    };
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Env, String, Vec,
    };
    use crate::errors::ContractError;

    fn setup() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let contract_id = env.register(QuorumCreditContract, ());
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token);

        (env, client, admin, deployer, token)
    }

    #[test]
    fn test_ratio_enforcement() {
        let (env, client, admin, deployer, token) = setup();

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        let token_admin = StellarAssetClient::new(&env, &token);
        token_admin.mint(&voucher, &10_000_000_000);
        token_admin.mint(&borrower, &10_000_000_000);
        token_admin.mint(&client.address, &10_000_000_000);

        // Vouch 1,000,000,000 stroops
        client.vouch(&voucher, &borrower, &1_000_000_000, &token, &None);

        // Under 50% max_loan_to_collateral_ratio:
        // default ratio = 50_000. total_stake = 1_000_000_000
        // max loan amount = 1_000_000_000 * 50_000 / 10_000 = 5_000_000_000.
        // A loan of 5_000_000_000 should succeed.
        client.request_loan(
            &borrower,
            &5_000_000_000,
            &5_000_000_000,
            &String::from_str(&env, "Allowed loan"),
            &token,
        );

        assert_eq!(client.loan_status(&borrower), crate::types::LoanStatus::Active);
    }

    #[test]
    fn test_ratio_enforcement_exceeded() {
        let (env, client, admin, deployer, token) = setup();

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        let token_admin = StellarAssetClient::new(&env, &token);
        token_admin.mint(&voucher, &10_000_000_000);
        token_admin.mint(&borrower, &10_000_000_000);
        token_admin.mint(&client.address, &10_000_000_000);

        // Vouch 1_000_000_000
        client.vouch(&voucher, &borrower, &1_000_000_000, &token, &None);

        // Exceeding 50% max_loan_to_collateral_ratio:
        // 5_000_000_001 is greater than 1_000_000_000 * 50_000 / 10_000
        // This request should return LoanExceedsMaxRatio error.
        let result = client.try_request_loan(
            &borrower,
            &5_000_000_001,
            &1_000_000_000,
            &String::from_str(&env, "Exceeded loan"),
            &token,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::LoanExceedsMaxRatio
        );
    }
}
