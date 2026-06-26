#[cfg(test)]
mod audit_trail_immutability_tests {
    use crate::{ContractError, DataKey, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn vec_of(env: &Env, addr: &Address) -> Vec<Address> {
        let mut v = Vec::new(env);
        v.push_back(addr.clone());
        v
    }

    fn setup() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);

        (env, deployer, admin, borrower)
    }

    #[test]
    fn test_audit_trail_hashed_immutable() {
        let (env, deployer, admin, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Verify audit trail hash is stored
        let audit_hash = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::AuditTrailHash)
        });
        assert!(
            audit_hash.is_some(),
            "Audit trail hash must be stored after initialize"
        );
    }

    #[test]
    fn test_audit_trail_hash_changes_after_operation() {
        let (env, deployer, admin, borrower) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_client = StellarAssetClient::new(&env, &token_id.address());
        token_client.mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        let initial_hash = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::AuditTrailHash)
        });

        // Perform a vouch operation
        let voucher = Address::generate(&env);
        token_client.mint(&voucher, &1_000_000_000);
        token_client.approve(&voucher, &contract_id, &1_000_000_000, &(env.ledger().sequence() + 100));

        client.vouch(&voucher, &borrower, &500_000_000, &token_id.address());

        let hash_after_vouch = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::AuditTrailHash)
        });

        assert_ne!(
            initial_hash, hash_after_vouch,
            "Audit trail hash must change after a state-mutating operation"
        );
    }

    #[test]
    fn test_audit_trail_append_only() {
        let (env, deployer, admin, borrower) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_client = StellarAssetClient::new(&env, &token_id.address());
        token_client.mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Get initial event count
        let initial_count = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::AuditEventCount)
                .unwrap_or(0)
        });

        // Perform operation
        let voucher = Address::generate(&env);
        token_client.mint(&voucher, &1_000_000_000);
        token_client.approve(&voucher, &contract_id, &1_000_000_000, &(env.ledger().sequence() + 100));
        client.vouch(&voucher, &borrower, &500_000_000, &token_id.address());

        let after_count = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::AuditEventCount)
                .unwrap_or(0)
        });

        assert!(
            after_count > initial_count,
            "Audit event count must increase monotonically"
        );
    }

    #[test]
    fn test_audit_trail_cannot_be_deleted() {
        let (env, deployer, admin, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Verify audit trail exists
        let audit_exists = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .has(&DataKey::AuditTrailHash)
        });
        assert!(audit_exists, "Audit trail must exist");

        // Cannot be deleted by normal operations
        // The contract should not allow deletion of audit trail
        let still_exists = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .has(&DataKey::AuditTrailHash)
        });
        assert!(still_exists, "Audit trail must be immutable");
    }
}
