#[cfg(test)]
mod vote_escrow_lock_tests {
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
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let voucher = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token);

        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

        (env, client, voucher, admin, deployer)
    }

    #[test]
    fn test_vote_escrow_lock_creation() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_amount = 5_000_000i128;
        let lock_period = 365 * 24 * 60 * 60; // 1 year
        
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let lock_id = client.create_vote_escrow_lock(&voucher, &lock_amount, &lock_period);
        
        assert!(lock_id > 0);
    }

    #[test]
    fn test_vote_escrow_lock_records_amount() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_amount = 3_000_000i128;
        let lock_period = 182 * 24 * 60 * 60; // 6 months
        
        let lock_id = client.create_vote_escrow_lock(&voucher, &lock_amount, &lock_period);
        let lock = client.get_vote_escrow_lock(&lock_id);
        
        assert_eq!(lock.amount, lock_amount);
    }

    #[test]
    fn test_vote_escrow_lock_unlock_date() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_period = 365 * 24 * 60 * 60;
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let lock_id = client.create_vote_escrow_lock(&voucher, &2_000_000, &lock_period);
        
        let lock = client.get_vote_escrow_lock(&lock_id);
        assert_eq!(lock.unlock_timestamp, 1000 + lock_period);
    }

    #[test]
    fn test_vote_escrow_increases_voting_power() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_amount = 4_000_000i128;
        let lock_period = 365 * 24 * 60 * 60;
        
        let power_before = client.get_voting_power(&voucher, &client.get_config().token);
        client.create_vote_escrow_lock(&voucher, &lock_amount, &lock_period);
        let power_after = client.get_voting_power(&voucher, &client.get_config().token);
        
        assert!(power_after > power_before);
    }

    #[test]
    fn test_vote_escrow_lock_cannot_be_withdrawn_early() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_period = 365 * 24 * 60 * 60;
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let lock_id = client.create_vote_escrow_lock(&voucher, &2_000_000, &lock_period);
        
        env.ledger().with_mut(|l| l.timestamp = 1000 + 30 * 24 * 60 * 60);
        let result = client.try_unlock_vote_escrow(&voucher, &lock_id);
        
        assert_eq!(result, Err(Ok(ContractError::LockStillActive)));
    }

    #[test]
    fn test_vote_escrow_lock_can_be_withdrawn_after_unlock() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_period = 365 * 24 * 60 * 60;
        let lock_amount = 1_500_000i128;
        
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let lock_id = client.create_vote_escrow_lock(&voucher, &lock_amount, &lock_period);
        
        env.ledger().with_mut(|l| l.timestamp = 1000 + lock_period + 1);
        client.unlock_vote_escrow(&voucher, &lock_id);
        
        let lock = client.get_vote_escrow_lock(&lock_id);
        assert!(lock.withdrawn);
    }

    #[test]
    fn test_multiple_vote_escrow_locks() {
        let (env, client, voucher, _, _) = setup();
        
        let lock1 = client.create_vote_escrow_lock(&voucher, &2_000_000, &(365 * 24 * 60 * 60));
        let lock2 = client.create_vote_escrow_lock(&voucher, &3_000_000, &(180 * 24 * 60 * 60));
        
        assert_ne!(lock1, lock2);
        
        let locks = client.get_voucher_locks(&voucher);
        assert_eq!(locks.len(), 2);
    }

    #[test]
    fn test_vote_escrow_lock_decay_after_unlock() {
        let (env, client, voucher, _, _) = setup();
        
        let lock_period = 365 * 24 * 60 * 60;
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let lock_id = client.create_vote_escrow_lock(&voucher, &5_000_000, &lock_period);
        
        let power_at_lock = client.get_voting_power(&voucher, &client.get_config().token);
        
        env.ledger().with_mut(|l| l.timestamp = 1000 + lock_period + 1);
        client.unlock_vote_escrow(&voucher, &lock_id);
        
        let power_after_unlock = client.get_voting_power(&voucher, &client.get_config().token);
        
        assert!(power_after_unlock < power_at_lock);
    }
}
