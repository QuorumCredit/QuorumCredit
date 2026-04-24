/// Tests that batch_vouch() rejects the entire batch when any individual vouch is invalid.
#[cfg(test)]
mod batch_vouch_partial_failure_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Env, Vec,
    };

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &Vec::from_array(env, [admin]), &1, &token_id);
        let voucher = Address::generate(env);
        StellarAssetClient::new(env, &token_id).mint(&voucher, &3_000_000);
        (contract_id, token_id, voucher)
    }

    /// batch_vouch() must reject the entire batch when one stake is 0 (InsufficientFunds).
    #[test]
    fn test_batch_vouch_partial_failure_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, voucher) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let borrower_a = Address::generate(&env);
        let borrower_b = Address::generate(&env);

        let borrowers = Vec::from_array(&env, [borrower_a.clone(), borrower_b.clone()]);
        // Second stake is 0 — invalid.
        let stakes = Vec::from_array(&env, [1_000_000_i128, 0_i128]);

        let result = client.try_batch_vouch(&voucher, &borrowers, &stakes, &token_id);
        assert_eq!(result, Err(Ok(ContractError::InsufficientFunds)));

        // Neither vouch should have been created.
        assert!(client.get_vouches(&borrower_a).is_none());
        assert!(client.get_vouches(&borrower_b).is_none());
    }

    /// batch_vouch() must succeed when all stakes are valid.
    #[test]
    fn test_batch_vouch_all_valid_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, voucher) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let borrower_a = Address::generate(&env);
        let borrower_b = Address::generate(&env);

        let borrowers = Vec::from_array(&env, [borrower_a.clone(), borrower_b.clone()]);
        let stakes = Vec::from_array(&env, [1_000_000_i128, 1_000_000_i128]);

        let result = client.try_batch_vouch(&voucher, &borrowers, &stakes, &token_id);
        assert!(result.is_ok());

        assert!(client.get_vouches(&borrower_a).is_some());
        assert!(client.get_vouches(&borrower_b).is_some());
    }
}
