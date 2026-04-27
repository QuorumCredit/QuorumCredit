#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{testutils::*, Address, Env, BytesN};

    #[test]
    fn test_validate_upgrade_zero_hash() {
        let env = Env::default();
        let contract = QuorumCreditContract;
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        let admins = soroban_sdk::vec![&env, admin.clone()];
        contract
            .initialize(env.clone(), deployer, admins, 1, token)
            .unwrap();

        let zero_hash = BytesN::<32>::from_array(&env, &[0u8; 32]);
        let result = contract.validate_upgrade(env.clone(), zero_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_upgrade_uninitialized() {
        let env = Env::default();
        let contract = QuorumCreditContract;

        let valid_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
        let result = contract.validate_upgrade(env.clone(), valid_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_upgrade_valid() {
        let env = Env::default();
        let contract = QuorumCreditContract;
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        let admins = soroban_sdk::vec![&env, admin.clone()];
        contract
            .initialize(env.clone(), deployer, admins, 1, token)
            .unwrap();

        let valid_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
        let result = contract.validate_upgrade(env.clone(), valid_hash);
        assert!(result.is_ok());
    }
}
