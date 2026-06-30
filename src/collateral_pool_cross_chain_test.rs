#[cfg(test)]
mod collateral_pool_cross_chain_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
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
    fn join_pool_cross_chain_requires_bridge_validation() {
        let s = setup();
        let creator = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&creator, &1_000_000);
        let pool_id = s.client.create_collateral_pool(&creator, &s.token, &1_000_000);

        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &500_000);

        let chain_id: u32 = 7;
        let result = s.client.try_join_collateral_pool_cross_chain(
            &voucher, &pool_id, &500_000, &chain_id,
        );
        assert_eq!(result, Err(Ok(ContractError::PoolChainNotValidated)));
    }

    #[test]
    fn validated_voucher_can_join_from_another_chain() {
        let s = setup();
        let creator = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&creator, &1_000_000);
        let pool_id = s.client.create_collateral_pool(&creator, &s.token, &1_000_000);

        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &500_000);

        let chain_id: u32 = 7;
        s.client.set_bridge_validated(
            &Vec::from_array(&s.env, [s.admin.clone()]),
            &voucher,
            &chain_id,
            &true,
        );

        s.client
            .join_collateral_pool_cross_chain(&voucher, &pool_id, &500_000, &chain_id);

        assert_eq!(s.client.get_collateral_pool_total_stake(&pool_id), 1_500_000);
        assert_eq!(
            s.client.get_collateral_pool_chain_stake(&pool_id, &chain_id),
            500_000
        );
        assert_eq!(s.client.get_collateral_pool_chain_stake(&pool_id, &0), 1_000_000);
    }

    #[test]
    fn native_chain_id_is_rejected_for_cross_chain_join() {
        let s = setup();
        let creator = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&creator, &1_000_000);
        let pool_id = s.client.create_collateral_pool(&creator, &s.token, &1_000_000);

        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &500_000);

        let result = s
            .client
            .try_join_collateral_pool_cross_chain(&voucher, &pool_id, &500_000, &0);
        assert_eq!(result, Err(Ok(ContractError::InvalidBridgeChain)));
    }
}
