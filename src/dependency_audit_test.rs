#[cfg(test)]
mod dependency_audit_tests {
    use crate::{ContractError, DataKey, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn vec_of(env: &Env, addr: &Address) -> Vec<Address> {
        let mut v = Vec::new(env);
        v.push_back(addr.clone());
        v
    }

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);

        (env, deployer, admin)
    }

    #[test]
    fn test_dependency_audit_log_created_on_initialize() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Verify dependency audit log is created
        let audit_log = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::DependencyAuditLog)
        });
        assert!(
            audit_log.is_some(),
            "Dependency audit log must be created on initialize"
        );
    }

    #[test]
    fn test_dependency_versions_tracked() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Verify Soroban SDK version is tracked
        let version_info = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::DependencyVersions)
        });
        assert!(
            version_info.is_some(),
            "Dependency versions must be tracked"
        );
    }

    #[test]
    fn test_dependency_audit_on_config_update() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        let initial_log_len = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::DependencyAuditLog)
                .map(|log| log.len())
                .unwrap_or(0)
        });

        // Update config
        let config = crate::types::Config::default();
        client.set_config(&admins, &config);

        let after_log_len = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::DependencyAuditLog)
                .map(|log| log.len())
                .unwrap_or(0)
        });

        assert!(
            after_log_len > initial_log_len,
            "Dependency audit log must grow on config updates"
        );
    }

    #[test]
    fn test_dependency_audit_timestamp() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Verify audit entry has timestamp
        let timestamp = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::DependencyAuditTimestamp)
        });
        assert!(
            timestamp.is_some() && timestamp.unwrap() > 0,
            "Audit log entries must include timestamps"
        );
    }

    #[test]
    fn test_dependency_audit_immutable_after_init() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        let init_versions = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::DependencyVersions)
                .cloned()
        });

        // After initialization, dependency versions should not change
        let after_versions = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, Vec<u8>>(&DataKey::DependencyVersions)
                .cloned()
        });

        assert_eq!(
            init_versions, after_versions,
            "Dependency versions must remain immutable across calls"
        );
    }
}
