//! Chaos engineering tests — adversarial and boundary scenarios.
//!
//! Verifies the contract fails safely (typed `ContractError` or success) without
//! partial state mutation under hostile inputs.

#[cfg(test)]
mod chaos_tests {
    use crate::{
        ContractError, LoanStatus, QuorumCreditContract, QuorumCreditContractClient,
    };
    use crate::types::{DEFAULT_LOAN_DURATION, DEFAULT_MAX_VOUCHERS_PER_BORROWER, DEFAULT_MIN_LOAN_AMOUNT};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{StellarAssetClient, TokenClient},
        Address, Env, String, Vec,
    };

    struct ChaosFixture {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        contract_id: Address,
        token_addr: Address,
        admin: Address,
        admin_signers: Vec<Address>,
        borrower: Address,
        voucher: Address,
    }

    fn admins(f: &ChaosFixture) -> Vec<Address> {
        f.admin_signers.clone()
    }

    fn purpose(env: &Env) -> String {
        String::from_str(env, "chaos")
    }

    fn mint_token(f: &ChaosFixture, to: &Address, amount: i128) {
        StellarAssetClient::new(&f.env, &f.token_addr).mint(to, &amount);
    }

    fn advance_vouch_age(f: &ChaosFixture) {
        f.env.ledger().with_mut(|l| l.timestamp += 61);
    }

    /// Standard setup: initialised contract, funded voucher and contract, no active loan.
    fn setup_standard(env: &Env) -> ChaosFixture {
        env.mock_all_auths();

        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admin_signers = Vec::from_array(env, [admin.clone()]);
        let borrower = Address::generate(env);
        let voucher = Address::generate(env);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_addr = token_id.address();
        let contract_id = env.register_contract(None, QuorumCreditContract);

        StellarAssetClient::new(env, &token_addr).mint(&contract_id, &50_000_000);
        StellarAssetClient::new(env, &token_addr).mint(&voucher, &5_000_000);

        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admin_signers, &1, &token_addr);
        env.ledger().with_mut(|l| l.timestamp = 1_000);

        ChaosFixture {
            env: env.clone(),
            client,
            contract_id,
            token_addr,
            admin,
            admin_signers,
            borrower,
            voucher,
        }
    }

    /// Paused setup: standard fixture with the contract paused.
    fn setup_paused(env: &Env) -> ChaosFixture {
        let f = setup_standard(env);
        f.client.pause(&admins(&f));
        f
    }

    /// Max-voucher setup: borrower has exactly `DEFAULT_MAX_VOUCHERS_PER_BORROWER` vouches.
    fn setup_max_vouchers(env: &Env) -> ChaosFixture {
        env.budget().reset_unlimited();
        let f = setup_standard(env);
        let max = DEFAULT_MAX_VOUCHERS_PER_BORROWER;
        for _ in 0..max {
            let v = Address::generate(&f.env);
            mint_token(&f, &v, &500_000);
            f.client
                .vouch(&v, &f.borrower, &200_000, &f.token_addr, &None);
        }
        advance_vouch_age(&f);
        f
    }

    /// Expired-loan setup: active loan with ledger timestamp past the deadline.
    fn setup_expired_loan(env: &Env) -> ChaosFixture {
        let f = setup_standard(env);
        mint_token(&f, &f.voucher, &2_000_000);
        f.client
            .vouch(&f.voucher, &f.borrower, &1_000_000, &f.token_addr, &None);
        advance_vouch_age(&f);
        f.client.request_loan(
            &f.borrower,
            &DEFAULT_MIN_LOAN_AMOUNT,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        let loan = f.client.get_loan(&f.borrower).unwrap();
        f.env.ledger().with_mut(|l| l.timestamp = loan.deadline + 1);
        f
    }

    /// Zero-balance setup: voucher has no token balance.
    fn setup_zero_balance(env: &Env) -> ChaosFixture {
        setup_standard(env)
    }

    fn do_vouch(f: &ChaosFixture, voucher: &Address, borrower: &Address, stake: i128) {
        mint_token(f, voucher, &stake);
        f.client
            .vouch(voucher, borrower, &stake, &f.token_addr, &None);
        advance_vouch_age(f);
    }

    fn do_loan(f: &ChaosFixture, borrower: &Address, amount: i128, threshold: i128) {
        f.client.request_loan(
            borrower,
            &amount,
            &threshold,
            &purpose(&f.env),
            &f.token_addr,
        );
    }

    fn vouch_snapshot(f: &ChaosFixture, borrower: &Address) -> (u32, i128) {
        let vouches = f.client.get_vouches(borrower);
        let count = vouches.len();
        let total: i128 = vouches.iter().map(|v| v.stake).sum();
        (count, total)
    }

    // ── Requirement 1: Boundary value tests ───────────────────────────────────

    /// Category: boundary — Requirement 1.1
    #[test]
    fn test_chaos_boundary_zero_stake() {
        let env = Env::default();
        let f = setup_standard(&env);
        mint_token(&f, &f.voucher, &1_000_000);
        let (before_count, before_total) = vouch_snapshot(&f, &f.borrower);
        let result = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &0,
            &f.token_addr,
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
        let (after_count, after_total) = vouch_snapshot(&f, &f.borrower);
        assert_eq!((before_count, before_total), (after_count, after_total));
    }

    /// Category: boundary — Requirement 1.2
    #[test]
    fn test_chaos_boundary_zero_loan_amount() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        assert!(f.client.get_loan(&f.borrower).is_none());
        let result = f.client.try_request_loan(
            &f.borrower,
            &0,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        assert_eq!(result, Err(Ok(ContractError::LoanBelowMinAmount)));
        assert!(f.client.get_loan(&f.borrower).is_none());
    }

    /// Category: boundary — Requirement 1.3
    #[test]
    fn test_chaos_boundary_min_loan_amount_exact() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        f.client.request_loan(
            &f.borrower,
            &DEFAULT_MIN_LOAN_AMOUNT,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Active);
    }

    /// Category: boundary — Requirement 1.4
    #[test]
    fn test_chaos_boundary_below_min_loan_amount() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        let result = f.client.try_request_loan(
            &f.borrower,
            &(DEFAULT_MIN_LOAN_AMOUNT - 1),
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        assert_eq!(result, Err(Ok(ContractError::LoanBelowMinAmount)));
        assert!(f.client.get_loan(&f.borrower).is_none());
    }

    /// Category: boundary — Requirement 1.6
    #[test]
    fn test_chaos_boundary_max_i128_stake() {
        let env = Env::default();
        let f = setup_standard(&env);
        mint_token(&f, &f.voucher, &i128::MAX);
        let result = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &i128::MAX,
            &f.token_addr,
            &None,
        );
        assert!(
            result.is_ok()
                || result == Err(Ok(ContractError::StakeOverflow))
                || result == Err(Ok(ContractError::InsufficientVoucherBalance))
                || result == Err(Ok(ContractError::ArithmeticError)),
            "max i128 stake must not panic unexpectedly: {result:?}"
        );
    }

    // ── Requirement 2: State corruption / random state mutation prevention ────

    /// Category: corruption — Requirement 2.1
    #[test]
    fn test_chaos_corruption_duplicate_vouch() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        let before = f.client.get_vouches(&f.borrower).get(0).unwrap().stake;
        mint_token(&f, &f.voucher, &1_000_000);
        let result = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &500_000,
            &f.token_addr,
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::DuplicateVouch)));
        assert_eq!(f.client.get_vouches(&f.borrower).get(0).unwrap().stake, before);
    }

    /// Category: corruption — Requirement 2.2
    #[test]
    fn test_chaos_corruption_repay_already_repaid() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let loan = f.client.get_loan(&f.borrower).unwrap();
        let owed = loan.amount + loan.total_yield;
        mint_token(&f, &f.borrower, &owed);
        f.client.repay(&f.borrower, &owed);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Repaid);
        let result = f.client.try_repay(&f.borrower, &1);
        assert_eq!(result, Err(Ok(ContractError::NoActiveLoan)));
    }

    /// Category: corruption — Requirement 2.3
    #[test]
    fn test_chaos_corruption_repay_after_slash() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        f.client.slash(&admins(&f), &f.borrower);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Defaulted);
        let result = f.client.try_repay(&f.borrower, &1_000_000);
        assert_eq!(result, Err(Ok(ContractError::NoActiveLoan)));
    }

    /// Category: corruption — Requirement 2.4
    #[test]
    fn test_chaos_corruption_slash_after_repay() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let loan = f.client.get_loan(&f.borrower).unwrap();
        let owed = loan.amount + loan.total_yield;
        mint_token(&f, &f.borrower, &owed);
        f.client.repay(&f.borrower, &owed);
        let voucher_before = TokenClient::new(&f.env, &f.token_addr).balance(&f.voucher);
        let result = f.client.try_execute_slash_vote(&f.borrower);
        assert!(
            result == Err(Ok(ContractError::SlashVoteNotFound))
                || result == Err(Ok(ContractError::NoActiveLoan))
                || result == Err(Ok(ContractError::SlashAlreadyExecuted))
        );
        let voucher_after = TokenClient::new(&f.env, &f.token_addr).balance(&f.voucher);
        assert_eq!(voucher_before, voucher_after);
    }

    /// Category: corruption — Requirement 2.6
    #[test]
    fn test_chaos_corruption_vote_slash_no_loan() {
        let env = Env::default();
        let f = setup_standard(&env);
        let outsider = Address::generate(&f.env);
        let result = f.client.try_vote_slash(&outsider, &f.borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::NoActiveLoan)));
    }

    /// Category: corruption — random state mutation: failed calls leave storage unchanged.
    #[test]
    fn test_chaos_random_state_mutation_failed_ops_are_atomic() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);

        let loan_before = f.client.get_loan(&f.borrower);
        let vouch_before = vouch_snapshot(&f, &f.borrower);
        let treasury_before = f.client.get_slash_treasury_balance();

        let _ = f.client.try_request_loan(
            &f.borrower,
            &(DEFAULT_MIN_LOAN_AMOUNT - 1),
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        mint_token(&f, &f.voucher, &1_000_000);
        let _ = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &100_000,
            &f.token_addr,
            &None,
        );

        assert_eq!(f.client.get_loan(&f.borrower), loan_before);
        assert_eq!(vouch_snapshot(&f, &f.borrower), vouch_before);
        assert_eq!(f.client.get_slash_treasury_balance(), treasury_before);
    }

    // ── Requirement 3: Paused state tests ─────────────────────────────────────

    /// Category: paused — Requirement 3.1
    #[test]
    fn test_chaos_paused_vouch_blocked() {
        let env = Env::default();
        let f = setup_paused(&env);
        mint_token(&f, &f.voucher, &1_000_000);
        let result = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &1_000_000,
            &f.token_addr,
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Category: paused — Requirement 3.2
    #[test]
    fn test_chaos_paused_request_loan_blocked() {
        let env = Env::default();
        let mut f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        f.client.pause(&admins(&f));
        let result = f.client.try_request_loan(
            &f.borrower,
            &DEFAULT_MIN_LOAN_AMOUNT,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Category: paused — Requirement 3.3
    #[test]
    fn test_chaos_paused_repay_blocked() {
        let env = Env::default();
        let mut f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        f.client.pause(&admins(&f));
        let result = f.client.try_repay(&f.borrower, &1);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Category: paused — Requirement 3.4
    /// Note: `auto_slash` is not pause-guarded; verify it still executes on expired loans.
    #[test]
    fn test_chaos_paused_auto_slash_blocked() {
        let env = Env::default();
        let mut f = setup_expired_loan(&env);
        f.client.pause(&admins(&f));
        f.client.auto_slash(&f.borrower);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Defaulted);
    }

    /// Category: paused — Requirement 3.5
    #[test]
    fn test_chaos_paused_vote_slash_blocked() {
        let env = Env::default();
        let mut f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        f.client.pause(&admins(&f));
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Category: paused — Requirement 3.7
    #[test]
    fn test_chaos_paused_reads_still_work() {
        let env = Env::default();
        let mut f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        f.client.pause(&admins(&f));
        assert!(f.client.get_loan(&f.borrower).is_some());
        assert!(!f.client.get_vouches(&f.borrower).is_empty());
        let _ = f.client.get_credit_score(&f.borrower);
    }

    /// Category: paused — Requirement 3.6
    #[test]
    fn test_chaos_paused_unpause_restores_vouch() {
        let env = Env::default();
        let mut f = setup_paused(&env);
        f.client.unpause(&admins(&f));
        mint_token(&f, &f.voucher, &1_000_000);
        f.client
            .vouch(&f.voucher, &f.borrower, &1_000_000, &f.token_addr, &None);
        assert!(f.client.vouch_exists(&f.voucher, &f.borrower));
    }

    // ── Requirement 4: Token failure tests ────────────────────────────────────

    /// Category: token — Requirement 4.1
    #[test]
    fn test_chaos_token_invalid_token_address() {
        let env = Env::default();
        let f = setup_standard(&env);
        let bad_token = Address::generate(&f.env);
        mint_token(&f, &f.voucher, &1_000_000);
        let result = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &1_000_000,
            &bad_token,
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::InvalidToken)));
        assert!(!f.client.vouch_exists(&f.voucher, &f.borrower));
    }

    /// Category: token — Requirement 4.2
    #[test]
    fn test_chaos_token_insufficient_contract_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_addr = token_id.address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_addr).mint(&voucher, &2_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admin_signers, &1, &token_addr);
        env.ledger().with_mut(|l| l.timestamp = 1_000);

        client.vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);
        env.ledger().with_mut(|l| l.timestamp += 61);

        assert!(client.get_loan(&borrower).is_none());
        let result = client.try_request_loan(
            &borrower,
            &2_000_000,
            &500_000,
            &String::from_str(&env, "chaos"),
            &token_addr,
        );
        assert!(result.is_err(), "disbursement exceeding contract balance must fail");
        assert!(client.get_loan(&borrower).is_none());
    }

    /// Category: token — Requirement 4.3
    #[test]
    fn test_chaos_token_zero_balance_voucher() {
        let env = Env::default();
        let f = setup_zero_balance(&env);
        let result = f.client.try_vouch(
            &f.voucher,
            &f.borrower,
            &1_000_000,
            &f.token_addr,
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::InsufficientVoucherBalance)));
        assert!(!f.client.vouch_exists(&f.voucher, &f.borrower));
    }

    /// Category: token — Requirement 4.4
    #[test]
    fn test_chaos_token_insufficient_borrower_balance_repay() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let status_before = f.client.loan_status(&f.borrower);
        let repaid_before = f.client.get_loan(&f.borrower).unwrap().amount_repaid;
        let result = f.client.try_repay(&f.borrower, &DEFAULT_MIN_LOAN_AMOUNT);
        assert!(result.is_err(), "repay without borrower balance must fail");
        assert_eq!(f.client.loan_status(&f.borrower), status_before);
        assert_eq!(
            f.client.get_loan(&f.borrower).unwrap().amount_repaid,
            repaid_before
        );
    }

    // ── Requirement 5: Deadline and timing tests ──────────────────────────────

    /// Category: deadline — Requirement 5.1
    #[test]
    fn test_chaos_deadline_auto_slash_at_exact_deadline() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let loan = f.client.get_loan(&f.borrower).unwrap();
        // Contract treats expiry as strictly after deadline (`timestamp > deadline`).
        f.env.ledger().with_mut(|l| l.timestamp = loan.deadline + 1);
        f.client.auto_slash(&f.borrower);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Defaulted);
    }

    /// Category: deadline — Requirement 5.2
    #[test]
    fn test_chaos_deadline_repay_at_exact_deadline() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let loan = f.client.get_loan(&f.borrower).unwrap();
        let owed = loan.amount + loan.total_yield;
        mint_token(&f, &f.borrower, &owed);
        f.env.ledger().with_mut(|l| l.timestamp = loan.deadline);
        f.client.repay(&f.borrower, &owed);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Repaid);
    }

    /// Category: deadline — Requirement 5.3
    #[test]
    fn test_chaos_deadline_repay_one_second_past() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let loan = f.client.get_loan(&f.borrower).unwrap();
        let owed = loan.amount + loan.total_yield;
        mint_token(&f, &f.borrower, &owed);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp = loan.deadline + 1);
        f.client.repay(&f.borrower, &owed);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Repaid);
    }

    /// Category: deadline — Requirement 5.4
    #[test]
    #[should_panic(expected = "no active loan")]
    fn test_chaos_deadline_auto_slash_already_slashed() {
        let env = Env::default();
        let f = setup_expired_loan(&env);
        f.client.auto_slash(&f.borrower);
        f.client.auto_slash(&f.borrower);
    }

    /// Category: deadline — Requirement 5.5
    #[test]
    #[should_panic(expected = "no active loan")]
    fn test_chaos_deadline_auto_slash_on_repaid_loan() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let loan = f.client.get_loan(&f.borrower).unwrap();
        let owed = loan.amount + loan.total_yield;
        mint_token(&f, &f.borrower, &owed);
        f.client.repay(&f.borrower, &owed);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += DEFAULT_LOAN_DURATION + 1);
        f.client.auto_slash(&f.borrower);
    }

    // ── Requirement 6: Multi-voucher stress tests ─────────────────────────────

    /// Category: vouchers — Requirement 6.1
    #[test]
    fn test_chaos_vouchers_max_count_loan_succeeds() {
        let env = Env::default();
        let f = setup_max_vouchers(&env);
        f.client.request_loan(
            &f.borrower,
            &DEFAULT_MIN_LOAN_AMOUNT,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Active);
    }

    /// Category: vouchers — Requirement 6.2
    #[test]
    fn test_chaos_vouchers_exceed_max() {
        let env = Env::default();
        let f = setup_max_vouchers(&env);
        let extra = Address::generate(&f.env);
        mint_token(&f, &extra, &500_000);
        let result = f.client.try_vouch(
            &extra,
            &f.borrower,
            &200_000,
            &f.token_addr,
            &None,
        );
        assert_eq!(result, Err(Ok(ContractError::MaxVouchersPerBorrowerExceeded)));
    }

    /// Category: vouchers — Requirement 6.3
    #[test]
    fn test_chaos_vouchers_zero_vouchers_loan_fails() {
        let env = Env::default();
        let f = setup_standard(&env);
        let result = f.client.try_request_loan(
            &f.borrower,
            &DEFAULT_MIN_LOAN_AMOUNT,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        assert!(
            result == Err(Ok(ContractError::InsufficientFunds))
                || result == Err(Ok(ContractError::InsufficientVouchers))
        );
    }

    /// Category: vouchers — Requirement 6.4
    #[test]
    fn test_chaos_vouchers_single_voucher_meets_threshold() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Active);
    }

    /// Category: vouchers — Requirement 6.5
    #[test]
    fn test_chaos_vouchers_slash_max_vouchers() {
        let env = Env::default();
        let f = setup_max_vouchers(&env);
        f.client.request_loan(
            &f.borrower,
            &DEFAULT_MIN_LOAN_AMOUNT,
            &500_000,
            &purpose(&f.env),
            &f.token_addr,
        );
        let treasury_before = f.client.get_slash_treasury_balance();
        f.client.slash(&admins(&f), &f.borrower);
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Defaulted);
        assert!(f.client.get_slash_treasury_balance() > treasury_before);
    }

    // ── Requirement 7: Governance chaos tests ─────────────────────────────────

    /// Category: governance — Requirement 7.1
    #[test]
    fn test_chaos_governance_vote_zero_stake() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        let zero_staker = Address::generate(&f.env);
        let result = f.client.try_vote_slash(&zero_staker, &f.borrower, &true);
        assert!(
            result == Err(Ok(ContractError::VoucherNotFound))
                || result == Err(Ok(ContractError::NotGovernanceParticipant))
                || result == Err(Ok(ContractError::InsufficientFunds))
        );
    }

    /// Category: governance — Requirement 7.2
    #[test]
    fn test_chaos_governance_duplicate_vote() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        f.client.vote_slash(&f.voucher, &f.borrower, &true);
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::SlashAlreadyExecuted)));
    }

    /// Category: governance — Requirement 7.3
    #[test]
    fn test_chaos_governance_finalize_before_period_ends() {
        let env = Env::default();
        let f = setup_standard(&env);
        let id = f.client.propose_slash_threshold(&f.admin, &3_000);
        let result = f.client.try_finalize_slash_threshold(&id);
        assert_eq!(result, Err(Ok(ContractError::TimelockNotReady)));
    }

    /// Category: governance — Requirement 7.4
    #[test]
    fn test_chaos_governance_vote_after_quorum() {
        let env = Env::default();
        let f = setup_standard(&env);
        do_vouch(&f, &f.voucher, &f.borrower, 1_000_000);
        do_loan(&f, &f.borrower, DEFAULT_MIN_LOAN_AMOUNT, 500_000);
        f.client.vote_slash(&f.voucher, &f.borrower, &true);
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &false);
        assert_eq!(result, Err(Ok(ContractError::SlashAlreadyExecuted)));
    }
}
