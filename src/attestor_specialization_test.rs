#[cfg(test)]
mod attestor_specialization_tests {
    use crate::attestor_specialization;
    use soroban_sdk::{testutils::Address as _, Bytes, Env, Address};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1717: Register an attestor with a specialization.
    #[test]
    fn test_register_attestor_specialization_succeeds() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let spec = Bytes::new(&env, &[1, 2, 3, 4]);

        env.as_contract(&contract_id, || {
            let result = attestor_specialization::register_attestor_specialization(&env, attestor.clone(), spec.clone());
            assert!(result.is_ok());
        });
    }

    /// Issue #1717: Retrieve a registered attestor specialization.
    #[test]
    fn test_get_attestor_specialization_returns_registered_spec() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let spec = Bytes::new(&env, &[1, 2, 3, 4]);

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor.clone(), spec.clone()).unwrap();
            let retrieved = attestor_specialization::get_attestor_specialization(&env, &attestor);
            assert_eq!(retrieved, Some(spec));
        });
    }

    /// Issue #1717: Getting an unregistered attestor returns None.
    #[test]
    fn test_get_attestor_specialization_returns_none_when_not_registered() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let retrieved = attestor_specialization::get_attestor_specialization(&env, &attestor);
            assert_eq!(retrieved, None);
        });
    }

    /// Issue #1717: Update specialization for an existing attestor.
    #[test]
    fn test_update_attestor_specialization() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let spec1 = Bytes::new(&env, &[1, 2, 3]);
        let spec2 = Bytes::new(&env, &[5, 6, 7]);

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor.clone(), spec1).unwrap();
            attestor_specialization::register_attestor_specialization(&env, attestor.clone(), spec2.clone()).unwrap();
            let retrieved = attestor_specialization::get_attestor_specialization(&env, &attestor);
            assert_eq!(retrieved, Some(spec2));
        });
    }

    /// Issue #1717: Initialize the specialization registry.
    #[test]
    fn test_create_specialization_registry_succeeds() {
        let (env, contract_id) = setup();

        env.as_contract(&contract_id, || {
            let result = attestor_specialization::create_specialization_registry(&env);
            assert!(result.is_ok());
        });
    }

    /// Issue #1717: Registry initialization is idempotent.
    #[test]
    fn test_create_specialization_registry_idempotent() {
        let (env, contract_id) = setup();

        env.as_contract(&contract_id, || {
            let result1 = attestor_specialization::create_specialization_registry(&env);
            let result2 = attestor_specialization::create_specialization_registry(&env);
            assert!(result1.is_ok());
            assert!(result2.is_ok());
        });
    }

    /// Issue #1717: Multiple attestors can have different specializations.
    #[test]
    fn test_multiple_attestors_different_specializations() {
        let (env, contract_id) = setup();
        let attestor1 = Address::generate(&env);
        let attestor2 = Address::generate(&env);
        let spec1 = Bytes::new(&env, &[1, 2, 3]);
        let spec2 = Bytes::new(&env, &[4, 5, 6]);

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor1.clone(), spec1.clone()).unwrap();
            attestor_specialization::register_attestor_specialization(&env, attestor2.clone(), spec2.clone()).unwrap();

            assert_eq!(attestor_specialization::get_attestor_specialization(&env, &attestor1), Some(spec1));
            assert_eq!(attestor_specialization::get_attestor_specialization(&env, &attestor2), Some(spec2));
        });
    }

    /// Issue #1717: Match credentials to specialized attestors (basic test).
    #[test]
    fn test_match_credentials_to_specialized_attestors() {
        let (env, contract_id) = setup();
        let credential_type = Bytes::new(&env, &[10, 20, 30]);

        env.as_contract(&contract_id, || {
            let matched = attestor_specialization::match_credentials_to_specialized_attestors(&env, &credential_type);
            assert_eq!(matched.len(), 0);
        });
    }
}
