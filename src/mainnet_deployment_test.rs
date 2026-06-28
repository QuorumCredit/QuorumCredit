#[cfg(test)]
mod mainnet_deployment_test {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_deployer_gated_initialization() {
        let env = Env::default();
        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        // Initialize requires deployer signature
        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization with deployer should succeed");
    }

    #[test]
    fn test_initialize_only_once() {
        let env = Env::default();
        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("first initialization should succeed");

        // Second initialization should fail
        let result = crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_mainnet_config_persistence() {
        let env = Env::default();
        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization failed");

        let config = crate::lib::get_config(&env);
        assert_eq!(config.token, token);
        assert_eq!(config.admin_threshold, 1u32);
    }

    #[test]
    fn test_admin_threshold_enforcement() {
        let env = Env::default();
        let admin1 = Address::random(&env);
        let admin2 = Address::random(&env);
        let admin3 = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        // Initialize with 3-of-3 multisig requirement
        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin1.clone(), admin2.clone(), admin3.clone()],
            &3u32,
            &token,
        )
        .expect("initialization failed");

        let config = crate::lib::get_config(&env);
        assert_eq!(config.admins.len(), 3);
        assert_eq!(config.admin_threshold, 3u32);
    }
}
