/// Chaos engineering test suite for QuorumCredit (#112: Disaster Recovery — Failure scenarios).
///
/// Exercises the contract under adversarial and boundary conditions to verify it fails safely
/// and predictably: no unexpected panics, no partial state updates, typed ContractError variants
/// returned for every known failure path.
///
/// Tests are organised into seven categories matching the requirement spec:
///   1. Boundary value inputs
///   2. State corruption / out-of-order transitions
///   3. Paused-state blocking
///   4. Token failure scenarios
///   5. Deadline and timing edge cases
///   6. Multi-voucher stress
///   7. Governance chaos
#[cfg(test)]
mod chaos_tests {
    use crate::{ContractError, LoanStatus, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    // ── Fixture ───────────────────────────────────────────────────────────────

    struct ChaosFixture {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
        admin: Address,
        borrower: Address,
        voucher: Address,
    }

    /// Standard setup: initialised contract, funded voucher and contract, no active loan.
    /// Timestamp starts at 90_000 to clear the 86 400-second vouch cooldown for fresh addresses.
    fn setup_standard() -> ChaosFixture {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        let token_asset = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token_asset.address();
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Fund the contract so it can disburse loans.
        StellarAssetClient::new(&env, &token).mint(&contract_id, &50_000_000);
        // Fund the voucher so it can stake.
        StellarAssetClient::new(&env, &token).mint(&voucher, &5_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(
            &deployer,
            &Vec::from_array(&env, [admin.clone()]),
            &1,
            &token,
        );

        // Advance past the vouch cooldown window (DEFAULT_VOUCH_COOLDOWN_SECS = 86 400).
        env.ledger().with_mut(|l| l.timestamp = 90_000);

        ChaosFixture { env, client, token, admin, borrower, voucher }
    }

    /// Paused setup: contract emergency-paused by admin immediately after init.
    fn setup_paused() -> ChaosFixture {
        let f = setup_standard();
        f.client.emergency_pause(&f.admin).unwrap();
        f
    }

    /// Setup with a single active vouch but no loan.
    fn setup_with_vouch() -> ChaosFixture {
        let f = setup_standard();
        f.client
            .vouch(&f.voucher, &f.borrower, &1_000_000, &f.token, &None)
            .unwrap();
        f
    }

    /// Setup with an active loan (vouch + disbursal done before pausing).
    fn setup_with_loan() -> ChaosFixture {
        let f = setup_with_vouch();
        f.client
            .request_loan(
                &f.borrower,
                &100_000,
                &500_000,
                &String::from_str(&f.env, "test"),
                &f.token,
            )
            .unwrap();
        f
    }

    /// Setup with a loan whose deadline has already passed.
    fn setup_expired_loan() -> ChaosFixture {
        let f = setup_with_loan();
        // Advance past the 30-day default loan duration.
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += 30 * 24 * 60 * 60 + 1);
        f
    }

    fn admins(f: &ChaosFixture) -> Vec<Address> {
        Vec::from_array(&f.env, [f.admin.clone()])
    }

    // ── Category 1: Boundary Value Input Tests (Req 1) ─────────────────────────

    /// Chaos/Boundary — Req 1.1: vouch with stake=0 → InvalidAmount
    #[test]
    fn test_chaos_boundary_zero_stake() {
        let f = setup_standard();
        let result = f.client.try_vouch(&f.voucher, &f.borrower, &0, &f.token, &None);
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }

    /// Chaos/Boundary — Req 1.1: vouch with negative stake → InvalidAmount
    #[test]
    fn test_chaos_boundary_negative_stake() {
        let f = setup_standard();
        let result = f.client.try_vouch(&f.voucher, &f.borrower, &-1, &f.token, &None);
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }

    /// Chaos/Boundary — Req 1.2: request_loan with amount=0 → LoanBelowMinAmount
    #[test]
    fn test_chaos_boundary_zero_loan_amount() {
        let f = setup_with_vouch();
        let result = f.client.try_request_loan(
            &f.borrower,
            &0,
            &500_000,
            &String::from_str(&f.env, "test"),
            &f.token,
        );
        assert_eq!(result, Err(Ok(ContractError::LoanBelowMinAmount)));
    }

    /// Chaos/Boundary — Req 1.3: request_loan with amount=min_loan_amount → succeeds
    #[test]
    fn test_chaos_boundary_min_loan_amount_exact() {
        let f = setup_with_vouch();
        // DEFAULT_MIN_LOAN_AMOUNT = 100_000
        let result = f.client.try_request_loan(
            &f.borrower,
            &100_000,
            &500_000,
            &String::from_str(&f.env, "test"),
            &f.token,
        );
        assert!(result.is_ok(), "exact minimum loan amount should succeed");
    }

    /// Chaos/Boundary — Req 1.4: request_loan with amount=min-1 → LoanBelowMinAmount
    #[test]
    fn test_chaos_boundary_below_min_loan_amount() {
        let f = setup_with_vouch();
        let result = f.client.try_request_loan(
            &f.borrower,
            &99_999,
            &500_000,
            &String::from_str(&f.env, "test"),
            &f.token,
        );
        assert_eq!(result, Err(Ok(ContractError::LoanBelowMinAmount)));
    }

    /// Chaos/Boundary — Req 1.6: vouch with stake=i128::MAX → error (no panic)
    #[test]
    fn test_chaos_boundary_max_i128_stake() {
        let f = setup_standard();
        // The voucher has only 5_000_000 tokens so the transfer will fail,
        // but the contract must not produce an unexpected panic.
        let result = f.client.try_vouch(&f.voucher, &f.borrower, &i128::MAX, &f.token, &None);
        assert!(result.is_err(), "i128::MAX stake must fail; contract must not panic");
    }

    // ── Category 2: State Corruption Prevention Tests (Req 2) ─────────────────

    /// Chaos/Corruption — Req 2.1: second vouch from same voucher for same borrower → DuplicateVouch
    #[test]
    fn test_chaos_corruption_duplicate_vouch() {
        let f = setup_with_vouch();
        // Mint more tokens for a second attempt.
        StellarAssetClient::new(&f.env, &f.token).mint(&f.voucher, &1_000_000);
        let result = f
            .client
            .try_vouch(&f.voucher, &f.borrower, &1_000_000, &f.token, &None);
        assert_eq!(result, Err(Ok(ContractError::DuplicateVouch)));
    }

    /// Chaos/Corruption — Req 2.2: repay on an already-repaid loan → NoActiveLoan
    #[test]
    fn test_chaos_corruption_repay_already_repaid() {
        let f = setup_with_loan();
        // Fund the borrower to repay (loan + yield).
        StellarAssetClient::new(&f.env, &f.token).mint(&f.borrower, &110_000);
        // Repay fully.
        f.client.repay(&f.borrower, &102_000).unwrap();
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Repaid);
        // Attempt a second repayment — must fail.
        let result = f.client.try_repay(&f.borrower, &1_000);
        assert_eq!(result, Err(Ok(ContractError::NoActiveLoan)));
    }

