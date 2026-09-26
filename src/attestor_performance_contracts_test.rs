#[cfg(test)]
mod attestor_performance_contracts_tests {
    use crate::attestor_performance_contracts;
    use soroban_sdk::{testutils::Address as _, Bytes, Env, Address};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1714: Register a performance contract for an attestor.
    #[test]
    fn test_register_performance_contract_succeeds() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let contract = Bytes::new(&env, &[1, 2, 3, 4]);

        env.as_contract(&contract_id, || {
            let result = attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract.clone());
            assert!(result.is_ok());
        });
    }

    /// Issue #1714: Retrieve a registered performance contract.
    #[test]
    fn test_get_performance_contract_returns_registered_contract() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let contract = Bytes::new(&env, &[1, 2, 3, 4]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract.clone()).unwrap();
            let retrieved = attestor_performance_contracts::get_performance_contract(&env, &attestor);
            assert_eq!(retrieved, Some(contract));
        });
    }

    /// Issue #1714: Getting an unregistered attestor returns None.
    #[test]
    fn test_get_performance_contract_returns_none_when_not_registered() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let retrieved = attestor_performance_contracts::get_performance_contract(&env, &attestor);
            assert_eq!(retrieved, None);
        });
    }

    /// Issue #1714: Track performance against SLAs.
    #[test]
    fn test_track_performance_against_slas() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let result = attestor_performance_contracts::track_performance_against_slas(&env, &attestor);
            assert!(result.is_ok());
            assert!(attestor_performance_contracts::get_performance_tracking_status(&env, &attestor));
        });
    }

    /// Issue #1714: Performance tracking is initially disabled.
    #[test]
    fn test_performance_tracking_initially_disabled() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert!(!attestor_performance_contracts::get_performance_tracking_status(&env, &attestor));
        });
    }

    /// Issue #1714: Record performance metrics.
    #[test]
    fn test_record_performance_metric() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let metric_value = 9500_u64;

        env.as_contract(&contract_id, || {
            let result = attestor_performance_contracts::record_performance_metric(&env, &attestor, metric_value);
            assert!(result.is_ok());
            assert_eq!(attestor_performance_contracts::get_performance_metric(&env, &attestor), Some(metric_value));
        });
    }

    /// Issue #1714: Performance metric is None when not recorded.
    #[test]
    fn test_get_performance_metric_returns_none_when_not_recorded() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let metric = attestor_performance_contracts::get_performance_metric(&env, &attestor);
            assert_eq!(metric, None);
        });
    }

    /// Issue #1714: Implement breach notifications.
    #[test]
    fn test_implement_breach_notifications() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let breach_reason = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            let result = attestor_performance_contracts::implement_breach_notifications(&env, &attestor, breach_reason.clone());
            assert!(result.is_ok());
            assert_eq!(attestor_performance_contracts::get_breach_notification(&env, &attestor), Some(breach_reason));
        });
    }

    /// Issue #1714: Breach count increments with each notification.
    #[test]
    fn test_breach_count_increments() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let breach_reason1 = Bytes::new(&env, &[1, 2, 3]);
        let breach_reason2 = Bytes::new(&env, &[4, 5, 6]);

        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 0);

            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, breach_reason1).unwrap();
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 1);

            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, breach_reason2).unwrap();
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 2);
        });
    }

    /// Issue #1714: Clear breach notifications.
    #[test]
    fn test_clear_breach_notification() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let breach_reason = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, breach_reason).unwrap();
            assert_eq!(attestor_performance_contracts::get_breach_notification(&env, &attestor), Some(breach_reason));

            let result = attestor_performance_contracts::clear_breach_notification(&env, &attestor);
            assert!(result.is_ok());
            assert_eq!(attestor_performance_contracts::get_breach_notification(&env, &attestor), None);
        });
    }

    /// Issue #1714: Multiple attestors can have independent performance contracts.
    #[test]
    fn test_multiple_attestors_independent_contracts() {
        let (env, contract_id) = setup();
        let attestor1 = Address::generate(&env);
        let attestor2 = Address::generate(&env);
        let contract1 = Bytes::new(&env, &[1, 2, 3]);
        let contract2 = Bytes::new(&env, &[4, 5, 6]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor1.clone(), contract1.clone()).unwrap();
            attestor_performance_contracts::register_performance_contract(&env, attestor2.clone(), contract2.clone()).unwrap();

            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor1), Some(contract1));
            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor2), Some(contract2));
        });
    }

    /// Issue #1714: Update performance contract for existing attestor.
    #[test]
    fn test_update_performance_contract() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let contract1 = Bytes::new(&env, &[1, 2, 3]);
        let contract2 = Bytes::new(&env, &[7, 8, 9]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract1).unwrap();
            attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract2.clone()).unwrap();

            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor), Some(contract2));
        });
    }

    /// Issue #1714: Update performance metrics.
    #[test]
    fn test_update_performance_metric() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let metric1 = 9500_u64;
        let metric2 = 9800_u64;

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::record_performance_metric(&env, &attestor, metric1).unwrap();
            assert_eq!(attestor_performance_contracts::get_performance_metric(&env, &attestor), Some(metric1));

            attestor_performance_contracts::record_performance_metric(&env, &attestor, metric2).unwrap();
            assert_eq!(attestor_performance_contracts::get_performance_metric(&env, &attestor), Some(metric2));
        });
    }
}
