#[cfg(test)]
mod loan_features_tests {
    use crate::{
        LoanExtensionRequest, LoanPrivacyLevel, QuorumCreditContract,
        QuorumCreditContractClient,
    };
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Env, String, Vec,
    };

    fn setup() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin1.clone())
            .address();

        let contract_id = env.register(QuorumCreditContract, ());
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &2, &token);

        (env, client, admin1, admin2, deployer, token)
    }

    fn setup_with_loan() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
        Address,
        Address,
    ) {
        let (env, client, admin1, admin2, deployer, token) = setup();

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        let token_admin = StellarAssetClient::new(&env, &token);
        token_admin.mint(&voucher, &10_000_000_000);
        token_admin.mint(&borrower, &10_000_000_000);
        token_admin.mint(&client.address, &10_000_000_000);

        client.vouch(&voucher, &borrower, &1_000_000_000, &token, &None);

        client.request_loan(
            &borrower,
            &500_000_000,
            &500_000_000,
            &String::from_str(&env, "test loan"),
            &token,
        );

        (env, client, admin1, admin2, borrower, voucher, token)
    }

    // ── #883: Loan Term Extension Tests ─────────────────────────────────────

    #[test]
    fn test_request_extension() {
        let (env, client, _admin1, _admin2, borrower, _voucher, _token) = setup_with_loan();

        let extension_secs: u64 = 7 * 24 * 60 * 60; // 7 days
        client.request_extension(&borrower, &extension_secs);

        let request = client.get_extension_request(&borrower);
        assert!(request.is_some());
        let req = request.unwrap();
        assert_eq!(req.extension_secs, extension_secs);
        assert_eq!(req.borrower, borrower);
    }

    #[test]
    fn test_approve_extension_extends_deadline() {
        let (env, client, _admin1, _admin2, borrower, voucher, _token) = setup_with_loan();

        let loan_before = client.get_loan(&borrower).unwrap();
        let deadline_before = loan_before.deadline;

        let extension_secs: u64 = 7 * 24 * 60 * 60;
        client.request_extension(&borrower, &extension_secs);

        client.approve_extension(&voucher, &borrower);

        let loan_after = client.get_loan(&borrower).unwrap();
        assert_eq!(loan_after.deadline, deadline_before + extension_secs);

        // Request should be cleared
        assert!(client.get_extension_request(&borrower).is_none());
    }

    #[test]
    fn test_extension_no_active_request() {
        let (_env, client, _admin1, _admin2, _borrower, _voucher, _token) = setup_with_loan();

        let request = client.get_extension_request(&_borrower);
        assert!(request.is_none());
    }

    // ── #882: Loan Insurance Integration Tests ──────────────────────────────

    #[test]
    fn test_contribute_to_insurance() {
        let (env, client, admin1, _admin2, _deployer, token) = setup();

        let contributor = Address::generate(&env);
        let token_admin = StellarAssetClient::new(&env, &token);
        token_admin.mint(&contributor, &10_000_000_000);

        client.contribute_to_insurance(&contributor, &1_000_000);

        let balance = client.get_insurance_pool_balance();
        assert!(balance >= 1_000_000);
    }

    #[test]
    fn test_insurance_fee_collection_on_loan() {
        let (_env, client, _admin1, _admin2, _borrower, _voucher, _token) = setup_with_loan();

        let balance = client.get_insurance_pool_balance();
        // Insurance fee should have been collected during loan creation
        assert!(balance > 0);
    }

    #[test]
    fn test_set_insurance_fee_bps() {
        let (_env, client, admin1, admin2, _deployer, _token) = setup();
        let admins = Vec::from_array(&_env, [admin1.clone(), admin2.clone()]);

        client.set_insurance_fee_bps(&admins, &100);
        let fee = client.get_insurance_fee_bps();
        assert_eq!(fee, 100);
    }

    #[test]
    fn test_set_insurance_coverage_bps() {
        let (_env, client, admin1, admin2, _deployer, _token) = setup();
        let admins = Vec::from_array(&_env, [admin1.clone(), admin2.clone()]);

        client.set_insurance_coverage_bps(&admins, &5000);
        let coverage = client.get_insurance_coverage_bps();
        assert_eq!(coverage, 5000);
    }

    #[test]
    fn test_purchase_slash_insurance() {
        let (_env, client, _admin1, _admin2, borrower, voucher, _token) = setup_with_loan();

        let premium = client.purchase_slash_insurance(&voucher, &borrower);
        assert!(premium >= 0);

        let insured = client.is_voucher_insured(&voucher, &borrower);
        assert!(insured);
    }

    // ── #884: Prepayment Bonus Tests ────────────────────────────────────────

    #[test]
    fn test_set_prepayment_bonus_bps() {
        let (_env, client, admin1, admin2, _deployer, _token) = setup();
        let admins = Vec::from_array(&_env, [admin1.clone(), admin2.clone()]);

        client.set_prepayment_bonus_bps(&admins, &100);
        let bonus = client.get_prepayment_bonus_bps();
        assert_eq!(bonus, 100);
    }

    #[test]
    fn test_default_prepayment_bonus_bps() {
        let (_env, client, _admin1, _admin2, _deployer, _token) = setup();

        let bonus = client.get_prepayment_bonus_bps();
        assert_eq!(bonus, 50); // DEFAULT_PREPAYMENT_BONUS_BPS
    }

    // ── #885: Loan Status Privacy Tests ─────────────────────────────────────

    #[test]
    fn test_default_privacy_is_public() {
        let (env, client, _admin1, _admin2, _deployer, _token) = setup();

        let borrower = Address::generate(&env);
        let privacy = client.get_loan_privacy(&borrower);
        assert_eq!(privacy, LoanPrivacyLevel::Public);
    }

    #[test]
    fn test_set_privacy_to_private() {
        let (env, client, _admin1, _admin2, _deployer, _token) = setup();

        let borrower = Address::generate(&env);
        client.set_loan_privacy(&borrower, &LoanPrivacyLevel::Private);
        let privacy = client.get_loan_privacy(&borrower);
        assert_eq!(privacy, LoanPrivacyLevel::Private);
    }

    #[test]
    fn test_set_privacy_to_vouchers_only() {
        let (env, client, _admin1, _admin2, _deployer, _token) = setup();

        let borrower = Address::generate(&env);
        client.set_loan_privacy(&borrower, &LoanPrivacyLevel::VouchersOnly);
        let privacy = client.get_loan_privacy(&borrower);
        assert_eq!(privacy, LoanPrivacyLevel::VouchersOnly);
    }

    #[test]
    fn test_privacy_borrower_can_view_own_loan() {
        let (_env, client, _admin1, _admin2, borrower, _voucher, _token) = setup_with_loan();

        client.set_loan_privacy(&borrower, &LoanPrivacyLevel::Private);
        let result = client.get_loan_with_privacy(&borrower, &borrower);
        assert!(result.is_some());
    }

    #[test]
    fn test_privacy_voucher_can_view_vouchers_only() {
        let (_env, client, _admin1, _admin2, borrower, voucher, _token) = setup_with_loan();

        client.set_loan_privacy(&borrower, &LoanPrivacyLevel::VouchersOnly);
        let result = client.get_loan_with_privacy(&borrower, &voucher);
        assert!(result.is_some());
    }
}
