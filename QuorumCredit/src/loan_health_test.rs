#[cfg(test)]
mod tests {
    use crate::types::{HealthAlertThresholds, LoanCategory, RiskLevel};
    use crate::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env,
    };

    fn setup_env() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        (env, deployer, admin, token)
    }

    fn init_contract(env: &Env, deployer: &Address, admin: &Address, token: &Address) -> Address {
        let contract_id = env.register(QuorumCreditContract, ());
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(
            deployer,
            &soroban_sdk::vec![env, admin.clone()],
            &1u32,
            token,
        );
        contract_id
    }

    #[test]
    fn test_get_loan_health_no_active_loan() {
        let (env, deployer, admin, token) = setup_env();
        let contract_id = init_contract(&env, &deployer, &admin, &token);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let borrower = Address::generate(&env);
        let result = client.get_loan_health(&borrower);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_at_risk_loans_empty() {
        let (env, deployer, admin, token) = setup_env();
        let contract_id = init_contract(&env, &deployer, &admin, &token);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let at_risk = client.get_at_risk_loans();
        assert_eq!(at_risk.len(), 0);
    }

    #[test]
    fn test_get_voucher_exposure_no_loans() {
        let (env, deployer, admin, token) = setup_env();
        let contract_id = init_contract(&env, &deployer, &admin, &token);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let report = client.get_voucher_exposure(&voucher);
        assert_eq!(report.total_active_stake, 0);
        assert_eq!(report.active_loan_count, 0);
        assert_eq!(report.at_risk_count, 0);
    }

    #[test]
    fn test_get_protocol_health_no_loans() {
        let (env, deployer, admin, token) = setup_env();
        let contract_id = init_contract(&env, &deployer, &admin, &token);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let report = client.get_protocol_health();
        assert_eq!(report.active_loan_count, 0);
        assert_eq!(report.at_risk_loan_count, 0);
        assert_eq!(report.total_outstanding, 0);
    }

    #[test]
    fn test_set_and_get_health_alert_thresholds() {
        let (env, deployer, admin, token) = setup_env();
        let contract_id = init_contract(&env, &deployer, &admin, &token);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let thresholds = HealthAlertThresholds {
            at_risk_deadline_secs: 3 * 24 * 60 * 60,
            critical_deadline_secs: 12 * 60 * 60,
            at_risk_repayment_bps: 3000,
            concentration_risk_bps: 7000,
        };
        client.set_health_alert_thresholds(&soroban_sdk::vec![&env, admin.clone()], &thresholds);

        let stored = client.get_health_alert_thresholds();
        assert_eq!(stored.at_risk_deadline_secs, 3 * 24 * 60 * 60);
        assert_eq!(stored.critical_deadline_secs, 12 * 60 * 60);
        assert_eq!(stored.at_risk_repayment_bps, 3000);
        assert_eq!(stored.concentration_risk_bps, 7000);
    }

    #[test]
    fn test_default_health_alert_thresholds() {
        let (env, deployer, admin, token) = setup_env();
        let contract_id = init_contract(&env, &deployer, &admin, &token);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let defaults = client.get_health_alert_thresholds();
        assert_eq!(defaults.at_risk_deadline_secs, 7 * 24 * 60 * 60);
        assert_eq!(defaults.critical_deadline_secs, 24 * 60 * 60);
        assert_eq!(defaults.at_risk_repayment_bps, 2500);
        assert_eq!(defaults.concentration_risk_bps, 8000);
    }

    #[test]
    fn test_get_loan_health_active_loan() {
        use soroban_sdk::testutils::MockAuth;
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_admin = StellarAssetClient::new(&env, &token);

        let contract_id = env.register(QuorumCreditContract, ());
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(
            &deployer,
            &soroban_sdk::vec![&env, admin.clone()],
            &1u32,
            &token,
        );

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        // Fund voucher and contract
        token_admin.mint(&voucher, &10_000_000_000);
        token_admin.mint(&contract_id, &10_000_000_000);

        let stake = 5_000_000_000i128;
        client.vouch(&voucher, &borrower, &stake, &token);

        let loan_amount = 1_000_000_000i128;
        client.request_loan(
            &borrower,
            &loan_amount,
            &stake,
            &soroban_sdk::String::from_str(&env, "test"),
            &token,
            &crate::types::LoanCategory::Personal,
        );

        let health = client.get_loan_health(&borrower);
        assert!(health.is_some());
        let h = health.unwrap();
        assert_eq!(h.borrower, borrower);
        assert!(h.score <= 100);
        // New loan with no repayment and far deadline should be healthy
        assert_eq!(h.risk_level, RiskLevel::Healthy);
    }
}
