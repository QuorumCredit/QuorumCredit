/// Specification Tests — Issue #107
///
/// These tests are auto-generated from the README / API spec. Each test
/// directly maps to a documented behaviour in the public API reference,
/// error reference, or "How It Works" section of the spec.
///
/// Naming convention:  spec_<section>_<behaviour>
/// All amounts are in stroops (1 XLM = 10_000_000 stroops).
#[cfg(test)]
mod spec_tests {
    use crate::{ContractError, LoanStatus, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    const ONE_XLM: i128 = 10_000_000;

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token: Address,
        contract_id: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address())
            .mint(&contract_id, &(500 * ONE_XLM));
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 200);
        Setup { env, client, admin, token: token_id.address(), contract_id }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    fn do_vouch(s: &Setup, voucher: &Address, borrower: &Address, stake: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(voucher, &stake);
        s.client.vouch(voucher, borrower, &stake, &s.token, &None);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
    }

    fn do_loan(s: &Setup, borrower: &Address, amount: i128) {
        s.client.request_loan(
            borrower,
            &amount,
            &(amount / 2),
            &String::from_str(&s.env, "spec test"),
            &s.token,
        );
    }

    // ── Spec: initialize (one-time setup) ─────────────────────────────────────

    /// Spec: initialize is one-time only — AlreadyInitialized on second call.
    #[test]
    fn spec_initialize_already_initialized() {
        let s = setup();
        let deployer2 = Address::generate(&s.env);
        let admins2 = Vec::from_array(&s.env, [s.admin.clone()]);
        let result = s.client.try_initialize(&deployer2, &admins2, &1, &s.token);
        assert!(
            matches!(result, Err(Ok(ContractError::AlreadyInitialized))),
            "spec: second initialize must return AlreadyInitialized"
        );
    }

    /// Spec: is_initialized returns true after initialize.
    #[test]
    fn spec_initialize_is_initialized() {
        let s = setup();
        assert!(s.client.is_initialized(), "spec: is_initialized must be true after init");
    }

    /// Spec: get_token returns the token set at initialize.
    #[test]
    fn spec_initialize_get_token() {
        let s = setup();
        assert_eq!(
            s.client.get_token(),
            s.token,
            "spec: get_token must return the token set at initialize"
        );
    }

    // ── Spec: vouch ───────────────────────────────────────────────────────────

    /// Spec: vouch creates social collateral — get_vouches returns the new record.
    #[test]
    fn spec_vouch_creates_vouch_record() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        let vouches = s.client.get_vouches(&borrower);
        assert_eq!(vouches.len(), 1, "spec: get_vouches must return 1 record after vouch");
        assert_eq!(vouches.get(0).unwrap().stake, ONE_XLM);
        assert_eq!(vouches.get(0).unwrap().voucher, voucher);
    }

    /// Spec: total_vouched increases after a vouch.
    #[test]
    fn spec_vouch_total_vouched_increases() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        let total = s.client.total_vouched(&borrower).unwrap();
        assert_eq!(total, ONE_XLM, "spec: total_vouched must equal stake after one vouch");
    }

    /// Spec: self-vouch must be rejected (SelfVouchNotAllowed).
    #[test]
    fn spec_vouch_self_vouch_rejected() {
        let s = setup();
        let user = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&user, &ONE_XLM);
        let result = s.client.try_vouch(&user, &user, &ONE_XLM, &s.token, &None);
        assert!(result.is_err(), "spec: self-vouch must be rejected");
    }

    /// Spec: duplicate vouch for same (voucher, borrower) is rejected.
    #[test]
    fn spec_vouch_duplicate_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        let result = s.client.try_vouch(&voucher, &borrower, &ONE_XLM, &s.token, &None);
        assert!(
            matches!(result, Err(Ok(ContractError::DuplicateVouch))),
            "spec: duplicate vouch must return DuplicateVouch"
        );
    }

    /// Spec: vouch with zero stake is rejected (MinStakeNotMet or InsufficientFunds).
    #[test]
    fn spec_vouch_zero_stake_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let result = s.client.try_vouch(&voucher, &borrower, &0, &s.token, &None);
        assert!(result.is_err(), "spec: zero-stake vouch must be rejected");
    }

    /// Spec: vouch on a borrower with an active loan is rejected (ActiveLoanExists).
    #[test]
    fn spec_vouch_rejected_when_active_loan() {
        let s = setup();
        let voucher1 = Address::generate(&s.env);
        let voucher2 = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher1, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher2, &ONE_XLM);
        let result = s.client.try_vouch(&voucher2, &borrower, &ONE_XLM, &s.token, &None);
        assert!(
            matches!(result, Err(Ok(ContractError::ActiveLoanExists))),
            "spec: vouch on active-loan borrower must return ActiveLoanExists"
        );
    }

    // ── Spec: is_eligible ─────────────────────────────────────────────────────

    /// Spec: is_eligible returns true when total_stake >= threshold.
    #[test]
    fn spec_is_eligible_true_when_stake_meets_threshold() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        assert!(
            s.client.is_eligible(&borrower, &ONE_XLM, &s.token),
            "spec: is_eligible must be true when stake >= threshold"
        );
    }

    /// Spec: is_eligible returns false when total_stake < threshold.
    #[test]
    fn spec_is_eligible_false_when_stake_below_threshold() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        assert!(
            !s.client.is_eligible(&borrower, &(10 * ONE_XLM), &s.token),
            "spec: is_eligible must be false when stake < threshold"
        );
    }

    /// Spec: is_eligible returns false with zero threshold (spec: threshold=0 is invalid).
    #[test]
    fn spec_is_eligible_zero_threshold_false() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        assert!(
            !s.client.is_eligible(&borrower, &0, &s.token),
            "spec: is_eligible with zero threshold must return false"
        );
    }

    // ── Spec: request_loan ────────────────────────────────────────────────────

    /// Spec: request_loan disburses funds to borrower and creates a LoanRecord.
    #[test]
    fn spec_request_loan_disburses_and_creates_record() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);
        let loan = s.client.get_loan(&borrower).expect("spec: loan record must exist after request_loan");
        assert_eq!(loan.amount, ONE_XLM, "spec: loan amount must equal requested amount");
        assert_eq!(loan.amount_repaid, 0, "spec: amount_repaid must be 0 at disbursement");
    }

    /// Spec: loan_status is Active after request_loan.
    #[test]
    fn spec_request_loan_status_is_active() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);
        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::Active,
            "spec: loan_status must be Active after disbursement"
        );
    }

    /// Spec: request_loan fails when total_stake < threshold (InsufficientFunds).
    #[test]
    fn spec_request_loan_insufficient_stake_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        let result = s.client.try_request_loan(
            &borrower,
            &ONE_XLM,
            &(10 * ONE_XLM),
            &String::from_str(&s.env, "test"),
            &s.token,
        );
        assert!(result.is_err(), "spec: request_loan with insufficient stake must fail");
    }

    /// Spec: blacklisted borrower cannot request a loan.
    #[test]
    fn spec_request_loan_blacklisted_borrower_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        s.client.blacklist(&admins(&s), &borrower);
        let result = s.client.try_request_loan(
            &borrower,
            &ONE_XLM,
            &ONE_XLM,
            &String::from_str(&s.env, "test"),
            &s.token,
        );
        assert!(
            matches!(result, Err(Ok(ContractError::Blacklisted))),
            "spec: blacklisted borrower must receive Blacklisted error"
        );
    }

    // ── Spec: repay ───────────────────────────────────────────────────────────

    /// Spec: full repay clears the debt — loan_status becomes Repaid.
    #[test]
    fn spec_repay_full_status_repaid() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);
        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &repayment);
        s.client.repay(&borrower, &repayment);
        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::Repaid,
            "spec: loan_status must be Repaid after full repayment"
        );
    }

    /// Spec: voucher earns 2% yield on repayment.
    #[test]
    fn spec_repay_voucher_receives_yield() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake = 10 * ONE_XLM;
        do_vouch(&s, &voucher, &borrower, stake);
        do_loan(&s, &borrower, stake / 2);

        let repayment = stake / 2 + stake / 2 * 200 / 10_000 + 1;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &repayment);

        let token_client = soroban_sdk::token::Client::new(&s.env, &s.token);
        let bal_before = token_client.balance(&voucher);
        s.client.repay(&borrower, &repayment);
        let bal_after = token_client.balance(&voucher);

        assert!(
            bal_after > bal_before,
            "spec: voucher balance must increase after repayment (yield); before={bal_before} after={bal_after}"
        );
    }

    /// Spec: repay on a non-existent loan returns NoActiveLoan.
    #[test]
    fn spec_repay_no_active_loan_error() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &ONE_XLM);
        let result = s.client.try_repay(&borrower, &ONE_XLM);
        assert!(
            matches!(result, Err(Ok(ContractError::NoActiveLoan))),
            "spec: repay without active loan must return NoActiveLoan"
        );
    }

    // ── Spec: slash ───────────────────────────────────────────────────────────

    /// Spec: admin slash marks loan Defaulted and burns 50% of voucher stakes.
    #[test]
    fn spec_slash_marks_defaulted_and_burns_half_stake() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake = 2 * ONE_XLM;
        do_vouch(&s, &voucher, &borrower, stake);
        do_loan(&s, &borrower, ONE_XLM);

        let token_client = soroban_sdk::token::Client::new(&s.env, &s.token);
        let bal_before = token_client.balance(&voucher);

        s.client.slash(&admins(&s), &borrower);

        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::Defaulted,
            "spec: loan_status must be Defaulted after slash"
        );

        let bal_after = token_client.balance(&voucher);
        let returned = bal_after - bal_before;
        let expected_returned = stake / 2; // 50% slash → 50% returned
        assert_eq!(
            returned, expected_returned,
            "spec: voucher must receive 50% of stake back after slash; got {returned}"
        );
    }

    // ── Spec: withdraw_vouch ──────────────────────────────────────────────────

    /// Spec: withdraw_vouch returns staked tokens when no active loan.
    #[test]
    fn spec_withdraw_vouch_returns_stake() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);

        let token_client = soroban_sdk::token::Client::new(&s.env, &s.token);
        let bal_before = token_client.balance(&voucher);
        s.client.withdraw_vouch(&voucher, &borrower);
        let bal_after = token_client.balance(&voucher);
        assert_eq!(
            bal_after - bal_before,
            ONE_XLM,
            "spec: withdraw_vouch must return exactly the staked amount"
        );
    }

    // ── Spec: pause / unpause ─────────────────────────────────────────────────

    /// Spec: paused contract rejects vouch with ContractPaused.
    #[test]
    fn spec_paused_contract_rejects_vouch() {
        let s = setup();
        s.client.pause(&admins(&s));
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        let result = s.client.try_vouch(&voucher, &borrower, &ONE_XLM, &s.token, &None);
        assert!(
            matches!(result, Err(Ok(ContractError::ContractPaused))),
            "spec: vouch while paused must return ContractPaused"
        );
    }

    /// Spec: unpause restores operations.
    #[test]
    fn spec_unpause_restores_vouch() {
        let s = setup();
        s.client.pause(&admins(&s));
        s.client.unpause(&admins(&s));
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        assert_eq!(s.client.total_vouched(&borrower).unwrap(), ONE_XLM);
    }

    // ── Spec: increase_stake / decrease_stake ─────────────────────────────────

    /// Spec: increase_stake adds to existing vouch.
    #[test]
    fn spec_increase_stake_adds_to_vouch() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        s.client.increase_stake(&voucher, &borrower, &ONE_XLM);
        let total = s.client.total_vouched(&borrower).unwrap();
        assert_eq!(total, 2 * ONE_XLM, "spec: total_vouched must be 2 XLM after increase");
    }

    // ── Spec: loan_status transitions ─────────────────────────────────────────

    /// Spec: loan_status is None before any loan.
    #[test]
    fn spec_loan_status_none_before_loan() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::None,
            "spec: loan_status must be None when no loan exists"
        );
    }

    // ── Spec: get_loan ────────────────────────────────────────────────────────

    /// Spec: get_loan returns None for an address with no loan.
    #[test]
    fn spec_get_loan_none_for_unknown_borrower() {
        let s = setup();
        let unknown = Address::generate(&s.env);
        assert!(
            s.client.get_loan(&unknown).is_none(),
            "spec: get_loan must return None for unknown borrower"
        );
    }

    // ── Spec: admin functions ─────────────────────────────────────────────────

    /// Spec: add_admin and get_admins roundtrip.
    #[test]
    fn spec_add_admin_appears_in_get_admins() {
        let s = setup();
        let new_admin = Address::generate(&s.env);
        s.client.add_admin(&admins(&s), &new_admin);
        assert!(
            s.client.get_admins().iter().any(|a| a == new_admin),
            "spec: new admin must appear in get_admins after add_admin"
        );
    }

    /// Spec: set_min_stake / get_min_stake roundtrip.
    #[test]
    fn spec_set_min_stake_roundtrip() {
        let s = setup();
        let new_min = 500_000i128;
        s.client.set_min_stake(&admins(&s), &new_min);
        assert_eq!(
            s.client.get_min_stake(),
            new_min,
            "spec: get_min_stake must return the value set by set_min_stake"
        );
    }

    /// Spec: set_max_loan_amount / get_max_loan_amount roundtrip.
    #[test]
    fn spec_set_max_loan_amount_roundtrip() {
        let s = setup();
        let cap = 100 * ONE_XLM;
        s.client.set_max_loan_amount(&admins(&s), &cap);
        assert_eq!(
            s.client.get_max_loan_amount(),
            cap,
            "spec: get_max_loan_amount must return the value set by set_max_loan_amount"
        );
    }

    // ── Spec: batch_vouch ─────────────────────────────────────────────────────

    /// Spec: batch_vouch vouches for multiple borrowers atomically.
    #[test]
    fn spec_batch_vouch_creates_multiple_vouches() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let b1 = Address::generate(&s.env);
        let b2 = Address::generate(&s.env);
        let total_stake = 2 * ONE_XLM;
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &total_stake);
        let borrowers = Vec::from_array(&s.env, [b1.clone(), b2.clone()]);
        let stakes = Vec::from_array(&s.env, [ONE_XLM, ONE_XLM]);
        s.client.batch_vouch(&voucher, &borrowers, &stakes, &s.token, &None);
        assert_eq!(s.client.total_vouched(&b1).unwrap(), ONE_XLM);
        assert_eq!(s.client.total_vouched(&b2).unwrap(), ONE_XLM);
    }

    // ── Spec: error codes from Error Reference ────────────────────────────────

    /// Spec Error #19: AlreadyInitialized on second initialize call.
    #[test]
    fn spec_error_19_already_initialized() {
        let s = setup();
        let d = Address::generate(&s.env);
        let a = Vec::from_array(&s.env, [s.admin.clone()]);
        let res = s.client.try_initialize(&d, &a, &1, &s.token);
        assert!(matches!(res, Err(Ok(ContractError::AlreadyInitialized))));
    }

    /// Spec Error #5: DuplicateVouch on same voucher+borrower.
    #[test]
    fn spec_error_5_duplicate_vouch() {
        let s = setup();
        let v = Address::generate(&s.env);
        let b = Address::generate(&s.env);
        do_vouch(&s, &v, &b, ONE_XLM);
        StellarAssetClient::new(&s.env, &s.token).mint(&v, &ONE_XLM);
        let res = s.client.try_vouch(&v, &b, &ONE_XLM, &s.token, &None);
        assert!(matches!(res, Err(Ok(ContractError::DuplicateVouch))));
    }

    /// Spec Error #6: NoActiveLoan when repaying without a loan.
    #[test]
    fn spec_error_6_no_active_loan_repay() {
        let s = setup();
        let b = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&b, &ONE_XLM);
        let res = s.client.try_repay(&b, &ONE_XLM);
        assert!(matches!(res, Err(Ok(ContractError::NoActiveLoan))));
    }

    /// Spec Error #7: ContractPaused when mutating while paused.
    #[test]
    fn spec_error_7_contract_paused() {
        let s = setup();
        s.client.pause(&admins(&s));
        let v = Address::generate(&s.env);
        let b = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&v, &ONE_XLM);
        let res = s.client.try_vouch(&v, &b, &ONE_XLM, &s.token, &None);
        assert!(matches!(res, Err(Ok(ContractError::ContractPaused))));
    }

    /// Spec Error #2: ActiveLoanExists when vouching for borrower with active loan.
    #[test]
    fn spec_error_2_active_loan_exists() {
        let s = setup();
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let b = Address::generate(&s.env);
        do_vouch(&s, &v1, &b, 2 * ONE_XLM);
        do_loan(&s, &b, ONE_XLM);
        StellarAssetClient::new(&s.env, &s.token).mint(&v2, &ONE_XLM);
        let res = s.client.try_vouch(&v2, &b, &ONE_XLM, &s.token, &None);
        assert!(matches!(res, Err(Ok(ContractError::ActiveLoanExists))));
    }
}
