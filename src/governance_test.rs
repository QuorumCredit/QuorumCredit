#[cfg(test)]
mod governance_tests {
    use crate::types::{Config, DataKey, VoteSlashResult};
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
        contract_id: Address,
        deployer: Address,
        admins: Vec<Address>,
    }

    fn setup(admin_threshold: u32, num_admins: usize) -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let mut admins = Vec::new(&env);
        for _ in 0..num_admins {
            admins.push_back(Address::generate(&env));
        }

        let token_id = env.register_stellar_asset_contract_v2(admins.get(0).unwrap().clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Fund contract
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &admin_threshold, &token_id.address());

        // Start at t=120 so all vouches pass MIN_VOUCH_AGE
        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            token: token_id.address(),
            contract_id,
            deployer,
            admins,
        }
    }

    fn single_admin_signers(env: &Env, admin: &Address) -> Vec<Address> {
        Vec::from_array(env, [admin.clone()])
    }

    #[test]
    #[should_panic(expected = "insufficient admin approvals")]
    fn test_emergency_pause_fewer_than_threshold_fails() {
        let s = setup(2, 3);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone()]);
        s.client.emergency_pause(&admin_signers);
    }

    #[test]
    fn test_emergency_pause_with_threshold_succeeds() {
        let s = setup(2, 3);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);
        s.client.emergency_pause(&admin_signers);
        // Verify emergency pause enabled
        assert!(s.client.get_config().emergency_pause_enabled);
    }

    #[test]
    #[should_panic(expected = "insufficient admin approvals")]
    fn test_queue_slash_fewer_than_threshold_fails() {
        let s = setup(2, 3);
        let borrower = Address::generate(&s.env);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone()]);
        s.client.queue_slash(&admin_signers, &borrower, &100_000);
    }

    #[test]
    #[should_panic(expected = "insufficient admin approvals")]
    fn test_execute_queued_slashes_fewer_than_threshold_fails() {
        let s = setup(2, 3);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone()]);
        s.client.execute_queued_slashes(&admin_signers);
    }

    #[test]
    fn test_queue_and_execute_slash_with_threshold_succeeds() {
        let s = setup(2, 3);
        let borrower = Address::generate(&s.env);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);
        
        // Setup vouch and loan to slash
        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);
        s.client.vouch(&voucher, &borrower, &1_000_000, &s.token, &None);
        
        s.client.request_loan(&borrower, &100_000, &1_000_000, &String::from_str(&s.env, "test"), &s.token);
        
        // Queue slash
        s.client.queue_slash(&admin_signers, &borrower, &100_000);
        
        // Execute queued slash (returns 0 because lazy_slash is a stub)
        let count = s.client.execute_queued_slashes(&admin_signers);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_vote_slash_delegated_vote_regression() {
        let s = setup(1, 1);
        let borrower = Address::generate(&s.env);
        
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        let token_admin = StellarAssetClient::new(&s.env, &s.token);
        token_admin.mint(&v1, &1_000_000);
        token_admin.mint(&v2, &1_000_000);
        token_admin.mint(&v3, &1_000_000);

        s.client.vouch(&v1, &borrower, &1_000_000, &s.token, &None);
        s.client.vouch(&v2, &borrower, &1_000_000, &s.token, &None);
        s.client.vouch(&v3, &borrower, &1_000_000, &s.token, &None);

        s.client.request_loan(&borrower, &100_000, &1_000_000, &String::from_str(&s.env, "test"), &s.token);

        // 1. Single delegation: v1 delegates to v2
        s.client.delegate_vote(&v1, &v2);

        // Calling vote_slash with v1 (delegated voter) should return DelegateWillVote
        let res1 = s.client.vote_slash(&v1, &borrower, &true);
        assert_eq!(res1, VoteSlashResult::DelegateWillVote);

        // 2. Chained delegation: v2 delegates to v3
        s.client.delegate_vote(&v2, &v3);

        // v1 delegates to v2, which delegates to v3.
        // Both v1 and v2 should return DelegateWillVote.
        let res1_chained = s.client.vote_slash(&v1, &borrower, &true);
        assert_eq!(res1_chained, VoteSlashResult::DelegateWillVote);

        let res2 = s.client.vote_slash(&v2, &borrower, &true);
        assert_eq!(res2, VoteSlashResult::DelegateWillVote);

        // v3 is the final delegate (not delegated to anyone), so its vote counts
        let res3 = s.client.vote_slash(&v3, &borrower, &true);
        assert_eq!(res3, VoteSlashResult::VoteCounted);
    }
}
