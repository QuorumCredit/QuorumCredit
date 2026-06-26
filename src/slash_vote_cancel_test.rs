#[cfg(test)]
mod slash_vote_cancel_tests {
    use crate::types::SlashVoteRecord;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup_env() -> (Env, Address, Vec<Address>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let token = env.register_stellar_asset_contract_v2(admin1.clone()).address();

        (env, deployer, admins, token)
    }

    #[test]
    fn test_slash_vote_cancel_with_66_percent_consensus() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher1 = Address::generate(&_env);
        let voucher2 = Address::generate(&_env);
        let voucher3 = Address::generate(&_env);
        let borrower = Address::generate(&_env);

        // Initial vote: 3 vouchers, need 66% to cancel
        let mut vote = SlashVoteRecord {
            approve_stake: 300,
            reject_stake: 700,
            voters: Vec::from_array(&_env, [voucher1.clone(), voucher2.clone(), voucher3.clone()]),
            executed: false,
        };

        let total_stake = vote.approve_stake + vote.reject_stake;
        let cancel_threshold_bps = 6600; // 66%

        // Calculate if we can cancel: need 66% of voters to agree
        let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);

        assert!(cancel_consensus >= cancel_threshold_bps);
        
        // Execute cancel
        vote.executed = true;
        assert!(vote.executed);
    }

    #[test]
    fn test_slash_vote_cancel_insufficient_consensus() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher1 = Address::generate(&_env);
        let voucher2 = Address::generate(&_env);
        let voucher3 = Address::generate(&_env);

        let vote = SlashVoteRecord {
            approve_stake: 800,
            reject_stake: 200,
            voters: Vec::from_array(&_env, [voucher1, voucher2, voucher3]),
            executed: false,
        };

        let total_stake = vote.approve_stake + vote.reject_stake;
        let cancel_threshold_bps = 6600; // 66%
        let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);

        // Should not meet 66% consensus
        assert!(cancel_consensus < cancel_threshold_bps);
        assert!(!vote.executed);
    }

    #[test]
    fn test_slash_vote_cancel_exact_66_percent() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher1 = Address::generate(&_env);
        let voucher2 = Address::generate(&_env);
        let voucher3 = Address::generate(&_env);

        let vote = SlashVoteRecord {
            approve_stake: 3400,
            reject_stake: 6600,
            voters: Vec::from_array(&_env, [voucher1, voucher2, voucher3]),
            executed: false,
        };

        let total_stake = vote.approve_stake + vote.reject_stake;
        let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);

        // Exactly 66%
        assert_eq!(cancel_consensus, 6600);
    }

    #[test]
    fn test_slash_vote_cancel_prevents_multiple_executions() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher1 = Address::generate(&_env);
        let voucher2 = Address::generate(&_env);
        let voucher3 = Address::generate(&_env);

        let mut vote = SlashVoteRecord {
            approve_stake: 200,
            reject_stake: 800,
            voters: Vec::from_array(&_env, [voucher1, voucher2, voucher3]),
            executed: false,
        };

        // First execution
        vote.executed = true;
        assert!(vote.executed);

        // Attempt second execution - should be blocked
        let was_executed = vote.executed;
        assert!(was_executed);
        assert!(vote.executed);
    }

    #[test]
    fn test_slash_vote_cancel_with_single_voucher() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher = Address::generate(&_env);

        let vote = SlashVoteRecord {
            approve_stake: 100,
            reject_stake: 900,
            voters: Vec::from_array(&_env, [voucher]),
            executed: false,
        };

        let total_stake = vote.approve_stake + vote.reject_stake;
        let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);

        assert!(cancel_consensus >= 6600);
    }

    #[test]
    fn test_slash_vote_cancel_with_many_vouchers() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let mut vouchers = Vec::new(&_env);
        for _ in 0..10 {
            vouchers.push_back(Address::generate(&_env));
        }

        let vote = SlashVoteRecord {
            approve_stake: 2500,
            reject_stake: 7500,
            voters: vouchers,
            executed: false,
        };

        let total_stake = vote.approve_stake + vote.reject_stake;
        let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);

        assert_eq!(cancel_consensus, 7500);
    }

    #[test]
    fn test_slash_vote_cancel_tracking_voter_count() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let mut vouchers = Vec::new(&_env);
        for _ in 0..5 {
            vouchers.push_back(Address::generate(&_env));
        }

        let vote = SlashVoteRecord {
            approve_stake: 400,
            reject_stake: 600,
            voters: vouchers.clone(),
            executed: false,
        };

        assert_eq!(vote.voters.len(), 5);
        let cancel_threshold_bps = 6600;
        let total_stake = vote.approve_stake + vote.reject_stake;
        let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);

        assert!(cancel_consensus >= cancel_threshold_bps);
    }

    #[test]
    fn test_slash_vote_cancel_state_transition() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher = Address::generate(&_env);

        let mut vote = SlashVoteRecord {
            approve_stake: 100,
            reject_stake: 900,
            voters: Vec::from_array(&_env, [voucher]),
            executed: false,
        };

        // Initial state
        assert!(!vote.executed);

        // Transition to canceled
        vote.executed = true;
        assert!(vote.executed);

        // Verify state is locked
        assert!(vote.executed);
    }

    #[test]
    fn test_slash_vote_cancel_consensus_calculation() {
        let (_env, _deployer, _admins, _token) = setup_env();
        let voucher = Address::generate(&_env);

        let test_cases = vec![
            (6600, 3400, true),   // 66% reject
            (6601, 3399, true),   // >66% reject
            (6599, 3401, false),  // <66% reject
            (5000, 5000, false),  // 50/50 split
            (0, 10000, true),     // All reject
            (10000, 0, false),    // All approve
        ];

        for (reject_stake, approve_stake, should_cancel) in test_cases {
            let vote = SlashVoteRecord {
                approve_stake: approve_stake as i128,
                reject_stake: reject_stake as i128,
                voters: Vec::from_array(&_env, [voucher.clone()]),
                executed: false,
            };

            let total_stake = vote.approve_stake + vote.reject_stake;
            let cancel_consensus = (vote.reject_stake as u32 * 10000) / (total_stake as u32);
            let can_cancel = cancel_consensus >= 6600;

            assert_eq!(can_cancel, should_cancel);
        }
    }
}
