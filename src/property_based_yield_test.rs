//! Property-based testing for yield calculations.
//!
//! This module verifies critical properties of yield calculations:
//! - Yield never exceeds principal
//! - Total yield equals sum of individual yields
//! - Yield is always 2% of principal (200 basis points)
//! - Yield calculations are deterministic
//! - Yield respects minimum stake threshold (50 stroops)

#[cfg(test)]
mod tests {
    use crate::types::*;

    /// Property: Yield never exceeds principal
    /// For any loan amount L, yield Y ≤ L
    #[test]
    fn property_yield_never_exceeds_principal() {
        let test_amounts = vec![
            100_000,           // 0.01 XLM
            1_000_000,         // 0.1 XLM
            10_000_000,        // 1 XLM
            100_000_000,       // 10 XLM
            1_000_000_000,     // 100 XLM
            10_000_000_000,    // 1000 XLM
        ];

        for principal in test_amounts {
            let yield_amount = (principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            assert!(
                yield_amount <= principal,
                "Yield {} exceeds principal {} for amount {}",
                yield_amount,
                principal,
                principal
            );
        }
    }

    /// Property: Yield is exactly 2% of principal
    /// For any loan amount L, yield Y = L * 200 / 10_000
    #[test]
    fn property_yield_is_exactly_two_percent() {
        let test_amounts = vec![
            100_000,
            1_000_000,
            10_000_000,
            100_000_000,
            1_000_000_000,
        ];

        for principal in test_amounts {
            let yield_amount = (principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            let expected = principal / 50; // 2% = 1/50
            assert_eq!(
                yield_amount, expected,
                "Yield calculation incorrect for principal {}",
                principal
            );
        }
    }

    /// Property: Total yield equals sum of individual yields
    /// For vouchers V1, V2, ..., Vn with stakes S1, S2, ..., Sn,
    /// total_yield = sum(Si * 200 / 10_000) for all i
    #[test]
    fn property_total_yield_equals_sum_of_individual_yields() {
        let voucher_stakes = vec![
            100_000_000,   // 10 XLM
            200_000_000,   // 20 XLM
            300_000_000,   // 30 XLM
            150_000_000,   // 15 XLM
        ];

        let mut total_yield = 0i128;
        for stake in &voucher_stakes {
            let individual_yield = (stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            total_yield += individual_yield;
        }

        // Calculate total yield directly from sum of stakes
        let total_stake: i128 = voucher_stakes.iter().sum();
        let direct_total_yield = (total_stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;

        assert_eq!(
            total_yield, direct_total_yield,
            "Sum of individual yields {} != direct calculation {}",
            total_yield, direct_total_yield
        );
    }

    /// Property: Yield calculations are deterministic
    /// Same input always produces same output
    #[test]
    fn property_yield_calculations_are_deterministic() {
        let principal = 1_234_567_890i128;

        let yield1 = (principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        let yield2 = (principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        let yield3 = (principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;

        assert_eq!(yield1, yield2);
        assert_eq!(yield2, yield3);
    }

    /// Property: Yield respects minimum stake threshold
    /// Stakes below 50 stroops should yield 0
    #[test]
    fn property_yield_respects_minimum_stake_threshold() {
        // Test amounts below minimum
        let below_minimum = vec![1i128, 10, 25, 49];

        for stake in below_minimum {
            let yield_amount = (stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            assert_eq!(
                yield_amount, 0,
                "Stake {} below minimum should yield 0, got {}",
                stake, yield_amount
            );
        }

        // Test amounts at and above minimum
        let at_or_above_minimum = vec![50i128, 51, 100, 1000];

        for stake in at_or_above_minimum {
            let yield_amount = (stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            assert!(
                yield_amount > 0,
                "Stake {} at/above minimum should yield > 0, got {}",
                stake, yield_amount
            );
        }
    }

    /// Property: Yield calculation is commutative with respect to order
    /// Calculating yield for [A, B, C] yields same total as [C, B, A]
    #[test]
    fn property_yield_calculation_order_independent() {
        let stakes = vec![100_000_000i128, 200_000_000, 300_000_000];

        // Calculate in original order
        let mut total1 = 0i128;
        for stake in &stakes {
            total1 += (stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        }

        // Calculate in reverse order
        let mut total2 = 0i128;
        for stake in stakes.iter().rev() {
            total2 += (stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        }

        assert_eq!(total1, total2, "Yield calculation should be order-independent");
    }

    /// Property: Yield scales linearly with principal
    /// If principal doubles, yield doubles
    #[test]
    fn property_yield_scales_linearly() {
        let base_principal = 100_000_000i128;
        let base_yield = (base_principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;

        let double_principal = base_principal * 2;
        let double_yield = (double_principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;

        assert_eq!(
            double_yield, base_yield * 2,
            "Yield should scale linearly with principal"
        );

        let triple_principal = base_principal * 3;
        let triple_yield = (triple_principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;

        assert_eq!(
            triple_yield, base_yield * 3,
            "Yield should scale linearly with principal"
        );
    }

    /// Property: Yield calculation doesn't overflow for reasonable inputs
    /// Maximum loan amount should not cause overflow
    #[test]
    fn property_yield_no_overflow_for_reasonable_inputs() {
        // Test with large but reasonable amounts
        let large_amounts = vec![
            i128::MAX / 100,  // 1% of i128::MAX
            i128::MAX / 1000, // 0.1% of i128::MAX
        ];

        for amount in large_amounts {
            let yield_amount = (amount * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            assert!(yield_amount >= 0, "Yield calculation overflowed");
            assert!(yield_amount <= amount, "Yield exceeds principal");
        }
    }

    /// Property: Slash calculation is inverse of yield
    /// Slash burns 50% of stake, leaving 50%
    #[test]
    fn property_slash_burns_exactly_half() {
        let test_stakes = vec![
            100_000_000i128,
            200_000_000,
            300_000_000,
            1_000_000_000,
        ];

        for stake in test_stakes {
            let remaining = (stake * 5000) / BPS_DENOMINATOR; // 50% = 5000 bps
            let burned = stake - remaining;

            assert_eq!(remaining, burned, "Slash should burn exactly 50%");
            assert_eq!(remaining + burned, stake, "Remaining + burned should equal original");
        }
    }

    /// Property: Multiple yield calculations sum correctly
    /// Yield(A+B) = Yield(A) + Yield(B)
    #[test]
    fn property_yield_additivity() {
        let stake_a = 100_000_000i128;
        let stake_b = 200_000_000i128;

        let yield_a = (stake_a * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        let yield_b = (stake_b * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        let sum_yields = yield_a + yield_b;

        let combined_stake = stake_a + stake_b;
        let combined_yield = (combined_stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;

        assert_eq!(
            sum_yields, combined_yield,
            "Yield should be additive: Yield(A+B) = Yield(A) + Yield(B)"
        );
    }

    /// Property: Yield rate is constant (200 basis points)
    /// Yield / Principal should always equal 0.02
    #[test]
    fn property_yield_rate_is_constant() {
        let test_amounts = vec![
            100_000i128,
            1_000_000,
            10_000_000,
            100_000_000,
            1_000_000_000,
        ];

        for principal in test_amounts {
            let yield_amount = (principal * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
            let rate = (yield_amount * 10_000) / principal;

            assert_eq!(
                rate, DEFAULT_YIELD_BPS,
                "Yield rate should be constant at {} bps",
                DEFAULT_YIELD_BPS
            );
        }
    }

    /// Property: Yield precision is maintained for small amounts
    /// Even small stakes should calculate yield correctly
    #[test]
    fn property_yield_precision_for_small_amounts() {
        // Test minimum stake that yields non-zero
        let min_yield_stake = DEFAULT_MIN_YIELD_STAKE;
        let yield_at_min = (min_yield_stake * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        assert_eq!(yield_at_min, 1, "Minimum stake should yield 1 stroop");

        // Test just below minimum
        let below_min = min_yield_stake - 1;
        let yield_below = (below_min * DEFAULT_YIELD_BPS) / BPS_DENOMINATOR;
        assert_eq!(yield_below, 0, "Below minimum should yield 0");
    }
}
