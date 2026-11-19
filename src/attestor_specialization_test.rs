#[cfg(test)]
mod attestor_specialization_tests {
    use crate::attestor_specialization::{self, AttestorSpecialization};
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Vec};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    fn spec(env: &Env, credential_type: &[u8], details: &[u8]) -> AttestorSpecialization {
        AttestorSpecialization {
            credential_type: Bytes::from_slice(env, credential_type),
            details: Bytes::from_slice(env, details),
        }
    }

    /// Issue #1717 / #2102: Register a specialization and read it back.
    #[test]
    fn test_register_and_get_attestor_specialization() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let s = spec(&env, &[1, 2], &[3, 4]);

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor.clone(), s.clone()).unwrap();
            assert_eq!(
                attestor_specialization::get_attestor_specialization(&env, &attestor),
                Some(s)
            );
        });
    }

    /// Issue #1717: Getting an unregistered attestor returns None.
    #[test]
    fn test_get_attestor_specialization_returns_none_when_not_registered() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(attestor_specialization::get_attestor_specialization(&env, &attestor), None);
        });
    }

    /// Issue #1717: Re-registering replaces the attestor's specialization and
    /// does not duplicate it in the matching index.
    #[test]
    fn test_update_attestor_specialization_replaces_value() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let spec1 = spec(&env, &[1, 1], &[1]);
        let spec2 = spec(&env, &[5, 5], &[2]);

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor.clone(), spec1.clone()).unwrap();
            attestor_specialization::register_attestor_specialization(&env, attestor.clone(), spec2.clone()).unwrap();
            assert_eq!(
                attestor_specialization::get_attestor_specialization(&env, &attestor),
                Some(spec2.clone())
            );

            // Matching on the old type finds nothing; the new type finds the attestor once.
            let old = attestor_specialization::match_credentials_to_specialized_attestors(&env, &spec1.credential_type);
            assert_eq!(old.len(), 0);
            let new = attestor_specialization::match_credentials_to_specialized_attestors(&env, &spec2.credential_type);
            assert_eq!(new.len(), 1);
        });
    }

    /// Issue #1717: Registry initialization is idempotent.
    #[test]
    fn test_create_specialization_registry_idempotent() {
        let (env, contract_id) = setup();

        env.as_contract(&contract_id, || {
            assert!(attestor_specialization::create_specialization_registry(&env).is_ok());
            assert!(attestor_specialization::create_specialization_registry(&env).is_ok());
        });
    }

    /// Issue #2102: Register two attestors with different specializations and
    /// check that matching returns only the attestor whose type matches.
    #[test]
    fn test_match_credentials_returns_matching_attestor() {
        let (env, contract_id) = setup();
        let attestor_a = Address::generate(&env);
        let attestor_b = Address::generate(&env);
        let kyc = spec(&env, b"kyc", b"identity");
        let income = spec(&env, b"income", b"payroll");

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor_a.clone(), kyc.clone()).unwrap();
            attestor_specialization::register_attestor_specialization(&env, attestor_b.clone(), income).unwrap();

            let matched = attestor_specialization::match_credentials_to_specialized_attestors(
                &env,
                &kyc.credential_type,
            );
            assert_eq!(matched, Vec::from_array(&env, [attestor_a.clone()]));
        });
    }

    /// Issue #2102: Matching returns every attestor sharing the credential type.
    #[test]
    fn test_match_returns_all_attestors_with_same_type() {
        let (env, contract_id) = setup();
        let attestor_a = Address::generate(&env);
        let attestor_b = Address::generate(&env);
        let kyc = spec(&env, b"kyc", b"a");

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor_a.clone(), kyc.clone()).unwrap();
            attestor_specialization::register_attestor_specialization(&env, attestor_b.clone(), kyc.clone()).unwrap();

            let matched = attestor_specialization::match_credentials_to_specialized_attestors(
                &env,
                &kyc.credential_type,
            );
            assert_eq!(matched, Vec::from_array(&env, [attestor_a, attestor_b]));
        });
    }

    /// Issue #2102: No registered specialization matches an unknown type.
    #[test]
    fn test_match_returns_empty_when_no_attestor_matches() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            attestor_specialization::register_attestor_specialization(&env, attestor, spec(&env, b"kyc", b"x")).unwrap();
            let matched = attestor_specialization::match_credentials_to_specialized_attestors(
                &env,
                &Bytes::from_slice(&env, b"unknown"),
            );
            assert_eq!(matched.len(), 0);
        });
    }

    /// Issue #2004: Register through the contract entrypoint and read back via the getter.
    #[test]
    fn test_contract_register_attestor_records_specialization() {
        let (env, contract_id) = setup();
        let attestor = Address::generate(&env);
        let s = spec(&env, b"kyc", b"contract");
        let client = crate::QuorumCreditContractClient::new(&env, &contract_id);

        client.register_attestor_specialization(&attestor, &s);
        assert_eq!(client.get_attestor_specialization(&attestor), Some(s));
    }
}
