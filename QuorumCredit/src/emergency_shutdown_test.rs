#[cfg(test)]
mod emergency_shutdown_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Vec<Address>, u32, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin]);
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(env))
            .address();
        (deployer, admins, 1, token)
    }

    #[test]
    fn test_emergency_shutdown_disabled_by_default() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (deployer, admins, threshold, token) = setup(&env);
        client.initialize(&deployer, &admins, &threshold, &token);

        let cfg = client.get_config();
        assert!(!cfg.emergency_shutdown_enabled);
    }

    #[test]
    fn test_set_emergency_shutdown_enabled() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (deployer, admins, threshold, token) = setup(&env);
        client.initialize(&deployer, &admins, &threshold, &token);

        // Enable emergency shutdown
        client.set_emergency_shutdown(&admins, &true);

        let cfg = client.get_config();
        assert!(cfg.emergency_shutdown_enabled);
    }

    #[test]
    fn test_set_emergency_shutdown_disabled() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (deployer, admins, threshold, token) = setup(&env);
        client.initialize(&deployer, &admins, &threshold, &token);

        // First enable it
        client.set_emergency_shutdown(&admins, &true);
        let cfg1 = client.get_config();
        assert!(cfg1.emergency_shutdown_enabled);

        // Then disable it
        client.set_emergency_shutdown(&admins, &false);
        let cfg2 = client.get_config();
        assert!(!cfg2.emergency_shutdown_enabled);
    }

    #[test]
    fn test_emergency_shutdown_toggle() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (deployer, admins, threshold, token) = setup(&env);
        client.initialize(&deployer, &admins, &threshold, &token);

        // Test multiple toggles
        for i in 0..5 {
            let should_be_enabled = (i % 2) == 0;
            client.set_emergency_shutdown(&admins, &should_be_enabled);
            let cfg = client.get_config();
            assert_eq!(cfg.emergency_shutdown_enabled, should_be_enabled);
        }
    }

    #[test]
    fn test_only_admin_can_set_emergency_shutdown() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (deployer, admins, threshold, token) = setup(&env);
        client.initialize(&deployer, &admins, &threshold, &token);

        let non_admin = Address::generate(&env);
        let non_admin_vec = Vec::from_array(&env, [non_admin.clone()]);

        // This should panic because non_admin is not an admin
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_emergency_shutdown(&non_admin_vec, &true);
        }));
        assert!(result.is_err(), "non-admin should not be able to set emergency shutdown");
    }

    #[test]
    fn test_emergency_shutdown_persists_across_calls() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (deployer, admins, threshold, token) = setup(&env);
        client.initialize(&deployer, &admins, &threshold, &token);

        // Enable emergency shutdown
        client.set_emergency_shutdown(&admins, &true);

        // Multiple reads should return true
        for _ in 0..3 {
            let cfg = client.get_config();
            assert!(cfg.emergency_shutdown_enabled);
        }
    }
}
