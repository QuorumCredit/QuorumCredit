#[cfg(test)]
mod tests {
    use crate::{AdminTimelockAction, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, Vec};

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
    fn test_queue_admin_action() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let delay = 48 * 60 * 60u64;
        let action = AdminTimelockAction::Pause;
        let action_id = client.queue_admin_action(&Vec::from_array(&env, [admin.clone()]), &action, &delay);

        let timelock = client.get_admin_timelock(&action_id).unwrap();
        assert_eq!(timelock.id, action_id);
        assert!(!timelock.executed);
    }

    #[test]
    fn test_execute_admin_action_after_delay() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let delay = 1u64;
        let action = AdminTimelockAction::Pause;
        let action_id = client.queue_admin_action(&Vec::from_array(&env, [admin.clone()]), &action, &delay);

        env.ledger().with_mut(|l| l.timestamp += 2);

        client.execute_admin_action(&action_id);

        let timelock = client.get_admin_timelock(&action_id).unwrap();
        assert!(timelock.executed);
    }

    #[test]
    fn test_cancel_admin_action() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let delay = 48 * 60 * 60u64;
        let action = AdminTimelockAction::Pause;
        let action_id = client.queue_admin_action(&Vec::from_array(&env, [admin.clone()]), &action, &delay);

        client.cancel_admin_action(&admin, &action_id);

        let timelock = client.get_admin_timelock(&action_id).unwrap();
        assert!(timelock.cancelled);
    }
}
