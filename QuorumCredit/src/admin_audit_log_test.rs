#[cfg(test)]
mod tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

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
    fn test_admin_audit_log_records_actions() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let new_admin = Address::generate(&env);
        client.add_admin(&Vec::from_array(&env, [admin.clone()]), &new_admin);

        let log = client.get_admin_audit_log();
        assert!(!log.is_empty());
        assert_eq!(log.get(0).unwrap().admin, admin);
    }

    #[test]
    fn test_pause_logged() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.pause(&Vec::from_array(&env, [admin.clone()]));

        let log = client.get_admin_audit_log();
        assert!(!log.is_empty());
        let last_entry = log.get(log.len() - 1).unwrap();
        assert_eq!(last_entry.admin, admin);
    }
}
