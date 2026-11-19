//! Tests for issue #2069: a voucher with no backed-borrower history must not
//! receive a negative diversification bonus. The yield for such a vouch must
//! equal the base yield exactly.
#[cfg(test)]
mod diversification_empty_history_tests {
    use crate::types::DataKey;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    #[test]
    fn empty_voucher_history_yields_base_bps_not_penalty() {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 120);

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id.address()).mint(&voucher, &1_000_000);
        client.vouch(&voucher, &borrower, &1_000_000, &token_id.address(), &None);

        let vouch = client.get_vouches(&borrower).unwrap().get(0).unwrap();
        let now = env.ledger().timestamp();

        // Remove the voucher's history so it is genuinely empty.
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .remove(&DataKey::VoucherHistory(voucher.clone()));
            let base = crate::helpers::config(&env).yield_bps;
            let bps = crate::loan::vouch_yield_bps(&env, &vouch, &borrower, now);
            assert_eq!(bps, base);
        });
    }
}
