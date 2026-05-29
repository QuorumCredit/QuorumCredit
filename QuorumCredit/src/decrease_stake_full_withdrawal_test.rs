#[cfg(test)]
mod decrease_stake_full_withdrawal_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Env, Vec,
    };

    fn setup(env: &Env) -> (Address, Address, Address, Address) {
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
        let voucher = Address::generate(env);
        StellarAssetClient::new(env, &token_id).mint(&voucher, &1_000_000);
        (contract_id, token_id, voucher, Address::generate(env))
    }

    /// decrease_stake() with amount == full stake removes the vouch and returns funds.
    #[test]
    fn test_decrease_stake_full_withdrawal_removes_vouch_and_returns_funds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Voucher A vouches for borrower B with 1_000_000 stroops.
        client.vouch(&voucher, &borrower, &1_000_000, &token_id);

        // Confirm balance left voucher.
        let balance_after_vouch = TokenClient::new(&env, &token_id).balance(&voucher);
        assert_eq!(balance_after_vouch, 0);

        // Full withdrawal: decrease_stake by the entire staked amount.
        client.decrease_stake(&voucher, &borrower, &1_000_000);

        // Vouch must be removed from the list.
        let vouches = client.get_vouches(&borrower);
        assert!(
            vouches.is_none() || vouches.unwrap().is_empty(),
            "vouch should be removed after full withdrawal"
        );

        // Funds must be returned to voucher.
        let balance_after_withdrawal = TokenClient::new(&env, &token_id).balance(&voucher);
        assert_eq!(balance_after_withdrawal, 1_000_000);
    }
}
