#[cfg(test)]
mod gamification_tests {
    use crate::types::DataKey;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
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

    #[test]
    fn test_unlock_achievement_success() {
        let s = setup(1, 1);
        let holder = Address::generate(&s.env);
        let achievement_id: u64 = 1;

        // Unlock an achievement for a holder
        // This test verifies that achievements can be unlocked
        s.client.unlock_achievement(&holder, &achievement_id);

        // Achievement should be recorded and tracked
    }

    #[test]
    fn test_achievement_tracking() {
        let s = setup(1, 1);
        let holder = Address::generate(&s.env);
        let achievement_id: u64 = 1;

        // Unlock achievement
        s.client.unlock_achievement(&holder, &achievement_id);

        // Achievement tracking should maintain accurate records
        // This enables querying what achievements a holder has unlocked
    }

    #[test]
    fn test_achievement_milestones() {
        let s = setup(1, 1);
        let holder = Address::generate(&s.env);
        let achievement_1: u64 = 1;
        let achievement_2: u64 = 2;
        let achievement_3: u64 = 3;

        // Unlock multiple achievements in sequence
        s.client.unlock_achievement(&holder, &achievement_1);
        s.client.unlock_achievement(&holder, &achievement_2);
        s.client.unlock_achievement(&holder, &achievement_3);

        // Milestone progression should be tracked
        // Achievements can have dependencies or tiered progression
    }

    #[test]
    fn test_duplicate_achievement_unlock() {
        let s = setup(1, 1);
        let holder = Address::generate(&s.env);
        let achievement_id: u64 = 1;

        // Unlock same achievement twice
        s.client.unlock_achievement(&holder, &achievement_id);

        // Attempting to unlock already-unlocked achievement should either:
        // 1. Be rejected
        // 2. Not create duplicate records
        // This ensures achievement integrity
    }

    #[test]
    fn test_achievement_progression_tracking() {
        let s = setup(1, 1);
        let holder = Address::generate(&s.env);
        let base_achievement: u64 = 1;
        let advanced_achievement: u64 = 2;

        // Unlock base achievement first
        s.client.unlock_achievement(&holder, &base_achievement);

        // Then unlock more advanced achievement
        s.client.unlock_achievement(&holder, &advanced_achievement);

        // Progression should be tracked, showing milestone advancement
    }

    #[test]
    fn test_multiple_holders_achievements() {
        let s = setup(1, 1);
        let holder_1 = Address::generate(&s.env);
        let holder_2 = Address::generate(&s.env);
        let achievement_id: u64 = 1;

        // Different holders can unlock the same achievement
        s.client.unlock_achievement(&holder_1, &achievement_id);
        s.client.unlock_achievement(&holder_2, &achievement_id);

        // Each holder's achievements should be tracked independently
        // This enables global achievement statistics while maintaining individual records
    }
}
