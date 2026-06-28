/// Incentive verification test suite.
///
/// Validates every incentive mechanism pays out the mathematically-correct
/// amount: vouch-age bonus, borrower reputation bonus, voucher reputation bonus,
/// voucher reliability bonus, credit-tier yield rewards, prepayment bonus, and
/// proportional yield distribution across multiple vouchers.
///
/// Call-signature conventions are derived directly from coverage_test.rs:
///   vouch(voucher, borrower, stake, token)          ← 4 args, no chain_id
///   is_eligible(borrower, threshold)                ← 2 args, uses config token
///   request_forbearance(borrower, duration: Option) ← with Option<u64>
///
/// All monetary values are in stroops (1 XLM = 10_000_000 stroops).
#[cfg(test)]
mod incentives_verification_tests {
    use crate::{CreditTier, QuorumCreditContract, QuorumCreditContractClient};
    use crate::types::{DataKey, VoucherStats};
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Env, String, Vec,
    };

    const ONE_XLM: i128    = 10_000_000;
    const DAY: u64          = 24 * 60 * 60;
    const THIRTY_DAYS: u64  = 30 * DAY;
    const SIXTY_DAYS: u64   = 60 * DAY;
    const BPS: i128         = 10_000;

    // ─────────────────────────── setup ────────────────────────────────────────
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

    /// Vouch and advance clock so the vouch has `age_secs` of age.
    fn vouch_aged(s: &Setup, voucher: &Address, borrower: &Address, stake: i128, age_secs: u64) {
        StellarAssetClient::new(&s.env, &s.token).mint(voucher, &stake);
        s.client.vouch(voucher, borrower, &stake, &s.token, &None);
        s.env.ledger().with_mut(|l| l.timestamp += age_secs);
    }

    /// Vouch and advance just past MIN_VOUCH_AGE (61 s).
    fn do_vouch(s: &Setup, voucher: &Address, borrower: &Address, stake: i128) {
        vouch_aged(s, voucher, borrower, stake, 61);
    }

    fn do_loan(s: &Setup, borrower: &Address, amount: i128) {
        s.client.request_loan(
            borrower, &amount, &(amount / 2),
            &String::from_str(&s.env, "test"), &s.token,
        );
    }

    fn do_full_repay(s: &Setup, borrower: &Address) {
        let loan = s.client.get_loan(borrower).expect("no loan");
        let owed = loan.amount + loan.total_yield - loan.amount_repaid;
        StellarAssetClient::new(&s.env, &s.token).mint(borrower, &owed);
        s.client.repay(borrower, &owed);
    }

    // ────────────────────── 1. Vouch-age bonus ────────────────────────────────

    #[test]
    fn test_vouch_age_no_bonus_under_30_days() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        // Only MIN_VOUCH_AGE elapses (61 s < 30 days)
        vouch_aged(&s, &voucher, &borrower, ONE_XLM, 61);
        do_loan(&s, &borrower, ONE_XLM / 2);
        let loan = s.client.get_loan(&borrower).unwrap();
        // base 200 bps, no age bonus
        assert_eq!(loan.total_yield, ONE_XLM / 2 * 200 / BPS,
            "yield must equal base-only when vouch age < 30 days");
    }

    #[test]
    fn test_vouch_age_bonus_one_period_adds_25_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        vouch_aged(&s, &voucher, &borrower, ONE_XLM, THIRTY_DAYS);
        do_loan(&s, &borrower, ONE_XLM / 2);
        let loan = s.client.get_loan(&borrower).unwrap();
        // 200 + 25 = 225 bps
        assert_eq!(loan.total_yield, ONE_XLM / 2 * 225 / BPS,
            "one 30-day period must add +25 bps");
    }

    #[test]
    fn test_vouch_age_bonus_two_periods_adds_50_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        vouch_aged(&s, &voucher, &borrower, ONE_XLM, SIXTY_DAYS);
        do_loan(&s, &borrower, ONE_XLM / 2);
        let loan = s.client.get_loan(&borrower).unwrap();
        // 200 + 50 = 250 bps
        assert_eq!(loan.total_yield, ONE_XLM / 2 * 250 / BPS,
            "two 30-day periods must add +50 bps");
    }

    #[test]
    fn test_vouch_age_bonus_capped_at_200_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        // 12 months = 12 × 25 = 300 raw → capped at 200
        vouch_aged(&s, &voucher, &borrower, ONE_XLM, 365 * DAY);
        do_loan(&s, &borrower, ONE_XLM / 2);
        let loan = s.client.get_loan(&borrower).unwrap();
        // 200 + 200 cap = 400 bps
        assert_eq!(loan.total_yield, ONE_XLM / 2 * 400 / BPS,
            "vouch-age bonus must cap at 200 bps");
    }

    // ──────────────────── 2. Borrower reputation bonus ────────────────────────

    #[test]
    fn test_borrower_rep_bonus_zero_history() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 2);
        let loan = s.client.get_loan(&borrower).unwrap();
        // new borrower: rep_bonus = 0, base = 200 bps
        assert_eq!(loan.total_yield, ONE_XLM / 2 * 200 / BPS);
    }

    #[test]
    fn test_borrower_rep_bonus_five_repayments_adds_50_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // Build 5 successful repayments
        for _ in 0..5 {
            do_vouch(&s, &voucher, &borrower, ONE_XLM);
            do_loan(&s, &borrower, ONE_XLM / 4);
            do_full_repay(&s, &borrower);
            s.env.ledger().with_mut(|l| l.timestamp += 10);
        }

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();

        // base=200, borrower_rep=50 (5×10), plus voucher_rep & reliability from 5 successes
        // Voucher stats: 5 successful, 0 slashed → reliability=150, voucher_rep=50
        // Total = 200 + 50 + 50 + 150 = 450 bps
        let expected = ONE_XLM / 4 * 450 / BPS;
        assert_eq!(loan.total_yield, expected,
            "5 repayments: base200 + borrower_rep50 + voucher_rep50 + reliability150 = 450 bps");
    }

    #[test]
    fn test_borrower_rep_penalty_one_default_is_zero_net() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        s.client.slash(&admins(&s), &borrower);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();

        // 0 repayments, 1 default → rep_bonus = max(0 - 20, 0) = 0
        // voucher: 0 successes, 1 slash → reliability=0, voucher_rep=0
        // total = 200 bps
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 200 / BPS,
            "one default should produce zero net rep bonus");
    }

    #[test]
    fn test_borrower_rep_bonus_capped_at_100_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // 20 repayments: raw = 200 bps, capped at 100
        for _ in 0..20 {
            do_vouch(&s, &voucher, &borrower, ONE_XLM);
            do_loan(&s, &borrower, ONE_XLM / 4);
            do_full_repay(&s, &borrower);
            s.env.ledger().with_mut(|l| l.timestamp += 10);
        }

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();

        // base=200, borrower_rep capped=100, voucher_rep capped=100, reliability=150
        let expected = ONE_XLM / 4 * (200 + 100 + 100 + 150) / BPS;
        assert_eq!(loan.total_yield, expected,
            "borrower reputation bonus must cap at 100 bps");
    }

    // ─────────────────── 3. Voucher reputation & reliability bonuses ──────────

    /// Inject VoucherStats directly — avoids the multi-cycle setup overhead.
    fn inject_voucher_stats(s: &Setup, voucher: &Address, stats: VoucherStats) {
        s.env.as_contract(&s.client.address, || {
            s.env.storage().persistent()
                .set(&DataKey::VoucherStats(voucher.clone()), &stats);
        });
    }

    #[test]
    fn test_voucher_rep_bonus_three_successes_no_slashes() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 3,
            total_vouches_slashed: 0,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan = s.client.get_loan(&borrower).unwrap();
        // voucher_rep = min(3×10, 100) = 30; reliability = 150 (perfect); borrower_rep = 0
        // total = 200 + 30 + 150 = 380 bps
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 380 / BPS,
            "3 successful vouches should yield +30 bps rep + 150 bps reliability");
    }

    #[test]
    fn test_voucher_rep_bonus_capped_at_100_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // 15 successes → 150 raw → capped at 100
        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 15,
            total_vouches_slashed: 0,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan = s.client.get_loan(&borrower).unwrap();
        // voucher_rep capped=100; reliability=150; borrower_rep=0
        // total = 200 + 100 + 150 = 450 bps
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 450 / BPS,
            "voucher reputation bonus must cap at 100 bps");
    }

    #[test]
    fn test_reliability_bonus_perfect_record_150_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 1,
            total_vouches_slashed: 0,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan = s.client.get_loan(&borrower).unwrap();
        // reliability = 150 (perfect); voucher_rep = min(10,100) = 10
        // total = 200 + 10 + 150 = 360 bps
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 360 / BPS,
            "perfect record (1 success, 0 slashes) must give +150 bps reliability");
    }

    #[test]
    fn test_reliability_bonus_mixed_record_50pct() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // 1 success, 1 slash → ratio = 50% → reliability = 150 * 5000/10000 = 75
        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 1,
            total_vouches_slashed: 1,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan = s.client.get_loan(&borrower).unwrap();
        // reliability = 75; voucher_rep = min(1×10,100) = 10
        // total = 200 + 10 + 75 = 285 bps
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 285 / BPS,
            "50%% success rate must give 75 bps reliability bonus");
    }

    #[test]
    fn test_reliability_bonus_only_slashes_zero() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // 0 successes, 3 slashes → reliability = 0
        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 0,
            total_vouches_slashed: 3,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan = s.client.get_loan(&borrower).unwrap();
        // reliability = 0; voucher_rep = 0; total = 200 bps
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 200 / BPS,
            "zero successes with slashes must give 0 reliability/rep bonus");
    }

    // ──────────────────── 4. Credit-tier yield rewards ────────────────────────

    fn inject_credit_score(s: &Setup, borrower: &Address, tier: CreditTier, score: u32) {
        use crate::types::CreditScore;
        s.env.as_contract(&s.client.address, || {
            s.env.storage().persistent().set(
                &DataKey::CreditScore(borrower.clone()),
                &CreditScore {
                    score,
                    tier,
                    last_updated: s.env.ledger().timestamp(),
                    total_loans: 0,
                    successful_repayments: 0,
                    defaults: 0,
                    total_borrowed: 0,
                    total_repaid: 0,
                    account_age: 0,
                    voucher_count: 0,
                    avg_repayment_time: 0,
                },
            );
        });
    }

    #[test]
    fn test_tier_poor_adds_zero_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        inject_credit_score(&s, &borrower, CreditTier::Poor, 200);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 200 / BPS, "Poor tier: +0 bps");
    }

    #[test]
    fn test_tier_fair_adds_50_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        inject_credit_score(&s, &borrower, CreditTier::Fair, 400);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 250 / BPS, "Fair tier: +50 bps");
    }

    #[test]
    fn test_tier_good_adds_100_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        inject_credit_score(&s, &borrower, CreditTier::Good, 600);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 300 / BPS, "Good tier: +100 bps");
    }

    #[test]
    fn test_tier_very_good_adds_150_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        inject_credit_score(&s, &borrower, CreditTier::VeryGood, 750);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 350 / BPS, "VeryGood tier: +150 bps");
    }

    #[test]
    fn test_tier_excellent_adds_200_bps() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        inject_credit_score(&s, &borrower, CreditTier::Excellent, 900);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan = s.client.get_loan(&borrower).unwrap();
        assert_eq!(loan.total_yield, ONE_XLM / 4 * 400 / BPS, "Excellent tier: +200 bps");
    }

    // ──────────────────── 5. Prepayment bonus ─────────────────────────────────

    #[test]
    fn test_prepayment_bonus_zero_bps_no_bonus() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        s.client.set_prepayment_bonus_bps(&admins(&s), &0u32);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan     = s.client.get_loan(&borrower).unwrap();
        let owed     = loan.amount + loan.total_yield;

        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &owed);
        let before = TokenClient::new(&s.env, &s.token).balance(&borrower);
        s.client.repay(&borrower, &owed);
        let after  = TokenClient::new(&s.env, &s.token).balance(&borrower);

        // Net cost == exactly owed (no bonus refunded)
        assert_eq!(before - after, owed,
            "bonus_bps=0 must yield no prepayment bonus");
    }

    #[test]
    fn test_prepayment_bonus_at_deadline_is_zero() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        s.client.set_prepayment_bonus_bps(&admins(&s), &100u32); // 1%

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);

        let loan = s.client.get_loan(&borrower).unwrap();
        // Advance to exact deadline: time_remaining = 0 → bonus = 0
        s.env.ledger().with_mut(|l| l.timestamp = loan.deadline);

        let owed = loan.amount + loan.total_yield;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &owed);
        let before = TokenClient::new(&s.env, &s.token).balance(&borrower);
        s.client.repay(&borrower, &owed);
        let after  = TokenClient::new(&s.env, &s.token).balance(&borrower);

        assert_eq!(before - after, owed,
            "repaying at deadline must produce zero prepayment bonus");
    }

    #[test]
    fn test_prepayment_bonus_immediate_repayment() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        let bonus_bps: u32 = 100; // 1%
        s.client.set_prepayment_bonus_bps(&admins(&s), &bonus_bps);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        let loan_amount: i128 = ONE_XLM / 4;
        do_loan(&s, &borrower, loan_amount);

        let loan           = s.client.get_loan(&borrower).unwrap();
        let now            = s.env.ledger().timestamp();
        let total_duration = loan.deadline - loan.disbursement_timestamp;
        let time_remaining = loan.deadline - now;

        // Replicate apply_prepayment_bonus formula exactly
        let early_ratio_bps = (time_remaining as i128 * BPS) / total_duration as i128;
        let expected_bonus  = loan_amount * bonus_bps as i128 * early_ratio_bps / (BPS * BPS);

        let owed = loan.amount + loan.total_yield;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &owed);
        let before = TokenClient::new(&s.env, &s.token).balance(&borrower);
        s.client.repay(&borrower, &owed);
        let after  = TokenClient::new(&s.env, &s.token).balance(&borrower);

        // Borrower paid `owed` and got `expected_bonus` back
        let net_out = owed - expected_bonus;
        assert_eq!(before - after, net_out,
            "immediate repayment must refund the expected prepayment bonus");
        assert!(expected_bonus > 0,
            "bonus must be positive for immediate repayment with non-zero bps");
    }

    #[test]
    fn test_prepayment_bonus_half_duration_remaining() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        let bonus_bps: u32 = 200;
        s.client.set_prepayment_bonus_bps(&admins(&s), &bonus_bps);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        let loan_amount: i128 = ONE_XLM / 4;
        do_loan(&s, &borrower, loan_amount);

        let loan           = s.client.get_loan(&borrower).unwrap();
        let total_duration = loan.deadline - loan.disbursement_timestamp;
        // Advance to the halfway point
        s.env.ledger().with_mut(|l| {
            l.timestamp = loan.disbursement_timestamp + total_duration / 2;
        });

        let time_remaining  = loan.deadline - s.env.ledger().timestamp();
        let early_ratio_bps = (time_remaining as i128 * BPS) / total_duration as i128;
        let expected_bonus  = loan_amount * bonus_bps as i128 * early_ratio_bps / (BPS * BPS);

        let owed = loan.amount + loan.total_yield;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &owed);
        let before = TokenClient::new(&s.env, &s.token).balance(&borrower);
        s.client.repay(&borrower, &owed);
        let after  = TokenClient::new(&s.env, &s.token).balance(&borrower);

        let net_out = owed - expected_bonus;
        assert_eq!(before - after, net_out,
            "half-duration remaining must produce proportional bonus (~50%% of max)");
    }

    // ─────────── 6. Reputation weight in is_eligible ──────────────────────────

    #[test]
    fn test_reputation_weight_increases_effective_stake() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // weight = BPS + 2*500 = 11_000 → effective_stake = stake * 1.1
        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 2,
            total_vouches_slashed: 0,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        let stake: i128 = ONE_XLM;
        do_vouch(&s, &voucher, &borrower, stake);

        // threshold = 1.05× raw stake (above raw, below weighted)
        let threshold = stake * 10_500 / BPS;
        assert!(s.client.is_eligible(&borrower, &threshold),
            "high-rep voucher's weighted stake must clear a threshold above raw stake");
    }

    #[test]
    fn test_is_eligible_zero_threshold_returns_false() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        // must return false cleanly, never panic
        assert!(!s.client.is_eligible(&borrower, &0i128));
    }

    #[test]
    fn test_is_eligible_no_vouches_returns_false() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        assert!(!s.client.is_eligible(&borrower, &ONE_XLM));
    }

    // ─────────── 7. Yield distribution locked at disbursement ────────────────

    #[test]
    fn test_yield_distribution_proportional_two_vouchers() {
        let s = setup();
        let voucher1 = Address::generate(&s.env);
        let voucher2 = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // 1 XLM vs 3 XLM stake → 1:3 ratio
        do_vouch(&s, &voucher1, &borrower, ONE_XLM);
        s.env.ledger().with_mut(|l| l.timestamp += 10);
        do_vouch(&s, &voucher2, &borrower, ONE_XLM * 3);

        let loan_amount: i128 = ONE_XLM / 2;
        do_loan(&s, &borrower, loan_amount);

        let loan        = s.client.get_loan(&borrower).unwrap();
        let total_yield = loan.total_yield;

        let v1_before = TokenClient::new(&s.env, &s.token).balance(&voucher1);
        let v2_before = TokenClient::new(&s.env, &s.token).balance(&voucher2);

        do_full_repay(&s, &borrower);

        let v1_gain = TokenClient::new(&s.env, &s.token).balance(&voucher1) - v1_before;
        let v2_gain = TokenClient::new(&s.env, &s.token).balance(&voucher2) - v2_before;

        let v1_yield = v1_gain - ONE_XLM;
        let v2_yield = v2_gain - ONE_XLM * 3;

        // Sum of yields must equal total_yield
        assert_eq!(v1_yield + v2_yield, total_yield,
            "sum of individual yields must equal total_yield");

        // v2_yield ≈ 3 × v1_yield (allow ±1 stroop for integer rounding)
        let diff = (v2_yield - v1_yield * 3).abs();
        assert!(diff <= 1,
            "voucher2 with 3x stake should receive ~3x yield (diff={})", diff);
    }

    #[test]
    fn test_voucher_stats_updated_after_successful_repayment() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        let loan_yield = s.client.get_loan(&borrower).unwrap().total_yield;
        do_full_repay(&s, &borrower);

        let stats: VoucherStats = s.env.as_contract(&s.client.address, || {
            s.env.storage().persistent()
                .get(&DataKey::VoucherStats(voucher.clone()))
                .expect("VoucherStats must exist after repayment")
        });

        assert_eq!(stats.successful_vouches, 1,
            "successful_vouches must increment on repayment");
        assert_eq!(stats.total_yield_earned, loan_yield,
            "total_yield_earned must equal the actual yield paid");
        assert_eq!(stats.total_vouches_slashed, 0);
    }

    #[test]
    fn test_voucher_stats_updated_after_slash() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM / 4);
        s.client.slash(&admins(&s), &borrower);

        let stats: VoucherStats = s.env.as_contract(&s.client.address, || {
            s.env.storage().persistent()
                .get(&DataKey::VoucherStats(voucher.clone()))
                .expect("VoucherStats must exist after slash")
        });

        assert_eq!(stats.total_vouches_slashed, 1,
            "total_vouches_slashed must increment on slash");
        assert!(stats.total_slashed > 0,
            "total_slashed must be non-zero after slash");
        assert_eq!(stats.successful_vouches, 0);
    }

    // ──────────────────── 8. End-to-end incentive flow ────────────────────────

    /// High-rep voucher + early repayment: all bonuses compose correctly.
    #[test]
    fn test_end_to_end_all_bonuses_compose() {
        let s = setup();
        let voucher  = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        // Enable prepayment bonus
        s.client.set_prepayment_bonus_bps(&admins(&s), &50u32);

        // Voucher: 5 successes, 0 slashes
        inject_voucher_stats(&s, &voucher, VoucherStats {
            successful_vouches: 5,
            total_vouches_slashed: 0,
            total_yield_earned: 0,
            total_slashed: 0,
        });

        // Vouch with 30-day age so age_bonus = +25 bps
        vouch_aged(&s, &voucher, &borrower, ONE_XLM, THIRTY_DAYS);

        let loan_amount: i128 = ONE_XLM / 2;
        do_loan(&s, &borrower, loan_amount);

        let loan = s.client.get_loan(&borrower).unwrap();

        // Expected yield components (borrower has no history → rep_bonus = 0):
        //   base           = 200
        //   age_bonus      =  25  (1 × 30-day period)
        //   borrower_rep   =   0  (fresh borrower)
        //   voucher_rep    =  50  (min(5×10, 100))
        //   reliability    = 150  (5 successes, 0 slashes)
        //   tier_delta     =   0  (no credit score stored)
        //   total rate     = 425 bps
        let expected_rate: i128 = 200 + 25 + 0 + 50 + 150;
        assert_eq!(loan.total_yield, loan_amount * expected_rate / BPS,
            "end-to-end: composed yield must equal 425 bps");

        // Repay immediately to capture prepayment bonus
        let now            = s.env.ledger().timestamp();
        let total_duration = loan.deadline - loan.disbursement_timestamp;
        let time_remaining = loan.deadline - now;
        let early_ratio    = (time_remaining as i128 * BPS) / total_duration as i128;
        let expected_bonus = loan_amount * 50i128 * early_ratio / (BPS * BPS);

        let voucher_before  = TokenClient::new(&s.env, &s.token).balance(&voucher);
        let borrower_before = TokenClient::new(&s.env, &s.token).balance(&borrower);

        let owed = loan.amount + loan.total_yield;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &owed);
        s.client.repay(&borrower, &owed);

        let voucher_gain  = TokenClient::new(&s.env, &s.token).balance(&voucher) - voucher_before;
        let borrower_end  = TokenClient::new(&s.env, &s.token).balance(&borrower);

        // Voucher receives stake + their yield share
        assert_eq!(voucher_gain, ONE_XLM + loan.total_yield,
            "voucher must receive stake + total yield on repayment");

        // Borrower receives prepayment bonus back
        let borrower_net_cost = (borrower_before + owed) - borrower_end;
        assert_eq!(borrower_net_cost, owed - expected_bonus,
            "borrower must receive prepayment bonus back");
    }

} // mod incentives_verification_tests
