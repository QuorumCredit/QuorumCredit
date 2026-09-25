#[cfg(test)]
mod tests {
    use soroban_sdk::{symbol_short, vec, Env, Address, BytesN, String};
    use crate::*;

    fn setup_env_and_contract() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        (env, contract_id)
    }

    fn create_test_addresses(env: &Env, count: usize) -> Vec<Address> {
        let mut addresses = vec![env];
        for i in 0..count {
            addresses.push_back(Address::generate(env));
        }
        addresses
    }

    // ── Issue #1599: Multi-Signature Endorsement Tests ─────────────────────

    #[test]
    fn test_create_multi_signed_credential() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let holder = Address::generate(&env);
        let issuer = Address::generate(&env);
        let signers = create_test_addresses(&env, 3);

        let metadata = String::from_str(&env, "Test Credential");
        let expires_at = env.ledger().timestamp() + 86400 * 365;

        let result = client.create_multi_signed_credential(
            &holder,
            &issuer,
            &signers,
            2,
            &metadata,
            expires_at,
        );

        assert!(result.is_ok());
        let credential = result.unwrap();
        assert_eq!(credential.holder, holder);
        assert_eq!(credential.issuer, issuer);
        assert_eq!(credential.threshold, 2);
        assert_eq!(credential.signers.len(), 3);
    }

    #[test]
    fn test_verify_credential_signatures() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let holder = Address::generate(&env);
        let issuer = Address::generate(&env);
        let signers = create_test_addresses(&env, 3);

        let metadata = String::from_str(&env, "Test Credential");
        let expires_at = env.ledger().timestamp() + 86400 * 365;

        let credential = client
            .create_multi_signed_credential(&holder, &issuer, &signers, 2, &metadata, expires_at)
            .unwrap();

        // Create signatures
        let mut signatures = vec![&env];
        for i in 0..2 {
            let sig = credential::Signature {
                signer: signers.get(i).unwrap(),
                signed_at: env.ledger().timestamp(),
                signature_data: BytesN::from_array(&env, &[0u8; 64]),
            };
            signatures.push_back(sig);
        }

        let result = client.verify_credential_signatures(&credential.id, &signatures);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_get_holder_credentials() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let holder = Address::generate(&env);
        let issuer = Address::generate(&env);
        let signers = create_test_addresses(&env, 2);

        let metadata = String::from_str(&env, "Test Credential");
        let expires_at = env.ledger().timestamp() + 86400 * 365;

        // Create two credentials for same holder
        let cred1 = client
            .create_multi_signed_credential(&holder, &issuer, &signers, 2, &metadata, expires_at)
            .unwrap();

        let cred2 = client
            .create_multi_signed_credential(&holder, &issuer, &signers, 2, &metadata, expires_at)
            .unwrap();

        let credentials = client.get_holder_credentials(&holder);
        assert_eq!(credentials.len(), 2);
    }

    // ── Issue #1603: Rate Limiting Tests ──────────────────────────────────

    #[test]
    fn test_set_verification_rate_limit() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let holder = Address::generate(&env);

        let result = client.set_verification_rate_limit(&admin, &holder, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_verification_rate_limit() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let holder = Address::generate(&env);

        // Set rate limit first
        client
            .set_verification_rate_limit(&admin, &holder, 10)
            .unwrap();

        // Should allow verification
        let result = client.check_verification_rate_limit(&holder);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Get token count
        let tokens = client.get_verification_tokens(&holder).unwrap();
        assert!(tokens > 0);
    }

    // ── Issue #1605: Metadata Tamper Detection Tests ──────────────────────

    #[test]
    fn test_create_versioned_metadata() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let credential_id = BytesN::from_array(&env, &[1u8; 32]);
        let metadata = String::from_str(&env, "Sensitive Credential Data");
        let issuer = Address::generate(&env);

        let result = client.create_versioned_metadata(&credential_id, &metadata, &issuer);
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_metadata_tampering() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let credential_id = BytesN::from_array(&env, &[2u8; 32]);
        let metadata = String::from_str(&env, "Original Metadata");
        let issuer = Address::generate(&env);

        client
            .create_versioned_metadata(&credential_id, &metadata, &issuer)
            .unwrap();

        // Detect tampering (should be false initially)
        let result = client.detect_metadata_tampering(&credential_id);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_get_forensic_history() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let credential_id = BytesN::from_array(&env, &[3u8; 32]);
        let metadata = String::from_str(&env, "Test Metadata");
        let issuer = Address::generate(&env);

        client
            .create_versioned_metadata(&credential_id, &metadata, &issuer)
            .unwrap();

        let result = client.get_forensic_history(&credential_id);
        assert!(result.is_ok());
        let history = result.unwrap();
        assert_eq!(history.credential_id, credential_id);
    }

    // ── Issue #1606: Quorum Slice Self-Healing Tests ──────────────────────

    #[test]
    fn test_create_quorum_slice() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let validators = create_test_addresses(&env, 5);
        let result = client.create_quorum_slice(1, &validators, 3);

        assert!(result.is_ok());
    }

    #[test]
    fn test_enable_slice_self_healing() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let validators = create_test_addresses(&env, 5);

        client.create_quorum_slice(2, &validators, 3).unwrap();

        let result = client.enable_slice_self_healing(&admin, 2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_slice_failures() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let validators = create_test_addresses(&env, 5);
        client.create_quorum_slice(3, &validators, 3).unwrap();

        let result = client.detect_slice_failures(3);
        assert!(result.is_ok());
    }

    #[test]
    fn test_replace_failed_attestor() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let validators = create_test_addresses(&env, 5);

        client.create_quorum_slice(4, &validators, 3).unwrap();
        client.enable_slice_self_healing(&admin, 4).unwrap();

        let failed = validators.get(0).unwrap();
        let replacement = Address::generate(&env);

        let result = client.replace_failed_attestor(4, &failed, &replacement);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_quorum_status() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let validators = create_test_addresses(&env, 5);
        client.create_quorum_slice(5, &validators, 3).unwrap();

        let result = client.get_quorum_status(5);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Should be healthy initially
    }

    #[test]
    fn test_get_slice_healing_events() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let validators = create_test_addresses(&env, 5);

        client.create_quorum_slice(6, &validators, 3).unwrap();
        client.enable_slice_self_healing(&admin, 6).unwrap();

        let failed = validators.get(0).unwrap();
        let replacement = Address::generate(&env);
        client.replace_failed_attestor(6, &failed, &replacement).unwrap();

        let events = client.get_slice_healing_events(6);
        assert_eq!(events.len(), 1);
    }

    // ── Integration Tests ────────────────────────────────────────────────

    #[test]
    fn test_credential_with_rate_limiting() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let holder = Address::generate(&env);
        let issuer = Address::generate(&env);
        let signers = create_test_addresses(&env, 3);

        // Set up rate limiting
        client
            .set_verification_rate_limit(&admin, &holder, 5)
            .unwrap();

        // Create credential
        let metadata = String::from_str(&env, "Rate-Limited Credential");
        let expires_at = env.ledger().timestamp() + 86400 * 365;
        let credential = client
            .create_multi_signed_credential(&holder, &issuer, &signers, 2, &metadata, expires_at)
            .unwrap();

        // Verify rate limit works
        assert!(client.check_verification_rate_limit(&holder).is_ok());

        // Get credential
        let retrieved = client.get_credential(&credential.id).unwrap();
        assert_eq!(retrieved.id, credential.id);
    }

    #[test]
    fn test_credential_with_tamper_detection() {
        let (env, contract_id) = setup_env_and_contract();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let holder = Address::generate(&env);
        let issuer = Address::generate(&env);
        let signers = create_test_addresses(&env, 2);

        let metadata = String::from_str(&env, "Tamper-Protected Credential");
        let expires_at = env.ledger().timestamp() + 86400 * 365;

        let credential = client
            .create_multi_signed_credential(&holder, &issuer, &signers, 1, &metadata, expires_at)
            .unwrap();

        // Add versioned metadata
        client
            .create_versioned_metadata(&credential.id, &metadata, &issuer)
            .unwrap();

        // Check for tampering
        assert!(!client.detect_metadata_tampering(&credential.id).unwrap());

        // Record an alert
        let detector = Address::generate(&env);
        let expected_hash = BytesN::from_array(&env, &[1u8; 32]);
        let actual_hash = BytesN::from_array(&env, &[2u8; 32]);

        client
            .record_tamper_alert(&credential.id, &expected_hash, &actual_hash, &detector)
            .unwrap();

        // Get forensic history with alert
        let history = client.get_forensic_history(&credential.id).unwrap();
        assert_eq!(history.tampering_events.len(), 1);
    }
}
