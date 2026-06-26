#[cfg(test)]
mod emergency_pause_auto_unpause_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    fn setup() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin1.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &2, &token);

        (env, client, admin1, admin2, deployer)
    }

    #[test]
    fn test_emergency_pause_with_duration() {
        let (env, client, admin1, _, _) = setup();
        
        let duration_secs = 3600; // 1 hour
        client.emergency_pause_with_duration(&admin1, &duration_secs);
        
        assert!(client.get_config().emergency_pause_enabled);
    }

    #[test]
    fn test_auto_unpause_after_duration_elapsed() {
        let (env, client, admin1, _, _) = setup();
        
        let duration_secs = 3600;
        env.ledger().with_mut(|l| l.timestamp = 1000);
        client.emergency_pause_with_duration(&admin1, &duration_secs);
        
        env.ledger().with_mut(|l| l.timestamp = 1000 + duration_secs + 1);
        client.tick_auto_unpause();
        
        assert!(!client.get_config().emergency_pause_enabled);
    }

    #[test]
    fn test_pause_active_before_duration_expires() {
        let (env, client, admin1, _, _) = setup();
        
        let duration_secs = 3600;
        env.ledger().with_mut(|l| l.timestamp = 1000);
        client.emergency_pause_with_duration(&admin1, &duration_secs);
        
        env.ledger().with_mut(|l| l.timestamp = 1000 + 1800);
        
        assert!(client.get_config().emergency_pause_enabled);
    }

    #[test]
    fn test_manual_unpause_before_auto_unpause() {
        let (env, client, admin1, admin2, _) = setup();
        
        let duration_secs = 3600;
        client.emergency_pause_with_duration(&admin1, &duration_secs);
        
        let signers = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        client.emergency_unpause(&signers);
        
        assert!(!client.get_config().emergency_pause_enabled);
    }

    #[test]
    fn test_auto_unpause_with_zero_duration() {
        let (env, client, admin1, _, _) = setup();
        
        env.ledger().with_mut(|l| l.timestamp = 1000);
        client.emergency_pause_with_duration(&admin1, &0);
        
        env.ledger().with_mut(|l| l.timestamp = 1001);
        client.tick_auto_unpause();
        
        assert!(!client.get_config().emergency_pause_enabled);
    }

    #[test]
    fn test_pause_duration_timestamp_recorded() {
        let (env, client, admin1, _, _) = setup();
        
        env.ledger().with_mut(|l| l.timestamp = 50000);
        let duration_secs = 7200;
        client.emergency_pause_with_duration(&admin1, &duration_secs);
        
        let pause_info = client.get_pause_info();
        assert_eq!(pause_info.pause_timestamp, 50000);
    }

    #[test]
    fn test_multiple_pause_resets_duration() {
        let (env, client, admin1, _, _) = setup();
        
        env.ledger().with_mut(|l| l.timestamp = 1000);
        client.emergency_pause_with_duration(&admin1, &3600);
        
        env.ledger().with_mut(|l| l.timestamp = 2000);
        client.emergency_pause_with_duration(&admin1, &3600);
        
        env.ledger().with_mut(|l| l.timestamp = 5600);
        client.tick_auto_unpause();
        
        assert!(client.get_config().emergency_pause_enabled);
    }
}
