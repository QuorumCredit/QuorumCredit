//! Property tests for daily-compound interest and milestone bonuses.
//!
//! Tests are organised in three sections:
//!
//! 1. Unit tests for `calculate_daily_compound_interest` (pure function, no env).
//! 2. Unit tests for `apply_milestone_bonus` (pure function, no env).
//! 3. Integration tests via the contract client — these exercise the full
//!    `repay()` pipeline including ledger-time advancement.
#[cfg(test)]
mod interest_tests {
    use crate::helpers::{apply_milestone_bonus, calculate_daily_compound_interest};
    use crate::types::{
        EscrowStatus, LoanRecord, LoanStatus, RateType,
        MILESTONE_FLAG_25, MILESTONE_FLAG_50, MILESTONE_FLAG_75, SECS_PER_DAY,
        COMPOUND_RATE_BPS,
    };
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    // ── Shared test helpers ───────────────────────────────────────────────────

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
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

        // Fund contract generously so yield + interest payouts never fail.
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Start at t=120 so all vouches pass MIN_VOUCH_AGE (60 s).
        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            token: token_id.address(),
            contract_id,
        }
    }

    fn purpose(env: &Env) -> String {
        String::from_str(env, "test loan")
    }

    /// Mint `amount` tokens to `addr`, vouch for `borrower`, return voucher address.
    fn do_vouch(s: &Setup, borrower: &Address, stake: i128) -> Address {
        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake);
        s.client.vouch(&voucher, borrower, &stake, &s.token, &None);
        voucher
    }

    /// Request a loan and return the loan record.
    fn do_loan(s: &Setup, borrower: &Address, amount: i128, threshold: i128) {
        s.client.request_loan(
            borrower,
            &amount,
            &threshold,
            &purpose(&s.env),
            &s.token,
        );
    }

    /// Advance ledger time by `days` days.
    fn advance_days(s: &Setup, days: u64) {
        s.env
            .ledger()
            .with_mut(|l| l.timestamp += days * SECS_PER_DAY);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Section 1: Unit tests for calculate_daily_compound_interest
    // ─────────────────────────────────────────────────────────────────────────

    /// Zero days elapsed → zero interest regardless of principal.
    #[test]
    fn test_interest_zero_days_is_zero() {
        assert_eq!(calculate_daily_compound_interest(1_000_000, 0), 0);
        assert_eq!(calculate_daily_compound_interest(i128::MAX / 2, 0), 0);
    }

    /// Non-positive principal → zero interest.
    #[test]
    fn test_interest_zero_principal_is_zero() {
        assert_eq!(calculate_daily_compound_interest(0, 30), 0);
        assert_eq!(calculate_daily_compound_interest(-1, 30), 0);
    }

    /// Sanity check: 1 day on 1_000_000 stroops at 500 bps / year.
    /// daily = 1_000_000 * 500 / 10_000 / 365 = 50_000 / 365 = 136 (truncated)
    #[test]
    fn test_interest_one_day_known_value() {
        let daily = calculate_daily_compound_interest(1_000_000, 1);
        // 1_000_000 * 500 / 10_000 / 365 = 136 (integer division)
        assert_eq!(daily, 136);
    }

    /// 30 days = 30 × daily_interest.
    #[test]
    fn test_interest_30_days_is_30x_single_day() {
        let one_day = calculate_daily_compound_interest(1_000_000, 1);
        let thirty = calculate_daily_compound_interest(1_000_000, 30);
        assert_eq!(thirty, one_day * 30);
    }

    /// 365 days ≈ annual rate (within integer rounding).
    #[test]
    fn test_interest_365_days_approximates_annual_rate() {
        let principal = 1_000_000_i128;
        let year = calculate_daily_compound_interest(principal, 365);
        let expected_annual = principal * COMPOUND_RATE_BPS / 10_000; // 50_000
        // Due to integer truncation per day, accumulated total is slightly less.
        assert!(year <= expected_annual, "should not exceed annual rate");
        // Should be very close (within 1 stroop per day max rounding = 365 stroops).
        assert!(
            year >= expected_annual - 365,
            "should not be more than 365 stroops below annual rate"
        );
    }

    /// Very large principal doesn't overflow (stays within i128).
    #[test]
    fn test_interest_large_principal_no_overflow() {
        // 10^15 stroops (1 billion XLM equivalent) over 365 days — realistic upper bound.
        let large = 1_000_000_000_000_000_i128;
        let result = calculate_daily_compound_interest(large, 365);
        assert!(result > 0);
        assert!(result < large); // interest < principal
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Section 2: Unit tests for apply_milestone_bonus
    // ─────────────────────────────────────────────────────────────────────────

    /// Helper: build a minimal LoanRecord for milestone tests without needing Env.
    /// Only the fields used by apply_milestone_bonus need to be populated.
    fn milestone_loan(
        env: &Env,
        amount: i128,
        total_yield: i128,
        accrued_interest: i128,
        milestone_bonus_applied: u32,
    ) -> LoanRecord {
        let dummy = Address::generate(env);
        LoanRecord {
            id: 1,
            borrower: dummy.clone(),
            guarantor: None,
            buyback_price: 0,
            auto_repay_enabled: false,
            auto_repay_attempts: 0,
            escrow_status: EscrowStatus::None,
            co_borrowers: Vec::new(env),
            amount,
            amount_repaid: 0,
            total_yield,
            repaid: false,
            defaulted: false,
            created_at: 0,
            disbursement_timestamp: 0,
            repayment_timestamp: None,
            deadline: 9_999_999,
            loan_purpose: String::from_str(env, "test"),
            token_address: dummy,
            last_interest_calc: 0,
            accrued_interest,
            milestone_bonus_applied,
            status: LoanStatus::Active,
            amortization_schedule: Vec::new(env),
            reminder_sent: false,
            risk_score: 0,
            deferment_periods: 0,
            maturity_date: None,
            rate_type: RateType::Fixed,
            index_reference: None,
            retry_count: 0,
            suspension_timestamp: None,
            suspension_amount_repaid: 0,
        }
    }

    /// No milestone fires when repaid fraction is below 25 %.
    #[test]
    fn test_milestone_no_bonus_below_25pct() {
        let env = Env::default();
        let loan = milestone_loan(&env, 1_000_000, 20_000, 5_000, 0);
        // total_obligation = 1_020_000; 24% repaid
        let (accrued, flags) = apply_milestone_bonus(&loan, 245_000, 1_020_000);
        assert_eq!(flags, 0, "no milestone should fire");
        assert_eq!(accrued, 5_000, "accrued interest unchanged");
    }

    /// 25 % milestone fires exactly once and reduces accrued_interest by 10 %.
    #[test]
    fn test_milestone_25pct_fires_once() {
        let env = Env::default();
        let accrued = 10_000_i128;
        let loan = milestone_loan(&env, 1_000_000, 0, accrued, 0);
        // total_obligation = 1_000_000; repaid = 250_000 (25%)
        let (new_accrued, flags) = apply_milestone_bonus(&loan, 250_000, 1_000_000);
        assert_eq!(flags & MILESTONE_FLAG_25, MILESTONE_FLAG_25, "25% flag must be set");
        // 10% discount: 10_000 - 1_000 = 9_000
        assert_eq!(new_accrued, 9_000);

        // Calling again with flag already set — no further reduction.
        let loan2 = milestone_loan(&env, 1_000_000, 0, new_accrued, flags);
        let (accrued2, flags2) = apply_milestone_bonus(&loan2, 250_000, 1_000_000);
        assert_eq!(flags2, flags, "flags unchanged on second call");
        assert_eq!(accrued2, new_accrued, "accrued unchanged on second call");
    }

    /// 50 % milestone fires exactly once and reduces by 20 %.
    #[test]
    fn test_milestone_50pct_fires_once() {
        let env = Env::default();
        let accrued = 10_000_i128;
        // 25% already applied.
        let loan = milestone_loan(&env, 1_000_000, 0, accrued, MILESTONE_FLAG_25);
        let (new_accrued, flags) = apply_milestone_bonus(&loan, 500_000, 1_000_000);
        assert_eq!(flags & MILESTONE_FLAG_50, MILESTONE_FLAG_50, "50% flag must be set");
        // 20% discount: 10_000 - 2_000 = 8_000
        assert_eq!(new_accrued, 8_000);

        // Idempotency: no re-application.
        let loan2 = milestone_loan(&env, 1_000_000, 0, new_accrued, flags);
        let (accrued2, flags2) = apply_milestone_bonus(&loan2, 500_000, 1_000_000);
        assert_eq!(accrued2, new_accrued);
        assert_eq!(flags2, flags);
    }

    /// 75 % milestone fires exactly once and reduces by 30 %.
    #[test]
    fn test_milestone_75pct_fires_once() {
        let env = Env::default();
        let accrued = 10_000_i128;
        let loan = milestone_loan(&env, 1_000_000, 0, accrued, MILESTONE_FLAG_25 | MILESTONE_FLAG_50);
        let (new_accrued, flags) = apply_milestone_bonus(&loan, 750_000, 1_000_000);
        assert_eq!(flags & MILESTONE_FLAG_75, MILESTONE_FLAG_75, "75% flag must be set");
        // 30% discount: 10_000 - 3_000 = 7_000
        assert_eq!(new_accrued, 7_000);

        // Idempotency.
        let loan2 = milestone_loan(&env, 1_000_000, 0, new_accrued, flags);
        let (accrued2, flags2) = apply_milestone_bonus(&loan2, 750_000, 1_000_000);
        assert_eq!(accrued2, new_accrued);
        assert_eq!(flags2, flags);
    }

    /// All three milestones fire in a single call when borrower repays 75%+ at once.
    #[test]
    fn test_all_milestones_fire_in_one_call() {
        let env = Env::default();
        let accrued = 10_000_i128;
        let loan = milestone_loan(&env, 1_000_000, 0, accrued, 0);
        let (new_accrued, flags) = apply_milestone_bonus(&loan, 800_000, 1_000_000);
        // All three flags set.
        assert_eq!(flags, MILESTONE_FLAG_25 | MILESTONE_FLAG_50 | MILESTONE_FLAG_75);
        // Applied in descending order (75→50→25).
        // After 75% (30%): 10_000 - 3_000 = 7_000
        // After 50% (20%): 7_000 - 1_400 = 5_600
        // After 25% (10%): 5_600 - 560 = 5_040
        assert_eq!(new_accrued, 5_040);
    }

    /// Accrued interest floor is 0 — never goes negative.
    #[test]
    fn test_milestone_discount_floors_at_zero() {
        let env = Env::default();
        // Very small accrued interest — discounts would make it negative without floor.
        let loan = milestone_loan(&env, 1_000_000, 0, 1, 0);
        let (new_accrued, _) = apply_milestone_bonus(&loan, 800_000, 1_000_000);
        assert!(new_accrued >= 0, "accrued interest must never be negative");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Section 3: Integration tests via contract client
    // ─────────────────────────────────────────────────────────────────────────

    /// Same-day repayment: interest accrues 0 days → total_owed unchanged from
    /// principal + static yield.
    #[test]
    fn test_same_day_repay_no_interest() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 2_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        let loan_before = s.client.get_loan(&borrower).unwrap();
        // No time advance — same ledger second.
        s.client.repay(&borrower, &1_000);
        // If repay panicked, the test would fail above.

        let loan_after = s.client.get_loan(&borrower).unwrap();
        // accrued_interest should still be 0 (0 whole days elapsed).
        assert_eq!(loan_after.accrued_interest, 0);
        assert_eq!(loan_after.amount_repaid, 1_000);
        // last_interest_calc unchanged (0 days elapsed means no advance).
        assert_eq!(loan_after.last_interest_calc, loan_before.last_interest_calc);
    }

    /// Two same-day repayments don't double-charge interest.
    #[test]
    fn test_two_same_day_repayments_no_double_interest() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 2_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        s.client.repay(&borrower, &1_000);
        s.client.repay(&borrower, &1_000);

        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.accrued_interest, 0, "still same day — zero interest");
        assert_eq!(loan.amount_repaid, 2_000);
    }

    /// After 30 days, accrued_interest is positive and matches hand-calculated value.
    #[test]
    fn test_30_day_gap_accrues_correct_interest() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 2_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        advance_days(&s, 30);

        // Outstanding principal = 100_000 (nothing repaid yet).
        let expected_interest = calculate_daily_compound_interest(100_000, 30);
        assert!(expected_interest > 0, "30-day interest must be positive");

        // Paying just 1 stroop triggers the accrual pipeline.
        s.client.repay(&borrower, &1);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(
            loan.accrued_interest, expected_interest,
            "accrued_interest after 30-day gap should match calculated value"
        );
    }

    /// After a 365-day gap the accrued interest is close to the annual rate.
    #[test]
    fn test_365_day_gap_interest_near_annual_rate() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 4_000_000);
        // Fund extra so 365-day payout is covered.
        StellarAssetClient::new(&s.env, &s.token).mint(&s.contract_id, &500_000);
        do_loan(&s, &borrower, 200_000, 2_000_000);

        advance_days(&s, 365);

        let expected = calculate_daily_compound_interest(200_000, 365);
        let annual_rate = 200_000_i128 * crate::types::COMPOUND_RATE_BPS / 10_000;

        // Trigger accrual.
        s.client.repay(&borrower, &1);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.accrued_interest, expected);
        // Interest should be ≤ annual rate (integer truncation keeps it slightly under).
        assert!(loan.accrued_interest <= annual_rate);
        assert!(loan.accrued_interest >= annual_rate - 365);
    }

    /// Sequential partial repayments over multiple days accumulate interest
    /// independently per period.
    #[test]
    fn test_sequential_partial_repayments_accumulate_interest() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 4_000_000);
        StellarAssetClient::new(&s.env, &s.token).mint(&s.contract_id, &500_000);
        do_loan(&s, &borrower, 300_000, 2_000_000);

        // Day 0 → advance 10 days → repay 30_000.
        advance_days(&s, 10);
        let interest_period_1 = calculate_daily_compound_interest(300_000, 10);
        s.client.repay(&borrower, &30_000);
        let loan_1 = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan_1.accrued_interest, interest_period_1);

        // Advance another 10 days → repay 30_000.
        advance_days(&s, 10);
        // Outstanding principal has not changed (amount_repaid applies to
        // total_owed which includes interest; see loan.rs).
        let outstanding_2 = (300_000_i128 - 30_000).max(0);
        let interest_period_2 = calculate_daily_compound_interest(outstanding_2, 10);
        s.client.repay(&borrower, &30_000);
        let loan_2 = s.client.get_loan(&borrower).unwrap();
        // Accrued should include both periods' interest.
        assert!(
            loan_2.accrued_interest > loan_1.accrued_interest,
            "accrued interest should grow with additional periods"
        );
    }

    /// Sub-day remainder: advancing 1.5 days should accrue exactly 1 day of interest,
    /// not 1.5 days (whole-day granularity).
    #[test]
    fn test_sub_day_remainder_truncated_to_whole_days() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 2_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        // Advance 1.5 days (1 day + 43_200 seconds).
        s.env
            .ledger()
            .with_mut(|l| l.timestamp += SECS_PER_DAY + 43_200);

        s.client.repay(&borrower, &1);
        let loan = s.client.get_loan(&borrower).unwrap();
        // Should charge exactly 1 day, not 1.5.
        let expected = calculate_daily_compound_interest(100_000, 1);
        assert_eq!(loan.accrued_interest, expected);

        // last_interest_calc should advance by exactly 1 day (not 1.5).
        let orig_ts = 120_u64; // setup timestamp
        let disbursement = orig_ts; // disbursed at ts=120
        assert_eq!(
            loan.last_interest_calc,
            disbursement + SECS_PER_DAY,
            "last_interest_calc advances by whole days only"
        );
    }

    /// Milestone: 25% milestone fires exactly once across multiple repayment calls.
    #[test]
    fn test_milestone_25pct_fires_exactly_once_via_repay() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 4_000_000);
        StellarAssetClient::new(&s.env, &s.token).mint(&s.contract_id, &500_000);
        do_loan(&s, &borrower, 200_000, 2_000_000);

        // Advance 30 days to build up some accrued interest.
        advance_days(&s, 30);

        // Pay just below the 25% threshold (24% of principal + static yield).
        // total_obligation = 200_000 + 200_000*200/10_000 = 200_000 + 4_000 = 204_000
        // 24% of 204_000 = 48_960 — pay 48_000 (below 25%).
        s.client.repay(&borrower, &48_000);
        let loan_a = s.client.get_loan(&borrower).unwrap();
        assert_eq!(
            loan_a.milestone_bonus_applied & crate::types::MILESTONE_FLAG_25,
            0,
            "25% milestone must not fire before threshold"
        );

        let accrued_before_milestone = loan_a.accrued_interest;

        // Now push over 25% (total_repaid = 48_000 + 5_000 = 53_000 > 51_000).
        s.client.repay(&borrower, &5_000);
        let loan_b = s.client.get_loan(&borrower).unwrap();
        assert_ne!(
            loan_b.milestone_bonus_applied & crate::types::MILESTONE_FLAG_25,
            0,
            "25% milestone must fire after crossing threshold"
        );
        // Accrued interest should be LESS after the bonus.
        assert!(
            loan_b.accrued_interest <= accrued_before_milestone,
            "milestone should reduce or hold accrued interest"
        );

        // Make another payment — milestone must NOT fire again.
        let accrued_after_milestone = loan_b.accrued_interest;
        let flags_after_milestone = loan_b.milestone_bonus_applied;
        s.client.repay(&borrower, &1_000);
        let loan_c = s.client.get_loan(&borrower).unwrap();
        assert_eq!(
            loan_c.milestone_bonus_applied, flags_after_milestone,
            "milestone flags must not change on subsequent calls"
        );
        // Accrued interest should stay the same (no new days elapsed).
        assert_eq!(
            loan_c.accrued_interest, accrued_after_milestone,
            "no new interest between same-day calls"
        );
    }

    /// Milestone: 50% and 75% milestones each fire exactly once.
    #[test]
    fn test_milestone_50_and_75_fire_exactly_once() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 4_000_000);
        StellarAssetClient::new(&s.env, &s.token).mint(&s.contract_id, &1_000_000);
        do_loan(&s, &borrower, 200_000, 2_000_000);

        advance_days(&s, 30);

        // Cross the 50% mark in one payment.
        // total_obligation_for_milestone = principal + total_yield = 204_000
        // 50% = 102_000
        s.client.repay(&borrower, &102_000);
        let loan_50 = s.client.get_loan(&borrower).unwrap();
        assert_ne!(
            loan_50.milestone_bonus_applied & crate::types::MILESTONE_FLAG_50,
            0,
            "50% milestone should fire"
        );

        let flags_at_50 = loan_50.milestone_bonus_applied;

        // Cross 75%: pay another 51_000 (total 153_000 = 75% of 204_000).
        advance_days(&s, 5);
        s.client.repay(&borrower, &51_000);
        let loan_75 = s.client.get_loan(&borrower).unwrap();
        assert_ne!(
            loan_75.milestone_bonus_applied & crate::types::MILESTONE_FLAG_75,
            0,
            "75% milestone should fire"
        );

        // Re-pay a small amount — no new milestone fires.
        let flags_at_75 = loan_75.milestone_bonus_applied;
        s.client.repay(&borrower, &100);
        let loan_after = s.client.get_loan(&borrower).unwrap();
        assert_eq!(
            loan_after.milestone_bonus_applied, flags_at_75,
            "no new flags after all milestones already set"
        );
        let _ = flags_at_50; // suppress unused warning
    }

    /// Very long gap (730 days = 2 years): interest accumulates correctly
    /// and does not overflow for realistic loan sizes.
    #[test]
    fn test_very_long_gap_no_overflow() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 4_000_000);
        // Extra funding to cover 2-year interest.
        StellarAssetClient::new(&s.env, &s.token).mint(&s.contract_id, &5_000_000);
        do_loan(&s, &borrower, 200_000, 2_000_000);

        advance_days(&s, 730);

        let expected = calculate_daily_compound_interest(200_000, 730);

        // Trigger accrual with minimum payment.
        s.client.repay(&borrower, &1);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.accrued_interest, expected);
        assert!(loan.accrued_interest >= 0, "must not overflow to negative");
    }
}
