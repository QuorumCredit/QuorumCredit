#[cfg(test)]
mod api_reference_test {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_get_config_api() {
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

        // API: get_config returns Config struct
        let config = crate::lib::get_config(&env);
        assert_eq!(config.token, token);
    }

    #[test]
    fn test_get_admins_api() {
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

        // API: get_admins returns Vec<Address>
        let admins = crate::lib::get_admins(&env);
        assert_eq!(admins.len(), 1);
    }

    #[test]
    fn test_loan_status_api() {
        let env = Env::default();
        let borrower = Address::random(&env);
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

        // API: loan_status returns LoanStatus
        let status = crate::lib::loan_status(&env, &borrower);
        assert_eq!(status, crate::types::LoanStatus::None);
    }

    #[test]
    fn test_get_loan_api() {
        let env = Env::default();
        let borrower = Address::random(&env);
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

        // API: get_loan returns Option<LoanRecord>
        let loan = crate::lib::get_loan(&env, &borrower);
        assert!(loan.is_none());
    }

    #[test]
    fn test_get_fee_treasury_api() {
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

        // API: get_fee_treasury returns i128
        let treasury = crate::lib::get_fee_treasury(&env);
        assert_eq!(treasury, 0i128);
    }
}