    /// Chaos/Corruption — Req 2.6: vote_slash with no active loan → error
    #[test]
    fn test_chaos_corruption_vote_slash_no_loan() {
        let f = setup_standard();
        // Voucher has no vouch on this borrower and borrower has no loan.
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &true);
        assert!(result.is_err(), "vote_slash with no loan must return an error");
    }

    /// Chaos/Corruption — second request_loan while first is active → ActiveLoanExists
    #[test]
    fn test_chaos_corruption_request_loan_twice() {
        let f = setup_with_loan();
        let result = f.client.try_request_loan(
            &f.borrower,
            &100_000,
            &500_000,
            &String::from_str(&f.env, "test2"),
            &f.token,
        );
        assert_eq!(result, Err(Ok(ContractError::ActiveLoanExists)));
    }

    /// Chaos/Corruption — repay with payment=0 after active loan → invalid amount error
    #[test]
    fn test_chaos_corruption_repay_zero_payment() {
        let f = setup_with_loan();
        let result = f.client.try_repay(&f.borrower, &0);
        assert!(result.is_err(), "repay with 0 payment must fail");
    }

    // ── Category 3: Paused-State Tests (Req 3) ────────────────────────────────

    /// Chaos/Paused — Req 3.1: vouch blocked when paused → ContractPaused
    #[test]
    fn test_chaos_paused_vouch_blocked() {
        let f = setup_paused();
        let result = f.client.try_vouch(&f.voucher, &f.borrower, &1_000_000, &f.token, &None);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Chaos/Paused — Req 3.2: request_loan blocked when paused → ContractPaused
    #[test]
    fn test_chaos_paused_request_loan_blocked() {
        let f = setup_paused();
        let result = f.client.try_request_loan(
            &f.borrower,
            &100_000,
            &500_000,
            &String::from_str(&f.env, "test"),
            &f.token,
        );
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Chaos/Paused — Req 3.3: repay blocked when paused → ContractPaused
    #[test]
    fn test_chaos_paused_repay_blocked() {
        // Set up vouch + loan BEFORE pausing, then pause.
        let f = setup_with_loan();
        f.client.emergency_pause(&f.admin).unwrap();
        let result = f.client.try_repay(&f.borrower, &50_000);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Chaos/Paused — Req 3.5: vote_slash blocked when paused → ContractPaused
    #[test]
    fn test_chaos_paused_vote_slash_blocked() {
        let f = setup_with_loan();
        f.client.emergency_pause(&f.admin).unwrap();
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    /// Chaos/Paused — Req 3.7: read-only functions still work when paused
    #[test]
    fn test_chaos_paused_reads_still_work() {
        let f = setup_with_loan();
        f.client.emergency_pause(&f.admin).unwrap();
        // get_loan and get_vouches are pure reads — must not return ContractPaused.
        let loan = f.client.get_loan(&f.borrower);
        assert!(loan.is_some(), "get_loan should return data while paused");
        let vouches = f.client.get_vouches(&f.borrower);
        assert_eq!(vouches.len(), 1, "get_vouches should return data while paused");
    }

    /// Chaos/Paused — Req 3.6: unpause restores full write access
    #[test]
    fn test_chaos_paused_unpause_restores_vouch() {
        let f = setup_paused();
        // Unpause using the admin multisig.
        f.client.emergency_unpause(&admins(&f)).unwrap();
        // Now vouch should succeed.
        let result = f.client.try_vouch(&f.voucher, &f.borrower, &1_000_000, &f.token, &None);
        assert!(result.is_ok(), "vouch must succeed after unpausing");
    }

    // ── Category 4: Token Failure Tests (Req 4) ───────────────────────────────

    /// Chaos/Token — Req 4.1: vouch with token not in allowed_tokens → InvalidToken
    #[test]
    fn test_chaos_token_invalid_token_address() {
        let f = setup_standard();
        // Register a second token that the contract doesn't know about.
        let other_asset = f
            .env
            .register_stellar_asset_contract_v2(f.admin.clone());
        let bad_token = other_asset.address();
        StellarAssetClient::new(&f.env, &bad_token).mint(&f.voucher, &1_000_000);

        let result = f
            .client
            .try_vouch(&f.voucher, &f.borrower, &1_000_000, &bad_token, &None);
        assert_eq!(result, Err(Ok(ContractError::InvalidToken)));
    }

    /// Chaos/Token — Req 4.3: vouch when voucher has zero token balance → error (no state change)
    #[test]
    fn test_chaos_token_zero_balance_voucher() {
        let f = setup_standard();
        // Create a fresh address with zero balance.
        let broke_voucher = Address::generate(&f.env);
        // Attempt to vouch with 1_000_000 but the address holds nothing.
        let result =
            f.client
                .try_vouch(&broke_voucher, &f.borrower, &1_000_000, &f.token, &None);
        assert!(result.is_err(), "vouch with zero balance must fail");
        // State must be unchanged — borrower still has no vouches.
        assert_eq!(
            f.client.get_vouches(&f.borrower).len(),
            0,
            "no vouch record should have been created"
        );
    }

    /// Chaos/Token — request_loan when contract has insufficient balance → error (no loan created)
    #[test]
    fn test_chaos_token_loan_with_insufficient_contract_balance() {
        // Set up a contract with NO token balance (skip the mint in setup_standard).
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token_asset = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token_asset.address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        // Do NOT mint any tokens to the contract.
        StellarAssetClient::new(&env, &token).mint(&voucher, &5_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client
            .initialize(&deployer, &Vec::from_array(&env, [admin.clone()]), &1, &token)
            .unwrap();
        env.ledger().with_mut(|l| l.timestamp = 90_000);
        client.vouch(&voucher, &borrower, &1_000_000, &token, &None).unwrap();

        let result = client.try_request_loan(
            &borrower,
            &100_000,
            &500_000,
            &String::from_str(&env, "test"),
            &token,
        );
        assert!(
            result.is_err(),
            "loan disbursal with zero contract balance must fail"
        );
        // Loan must not have been created.
        assert!(
            client.get_loan(&borrower).is_none(),
            "no loan record should exist after a failed disbursal"
        );
    }

    // ── Category 5: Deadline and Timing Tests (Req 5) ─────────────────────────

    /// Chaos/Deadline — Req 5.1: auto_slash at timestamp > deadline → loan marked Defaulted
    #[test]
    fn test_chaos_deadline_auto_slash_at_exact_deadline() {
        let f = setup_expired_loan();
        f.client.auto_slash(&f.borrower);
        assert_eq!(
            f.client.loan_status(&f.borrower),
            LoanStatus::Defaulted,
            "loan must be Defaulted after auto_slash past deadline"
        );
    }

    /// Chaos/Deadline — Req 5.2: repay at timestamp == deadline → succeeds (loan still active)
    #[test]
    fn test_chaos_deadline_repay_at_exact_deadline() {
        let f = setup_with_loan();
        // Move to the exact deadline (created_at=90_000, duration=30 days).
        let deadline = 90_000u64 + 30 * 24 * 60 * 60;
        f.env.ledger().with_mut(|l| l.timestamp = deadline);
        StellarAssetClient::new(&f.env, &f.token).mint(&f.borrower, &110_000);
        // Repaying at exact deadline should succeed.
        let result = f.client.try_repay(&f.borrower, &102_000);
        assert!(result.is_ok(), "repay at exact deadline must succeed");
    }

    /// Chaos/Deadline — Req 5.4: auto_slash called twice on same loan → error (no double-slash)
    #[test]
    #[should_panic]
    fn test_chaos_deadline_auto_slash_already_slashed() {
        let f = setup_expired_loan();
        f.client.auto_slash(&f.borrower);
        // Second auto_slash: ActiveLoan key is gone; the contract panics.
        f.client.auto_slash(&f.borrower);
    }

    /// Chaos/Deadline — Req 5.5: auto_slash on a fully-repaid loan → contract panics
    #[test]
    #[should_panic]
    fn test_chaos_deadline_auto_slash_on_repaid_loan() {
        let f = setup_with_loan();
        StellarAssetClient::new(&f.env, &f.token).mint(&f.borrower, &110_000);
        f.client.repay(&f.borrower, &102_000).unwrap();
        assert_eq!(f.client.loan_status(&f.borrower), LoanStatus::Repaid);
        // Advance past deadline and attempt auto_slash on an already-repaid loan.
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += 30 * 24 * 60 * 60 + 1);
        // The contract will panic because no active loan record exists.
        f.client.auto_slash(&f.borrower);
    }

    /// Chaos/Deadline — Req 5.6: vote_slash called after deadline (before auto_slash) → accepted
    #[test]
    fn test_chaos_deadline_vote_slash_after_deadline() {
        let f = setup_expired_loan();
        // Governance vote is still valid after deadline — auto_slash is a separate path.
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &true);
        assert!(
            result.is_ok(),
            "vote_slash should be accepted after deadline while loan is still Active"
        );
    }

    // ── Category 6: Multi-Voucher Stress Tests (Req 6) ────────────────────────

    /// Chaos/Vouchers — Req 6.1: loan succeeds with exactly max-vouchers-per-borrower vouches
    #[test]
    fn test_chaos_vouchers_max_count_loan_succeeds() {
        let f = setup_standard();
        // DEFAULT_MAX_VOUCHERS_PER_BORROWER = 50; add exactly 50 vouchers.
        for _ in 0..50 {
            let v = Address::generate(&f.env);
            StellarAssetClient::new(&f.env, &f.token).mint(&v, &100_000);
            f.client.vouch(&v, &f.borrower, &100_000, &f.token, &None).unwrap();
        }
        // Total stake = 50 × 100_000 = 5_000_000.  Threshold = 1_000_000.
        let result = f.client.try_request_loan(
            &f.borrower,
            &100_000,
            &1_000_000,
            &String::from_str(&f.env, "max_vouchers"),
            &f.token,
        );
        assert!(
            result.is_ok(),
            "loan with exactly max vouchers must succeed"
        );
    }

    /// Chaos/Vouchers — Req 6.2: (max+1)-th vouch → MaxVouchersPerBorrowerExceeded
    #[test]
    fn test_chaos_vouchers_exceed_max() {
        let f = setup_standard();
        // Add 50 vouchers (the maximum).
        for _ in 0..50 {
            let v = Address::generate(&f.env);
            StellarAssetClient::new(&f.env, &f.token).mint(&v, &100_000);
            f.client.vouch(&v, &f.borrower, &100_000, &f.token, &None).unwrap();
        }
        // 51st vouch must be rejected.
        let extra_voucher = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token).mint(&extra_voucher, &100_000);
        let result = f
            .client
            .try_vouch(&extra_voucher, &f.borrower, &100_000, &f.token, &None);
        assert_eq!(result, Err(Ok(ContractError::MaxVouchersPerBorrowerExceeded)));
    }

    /// Chaos/Vouchers — Req 6.3: request_loan with zero vouches → error
    #[test]
    fn test_chaos_vouchers_zero_vouchers_loan_fails() {
        let f = setup_standard();
        // No vouches for borrower.
        let result = f.client.try_request_loan(
            &f.borrower,
            &100_000,
            &500_000,
            &String::from_str(&f.env, "no_vouches"),
            &f.token,
        );
        assert!(result.is_err(), "loan with no vouches must fail");
    }

    /// Chaos/Vouchers — Req 6.4: single voucher at threshold → loan succeeds
    #[test]
    fn test_chaos_vouchers_single_voucher_meets_threshold() {
        let f = setup_with_vouch(); // voucher stake = 1_000_000, threshold = 500_000
        let result = f.client.try_request_loan(
            &f.borrower,
            &100_000,
            &500_000,
            &String::from_str(&f.env, "single_vouch"),
            &f.token,
        );
        assert!(
            result.is_ok(),
            "one voucher meeting the threshold must allow a loan"
        );
    }

    /// Chaos/Vouchers — Req 6.5: slash with max vouchers → all vouchers slashed, no panic
    #[test]
    fn test_chaos_vouchers_slash_max_vouchers() {
        let f = setup_standard();
        f.env.budget().reset_unlimited();

        let mut vouchers: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&f.env);
        for _ in 0..50 {
            let v = Address::generate(&f.env);
            StellarAssetClient::new(&f.env, &f.token).mint(&v, &100_000);
            f.client.vouch(&v, &f.borrower, &100_000, &f.token, &None).unwrap();
            vouchers.push_back(v);
        }

        f.client
            .request_loan(
                &f.borrower,
                &100_000,
                &1_000_000,
                &String::from_str(&f.env, "max_v_slash"),
                &f.token,
            )
            .unwrap();

        // Admin slash: must not panic and must default the loan.
        f.client.slash(&admins(&f), &f.borrower);
        assert_eq!(
            f.client.loan_status(&f.borrower),
            LoanStatus::Defaulted,
            "loan must be Defaulted after slashing max vouchers"
        );
    }

    // ── Category 7: Governance Chaos Tests (Req 7) ────────────────────────────

    /// Chaos/Governance — Req 7.1: vote_slash from non-voucher (zero stake) → error
    #[test]
    fn test_chaos_governance_vote_zero_stake() {
        let f = setup_with_loan();
        let outsider = Address::generate(&f.env);
        // outsider has no vouch on this borrower.
        let result = f.client.try_vote_slash(&outsider, &f.borrower, &true);
        assert!(
            result.is_err(),
            "vote_slash from an address with no stake must fail"
        );
    }

    /// Chaos/Governance — Req 7.2: same voucher votes twice → AlreadyVoted
    ///
    /// Uses three equal-stake vouchers so the first vote (33%) does not reach the 50% quorum,
    /// giving the duplicate-vote check a chance to fire.
    #[test]
    fn test_chaos_governance_duplicate_vote() {
        let f = setup_standard();

        let v1 = Address::generate(&f.env);
        let v2 = Address::generate(&f.env);
        let v3 = Address::generate(&f.env);
        for v in &[&v1, &v2, &v3] {
            StellarAssetClient::new(&f.env, &f.token).mint(*v, &1_000_000);
            f.client
                .vouch(*v, &f.borrower, &1_000_000, &f.token, &None)
                .unwrap();
        }

        f.client
            .request_loan(
                &f.borrower,
                &100_000,
                &500_000,
                &String::from_str(&f.env, "dup_vote"),
                &f.token,
            )
            .unwrap();

        // First vote by v1: 1_000_000 / 3_000_000 ≈ 33% — below 50% quorum.
        f.client.vote_slash(&v1, &f.borrower, &true).unwrap();

        // Second vote by the same v1 — must be rejected.
        let result = f.client.try_vote_slash(&v1, &f.borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::AlreadyVoted)));
    }

    /// Chaos/Governance — Req 7.4: vote_slash after quorum already executed → SlashAlreadyExecuted
    #[test]
    fn test_chaos_governance_vote_after_quorum() {
        let f = setup_with_loan();
        // With a single voucher, the first vote reaches 100% quorum → executed.
        f.client.vote_slash(&f.voucher, &f.borrower, &true).unwrap();

        // Second vote on the same (now-executed) proposal must be rejected.
        let result = f.client.try_vote_slash(&f.voucher, &f.borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::SlashAlreadyExecuted)));
    }
}
