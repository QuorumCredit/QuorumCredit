#[cfg(test)]
mod quorum_slice_rebalancing_tests {
    use crate::quorum_slice_rebalancing;
    use soroban_sdk::{testutils::Address as _, Bytes, Env, Address};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1716: Enable dynamic rebalancing for a quorum slice.
    #[test]
    fn test_enable_dynamic_rebalancing_succeeds() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            let result = quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params);
            assert!(result.is_ok());
            assert!(quorum_slice_rebalancing::is_rebalancing_enabled(&env, &slice_id));
        });
    }

    /// Issue #1716: Rebalancing is initially disabled.
    #[test]
    fn test_rebalancing_initially_disabled() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert!(!quorum_slice_rebalancing::is_rebalancing_enabled(&env, &slice_id));
        });
    }

    /// Issue #1716: Disable dynamic rebalancing.
    #[test]
    fn test_disable_dynamic_rebalancing() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params).unwrap();
            assert!(quorum_slice_rebalancing::is_rebalancing_enabled(&env, &slice_id));

            let result = quorum_slice_rebalancing::disable_dynamic_rebalancing(&env, &slice_id);
            assert!(result.is_ok());
            assert!(!quorum_slice_rebalancing::is_rebalancing_enabled(&env, &slice_id));
        });
    }

    /// Issue #1716: Store and retrieve rebalancing parameters.
    #[test]
    fn test_get_rebalancing_parameters() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params = Bytes::new(&env, &[5, 10, 15]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params.clone()).unwrap();
            let retrieved = quorum_slice_rebalancing::get_rebalancing_parameters(&env, &slice_id);
            assert_eq!(retrieved, Some(params));
        });
    }

    /// Issue #1716: Parameters are None when rebalancing not enabled.
    #[test]
    fn test_get_rebalancing_parameters_returns_none_when_disabled() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let retrieved = quorum_slice_rebalancing::get_rebalancing_parameters(&env, &slice_id);
            assert_eq!(retrieved, None);
        });
    }

    /// Issue #1716: Execute rebalancing when enabled.
    #[test]
    fn test_execute_rebalancing_when_enabled() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params).unwrap();
            let result = quorum_slice_rebalancing::execute_rebalancing(&env, &slice_id);
            assert!(result.is_ok());
        });
    }

    /// Issue #1716: Rebalancing succeeds even when disabled.
    #[test]
    fn test_execute_rebalancing_when_disabled() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let result = quorum_slice_rebalancing::execute_rebalancing(&env, &slice_id);
            assert!(result.is_ok());
        });
    }

    /// Issue #1716: Track rebalancing events.
    #[test]
    fn test_track_rebalancing_events() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params).unwrap();

            assert_eq!(quorum_slice_rebalancing::get_rebalancing_event_count(&env, &slice_id), 0);

            quorum_slice_rebalancing::execute_rebalancing(&env, &slice_id).unwrap();
            assert_eq!(quorum_slice_rebalancing::get_rebalancing_event_count(&env, &slice_id), 1);

            quorum_slice_rebalancing::execute_rebalancing(&env, &slice_id).unwrap();
            assert_eq!(quorum_slice_rebalancing::get_rebalancing_event_count(&env, &slice_id), 2);
        });
    }

    /// Issue #1716: Implement periodic rebalancing.
    #[test]
    fn test_implement_periodic_rebalancing() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params = Bytes::new(&env, &[1, 2, 3]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params).unwrap();
            let result = quorum_slice_rebalancing::implement_periodic_rebalancing(&env, &slice_id);
            assert!(result.is_ok());
        });
    }

    /// Issue #1716: Multiple slices can have independent rebalancing configs.
    #[test]
    fn test_multiple_slices_independent_rebalancing() {
        let (env, contract_id) = setup();
        let slice1 = Address::generate(&env);
        let slice2 = Address::generate(&env);
        let params1 = Bytes::new(&env, &[1, 2, 3]);
        let params2 = Bytes::new(&env, &[4, 5, 6]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice1, params1.clone()).unwrap();
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice2, params2.clone()).unwrap();

            assert!(quorum_slice_rebalancing::is_rebalancing_enabled(&env, &slice1));
            assert!(quorum_slice_rebalancing::is_rebalancing_enabled(&env, &slice2));

            assert_eq!(quorum_slice_rebalancing::get_rebalancing_parameters(&env, &slice1), Some(params1));
            assert_eq!(quorum_slice_rebalancing::get_rebalancing_parameters(&env, &slice2), Some(params2));
        });
    }

    /// Issue #1716: Update rebalancing parameters.
    #[test]
    fn test_update_rebalancing_parameters() {
        let (env, contract_id) = setup();
        let slice_id = Address::generate(&env);
        let params1 = Bytes::new(&env, &[1, 2, 3]);
        let params2 = Bytes::new(&env, &[7, 8, 9]);

        env.as_contract(&contract_id, || {
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params1).unwrap();
            quorum_slice_rebalancing::enable_dynamic_rebalancing(&env, &slice_id, params2.clone()).unwrap();

            assert_eq!(quorum_slice_rebalancing::get_rebalancing_parameters(&env, &slice_id), Some(params2));
        });
    }
}
