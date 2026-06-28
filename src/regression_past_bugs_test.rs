/// Regression tests for historical bugs.
///
/// Every test targets a specific known bug and serves as a permanent guard
/// against re-introduction. Test names encode the bug being covered.
///
/// Call-signature conventions mirror coverage_test.rs (the reference test file
/// that already compiles cleanly):
///   vouch(voucher, borrower, stake, token)               ← 4 args
///   try_vouch(voucher, borrower, stake, token)            ← 4 args
///   is_eligible(borrower, threshold)                     ← 2 args
///   request_forbearance(borrower, duration: &Option<u64>) ← with Option
///   default_count / repayment_count / loan_count         ← 1-arg views
///
/// All amounts are in stroops (1 XLM = 10_000_000 stroops).
#[cfg(test)]
mod regression_past_bugs_tests {
    use crate::{ContractError, LoanStatus, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Env, String, Vec,
    };

    const ONE_XLM: i128 = 10_000_000;

    // ── Setup ─────────────────────────────────────────────────────────────────

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
        let admin    = Address::generate(&env);
        let admins   = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let cid      = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&cid, &(500 * ONE_XLM));
        let client = QuorumCreditContractClient::new(&env, &cid);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 200);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    /// Vouch then advance past MIN_VOUCH_AGE (61 s).
    fn do_vouch(s: &Setup, voucher: &Address, borrower: &Address, stake: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(voucher, &stake);
        s.client.vouch(voucher, borrower, &stake, &s.token, &None);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
    }

    fn do_loan(s: &Setup, borrower: &Address, amount: i128) {
        s.client.request_loan(
            borrower, &amount, &(amount / 2),
            &String::from_str(&s.env, "test"), &s.token,
        );
    }

    // ── Bug 1: repay_partial transfer direction ───────────────────────────────
    //
    // Was: contract → borrower (wrong).
    // Fix: borrower → contract.
    // Guard: borrower balance decreases; contract balance increases.

    #[test]
    fn test_repay_partial_tokens_move_borrower_to_contract() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 2);

        let payment: i128 = ONE_XLM / 10;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &payment);

        let borrower_before  = TokenClient::new(&s.env, &s.token).balance(&borrower);
        let contract_before  = s.client.get_contract_balance();

        s.client.repay_partial(&borrower, &payment, &s.token);

        let borrower_after   = TokenClient::new(&s.env, &s.token).balance(&borrower);
        let contract_after   = s.client.get_contract_balance();

        assert_eq!(borrower_before  - borrower_after,  payment,
            "borrower balance must decrease by payment");
        assert_eq!(contract_after   - contract_before, payment,
            "contract balance must increase by payment");
    }

    #[test]
    fn test_repay_partial_amount_repaid_increases() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 2);

        let payment: i128 = ONE_XLM / 10;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &payment);

        assert_eq!(s.client.get_loan(&borrower).unwrap().amount_repaid, 0);
        s.client.repay_partial(&borrower, &payment, &s.token);
        assert_eq!(s.client.get_loan(&borrower).unwrap().amount_repaid, payment,
            "amount_repaid must equal the partial payment");
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active,
            "loan must remain Active after partial repayment");
    }

    #[test]
    fn test_repay_partial_no_active_loan_returns_error() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &ONE_XLM);
        let result = s.client.try_repay_partial(&borrower, &ONE_XLM, &s.token);
        assert_eq!(result, Err(Ok(ContractError::NoActiveLoan)));
    }

    // ── Bug 2: duplicate vouch / vouch-during-active-loan ────────────────────

    #[test]
    fn test_new_voucher_during_active_loan_rejected() {
        let s = setup();
        let voucher1 = Address::generate(&s.env);
        let voucher2 = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher1, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher2, &ONE_XLM);
        let result = s.client.try_vouch(&voucher2, &borrower, &ONE_XLM, &s.token, &None);
        assert_eq!(result, Err(Ok(ContractError::ActiveLoanExists)),
            "vouch during active loan must return ActiveLoanExists");
    }

    #[test]
    fn test_same_voucher_vouch_twice_rejected() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        let result = s.client.try_vouch(&voucher, &borrower, &ONE_XLM, &s.token, &None);
        assert_eq!(result, Err(Ok(ContractError::DuplicateVouch)),
            "same voucher vouching again must return DuplicateVouch");
    }

    // ── Bug 3: slash-already-executed guard ───────────────────────────────────

    #[test]
    fn test_vote_slash_after_execution_returns_slash_already_executed() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        // Single voucher = 100% stake → quorum met immediately → slash executed
        s.client.vote_slash(&voucher, &borrower, &true);

        let result = s.client.try_vote_slash(&voucher, &borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::SlashAlreadyExecuted)),
            "vote_slash after execution must return SlashAlreadyExecuted");
    }

    #[test]
    fn test_execute_slash_vote_twice_returns_slash_already_executed() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        s.client.vote_slash(&voucher, &borrower, &true);

        // vote already executed the slash; calling execute_slash_vote again must fail
        let result = s.client.try_execute_slash_vote(&borrower);
        assert_eq!(result, Err(Ok(ContractError::SlashAlreadyExecuted)),
            "execute_slash_vote on already-executed record must return SlashAlreadyExecuted");
    }

    // ── Bug 4: effective_slash_bps shadowing (dead dynamic calculation) ───────
    //
    // The first `effective_slash_bps` from `calculate_effective_slash_bps` was
    // silently shadowed by the graduated default_count logic. The graduated
    // logic is now the single authoritative path.
    //
    // Guard: verify the exact slash amounts that the graduated logic produces.

    #[test]
    fn test_first_slash_uses_base_5000_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let voucher_before = TokenClient::new(&s.env, &s.token).balance(&voucher);
        // default_count = 0 before this slash → effective_bps = 5000 + 0×500 = 5000
        s.client.slash(&admins(&s), &borrower);
        let voucher_after = TokenClient::new(&s.env, &s.token).balance(&voucher);

        // Returned = stake × (10000 - 5000) / 10000 = ONE_XLM / 2
        assert_eq!(voucher_after - voucher_before, ONE_XLM / 2,
            "first slash (no prior defaults) must use base slash_bps=5000, returning 50%% of stake");
        assert_eq!(s.client.default_count(&borrower), 1);
    }

    #[test]
    fn test_second_slash_uses_graduated_5500_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // First default cycle
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        s.client.slash(&admins(&s), &borrower);
        assert_eq!(s.client.default_count(&borrower), 1);

        // Second cycle — re-vouch and re-borrow
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let voucher_before = TokenClient::new(&s.env, &s.token).balance(&voucher);
        // default_count = 1 → effective_bps = 5000 + 1×500 = 5500
        s.client.vote_slash(&voucher, &borrower, &true);
        let voucher_after = TokenClient::new(&s.env, &s.token).balance(&voucher);

        // Returned = ONE_XLM × (10000 - 5500) / 10000 = ONE_XLM × 4500 / 10000
        let expected = ONE_XLM * 4_500 / 10_000;
        assert_eq!(voucher_after - voucher_before, expected,
            "second default must use graduated rate 5500 bps");
    }

    // ── Bug 5: self-vouch rejection ───────────────────────────────────────────

    #[test]
    fn test_self_vouch_returns_self_vouch_not_allowed() {
        let s = setup();
        let user = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&user, &ONE_XLM);
        let result = s.client.try_vouch(&user, &user, &ONE_XLM, &s.token, &None);
        assert_eq!(result, Err(Ok(ContractError::SelfVouchNotAllowed)));
    }

    // ── Bug 6: loan below minimum amount ─────────────────────────────────────
    // Default min_loan_amount = 100_000 stroops.

    #[test]
    fn test_loan_below_minimum_returns_loan_below_min_amount() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        let result = s.client.try_request_loan(
            &borrower, &1i128, &1i128,
            &String::from_str(&s.env, "test"), &s.token,
        );
        assert_eq!(result, Err(Ok(ContractError::LoanBelowMinAmount)));
    }

    #[test]
    fn test_loan_at_minimum_amount_accepted() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        // min_loan_amount = 100_000 stroops
        let result = s.client.try_request_loan(
            &borrower, &100_000i128, &50_000i128,
            &String::from_str(&s.env, "test"), &s.token,
        );
        assert!(result.is_ok(), "loan at min amount must be accepted");
    }

    // ── Bug 7: zero threshold in is_eligible returns false (not panic) ────────

    #[test]
    fn test_is_eligible_zero_threshold_false_not_panic() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        assert!(!s.client.is_eligible(&borrower, &0i128),
            "is_eligible(threshold=0) must return false, not panic");
    }

    // ── Bug 8: forbearance double-request rejected ────────────────────────────

    #[test]
    fn test_forbearance_second_request_returns_loan_in_forbearance() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        // First request succeeds
        s.client.request_forbearance(&borrower, &None::<u64>);

        // Second request on the same loan must fail
        let result = s.client.try_request_forbearance(&borrower, &None::<u64>);
        assert_eq!(result, Err(Ok(ContractError::LoanInForbearance)),
            "second forbearance request must return LoanInForbearance");
    }

    // ── Bug 9: withdrawal queue priority-fee ordering ─────────────────────────
    // Higher-fee entry must be first in the queue.

    #[test]
    fn test_withdrawal_queue_higher_fee_sorted_first() {
        let s = setup();
        let voucher_lo = Address::generate(&s.env);
        let voucher_hi = Address::generate(&s.env);
        let borrower   = Address::generate(&s.env);

        do_vouch(&s, &voucher_lo, &borrower, ONE_XLM);
        s.env.ledger().with_mut(|l| l.timestamp += 10);
        do_vouch(&s, &voucher_hi, &borrower, ONE_XLM);

        do_loan(&s, &borrower, ONE_XLM / 2);

        let low_fee:  i128 = 100;
        let high_fee: i128 = 1_000;
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher_lo, &low_fee);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher_hi, &high_fee);

        // Enqueue low-fee first, high-fee second
        s.client.request_withdrawal(&voucher_lo, &borrower, &low_fee);
        s.client.request_withdrawal(&voucher_hi, &borrower, &high_fee);

        let queue = s.client.get_withdrawal_queue(&borrower);
        assert!(queue.len() >= 2, "both withdrawals must be queued");

        // Index 0 must be the high-fee entry
        assert_eq!(queue.get(0).unwrap().voucher, voucher_hi,
            "higher priority-fee must sort to front of queue");
        assert_eq!(queue.get(1).unwrap().voucher, voucher_lo,
            "lower priority-fee must be second in queue");
    }

    // ── Bug 10: withdraw_vouch used undefined `stake` variable ────────────────
    // Was: event published `stake` (undeclared in scope).
    // Fix: event now uses `vouch_stake`.
    // Guard: withdraw_vouch returns Ok and the voucher receives exactly the
    //        original stake amount back.

    #[test]
    fn test_withdraw_vouch_returns_exact_stake_no_undefined_var() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        let stake: i128 = ONE_XLM * 2;
        do_vouch(&s, &voucher, &borrower, stake);

        let before = TokenClient::new(&s.env, &s.token).balance(&voucher);
        s.client.withdraw_vouch(&voucher, &borrower);
        let after  = TokenClient::new(&s.env, &s.token).balance(&voucher);

        assert_eq!(after - before, stake,
            "withdraw_vouch must return exactly the staked amount (uses vouch_stake, not undefined `stake`)");
        assert!(!s.client.vouch_exists(&voucher, &borrower),
            "vouch record must be removed after withdrawal");
    }

    // ── Additional: duplicate error code 57 is fixed ─────────────────────────
    // SlashRecordNotFound was = 57 (same as WithdrawalAlreadyQueued).
    // Fix: SlashRecordNotFound renumbered to 142.
    // Guard: both errors have distinct discriminants.

    #[test]
    fn test_error_codes_withdrawal_already_queued_and_slash_record_not_found_distinct() {
        // These are repr(u32) discriminants — we just verify they compile and differ.
        let a = ContractError::WithdrawalAlreadyQueued as u32;
        let b = ContractError::SlashRecordNotFound     as u32;
        assert_ne!(a, b,
            "WithdrawalAlreadyQueued and SlashRecordNotFound must have distinct error codes");
        assert_eq!(a, 57,  "WithdrawalAlreadyQueued must remain 57");
        assert_eq!(b, 142, "SlashRecordNotFound must be renumbered to 142");
    }

    // ── Additional: duplicate escrow_status field removed ────────────────────
    // LoanRecord struct initialiser in request_loan had escrow_status set twice.
    // Fix: removed the duplicate field. Guard: loan is created and readable.

    #[test]
    fn test_request_loan_no_duplicate_field_panic() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        // If the duplicate field caused a compile error this test would not exist.
        // A successful call confirms the fix:
        do_loan(&s, &borrower, ONE_XLM / 4);
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active);
    }

    // ── Additional: vouch too recent blocks loan ──────────────────────────────

    #[test]
    fn test_vouch_too_recent_blocks_loan_request() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // Vouch but do NOT advance the clock past MIN_VOUCH_AGE
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        s.client.vouch(&voucher, &borrower, &ONE_XLM, &s.token, &None);
        // timestamp unchanged → vouch age = 0 < 60 s

        let result = s.client.try_request_loan(
            &borrower, &100_000i128, &50_000i128,
            &String::from_str(&s.env, "test"), &s.token,
        );
        assert_eq!(result, Err(Ok(ContractError::VouchTooRecent)));
    }

    // ── Additional: loan status after admin slash is Defaulted ───────────────

    #[test]
    fn test_loan_status_after_admin_slash_is_defaulted() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        s.client.slash(&admins(&s), &borrower);

        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Defaulted);
        assert_eq!(s.client.default_count(&borrower), 1);
    }

    // ── Additional: repayment_count increments on full repay ─────────────────

    #[test]
    fn test_repayment_count_increments_on_full_repay() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        assert_eq!(s.client.repayment_count(&borrower), 0);

        let loan = s.client.get_loan(&borrower).unwrap();
        let owed = loan.amount + loan.total_yield;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &owed);
        s.client.repay(&borrower, &owed);

        assert_eq!(s.client.repayment_count(&borrower), 1);
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Repaid);
    }

} // mod regression_past_bugs_tests
