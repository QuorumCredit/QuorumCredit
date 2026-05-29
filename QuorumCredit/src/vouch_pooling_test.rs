#[cfg(test)]
mod vouch_pooling_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        QuorumCreditContractClient::new(env, &contract_id).initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token_id,
        );
        (contract_id, token_id, admin)
    }

    fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(to, &amount);
    }

    /// #638: create_vouch_pool returns a pool_id and get_vouch_pool returns the pool.
    #[test]
    fn test_create_vouch_pool_returns_pool() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let creator = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_id, &creator, 1_000_000);

        let pool_id = client.create_vouch_pool(&creator, &borrower);
        assert_eq!(pool_id, 1);

        let pool = client.get_vouch_pool(&pool_id).expect("pool should exist");
        assert_eq!(pool.pool_id, 1);
        assert_eq!(pool.borrower, borrower);
        assert_eq!(pool.members.len(), 1);
    }

    /// #638: join_vouch_pool adds a member and updates the voucher's VouchRecord pool_id.
    #[test]
    fn test_join_vouch_pool_updates_vouch_record() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let creator = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_id, &creator, 1_000_000);
        mint(&env, &token_id, &voucher2, 1_000_000);

        // Creator creates pool and vouches
        let pool_id = client.create_vouch_pool(&creator, &borrower);
        client.vouch(&creator, &borrower, &100_000, &token_id);

        // voucher2 vouches and joins pool
        client.vouch(&voucher2, &borrower, &100_000, &token_id);
        client.join_vouch_pool(&voucher2, &borrower, &pool_id);

        let pool = client.get_vouch_pool(&pool_id).expect("pool should exist");
        assert_eq!(pool.members.len(), 2);

        // VouchRecord for voucher2 should have pool_id set
        let vouches = client.get_vouches(&borrower).expect("vouches should exist");
        let v2_record = vouches.iter().find(|v| v.voucher == voucher2).expect("voucher2 record");
        assert_eq!(v2_record.pool_id, Some(pool_id));
    }

    /// #638: multiple pools can exist independently.
    #[test]
    fn test_multiple_pools_independent() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let creator1 = Address::generate(&env);
        let creator2 = Address::generate(&env);
        let borrower1 = Address::generate(&env);
        let borrower2 = Address::generate(&env);
        mint(&env, &token_id, &creator1, 1_000_000);
        mint(&env, &token_id, &creator2, 1_000_000);

        let pool1 = client.create_vouch_pool(&creator1, &borrower1);
        let pool2 = client.create_vouch_pool(&creator2, &borrower2);

        assert_eq!(pool1, 1);
        assert_eq!(pool2, 2);

        let p1 = client.get_vouch_pool(&pool1).unwrap();
        let p2 = client.get_vouch_pool(&pool2).unwrap();
        assert_eq!(p1.borrower, borrower1);
        assert_eq!(p2.borrower, borrower2);
    }
}
