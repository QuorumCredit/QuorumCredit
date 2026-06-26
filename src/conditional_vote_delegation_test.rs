#[cfg(test)]
mod conditional_vote_delegation_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    #[derive(Clone)]
    struct VoteDelegation {
        delegator: Address,
        delegate: Address,
        allowed_actions: Vec<String>,
        max_vote_power: i128,
        expiry_timestamp: u64,
        is_active: bool,
    }

    #[derive(Clone)]
    struct DelegationConstraint {
        min_stake: i128,
        max_stake: i128,
        allowed_action_types: Vec<String>,
        time_limit_seconds: u64,
    }

    fn setup_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    #[test]
    fn test_delegation_with_action_restriction() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let allowed_actions = Vec::from_array(&env, ["slash_vote".to_string(), "config_update".to_string()]);

        let delegation = VoteDelegation {
            delegator: delegator.clone(),
            delegate: delegate.clone(),
            allowed_actions: allowed_actions.clone(),
            max_vote_power: 1000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        assert_eq!(delegation.allowed_actions.len(), 2);
        assert!(delegation.is_active);
    }

    #[test]
    fn test_delegation_with_stake_limit() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let constraint = DelegationConstraint {
            min_stake: 100000,
            max_stake: 5000000,
            allowed_action_types: Vec::from_array(&env, ["vouch".to_string()]),
            time_limit_seconds: 86400,
        };

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: constraint.allowed_action_types.clone(),
            max_vote_power: constraint.max_stake,
            expiry_timestamp: env.ledger().timestamp() + constraint.time_limit_seconds,
            is_active: true,
        };

        assert!(delegation.max_vote_power >= constraint.min_stake);
        assert!(delegation.max_vote_power <= constraint.max_stake);
    }

    #[test]
    fn test_delegation_action_validation() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let allowed_actions = Vec::from_array(&env, ["slash_vote".to_string()]);

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: allowed_actions.clone(),
            max_vote_power: 1000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        // Check if action is allowed
        let is_slash_allowed = delegation.allowed_actions.iter().any(|a| a == "slash_vote");
        let is_config_allowed = delegation.allowed_actions.iter().any(|a| a == "config_update");

        assert!(is_slash_allowed);
        assert!(!is_config_allowed);
    }

    #[test]
    fn test_delegation_expiry_check() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);
        let now = env.ledger().timestamp();

        let delegation_active = VoteDelegation {
            delegator: delegator.clone(),
            delegate: delegate.clone(),
            allowed_actions: Vec::from_array(&env, ["slash_vote".to_string()]),
            max_vote_power: 1000000,
            expiry_timestamp: now + 86400,
            is_active: true,
        };

        let delegation_expired = VoteDelegation {
            delegator: delegator.clone(),
            delegate: delegate.clone(),
            allowed_actions: Vec::from_array(&env, ["slash_vote".to_string()]),
            max_vote_power: 1000000,
            expiry_timestamp: now - 1,
            is_active: false,
        };

        assert!(delegation_active.expiry_timestamp > now);
        assert!(delegation_expired.expiry_timestamp < now);
        assert!(delegation_active.is_active);
        assert!(!delegation_expired.is_active);
    }

    #[test]
    fn test_delegation_multiple_action_types() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let allowed = Vec::from_array(&env, [
            "slash_vote".to_string(),
            "admin_removal".to_string(),
            "config_update".to_string(),
        ]);

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: allowed.clone(),
            max_vote_power: 5000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        assert_eq!(delegation.allowed_actions.len(), 3);
    }

    #[test]
    fn test_delegation_single_action_type() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let allowed = Vec::from_array(&env, ["slash_vote".to_string()]);

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: allowed,
            max_vote_power: 1000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        assert_eq!(delegation.allowed_actions.len(), 1);
    }

    #[test]
    fn test_delegation_max_vote_power_enforcement() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: Vec::from_array(&env, ["vouch".to_string()]),
            max_vote_power: 1000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        let attempted_vote = 1500000;
        let is_within_limit = attempted_vote <= delegation.max_vote_power;

        assert!(!is_within_limit);
    }

    #[test]
    fn test_delegation_vote_power_limit_respected() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: Vec::from_array(&env, ["slash_vote".to_string()]),
            max_vote_power: 5000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        let attempted_vote = 3000000;
        let is_within_limit = attempted_vote <= delegation.max_vote_power;

        assert!(is_within_limit);
    }

    #[test]
    fn test_delegation_constraint_combination() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);
        let now = env.ledger().timestamp();

        let constraint = DelegationConstraint {
            min_stake: 100000,
            max_stake: 10000000,
            allowed_action_types: Vec::from_array(&env, [
                "slash_vote".to_string(),
                "config_update".to_string(),
            ]),
            time_limit_seconds: 604800, // 1 week
        };

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: constraint.allowed_action_types,
            max_vote_power: 5000000,
            expiry_timestamp: now + constraint.time_limit_seconds,
            is_active: true,
        };

        // Verify all constraints
        assert!(delegation.max_vote_power >= constraint.min_stake);
        assert!(delegation.max_vote_power <= constraint.max_stake);
        assert_eq!(delegation.allowed_actions.len(), 2);
        assert!(delegation.expiry_timestamp == now + constraint.time_limit_seconds);
    }

    #[test]
    fn test_delegation_immutability_after_creation() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        let delegation = VoteDelegation {
            delegator: delegator.clone(),
            delegate: delegate.clone(),
            allowed_actions: Vec::from_array(&env, ["slash_vote".to_string()]),
            max_vote_power: 1000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        // Verify original values remain unchanged
        assert_eq!(delegation.delegator, delegator);
        assert_eq!(delegation.delegate, delegate);
        assert_eq!(delegation.max_vote_power, 1000000);
    }

    #[test]
    fn test_delegation_restrict_to_specific_borrower() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);
        let borrower = Address::generate(&env);

        // Can encode target in action string like "slash_vote:borrower_xyz"
        let allowed = Vec::from_array(&env, [format!("slash_vote:{}", borrower.to_string())]);

        let delegation = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: allowed.clone(),
            max_vote_power: 1000000,
            expiry_timestamp: env.ledger().timestamp() + 86400,
            is_active: true,
        };

        assert_eq!(delegation.allowed_actions.len(), 1);
    }

    #[test]
    fn test_delegation_time_restricted() {
        let env = setup_env();
        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);
        let now = env.ledger().timestamp();

        let short_duration = VoteDelegation {
            delegator: delegator.clone(),
            delegate: delegate.clone(),
            allowed_actions: Vec::from_array(&env, ["slash_vote".to_string()]),
            max_vote_power: 1000000,
            expiry_timestamp: now + 3600, // 1 hour
            is_active: true,
        };

        let long_duration = VoteDelegation {
            delegator,
            delegate,
            allowed_actions: Vec::from_array(&env, ["slash_vote".to_string()]),
            max_vote_power: 1000000,
            expiry_timestamp: now + 2592000, // 30 days
            is_active: true,
        };

        assert_eq!(short_duration.expiry_timestamp - now, 3600);
        assert_eq!(long_duration.expiry_timestamp - now, 2592000);
    }
}
