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
        #[allow(dead_code)]
        contract_id: Address,
        #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn single_admin_signers(env: &Env, admin: &Address) -> Vec<Address> {
        Vec::from_array(env, [admin.clone()])
    }

    #[test]
    fn test_emergency_pause_fewer_than_threshold_fails() {
        let s = setup(2, 3);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone()]);
        let result = s.client.try_emergency_pause(&admin_signers);
        assert_eq!(result, Err(Ok(crate::errors::ContractError::UnauthorizedCaller)));
    }

    #[test]
    fn test_emergency_pause_denies_role_without_pause_permission() {
        let s = setup(1, 1);
        let admin = s.admins.get(0).unwrap();
        // Monitor has no Pause permission, even though it alone satisfies the threshold.
        s.env.as_contract(&s.contract_id, || {
            crate::rbac::assign_admin_role(&s.env, s.admins.clone(), admin.clone(), crate::types::AdminRole::Monitor);
        });
        let admin_signers = Vec::from_array(&s.env, [admin.clone()]);
        let result = s.client.try_emergency_pause(&admin_signers);
        assert_eq!(result, Err(Ok(crate::errors::ContractError::PermissionDenied)));
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
        
        // Execute queued slash (should actually execute now)
        let count = s.client.execute_queued_slashes(&admin_signers);
        assert_eq!(count, 1);

        // After slashing, ActiveLoan is removed, so get_loan returns None
        // This is expected behavior - the loan is now defaulted and no longer "active"
        let loan = s.client.get_loan(&borrower);
        assert!(loan.is_none());
    }

    #[test]
    fn test_queue_slash_empty_queue_returns_zero() {
        let s = setup(2, 3);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);
        
        // Execute with empty queue should return 0
        let count = s.client.execute_queued_slashes(&admin_signers);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_queue_slash_twice_executes_both() {
        let s = setup(2, 3);
        let borrower1 = Address::generate(&s.env);
        let borrower2 = Address::generate(&s.env);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);
        
        // Setup two borrowers with vouches and loans
        let voucher1 = Address::generate(&s.env);
        let voucher2 = Address::generate(&s.env);
        let token_admin = StellarAssetClient::new(&s.env, &s.token);
        
        token_admin.mint(&voucher1, &2_000_000);
        token_admin.mint(&voucher2, &2_000_000);
        
        s.client.vouch(&voucher1, &borrower1, &1_000_000, &s.token, &None);
        s.client.vouch(&voucher2, &borrower2, &1_000_000, &s.token, &None);
        
        s.client.request_loan(&borrower1, &100_000, &1_000_000, &String::from_str(&s.env, "test1"), &s.token);
        s.client.request_loan(&borrower2, &100_000, &1_000_000, &String::from_str(&s.env, "test2"), &s.token);
        
        // Queue both slashes
        s.client.queue_slash(&admin_signers, &borrower1, &100_000);
        s.client.queue_slash(&admin_signers, &borrower2, &100_000);
        
        // Execute queued slashes - should execute both
        let count = s.client.execute_queued_slashes(&admin_signers);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_queue_slash_duplicate_borrower_executes_both() {
        let s = setup(2, 3);
        let borrower = Address::generate(&s.env);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);
        
        // Setup borrower with vouch and loan
        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &2_000_000);
        s.client.vouch(&voucher, &borrower, &2_000_000, &s.token, &None);
        
        s.client.request_loan(&borrower, &100_000, &2_000_000, &String::from_str(&s.env, "test"), &s.token);
        
        // Queue the same borrower twice with different amounts
        s.client.queue_slash(&admin_signers, &borrower, &50_000);
        s.client.queue_slash(&admin_signers, &borrower, &50_000);
        
        // Execute queued slashes - first should succeed, second should fail (already slashed)
        let count = s.client.execute_queued_slashes(&admin_signers);
        // Only the first slash should execute successfully
        assert_eq!(count, 1);
    }

    #[test]
    #[should_panic]
    fn test_queue_slash_zero_amount_fails() {
        let s = setup(2, 3);
        let borrower = Address::generate(&s.env);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);

        // Queue slash with zero amount should fail with InvalidAmount error
        s.client.queue_slash(&admin_signers, &borrower, &0);
    }

    #[test]
    #[should_panic]
    fn test_queue_slash_negative_amount_fails() {
        let s = setup(2, 3);
        let borrower = Address::generate(&s.env);
        let admin_signers = Vec::from_array(&s.env, [s.admins.get(0).unwrap().clone(), s.admins.get(1).unwrap().clone()]);

        // Queue slash with negative amount should fail with InvalidAmount error
        s.client.queue_slash(&admin_signers, &borrower, &-100);
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
