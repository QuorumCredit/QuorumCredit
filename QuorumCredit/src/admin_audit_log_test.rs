#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_admin_audit_log_records_actions() {
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

        let new_admin = Address::random(&env);
        QuorumCreditContract::add_admin(env.clone(), vec![&env, admin.clone()], new_admin.clone());

        let log = QuorumCreditContract::get_admin_audit_log(env.clone());
        assert!(!log.is_empty());
        assert_eq!(log.get(0).unwrap().admin, admin);
    }

    #[test]
    fn test_pause_logged() {
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

        QuorumCreditContract::pause(env.clone(), vec![&env, admin.clone()]);

        let log = QuorumCreditContract::get_admin_audit_log(env.clone());
        assert!(!log.is_empty());
        let last_entry = log.get(log.len() - 1).unwrap();
        assert_eq!(last_entry.admin, admin);
    }
}
