#[cfg(test)]
mod tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    fn setup_uninitialized(env: &Env) -> Address {
        env.register_contract(None, QuorumCreditContract)
    }

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        QuorumCreditContractClient::new(env, &contract_id).initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token,
        );
        (contract_id, admin, token)
    }

    #[test]
    fn test_validate_upgrade_zero_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let zero_hash = BytesN::<32>::from_array(&env, &[0u8; 32]);
        let result = client.try_validate_upgrade(&zero_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_upgrade_uninitialized() {
        let env = Env::default();
        let contract_id = setup_uninitialized(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let valid_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
        let result = client.try_validate_upgrade(&valid_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_upgrade_valid() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let valid_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
        let result = client.try_validate_upgrade(&valid_hash);
        assert!(result.is_ok());
    }
}
