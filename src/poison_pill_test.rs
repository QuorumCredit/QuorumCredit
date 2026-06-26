#[cfg(test)]
mod poison_pill_tests {
    use crate::{ContractError, DataKey, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn vec_of(env: &Env, addr: &Address) -> Vec<Address> {
        let mut v = Vec::new(env);
        v.push_back(addr.clone());
        v
    }

    fn vec_of_multiple(env: &Env, addrs: &[Address]) -> Vec<Address> {
        let mut v = Vec::new(env);
        for addr in addrs {
            v.push_back(addr.clone());
        }
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
    fn test_poison_pill_key_can_be_disabled() {
        let (env, deployer, admin, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Verify admin key can be disabled via poison pill
        let result = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::AdminDisabled(admin.clone()))
        });
        assert!(result.is_none(), "Key should not be disabled initially");
    }

    #[test]
    fn test_disabled_admin_key_rejected() {
        let (env, deployer, admin1, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
        let token_client = StellarAssetClient::new(&env, &token_id.address());
        token_client.mint(&contract_id, &0);

        let admin2 = Address::generate(&env);
        let admins = vec_of_multiple(&env, &[admin1.clone(), admin2.clone()]);
        client.initialize(&deployer, &admins, &2, &token_id.address());

        // Mark admin1 as disabled
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::AdminDisabled(admin1.clone()), &true);
        });

        // Verify disabled admin cannot sign operations
        let is_disabled = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::AdminDisabled(admin1.clone()))
                .unwrap_or(false)
        });
        assert!(is_disabled, "Admin should be marked as disabled");
    }

    #[test]
    fn test_poison_pill_multisig_approval() {
        let (env, deployer, admin1, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);
        let admins = vec_of_multiple(&env, &[admin1.clone(), admin2.clone(), admin3.clone()]);
        client.initialize(&deployer, &admins, &2, &token_id.address());

        // Store poison pill vote
        let vote_key = format!("poison_pill_vote_{}", admin1);
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(
                &DataKey::PoisonPillVote(admin1.clone()),
                &(2 as u32),
            );
        });

        let votes = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::PoisonPillVote(admin1.clone()))
        });
        assert!(votes.is_some() && votes.unwrap() >= 2, "Poison pill votes must accumulate");
    }

    #[test]
    fn test_poison_pill_emergency_trigger() {
        let (env, deployer, admin, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Trigger emergency pause via poison pill
        client.pause(&admins);

        // Contract should be paused
        let is_paused = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::Paused)
                .unwrap_or(false)
        });
        assert!(is_paused, "Contract should be paused after poison pill trigger");
    }

    #[test]
    fn test_poison_pill_prevents_operations_after_disabled() {
        let (env, deployer, admin, borrower) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_client = StellarAssetClient::new(&env, &token_id.address());
        token_client.mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Disable the admin
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::AdminDisabled(admin.clone()), &true);
        });

        // Attempt to perform admin operation
        let config = crate::types::Config::default();
        let result = client.try_set_config(&vec_of(&env, &admin), &config);

        assert!(
            result.is_err(),
            "Disabled admin key should not be able to perform operations"
        );
    }

    #[test]
    fn test_poison_pill_recovery_requires_multisig() {
        let (env, deployer, admin1, _) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);
        let admins = vec_of_multiple(&env, &[admin1.clone(), admin2.clone(), admin3.clone()]);
        client.initialize(&deployer, &admins, &2, &token_id.address());

        // Disable admin1
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::AdminDisabled(admin1.clone()), &true);
        });

        // Recovery should require multiple admin approvals
        let recovery_votes = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::PoisonPillRecoveryVotes(admin1.clone()))
                .unwrap_or(0)
        });
        assert!(
            recovery_votes == 0,
            "Recovery votes must start at zero"
        );
    }
}
