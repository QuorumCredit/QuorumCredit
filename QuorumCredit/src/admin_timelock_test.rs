#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_queue_admin_action() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        let delay = 48 * 60 * 60; // 48 hours
        let action = AdminTimelockAction::Pause;
        let action_id = QuorumCreditContract::queue_admin_action(
            env.clone(),
            vec![&env, admin.clone()],
            action,
            delay,
        )
        .unwrap();

        let timelock = QuorumCreditContract::get_admin_timelock(env.clone(), action_id).unwrap();
        assert_eq!(timelock.id, action_id);
        assert!(!timelock.executed);
    }

    #[test]
    fn test_execute_admin_action_after_delay() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        let delay = 1; // 1 second
        let action = AdminTimelockAction::Pause;
        let action_id = QuorumCreditContract::queue_admin_action(
            env.clone(),
            vec![&env, admin.clone()],
            action,
            delay,
        )
        .unwrap();

        // Advance ledger time
        env.ledger().with_mut(|l| {
            l.timestamp = l.timestamp + 2;
        });

        QuorumCreditContract::execute_admin_action(env.clone(), action_id).unwrap();

        let timelock = QuorumCreditContract::get_admin_timelock(env.clone(), action_id).unwrap();
        assert!(timelock.executed);
    }

    #[test]
    fn test_cancel_admin_action() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        let delay = 48 * 60 * 60;
        let action = AdminTimelockAction::Pause;
        let action_id = QuorumCreditContract::queue_admin_action(
            env.clone(),
            vec![&env, admin.clone()],
            action,
            delay,
        )
        .unwrap();

        QuorumCreditContract::cancel_admin_action(env.clone(), admin.clone(), action_id).unwrap();

        let timelock = QuorumCreditContract::get_admin_timelock(env.clone(), action_id).unwrap();
        assert!(timelock.cancelled);
    }
}
