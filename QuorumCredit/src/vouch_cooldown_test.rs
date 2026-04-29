#[cfg(test)]
mod vouch_cooldown_tests {
    use crate::errors::ContractError;
    use crate::types::DEFAULT_VOUCH_COOLDOWN_SECS;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
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
        StellarAssetClient::new(env, &token_id).mint(&voucher, &10_000_000);
        (contract_id, token_id, voucher, Address::generate(env))
    }

    /// Second vouch from the same voucher within the cooldown window must be rejected.
    #[test]
    fn test_vouch_cooldown_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, voucher, borrower1) = setup(&env);
        let borrower2 = Address::generate(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // First vouch succeeds.
        client.vouch(&voucher, &borrower1, &1_000_000, &token_id);

        // Immediately try a second vouch (same voucher, different borrower) — must fail.
        let result = client.try_vouch(&voucher, &borrower2, &1_000_000, &token_id);
        assert_eq!(result, Err(Ok(ContractError::VouchCooldownActive)));
    }

    /// Vouch after the cooldown window has elapsed must succeed.
    #[test]
    fn test_vouch_after_cooldown_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, voucher, borrower1) = setup(&env);
        let borrower2 = Address::generate(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // First vouch.
        client.vouch(&voucher, &borrower1, &1_000_000, &token_id);

        // Advance ledger time past the cooldown.
        env.ledger().with_mut(|l| {
            l.timestamp += DEFAULT_VOUCH_COOLDOWN_SECS;
        });

        // Second vouch after cooldown must succeed.
        let result = client.try_vouch(&voucher, &borrower2, &1_000_000, &token_id);
        assert!(result.is_ok());
    }

    /// First-ever vouch (no prior timestamp) must not be blocked.
    #[test]
    fn test_first_vouch_not_blocked() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, voucher, borrower) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let result = client.try_vouch(&voucher, &borrower, &1_000_000, &token_id);
        assert!(result.is_ok());
    }
}
