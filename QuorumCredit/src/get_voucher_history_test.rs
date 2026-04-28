/// Issue #461: get_voucher_history query function tests
#[cfg(test)]
mod get_voucher_history_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token_id,
        );
        (contract_id, token_id, admin)
    }

    /// A voucher with no vouches should return an empty history.
    #[test]
    fn test_get_voucher_history_empty_for_unknown_voucher() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let unknown = Address::generate(&env);
        let history = client.get_voucher_history(&unknown);
        assert_eq!(history.len(), 0);
    }

    /// After vouching for a borrower, that borrower should appear in the voucher's history.
    #[test]
    fn test_get_voucher_history_tracks_borrower_after_vouch() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        StellarAssetClient::new(&env, &token_id).mint(&voucher, &1_000_000);
        client.vouch(&voucher, &borrower, &100_000, &token_id);

        let history = client.get_voucher_history(&voucher);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), borrower);
    }

    /// Vouching for multiple borrowers should record all of them in history.
    #[test]
    fn test_get_voucher_history_tracks_multiple_borrowers() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let borrower_a = Address::generate(&env);
        let borrower_b = Address::generate(&env);

        StellarAssetClient::new(&env, &token_id).mint(&voucher, &2_000_000);
        client.vouch(&voucher, &borrower_a, &100_000, &token_id);
        client.vouch(&voucher, &borrower_b, &100_000, &token_id);

        let history = client.get_voucher_history(&voucher);
        assert_eq!(history.len(), 2);
    }
}
