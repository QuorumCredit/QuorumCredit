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
    fn test_set_admin_key_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let future_time = env.ledger().timestamp() + 86400;
        client.set_admin_key_expiry(&Vec::from_array(&env, [admin.clone()]), &admin, &future_time);

        let expiry = client.get_admin_key_expiry(&admin);
        assert_eq!(expiry, future_time);
    }

    #[test]
    fn test_rotate_admin_clears_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, _token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let new_admin = Address::generate(&env);

        let future_time = env.ledger().timestamp() + 86400;
        client.set_admin_key_expiry(&Vec::from_array(&env, [admin.clone()]), &admin, &future_time);

        client.rotate_admin(&Vec::from_array(&env, [admin.clone()]), &admin, &new_admin);

        let expiry = client.get_admin_key_expiry(&new_admin);
        assert_eq!(expiry, 0);
    }
}
