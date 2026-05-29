#[cfg(test)]
mod tests {
    use crate::types::{LoanCategory, VouchConditions};
    use crate::*;
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Env,
    };

    fn setup() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register(QuorumCreditContract, ());
        QuorumCreditContractClient::new(&env, &contract_id).initialize(
            &deployer,
            &soroban_sdk::vec![&env, admin.clone()],
            &1u32,
            &token,
        );
        (env, contract_id, admin, token)
    }

    fn fund(env: &Env, token: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(to, &amount);
    }

    /// Vouch with max_loan_amount condition: loan under cap → eligible.
    #[test]
    fn test_conditions_max_loan_amount_satisfied() {
        let (env, contract_id, admin, token) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        fund(&env, &token, &voucher, 10_000_000_000);
        fund(&env, &contract_id, &admin, 10_000_000_000); // yield reserve

        // Vouch only valid for loans ≤ 500 XLM (5_000_000_000 stroops)
        client.vouch_with_conditions(
            &voucher,
            &borrower,
            &5_000_000_000,
            &token,
            &VouchConditions {
                max_loan_amount: Some(5_000_000_000),
                min_loan_amount: None,
            },
        );

        // Request 400 XLM — within cap → should succeed
        client.request_loan(
            &borrower,
            &4_000_000_000,
            &4_000_000_000,
            &soroban_sdk::String::from_str(&env, "test"),
            &token,
            &LoanCategory::Personal,
        );
    }

    /// Vouch with max_loan_amount condition: loan over cap → vouch excluded → InsufficientFunds.
    #[test]
    fn test_conditions_max_loan_amount_excluded() {
        let (env, contract_id, admin, token) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        fund(&env, &token, &voucher, 10_000_000_000);

        // Vouch only valid for loans ≤ 500 XLM
        client.vouch_with_conditions(
            &voucher,
            &borrower,
            &5_000_000_000,
            &token,
            &VouchConditions {
                max_loan_amount: Some(5_000_000_000),
                min_loan_amount: None,
            },
        );

        // Request 600 XLM — exceeds cap → vouch excluded → insufficient stake
        let result = client.try_request_loan(
            &borrower,
            &6_000_000_000,
            &5_000_000_000,
            &soroban_sdk::String::from_str(&env, "test"),
            &token,
            &LoanCategory::Personal,
        );
        assert!(result.is_err());
    }

    /// Vouch with min_loan_amount condition: loan below floor → vouch excluded.
    #[test]
    fn test_conditions_min_loan_amount_excluded() {
        let (env, contract_id, admin, token) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        fund(&env, &token, &voucher, 10_000_000_000);

        // Vouch only valid for loans ≥ 200 XLM
        client.vouch_with_conditions(
            &voucher,
            &borrower,
            &5_000_000_000,
            &token,
            &VouchConditions {
                max_loan_amount: None,
                min_loan_amount: Some(2_000_000_000),
            },
        );

        // Request 100 XLM — below floor → vouch excluded → insufficient stake
        let result = client.try_request_loan(
            &borrower,
            &1_000_000_000,
            &1_000_000_000,
            &soroban_sdk::String::from_str(&env, "test"),
            &token,
            &LoanCategory::Personal,
        );
        assert!(result.is_err());
    }

    /// Mixed vouches: one conditional (excluded), one unconditional (included) → eligible.
    #[test]
    fn test_unconditional_vouch_always_counts() {
        let (env, contract_id, admin, token) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let borrower = Address::generate(&env);

        fund(&env, &token, &voucher1, 10_000_000_000);
        fund(&env, &token, &voucher2, 10_000_000_000);
        fund(&env, &contract_id, &admin, 10_000_000_000);

        // voucher1: conditional — only for loans ≤ 100 XLM (excluded for 600 XLM loan)
        client.vouch_with_conditions(
            &voucher1,
            &borrower,
            &3_000_000_000,
            &token,
            &VouchConditions {
                max_loan_amount: Some(1_000_000_000),
                min_loan_amount: None,
            },
        );

        // Advance past cooldown
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            timestamp: env.ledger().timestamp() + crate::types::DEFAULT_VOUCH_COOLDOWN_SECS + 1,
            ..env.ledger().get()
        });

        // voucher2: unconditional — always counts
        client.vouch(&voucher2, &borrower, &6_000_000_000, &token);

        // Request 600 XLM — voucher1 excluded, voucher2 (6B) covers threshold
        client.request_loan(
            &borrower,
            &6_000_000_000,
            &6_000_000_000,
            &soroban_sdk::String::from_str(&env, "test"),
            &token,
            &LoanCategory::Personal,
        );
    }

    /// get_vouches returns conditions field correctly.
    #[test]
    fn test_get_vouches_includes_conditions() {
        let (env, contract_id, _, token) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        fund(&env, &token, &voucher, 10_000_000_000);

        client.vouch_with_conditions(
            &voucher,
            &borrower,
            &1_000_000_000,
            &token,
            &VouchConditions {
                max_loan_amount: Some(2_000_000_000),
                min_loan_amount: Some(500_000_000),
            },
        );

        let vouches = client.get_vouches(&borrower).unwrap();
        assert_eq!(vouches.len(), 1);
        let v = vouches.get(0).unwrap();
        let cond = v.conditions.unwrap();
        assert_eq!(cond.max_loan_amount, Some(2_000_000_000));
        assert_eq!(cond.min_loan_amount, Some(500_000_000));
    }
}
