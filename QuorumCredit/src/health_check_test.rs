#[cfg(test)]
mod tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup_uninitialized(env: &Env) -> Address {
        env.register_contract(None, QuorumCreditContract)
    }

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        QuorumCreditContractClient::new(env, &contract_id).initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token,
        );
        (contract_id, admin, token)
    }

    #[test]
    fn test_health_check_uninitialized() {
        let env = Env::default();
        let contract_id = setup_uninitialized(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let status = client.health_check();
        assert!(!status.is_healthy);
        assert!(!status.initialized);
        assert!(!status.yield_reserve_solvent);
    }

    #[test]
    fn test_health_check_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let status = client.health_check();
        assert!(status.initialized);
    }

    #[test]
    fn test_health_check_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.pause(&Vec::from_array(&env, [admin.clone()]));

        let status = client.health_check();
        assert!(status.paused);
    }
}
