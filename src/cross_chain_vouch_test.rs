#[cfg(test)]
mod cross_chain_vouch_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        /// Primary token — also used as the bridge address in tests for simplicity.
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.initialize(
            &deployer,
            &Vec::from_array(&env, [admin.clone()]),
            &1,
            &token,
        );

        Setup { env, client, admin, token }
    }

    #[test]
    fn test_register_bridge_and_cross_chain_vouch() {
        let s = setup();
        let chain_id: u32 = 1; // Ethereum mainnet

        // Register the primary token as the bridge address for chain 1.
        // In production the bridge address would be a separate wrapped-token contract.
        s.client.register_bridge(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &chain_id,
            &String::from_str(&s.env, "ethereum"),
            &s.token,
        );

        // Verify bridge is registered
        let bridges = s.client.get_bridges();
        assert_eq!(bridges.len(), 1);
        assert_eq!(bridges.get(0).unwrap().chain_id, chain_id);
        assert!(bridges.get(0).unwrap().active);

        // Voucher stakes using the bridge token with chain_id
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);

        s.client
            .vouch(&voucher, &borrower, &1_000_000, &s.token, &Some(chain_id));

        // Verify vouch was recorded with chain_id
        let vouches = s.client.get_vouches(&borrower);
        assert_eq!(vouches.len(), 1);
        let vouch = vouches.get(0).unwrap();
        assert_eq!(vouch.chain_id, Some(chain_id));
        assert_eq!(vouch.stake, 1_000_000);
    }

    #[test]
    fn test_vouch_with_unregistered_chain_id_rejected() {
        let s = setup();

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);

        // chain_id 99 is not registered — should fail
        let result = s
            .client
            .try_vouch(&voucher, &borrower, &1_000_000, &s.token, &Some(99u32));
        assert_eq!(result, Err(Ok(ContractError::InvalidChain)));
    }

    #[test]
    fn test_remove_bridge_prevents_new_vouches() {
        let s = setup();
        let chain_id: u32 = 137; // Polygon

        s.client.register_bridge(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &chain_id,
            &String::from_str(&s.env, "polygon"),
            &s.token,
        );

        // Deactivate the bridge
        s.client.remove_bridge(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &chain_id,
        );

        let bridges = s.client.get_bridges();
        assert!(!bridges.get(0).unwrap().active);

        // New vouch with deactivated bridge should fail
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);

        let result = s
            .client
            .try_vouch(&voucher, &borrower, &1_000_000, &s.token, &Some(chain_id));
        assert_eq!(result, Err(Ok(ContractError::InvalidChain)));
    }

    #[test]
    fn test_duplicate_bridge_registration_rejected() {
        let s = setup();
        let chain_id: u32 = 1;

        s.client.register_bridge(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &chain_id,
            &String::from_str(&s.env, "ethereum"),
            &s.token,
        );

        let result = s.client.try_register_bridge(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &chain_id,
            &String::from_str(&s.env, "ethereum"),
            &s.token,
        );
        assert_eq!(result, Err(Ok(ContractError::BridgeAlreadyRegistered)));
    }

    #[test]
    fn test_native_vouch_no_chain_id_unaffected() {
        let s = setup();

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);

        // Native vouch with no chain_id — should succeed without any bridge registered
        s.client
            .vouch(&voucher, &borrower, &1_000_000, &s.token, &None);

        let vouches = s.client.get_vouches(&borrower);
        assert_eq!(vouches.len(), 1);
        assert_eq!(vouches.get(0).unwrap().chain_id, None);
    }

    /// Mutation target: min_stake boundary uses `<` not `<=` (exact minimum must pass).
    #[test]
    fn test_vouch_at_exact_min_stake_succeeds() {
        let s = setup();
        let min_stake: i128 = 500_000;
        s.client.set_min_stake(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &min_stake,
        );

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &min_stake);

        s.client
            .vouch(&voucher, &borrower, &min_stake, &s.token, &None);

        let vouches = s.client.get_vouches(&borrower);
        assert_eq!(vouches.len(), 1);
        assert_eq!(vouches.get(0).unwrap().stake, min_stake);
    }

    /// Mutation target: vouch cooldown guard blocks rapid successive vouches.
    #[test]
    fn test_vouch_cooldown_blocks_second_vouch() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower1 = Address::generate(&s.env);
        let borrower2 = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &2_000_000);

        s.client
            .vouch(&voucher, &borrower1, &1_000_000, &s.token, &None);

        let result = s
            .client
            .try_vouch(&voucher, &borrower2, &1_000_000, &s.token, &None);
        assert_eq!(result, Err(Ok(ContractError::VouchCooldownActive)));
    }

    /// Mutation target: require_positive_amount rejects zero stake.
    #[test]
    fn test_zero_stake_vouch_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);

        let result = s
            .client
            .try_vouch(&voucher, &borrower, &0, &s.token, &None);
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }
}
