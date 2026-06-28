#[cfg(test)]
mod operations_runbook_test {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_admin_pause_unpause_sequence() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        // Initialize contract
        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin.clone()],
            &1u32,
            &token,
        )
        .expect("initialization failed");

        // Admin can pause
        crate::lib::pause(&env, &vec![&env, admin.clone()]).expect("pause failed");

        // Admin can unpause
        crate::lib::unpause(&env, &vec![&env, admin.clone()]).expect("unpause failed");
    }

    #[test]
    fn test_admin_multisig_operation() {
        let env = Env::default();
        let admin1 = Address::random(&env);
        let admin2 = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        // Initialize with 2-of-2 multisig
        crate::lib::initialize(
            &env,
            &deployer,
            &vec![&env, admin1.clone(), admin2.clone()],
            &2u32,
            &token,
        )
        .expect("initialization failed");

        // Both admins required for pause
        crate::lib::pause(&env, &vec![&env, admin1.clone(), admin2.clone()])
            .expect("pause with 2 admins failed");
    }

    #[test]
    fn test_get_admins_operational() {
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

        let admins = crate::lib::get_admins(&env);
        assert_eq!(admins.len(), 1);
    }

    #[test]
    fn test_get_config_for_operational_monitoring() {
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

        let config = crate::lib::get_config(&env);
        assert_eq!(config.admin_threshold, 1u32);
    }
}
