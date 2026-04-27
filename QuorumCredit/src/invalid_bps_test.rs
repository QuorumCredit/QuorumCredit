/// Tests verifying that yield_bps values outside 0–10,000 are rejected.
#[cfg(test)]
mod invalid_bps_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup() -> (Env, QuorumCreditContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token.address());

        (env, client, admin)
    }

    #[test]
    #[should_panic]
    fn test_set_config_rejects_yield_bps_above_10000() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        let mut cfg = client.get_config();
        cfg.yield_bps = 10_001;
        client.set_config(&admin_signers, &cfg);
    }

    #[test]
    #[should_panic]
    fn test_update_config_rejects_yield_bps_above_10000() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        client.update_config(&admin_signers, &Some(50_000), &None);
    }

    #[test]
    fn test_set_config_accepts_yield_bps_at_boundary() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        let mut cfg = client.get_config();
        cfg.yield_bps = 10_000;
        client.set_config(&admin_signers, &cfg); // must not panic
        assert_eq!(client.get_config().yield_bps, 10_000);
    }

    #[test]
    #[should_panic]
    fn test_set_config_rejects_slash_bps_below_zero() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        let mut cfg = client.get_config();
        cfg.slash_bps = -1;
        client.set_config(&admin_signers, &cfg);
    }

    #[test]
    #[should_panic]
    fn test_set_config_rejects_slash_bps_above_10000() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        let mut cfg = client.get_config();
        cfg.slash_bps = 10_001;
        client.set_config(&admin_signers, &cfg);
    }

    #[test]
    #[should_panic]
    fn test_update_config_rejects_slash_bps_below_zero() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        client.update_config(&admin_signers, &None, &Some(-1));
    }

    #[test]
    #[should_panic]
    fn test_update_config_rejects_slash_bps_above_10000() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        client.update_config(&admin_signers, &None, &Some(50_000));
    }

    #[test]
    fn test_update_config_accepts_slash_bps_at_boundary() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        client.update_config(&admin_signers, &None, &Some(10_000));
        assert_eq!(client.get_config().slash_bps, 10_000);
    }
}
