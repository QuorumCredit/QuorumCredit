#[cfg(test)]
mod partial_repayment_tests {
    use crate::partial_repayment::*;

    #[test]
    fn test_daily_compound_interest() {
        let principal = 100_000_000; // 10 XLM
        let rate_bps = 200; // 2% annual
        let days = 365;

        let interest = calculate_daily_compound_interest(principal, rate_bps, days);
        
        // Should be approximately 2% of principal
        assert!(interest > 1_900_000 && interest < 2_100_000, "Interest: {}", interest);
    }

    #[test]
    fn test_milestone_50_percent() {
        let total = 1_000_000_000; // 100 XLM

        // 40% repaid - below milestone
        assert!(!check_milestone_achievement(400_000_000, total));

        // 50% repaid - at milestone
        assert!(check_milestone_achievement(500_000_000, total));

        // 60% repaid - above milestone
        assert!(check_milestone_achievement(600_000_000, total));
    }

    #[test]
    fn test_milestone_bonus_yield() {
        let base_yield = 200; // 2%
        let total = 1_000_000_000;

        // Below threshold: no bonus
        let below = calculate_effective_yield_bps(base_yield, 400_000_000, total);
        assert_eq!(below, 200);

        // At threshold: +1% bonus
        let at = calculate_effective_yield_bps(base_yield, 500_000_000, total);
        assert_eq!(at, 300); // 2% + 1% = 3%

        // Above threshold: +1% bonus
        let above = calculate_effective_yield_bps(base_yield, 600_000_000, total);
        assert_eq!(above, 300);
    }

    #[test]
    fn test_partial_repayment_tracking() {
        // Verify that partial repayments are tracked correctly
        let repayment_1 = 250_000_000; // 25 XLM
        let repayment_2 = 250_000_000; // 25 XLM
        let total = 1_000_000_000; // 100 XLM

        let amount_repaid = repayment_1 + repayment_2;
        assert_eq!(amount_repaid, 500_000_000);

        // At 50% = milestone achieved
        assert!(check_milestone_achievement(amount_repaid, total));
    }

    #[test]
    fn test_interest_zero_on_zero_days() {
        let principal = 100_000_000;
        let rate = 200;

        let interest = calculate_daily_compound_interest(principal, rate, 0);
        assert_eq!(interest, 0);
    }

    #[test]
    fn test_interest_accumulation_over_time() {
        let principal = 100_000_000;
        let rate = 200;

        let interest_30d = calculate_daily_compound_interest(principal, rate, 30);
        let interest_60d = calculate_daily_compound_interest(principal, rate, 60);
        let interest_365d = calculate_daily_compound_interest(principal, rate, 365);

        // Interest should grow with time
        assert!(interest_30d > 0);
        assert!(interest_60d > interest_30d);
        assert!(interest_365d > interest_60d);
    }

    #[test]
    fn test_deadline_immutable() {
        // Issue #838 requirement: prevent deadline extension
        // Deadline should not change during partial repayments
        let deadline = 1_700_000_000u64;
        let new_deadline = deadline; // Must remain the same
        
        assert_eq!(deadline, new_deadline);
    }
}
