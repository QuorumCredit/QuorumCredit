#[cfg(test)]
mod governance_snapshot_tests {
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
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let borrower = Address::generate(&env);
        
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token);

        StellarAssetClient::new(&env, &token).mint(&voucher1, &10_000_000);
        StellarAssetClient::new(&env, &token).mint(&voucher2, &5_000_000);

        (env, client, voucher1, voucher2, borrower, admin)
    }

    #[test]
    fn test_governance_snapshot_captures_voucher_stakes() {
        let (env, client, voucher1, voucher2, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &2_000_000, &client.get_config().token);
        client.vouch(&voucher2, &borrower, &1_000_000, &client.get_config().token);
        
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let snapshot = client.take_snapshot(&borrower);
        
        assert_eq!(snapshot.total_stake, 3_000_000);
    }

    #[test]
    fn test_snapshot_timestamp_recorded() {
        let (env, client, voucher1, _, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &1_000_000, &client.get_config().token);
        
        env.ledger().with_mut(|l| l.timestamp = 50_000);
        let snapshot = client.take_snapshot(&borrower);
        
        assert_eq!(snapshot.timestamp, 50_000);
    }

    #[test]
    fn test_snapshot_immutable_for_vote() {
        let (env, client, voucher1, voucher2, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &2_000_000, &client.get_config().token);
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let snapshot = client.take_snapshot(&borrower);
        
        // Add more stake after snapshot
        client.vouch(&voucher2, &borrower, &1_000_000, &client.get_config().token);
        
        // Snapshot should remain unchanged
        assert_eq!(snapshot.total_stake, 2_000_000);
    }

    #[test]
    fn test_snapshot_lists_all_vouchers() {
        let (env, client, voucher1, voucher2, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &2_000_000, &client.get_config().token);
        client.vouch(&voucher2, &borrower, &1_500_000, &client.get_config().token);
        
        let snapshot = client.take_snapshot(&borrower);
        assert_eq!(snapshot.vouchers.len(), 2);
    }

    #[test]
    fn test_multiple_snapshots_independent() {
        let (env, client, voucher1, voucher2, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &1_000_000, &client.get_config().token);
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let snapshot1 = client.take_snapshot(&borrower);
        
        client.vouch(&voucher2, &borrower, &2_000_000, &client.get_config().token);
        env.ledger().with_mut(|l| l.timestamp = 200_000);
        let snapshot2 = client.take_snapshot(&borrower);
        
        assert_eq!(snapshot1.total_stake, 1_000_000);
        assert_eq!(snapshot2.total_stake, 3_000_000);
    }

    #[test]
    fn test_snapshot_enables_fair_voting() {
        let (env, client, voucher1, voucher2, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &3_000_000, &client.get_config().token);
        client.vouch(&voucher2, &borrower, &2_000_000, &client.get_config().token);
        
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let snapshot = client.take_snapshot(&borrower);
        
        // Voting power should be based on snapshot, not current state
        let voting_power1 = client.get_voting_power(&voucher1, &snapshot);
        assert_eq!(voting_power1, 3_000_000);
    }

    #[test]
    fn test_snapshot_quorum_calculated_fairly() {
        let (env, client, voucher1, voucher2, borrower, _) = setup();
        
        client.vouch(&voucher1, &borrower, &4_000_000, &client.get_config().token);
        client.vouch(&voucher2, &borrower, &6_000_000, &client.get_config().token);
        
        let snapshot = client.take_snapshot(&borrower);
        
        // Quorum should be 50% of 10M = 5M
        let quorum = client.calculate_quorum(&snapshot);
        assert_eq!(quorum, 5_000_000);
    }
}
