#[cfg(test)]
mod quorum_slice_migration_tests {
    use crate::quorum_slice_migration;
    use soroban_sdk::{testutils::Address as _, Env, Address};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1713: Migrate a credential from one slice to another.
    #[test]
    fn test_migrate_credential_slice_succeeds() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let old_slice = Address::generate(&env);
        let new_slice = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice, true).unwrap();
            let result = quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &old_slice, &new_slice);
            assert!(result.is_ok());
        });
    }

    /// Issue #1713: Get the current slice of a credential.
    #[test]
    fn test_get_credential_current_slice() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let old_slice = Address::generate(&env);
        let new_slice = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice, true).unwrap();
            quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &old_slice, &new_slice.clone()).unwrap();

            let current_slice = quorum_slice_migration::get_credential_current_slice(&env, &credential_id);
            assert_eq!(current_slice, Some(new_slice));
        });
    }

    /// Issue #1713: Current slice is None before migration.
    #[test]
    fn test_get_credential_current_slice_returns_none_before_migration() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let current_slice = quorum_slice_migration::get_credential_current_slice(&env, &credential_id);
            assert_eq!(current_slice, None);
        });
    }

    /// Issue #1713: Maintain migration history.
    #[test]
    fn test_maintain_migration_history() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let old_slice = Address::generate(&env);
        let new_slice1 = Address::generate(&env);
        let new_slice2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice1, true).unwrap();
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice2, true).unwrap();

            assert_eq!(quorum_slice_migration::get_migration_history_count(&env, &credential_id), 0);

            quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &old_slice, &new_slice1).unwrap();
            assert_eq!(quorum_slice_migration::get_migration_history_count(&env, &credential_id), 1);

            quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &new_slice1, &new_slice2).unwrap();
            assert_eq!(quorum_slice_migration::get_migration_history_count(&env, &credential_id), 2);
        });
    }

    /// Issue #1713: Verify slice compatibility before migration.
    #[test]
    fn test_verify_slice_compatibility_returns_true_by_default() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let new_slice = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let compatible = quorum_slice_migration::verify_slice_compatibility(&env, &credential_id, &new_slice);
            assert!(compatible);
        });
    }

    /// Issue #1713: Set and verify slice compatibility.
    #[test]
    fn test_set_slice_compatibility_incompatible() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let new_slice = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice, false).unwrap();
            let compatible = quorum_slice_migration::verify_slice_compatibility(&env, &credential_id, &new_slice);
            assert!(!compatible);
        });
    }

    /// Issue #1713: Implement slice migration protocol with compatibility check.
    #[test]
    fn test_implement_slice_migration_protocol_succeeds_when_compatible() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let old_slice = Address::generate(&env);
        let new_slice = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice, true).unwrap();
            let result = quorum_slice_migration::implement_slice_migration_protocol(&env, &credential_id, &old_slice, &new_slice);
            assert!(result.is_ok());
        });
    }

    /// Issue #1713: Implement slice migration protocol fails when incompatible.
    #[test]
    fn test_implement_slice_migration_protocol_fails_when_incompatible() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let old_slice = Address::generate(&env);
        let new_slice = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice, false).unwrap();
            let result = quorum_slice_migration::implement_slice_migration_protocol(&env, &credential_id, &old_slice, &new_slice);
            assert!(result.is_err());
        });
    }

    /// Issue #1713: Multiple credentials can migrate independently.
    #[test]
    fn test_multiple_credentials_independent_migration() {
        let (env, contract_id) = setup();
        let credential1 = Address::generate(&env);
        let credential2 = Address::generate(&env);
        let old_slice = Address::generate(&env);
        let new_slice1 = Address::generate(&env);
        let new_slice2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice1, true).unwrap();
            quorum_slice_migration::set_slice_compatibility(&env, &new_slice2, true).unwrap();

            quorum_slice_migration::migrate_credential_slice(&env, &credential1, &old_slice, &new_slice1.clone()).unwrap();
            quorum_slice_migration::migrate_credential_slice(&env, &credential2, &old_slice, &new_slice2.clone()).unwrap();

            assert_eq!(quorum_slice_migration::get_credential_current_slice(&env, &credential1), Some(new_slice1));
            assert_eq!(quorum_slice_migration::get_credential_current_slice(&env, &credential2), Some(new_slice2));
        });
    }

    /// Issue #1713: Chain migrations maintain history.
    #[test]
    fn test_chain_migrations_maintain_history() {
        let (env, contract_id) = setup();
        let credential_id = Address::generate(&env);
        let slice1 = Address::generate(&env);
        let slice2 = Address::generate(&env);
        let slice3 = Address::generate(&env);
        let slice4 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            quorum_slice_migration::set_slice_compatibility(&env, &slice2, true).unwrap();
            quorum_slice_migration::set_slice_compatibility(&env, &slice3, true).unwrap();
            quorum_slice_migration::set_slice_compatibility(&env, &slice4, true).unwrap();

            quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &slice1, &slice2).unwrap();
            quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &slice2, &slice3).unwrap();
            quorum_slice_migration::migrate_credential_slice(&env, &credential_id, &slice3, &slice4).unwrap();

            assert_eq!(quorum_slice_migration::get_credential_current_slice(&env, &credential_id), Some(slice4));
            assert_eq!(quorum_slice_migration::get_migration_history_count(&env, &credential_id), 3);
        });
    }
}
