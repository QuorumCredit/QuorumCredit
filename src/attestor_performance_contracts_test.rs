#[cfg(test)]
mod attestor_performance_contracts_tests {
    use crate::attestor_performance_contracts;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    // The host allows a single `require_auth` per address within one contract
    // frame, so each authorized write runs in its own `as_contract` frame. The
    // module itself calls `require_auth` at most once per entrypoint.
    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    fn bytes(env: &Env, data: &[u8]) -> Bytes {
        Bytes::from_slice(env, data)
    }

    /// Issue #1714: Register a performance contract for an attestor.
    #[test]
    fn test_register_performance_contract_succeeds() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let contract = bytes(&env, &[1, 2, 3, 4]);

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
        let contract = bytes(&env, &[1, 2, 3, 4]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract.clone()).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(
                attestor_performance_contracts::get_performance_contract(&env, &attestor),
                Some(contract)
            );
        });
    }

    /// Issue #1714: Getting an unregistered contract returns None.
    #[test]
    fn test_get_performance_contract_returns_none_when_not_registered() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor), None);
        });
    }

    /// Issue #1714: Tracking is enabled by `track_performance_against_slas`.
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

    /// Issue #1714: Tracking is disabled by default.
    #[test]
    fn test_performance_tracking_initially_disabled() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert!(!attestor_performance_contracts::get_performance_tracking_status(&env, &attestor));
        });
    }

    /// Issue #1714: Recording a metric stores it.
    #[test]
    fn test_record_performance_metric() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::record_performance_metric(&env, &attestor, 95).unwrap();
            assert_eq!(attestor_performance_contracts::get_performance_metric(&env, &attestor), Some(95));
        });
    }

    /// Issue #1714: No metric is returned before one is recorded.
    #[test]
    fn test_get_performance_metric_returns_none_when_not_recorded() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_performance_metric(&env, &attestor), None);
        });
    }

    /// Issue #1714: Recording a breach stores its reason.
    #[test]
    fn test_implement_breach_notifications() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let breach_reason = bytes(&env, &[9, 9]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, breach_reason.clone()).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(
                attestor_performance_contracts::get_breach_notification(&env, &attestor),
                Some(breach_reason)
            );
        });
    }

    /// Issue #1714: Each breach notification increments the breach count.
    #[test]
    fn test_breach_count_increments() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 0);
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, bytes(&env, &[1])).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 1);
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, bytes(&env, &[2])).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 2);
        });
    }

    /// Issue #1714: Clearing removes the breach notification.
    #[test]
    fn test_clear_breach_notification() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::implement_breach_notifications(&env, &attestor, bytes(&env, &[5])).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::clear_breach_notification(&env, &attestor).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_notification(&env, &attestor), None);
        });
    }

    /// Issue #1714: Contracts for different attestors are stored independently.
    #[test]
    fn test_multiple_attestors_independent_contracts() {
        let (env, contract_id) = setup();
        let attestor1 = Address::generate(&env);
        let attestor2 = Address::generate(&env);
        let contract1 = bytes(&env, &[1]);
        let contract2 = bytes(&env, &[2]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor1.clone(), contract1.clone()).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor2.clone(), contract2.clone()).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor1), Some(contract1));
            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor2), Some(contract2));
        });
    }

    /// Issue #1714: Re-registering a contract replaces the previous one.
    #[test]
    fn test_update_performance_contract() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let contract1 = bytes(&env, &[1]);
        let contract2 = bytes(&env, &[2]);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract1).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::register_performance_contract(&env, attestor.clone(), contract2.clone()).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_performance_contract(&env, &attestor), Some(contract2));
        });
    }

    /// Issue #1714: Re-recording a metric replaces the previous value.
    #[test]
    fn test_update_performance_metric() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::record_performance_metric(&env, &attestor, 10).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::record_performance_metric(&env, &attestor, 20).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_performance_metric(&env, &attestor), Some(20));
        });
    }

    /// Issue #2103: A metric below the SLA threshold is recorded as a breach
    /// when the metric is tracked against the SLA.
    #[test]
    fn test_metric_crossing_sla_threshold_records_breach() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::set_performance_sla(&env, attestor.clone(), 90).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::record_performance_metric(&env, &attestor, 75).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 0);
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::track_performance_against_slas(&env, &attestor).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 1);
            assert_eq!(
                attestor_performance_contracts::get_breach_notification(&env, &attestor),
                Some(bytes(&env, b"sla_below_threshold"))
            );
            assert!(attestor_performance_contracts::get_performance_tracking_status(&env, &attestor));
        });
    }

    /// Issue #2103: A metric at the SLA threshold records no breach.
    #[test]
    fn test_metric_meeting_sla_records_no_breach() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::set_performance_sla(&env, attestor.clone(), 90).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::record_performance_metric(&env, &attestor, 90).unwrap();
        });
        env.as_contract(&contract_id, || {
            attestor_performance_contracts::track_performance_against_slas(&env, &attestor).unwrap();
        });
        env.as_contract(&contract_id, || {
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 0);
            assert_eq!(attestor_performance_contracts::get_breach_notification(&env, &attestor), None);
        });
    }

    /// Issue #2103: Tracking without an SLA, or without a metric, cannot breach.
    #[test]
    fn test_track_without_sla_or_metric_records_no_breach() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_performance_contracts::track_performance_against_slas(&env, &attestor).unwrap();
            assert_eq!(attestor_performance_contracts::get_breach_count(&env, &attestor), 0);
        });
    }

    /// Issue #2103: Writes require the attestor's authorization.
    #[test]
    #[should_panic]
    fn test_record_metric_without_attestor_auth_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let _ = attestor_performance_contracts::record_performance_metric(&env, &attestor, 50);
        });
    }
}
