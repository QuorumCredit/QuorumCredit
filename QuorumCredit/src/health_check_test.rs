#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{testutils::*, Address, Env};

    #[test]
    fn test_health_check_uninitialized() {
        let env = Env::default();
        let contract = QuorumCreditContract;

        let status = contract.health_check(env.clone());
        assert!(!status.is_healthy);
        assert!(!status.initialized);
        assert!(!status.yield_reserve_solvent);
    }

    #[test]
    fn test_health_check_initialized() {
        let env = Env::default();
        let contract = QuorumCreditContract;
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        let admins = soroban_sdk::vec![&env, admin.clone()];
        contract
            .initialize(env.clone(), deployer, admins, 1, token)
            .unwrap();

        let status = contract.health_check(env.clone());
        assert!(status.initialized);
    }

    #[test]
    fn test_health_check_paused() {
        let env = Env::default();
        let contract = QuorumCreditContract;
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let token = Address::random(&env);

        let admins = soroban_sdk::vec![&env, admin.clone()];
        contract
            .initialize(env.clone(), deployer, admins.clone(), 1, token)
            .unwrap();

        contract.pause(env.clone(), admins);

        let status = contract.health_check(env.clone());
        assert!(status.paused);
    }
}
