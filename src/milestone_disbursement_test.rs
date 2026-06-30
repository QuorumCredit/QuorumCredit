#[cfg(test)]
mod milestone_disbursement_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient, MilestoneRecord, MilestoneStatus};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec, String as SorobanString};

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

        (env, client, admin1, admin2, token)
    }

    #[test]
    fn test_milestone_disbursement_flow() {
        let (env, client, admin1, admin2, token) = setup();

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
        let token_balance_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.mint(&voucher, &50_000_000);

        // Vouch for borrower
        client.vouch(&voucher, &borrower, &30_000_000, &token, &None);

        let now = env.ledger().timestamp();
        
        // Define 3 milestones
        let mut milestones = Vec::new(&env);
        
        milestones.push_back(MilestoneRecord {
            milestone_id: 1,
            loan_id: 0,
            tranche_id: 1,
            status: MilestoneStatus::Pending,
            deadline: now + 1000,
            description: SorobanString::from_slice(&env, b"Milestone 1"),
            submitted_at: None,
            evidence_hash: None,
            proof_uri: None,
            approved_at: None,
            approvers: Vec::new(&env),
            rejection_reason: None,
            tranche_released: false,
        });

        milestones.push_back(MilestoneRecord {
            milestone_id: 2,
            loan_id: 0,
            tranche_id: 2,
            status: MilestoneStatus::Pending,
            deadline: now + 2000,
            description: SorobanString::from_slice(&env, b"Milestone 2"),
            submitted_at: None,
            evidence_hash: None,
            proof_uri: None,
            approved_at: None,
            approvers: Vec::new(&env),
            rejection_reason: None,
            tranche_released: false,
        });

        milestones.push_back(MilestoneRecord {
            milestone_id: 3,
            loan_id: 0,
            tranche_id: 3,
            status: MilestoneStatus::Pending,
            deadline: now + 3000,
            description: SorobanString::from_slice(&env, b"Milestone 3"),
            submitted_at: None,
            evidence_hash: None,
            proof_uri: None,
            approved_at: None,
            approvers: Vec::new(&env),
            rejection_reason: None,
            tranche_released: false,
        });

        // Create milestone loan
        let total_amount = 9_000_000i128;
        let loan_id = client.create_milestone_loan(&borrower, &total_amount, &milestones).unwrap();

        // 1st tranche (1/3 of total_amount = 3_000_000) should be disbursed immediately
        let balance_after_creation = token_balance_client.balance(&borrower);
        assert_eq!(balance_after_creation, 3_000_000);

        // Try to disburse next tranche before verification (should fail/error)
        let disburse_fail = client.try_disburse_next_tranche(&borrower);
        assert!(disburse_fail.is_err());

        // Verify Milestone 2 by admins
        let admin_signers = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let proof = SorobanString::from_slice(&env, b"ipfs://proof-milestone-2");
        client.verify_milestone(&admin_signers, &borrower, &2, &proof).unwrap();

        // Disburse tranche 2
        client.disburse_next_tranche(&borrower).unwrap();
        let balance_after_tranche2 = token_balance_client.balance(&borrower);
        assert_eq!(balance_after_tranche2, 6_000_000);

        // Try to disburse tranche 3 before verification (should fail)
        let disburse_fail_2 = client.try_disburse_next_tranche(&borrower);
        assert!(disburse_fail_2.is_err());

        // Verify Milestone 3
        client.verify_milestone(&admin_signers, &borrower, &3, &proof).unwrap();

        // Disburse tranche 3
        client.disburse_next_tranche(&borrower).unwrap();
        let balance_after_tranche3 = token_balance_client.balance(&borrower);
        assert_eq!(balance_after_tranche3, 9_000_000);
    }
}
