#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_set_admin_key_expiry() {
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

        let future_time = env.ledger().timestamp() + 86400; // 1 day from now
        QuorumCreditContract::set_admin_key_expiry(
            env.clone(),
            vec![&env, admin.clone()],
            admin.clone(),
            future_time,
        );

        let expiry = QuorumCreditContract::get_admin_key_expiry(env.clone(), admin.clone());
        assert_eq!(expiry, future_time);
    }

    #[test]
    fn test_rotate_admin_clears_expiry() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);
        let new_admin = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        let future_time = env.ledger().timestamp() + 86400;
        QuorumCreditContract::set_admin_key_expiry(
            env.clone(),
            vec![&env, admin.clone()],
            admin.clone(),
            future_time,
        );

        QuorumCreditContract::rotate_admin(
            env.clone(),
            vec![&env, admin.clone()],
            admin.clone(),
            new_admin.clone(),
        );

        let expiry = QuorumCreditContract::get_admin_key_expiry(env.clone(), new_admin.clone());
        assert_eq!(expiry, 0); // Expiry cleared for new admin
    }
}
