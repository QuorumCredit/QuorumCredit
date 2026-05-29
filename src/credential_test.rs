#[cfg(test)]
mod credential_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use crate::errors::ContractError;
    use crate::types::CredentialStatus;
    use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

    fn setup(env: &Env) -> (Address, Address, QuorumCreditContractClient) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(env))
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token);
        (admin, deployer, client)
    }

    // ── issue_credential ──────────────────────────────────────────────────────

    #[test]
    fn test_issue_credential_returns_id() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(
            &admin,
            &holder,
            &String::from_str(&env, "KYC"),
            &None,
        );
        assert_eq!(id, 1);
    }

    #[test]
    fn test_issue_credential_increments_id() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id1 = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        let id2 = client.issue_credential(&admin, &holder, &String::from_str(&env, "AML"), &None);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_issue_credential_non_admin_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (_admin, _deployer, client) = setup(&env);
        let non_admin = Address::generate(&env);
        let holder = Address::generate(&env);

        let result = client.try_issue_credential(
            &non_admin,
            &holder,
            &String::from_str(&env, "KYC"),
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::UnauthorizedCaller)));
    }

    // ── get_credentials ───────────────────────────────────────────────────────

    #[test]
    fn test_get_credentials_shows_all_statuses() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id1 = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        let id2 = client.issue_credential(&admin, &holder, &String::from_str(&env, "AML"), &None);
        client.suspend_credential(&admin, &holder, &id2);

        let creds = client.get_credentials(&holder);
        assert_eq!(creds.len(), 2);

        let c1 = creds.get(0).unwrap();
        let c2 = creds.get(1).unwrap();
        assert_eq!(c1.id, id1);
        assert_eq!(c1.status, CredentialStatus::Active);
        assert_eq!(c2.id, id2);
        assert_eq!(c2.status, CredentialStatus::Suspended);
    }

    #[test]
    fn test_get_credentials_shows_attestor() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        let creds = client.get_credentials(&holder);
        assert_eq!(creds.get(0).unwrap().attestor, admin);
    }

    #[test]
    fn test_get_credentials_shows_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let expiry: u64 = 9_999_999_999;
        client.issue_credential(
            &admin,
            &holder,
            &String::from_str(&env, "KYC"),
            &Some(expiry),
        );
        let creds = client.get_credentials(&holder);
        assert_eq!(creds.get(0).unwrap().expiry_timestamp, Some(expiry));
    }

    // ── revoke_credential ─────────────────────────────────────────────────────

    #[test]
    fn test_revoke_credential_sets_revoked_status() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.revoke_credential(&admin, &holder, &id);

        let creds = client.get_credentials(&holder);
        assert_eq!(creds.get(0).unwrap().status, CredentialStatus::Revoked);
    }

    #[test]
    fn test_revoke_already_revoked_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.revoke_credential(&admin, &holder, &id);

        let result = client.try_revoke_credential(&admin, &holder, &id);
        assert_eq!(result, Err(Ok(ContractError::CredentialAlreadyRevoked)));
    }

    #[test]
    fn test_revoke_nonexistent_credential_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let result = client.try_revoke_credential(&admin, &holder, &999);
        assert_eq!(result, Err(Ok(ContractError::CredentialNotFound)));
    }

    // ── suspend_credential ────────────────────────────────────────────────────

    #[test]
    fn test_suspend_credential_sets_suspended_status() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.suspend_credential(&admin, &holder, &id);

        let creds = client.get_credentials(&holder);
        assert_eq!(creds.get(0).unwrap().status, CredentialStatus::Suspended);
    }

    #[test]
    fn test_suspend_revoked_credential_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.revoke_credential(&admin, &holder, &id);

        let result = client.try_suspend_credential(&admin, &holder, &id);
        assert_eq!(result, Err(Ok(ContractError::CredentialAlreadyRevoked)));
    }

    #[test]
    fn test_suspend_already_suspended_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.suspend_credential(&admin, &holder, &id);

        let result = client.try_suspend_credential(&admin, &holder, &id);
        assert_eq!(result, Err(Ok(ContractError::CredentialStatusUnchanged)));
    }

    // ── activate_credential ───────────────────────────────────────────────────

    #[test]
    fn test_activate_suspended_credential() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.suspend_credential(&admin, &holder, &id);
        client.activate_credential(&admin, &holder, &id);

        let creds = client.get_credentials(&holder);
        assert_eq!(creds.get(0).unwrap().status, CredentialStatus::Active);
    }

    #[test]
    fn test_activate_revoked_credential_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        client.revoke_credential(&admin, &holder, &id);

        let result = client.try_activate_credential(&admin, &holder, &id);
        assert_eq!(result, Err(Ok(ContractError::CredentialAlreadyRevoked)));
    }

    #[test]
    fn test_activate_already_active_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        let id = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        let result = client.try_activate_credential(&admin, &holder, &id);
        assert_eq!(result, Err(Ok(ContractError::CredentialStatusUnchanged)));
    }

    // ── export_credentials ────────────────────────────────────────────────────

    #[test]
    fn test_export_credentials_only_active_non_expired() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        // Active, no expiry
        let id1 = client.issue_credential(&admin, &holder, &String::from_str(&env, "KYC"), &None);
        // Active, future expiry
        let id2 = client.issue_credential(
            &admin,
            &holder,
            &String::from_str(&env, "AML"),
            &Some(9_999_999_999u64),
        );
        // Suspended
        let id3 = client.issue_credential(&admin, &holder, &String::from_str(&env, "CREDIT"), &None);
        client.suspend_credential(&admin, &holder, &id3);
        // Revoked
        let id4 = client.issue_credential(&admin, &holder, &String::from_str(&env, "ID"), &None);
        client.revoke_credential(&admin, &holder, &id4);

        let exported = client.export_credentials(&holder);
        // Only id1 and id2 should appear
        assert_eq!(exported.len(), 2);
        assert_eq!(exported.get(0).unwrap().id, id1);
        assert_eq!(exported.get(1).unwrap().id, id2);
    }

    #[test]
    fn test_export_credentials_excludes_expired() {
        let env = Env::default();
        env.mock_all_auths();
        let (admin, _deployer, client) = setup(&env);
        let holder = Address::generate(&env);

        // Expired: expiry in the past (timestamp 1 is always in the past)
        client.issue_credential(
            &admin,
            &holder,
            &String::from_str(&env, "KYC"),
            &Some(1u64),
        );

        let exported = client.export_credentials(&holder);
        assert_eq!(exported.len(), 0);
    }

    // ── Validation helpers ────────────────────────────────────────────────────

    #[test]
    fn test_validate_amount_rejects_zero() {
        use crate::helpers::validate_amount;
        let env = Env::default();
        let result = validate_amount(&env, 0);
        assert_eq!(result, Err(ContractError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_rejects_negative() {
        use crate::helpers::validate_amount;
        let env = Env::default();
        let result = validate_amount(&env, -1);
        assert_eq!(result, Err(ContractError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_accepts_positive() {
        use crate::helpers::validate_amount;
        let env = Env::default();
        assert!(validate_amount(&env, 1).is_ok());
        assert!(validate_amount(&env, i128::MAX).is_ok());
    }

    #[test]
    fn test_validate_timestamp_rejects_zero() {
        use crate::helpers::validate_timestamp;
        let env = Env::default();
        assert_eq!(
            validate_timestamp(&env, 0, 100),
            Err(ContractError::InvalidAmount)
        );
    }

    #[test]
    fn test_validate_timestamp_rejects_past() {
        use crate::helpers::validate_timestamp;
        let env = Env::default();
        // timestamp 50 is in the past relative to now=100
        assert_eq!(
            validate_timestamp(&env, 50, 100),
            Err(ContractError::InvalidAmount)
        );
    }

    #[test]
    fn test_validate_timestamp_accepts_future() {
        use crate::helpers::validate_timestamp;
        let env = Env::default();
        assert!(validate_timestamp(&env, 200, 100).is_ok());
    }

    // ── Reentrancy guard ──────────────────────────────────────────────────────

    #[test]
    fn test_reentrancy_guard_blocks_second_acquire() {
        use crate::helpers::{acquire_lock, release_lock};
        let env = Env::default();
        // First acquire succeeds
        assert!(acquire_lock(&env).is_ok());
        // Second acquire while locked must fail
        assert_eq!(acquire_lock(&env), Err(ContractError::Reentrancy));
        // After release, acquire succeeds again
        release_lock(&env);
        assert!(acquire_lock(&env).is_ok());
        release_lock(&env);
    }
}
