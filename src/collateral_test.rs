#[cfg(test)]
mod tests {
    use crate::loan::{deposit_collateral, get_collateral};
    use soroban_sdk::Address;

    #[test]
    fn test_deposit_collateral() {
        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        // Initialize contract
        crate::lib::QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            soroban_sdk::vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        // Setup token mock
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.set_balance(&borrower, &1_000_000);

        // Deposit collateral
        deposit_collateral(env.clone(), borrower.clone(), 50_000, token.clone()).unwrap();

        // Verify collateral was deposited
        let collateral = get_collateral(env.clone(), borrower.clone());
        assert_eq!(collateral, 50_000);
    }

    #[test]
    fn test_deposit_collateral_multiple_times() {
        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        // Initialize contract
        crate::lib::QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            soroban_sdk::vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        // Setup token mock
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.set_balance(&borrower, &1_000_000);

        // Deposit collateral twice
        deposit_collateral(env.clone(), borrower.clone(), 30_000, token.clone()).unwrap();
        deposit_collateral(env.clone(), borrower.clone(), 20_000, token.clone()).unwrap();

        // Verify total collateral
        let collateral = get_collateral(env.clone(), borrower.clone());
        assert_eq!(collateral, 50_000);
    }
}
