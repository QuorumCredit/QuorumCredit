#[cfg(test)]
mod monitoring_setup_test {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_contract_state_queryable_for_monitoring() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization failed");

        // Monitoring: Can query paused state
        let config = crate::lib::get_config(&env);
        assert!(!config.paused);
    }

    #[test]
    fn test_pause_event_for_monitoring() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization failed");

        crate::lib::pause(&env, &vec![&env, admin.clone()]).expect("pause failed");

        let config = crate::lib::get_config(&env);
        assert!(config.paused);
    }

    #[test]
    fn test_fee_treasury_monitoring() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization failed");

        // Monitoring: Fee treasury is queryable
        let treasury = crate::lib::get_fee_treasury(&env);
        assert_eq!(treasury, 0i128);
    }

    #[test]
    fn test_admin_count_monitoring() {
        let env = Env::default();
        let admin1 = Address::random(&env);
        let admin2 = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin1.clone(), admin2.clone()],
            &2u32,
            &token,
        )
        .expect("initialization failed");

        // Monitoring: Admin count queryable
        let admins = crate::lib::get_admins(&env);
        assert_eq!(admins.len(), 2);
    }

    #[test]
    fn test_config_thresholds_monitoring() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization failed");

        // Monitoring: Configuration parameters are queryable
        let config = crate::lib::get_config(&env);
        assert!(config.yield_bps > 0i128);
        assert!(config.slash_bps > 0i128);
    }
}
