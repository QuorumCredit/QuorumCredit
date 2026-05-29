#[cfg(test)]
mod initialize_admin_threshold_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn make_token(env: &Env) -> Address {
        let admin = Address::generate(env);
        env.register_stellar_asset_contract_v2(admin).address()
    }

    /// threshold > admins.len() must return InvalidAdminThreshold.
    #[test]
    fn test_threshold_exceeds_admin_count_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admins = Vec::from_array(&env, [Address::generate(&env), Address::generate(&env)]);
        let token = make_token(&env);

        // 2 admins, threshold 3 — must fail.
        let result = client.try_initialize(&deployer, &admins, &3, &token);
        assert_eq!(result, Err(Ok(ContractError::InvalidAdminThreshold)));
    }

    /// threshold <= admins.len() must succeed.
    #[test]
    fn test_threshold_within_admin_count_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admins = Vec::from_array(
            &env,
            [
                Address::generate(&env),
                Address::generate(&env),
                Address::generate(&env),
            ],
        );
        let token = make_token(&env);

        // 3 admins, threshold 2 — must succeed.
        let result = client.try_initialize(&deployer, &admins, &2, &token);
        assert!(result.is_ok());
    }
}
