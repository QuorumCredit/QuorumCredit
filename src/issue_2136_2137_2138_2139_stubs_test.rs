//! Tests for issues #2136–#2139: stub entrypoints must return NotImplemented.
//!
//! Each group of tests corresponds to one issue:
//!   #2136 — vouch group stubs
//!   #2137 — success-story and similar-borrower stubs
//!   #2138 — insurance setter and claim stubs
//!   #2139 — rebalancing and yield-streaming stubs

#[cfg(test)]
mod issue_2136_2137_2138_2139_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        (env, contract_id)
    }

    // ── Issue #2136: vouch group stubs ────────────────────────────────────────

    #[test]
    fn create_vouch_group_returns_not_implemented() {
        let (env, contract_id) = setup();
        let caller = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::create_vouch_group(
                env.clone(),
                caller.clone(),
                String::from_str(&env, "test"),
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn add_voucher_to_group_returns_not_implemented() {
        let (env, contract_id) = setup();
        let caller = Address::generate(&env);
        let voucher = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::add_voucher_to_group(env.clone(), caller.clone(), 1, voucher.clone())
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn remove_voucher_from_group_returns_not_implemented() {
        let (env, contract_id) = setup();
        let caller = Address::generate(&env);
        let voucher = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::remove_voucher_from_group(env.clone(), caller.clone(), 1, voucher.clone())
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    // ── Issue #2137: success-story and similar-borrower stubs ─────────────────

    #[test]
    fn submit_success_story_returns_not_implemented() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::QuorumCreditContract::submit_success_story(
                env.clone(),
                borrower.clone(),
                String::from_str(&env, "title"),
                String::from_str(&env, "content"),
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn publish_success_story_returns_not_implemented() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::QuorumCreditContract::publish_success_story(
                env.clone(),
                borrower.clone(),
                0,
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn set_success_story_consent_returns_not_implemented() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::QuorumCreditContract::set_success_story_consent(
                env.clone(),
                borrower.clone(),
                true,
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn get_borrower_success_stories_returns_not_implemented() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::QuorumCreditContract::get_borrower_success_stories(
                env.clone(),
                borrower.clone(),
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn find_similar_borrowers_returns_not_implemented() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::QuorumCreditContract::find_similar_borrowers(
                env.clone(),
                borrower.clone(),
                10,
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn get_retention_metrics_returns_not_implemented() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::QuorumCreditContract::get_retention_metrics(
                env.clone(),
                borrower.clone(),
            )
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    // ── Issue #2138: insurance setter and claim stubs ─────────────────────────

    #[test]
    fn set_insurance_coverage_bps_returns_not_implemented() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let result = env.as_contract(&contract_id, || {
            crate::set_insurance_coverage_bps(env.clone(), admin_signers.clone(), 500)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn set_insurance_fee_bps_returns_not_implemented() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let result = env.as_contract(&contract_id, || {
            crate::set_insurance_fee_bps(env.clone(), admin_signers.clone(), 100)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn claim_insurance_returns_not_implemented() {
        let (env, contract_id) = setup();
        let voucher = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::claim_insurance(env.clone(), voucher.clone(), 1)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn contribute_to_insurance_returns_not_implemented() {
        let (env, contract_id) = setup();
        let caller = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::contribute_to_insurance(env.clone(), caller.clone(), 1_000)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn purchase_slash_insurance_returns_not_implemented() {
        let (env, contract_id) = setup();
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::purchase_slash_insurance(env.clone(), voucher.clone(), borrower.clone())
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    // ── Issue #2139: rebalancing and yield-streaming stubs ────────────────────

    #[test]
    fn rebalance_pools_returns_not_implemented() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let result = env.as_contract(&contract_id, || {
            crate::rebalance_pools(env.clone(), admin_signers.clone(), 1, 2, 500)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn auto_rebalance_pools_returns_not_implemented() {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let result = env.as_contract(&contract_id, || {
            crate::auto_rebalance_pools(env.clone(), admin_signers.clone(), 1_000)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn get_pool_liquidity_returns_not_implemented() {
        let (env, contract_id) = setup();
        let result = env.as_contract(&contract_id, || {
            crate::get_pool_liquidity(env.clone(), 1)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }

    #[test]
    fn claim_streamed_yield_returns_not_implemented() {
        let (env, contract_id) = setup();
        let caller = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            crate::claim_streamed_yield(env.clone(), caller.clone(), 1)
        });
        assert_eq!(result, Err(ContractError::NotImplemented));
    }
}
