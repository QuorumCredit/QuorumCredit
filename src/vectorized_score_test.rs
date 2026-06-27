/// Issue #940: Vectorized Score Updates — batch credit score updates
#[cfg(test)]
mod vectorized_score_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &100_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    #[test]
    fn test_batch_update_credit_scores_returns_counts() {
        let s = setup();

        let b1 = Address::generate(&s.env);
        let b2 = Address::generate(&s.env);
        let b3 = Address::generate(&s.env);

        // Enable credit score config so updates succeed.
        let mut cfg = s.client.get_credit_score_config();
        cfg.enabled = true;
        s.client.set_credit_score_config(&admins(&s), &cfg);

        let borrowers = Vec::from_array(&s.env, [b1.clone(), b2.clone(), b3.clone()]);
        let (updated, skipped) = s.client.batch_update_credit_scores(&admins(&s), &borrowers);

        // All 3 should succeed (neutral default scores computed for new borrowers).
        assert_eq!(updated, 3);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_batch_update_empty_list() {
        let s = setup();
        let empty: Vec<Address> = Vec::new(&s.env);
        let (updated, skipped) = s.client.batch_update_credit_scores(&admins(&s), &empty);
        assert_eq!(updated, 0);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_batch_update_skips_when_credit_score_disabled() {
        let s = setup();

        // Credit score disabled by default — all updates should be skipped.
        let b1 = Address::generate(&s.env);
        let borrowers = Vec::from_array(&s.env, [b1.clone()]);
        let (updated, skipped) = s.client.batch_update_credit_scores(&admins(&s), &borrowers);

        assert_eq!(updated, 0);
        assert_eq!(skipped, 1);
    }
}
