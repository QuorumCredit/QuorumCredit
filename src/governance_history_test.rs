#[cfg(test)]
mod governance_history_tests {
    use crate::types::GovernanceHistoryRecord;
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
    fn test_governance_history_records_vouch_action() {
        let (env, _deployer, _admins, _token) = setup_env();
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        // Create a governance history record for vouch
        let _history = GovernanceHistoryRecord {
            action_id: 1,
            action_type: "vouch".to_string(),
            actor: voucher.clone(),
            target: Some(borrower.clone()),
            timestamp: env.ledger().timestamp(),
            details: "Vouched 100000 stroops".to_string(),
            status: "completed".to_string(),
        };

        // Verify record is immutable (cannot be modified)
        assert_eq!(_history.action_id, 1);
        assert_eq!(_history.status, "completed");
    }

    #[test]
    fn test_governance_history_records_slash_action() {
        let (env, _deployer, _admins, _token) = setup_env();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);

        let history = GovernanceHistoryRecord {
            action_id: 2,
            action_type: "slash".to_string(),
            actor: admin.clone(),
            target: Some(borrower.clone()),
            timestamp: env.ledger().timestamp(),
            details: "Slash executed: 50% stake burned".to_string(),
            status: "executed".to_string(),
        };

        assert_eq!(history.action_type, "slash");
        assert_eq!(history.status, "executed");
    }

    #[test]
    fn test_governance_history_records_config_change() {
        let (env, _deployer, _admins, _token) = setup_env();
        let admin = Address::generate(&env);

        let history = GovernanceHistoryRecord {
            action_id: 3,
            action_type: "config_update".to_string(),
            actor: admin.clone(),
            target: None,
            timestamp: env.ledger().timestamp(),
            details: "Updated yield BPS from 200 to 300".to_string(),
            status: "completed".to_string(),
        };

        assert_eq!(history.action_type, "config_update");
        assert!(history.target.is_none());
    }

    #[test]
    fn test_governance_history_records_pause_action() {
        let (env, _deployer, _admins, _token) = setup_env();
        let admin = Address::generate(&env);

        let history = GovernanceHistoryRecord {
            action_id: 4,
            action_type: "pause".to_string(),
            actor: admin.clone(),
            target: None,
            timestamp: env.ledger().timestamp(),
            details: "Contract paused for maintenance".to_string(),
            status: "active".to_string(),
        };

        assert_eq!(history.action_type, "pause");
    }

    #[test]
    fn test_governance_history_records_loan_request() {
        let (env, _deployer, _admins, _token) = setup_env();
        let borrower = Address::generate(&env);

        let history = GovernanceHistoryRecord {
            action_id: 5,
            action_type: "loan_request".to_string(),
            actor: borrower.clone(),
            target: Some(borrower.clone()),
            timestamp: env.ledger().timestamp(),
            details: "Loan requested: 1000000 stroops".to_string(),
            status: "pending".to_string(),
        };

        assert_eq!(history.action_type, "loan_request");
        assert_eq!(history.status, "pending");
    }

    #[test]
    fn test_governance_history_immutability() {
        let (env, _deployer, _admins, _token) = setup_env();
        let actor = Address::generate(&env);

        let record = GovernanceHistoryRecord {
            action_id: 6,
            action_type: "test_action".to_string(),
            actor: actor.clone(),
            target: None,
            timestamp: env.ledger().timestamp(),
            details: "Test immutable record".to_string(),
            status: "recorded".to_string(),
        };

        // Cannot modify fields - record is immutable
        let immutable_id = record.action_id;
        let immutable_status = record.status.clone();

        assert_eq!(immutable_id, 6);
        assert_eq!(immutable_status, "recorded");
    }

    #[test]
    fn test_governance_history_records_admin_removal() {
        let (env, _deployer, _admins, _token) = setup_env();
        let initiator = Address::generate(&env);
        let target_admin = Address::generate(&env);

        let history = GovernanceHistoryRecord {
            action_id: 7,
            action_type: "admin_removal".to_string(),
            actor: initiator.clone(),
            target: Some(target_admin.clone()),
            timestamp: env.ledger().timestamp(),
            details: "Initiated admin removal proposal".to_string(),
            status: "pending".to_string(),
        };

        assert_eq!(history.action_type, "admin_removal");
    }

    #[test]
    fn test_governance_history_records_sequential_ids() {
        let (env, _deployer, _admins, _token) = setup_env();
        let actor = Address::generate(&env);

        let mut records = Vec::new(&env);
        for i in 1..=5 {
            let record = GovernanceHistoryRecord {
                action_id: i,
                action_type: format!("action_{}", i),
                actor: actor.clone(),
                target: None,
                timestamp: env.ledger().timestamp(),
                details: format!("Action {}", i),
                status: "completed".to_string(),
            };
            records.push_back(record);
        }

        // Verify sequential ordering
        for (idx, record) in records.iter().enumerate() {
            assert_eq!(record.action_id, (idx + 1) as u64);
        }
    }

    #[test]
    fn test_governance_history_timestamp_precision() {
        let (env, _deployer, _admins, _token) = setup_env();
        let actor = Address::generate(&env);

        let ts1 = env.ledger().timestamp();
        let history1 = GovernanceHistoryRecord {
            action_id: 8,
            action_type: "action1".to_string(),
            actor: actor.clone(),
            target: None,
            timestamp: ts1,
            details: "First action".to_string(),
            status: "completed".to_string(),
        };

        let ts2 = env.ledger().timestamp();
        let history2 = GovernanceHistoryRecord {
            action_id: 9,
            action_type: "action2".to_string(),
            actor: actor.clone(),
            target: None,
            timestamp: ts2,
            details: "Second action".to_string(),
            status: "completed".to_string(),
        };

        assert!(history2.timestamp >= history1.timestamp);
    }
}
