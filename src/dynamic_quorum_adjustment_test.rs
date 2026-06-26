#[cfg(test)]
mod dynamic_quorum_adjustment_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    #[derive(Clone)]
    struct HealthMetrics {
        repayment_rate: u32,       // percentage (0-10000 = 0-100%)
        default_count: u32,
        active_loans: u32,
        total_vouchers: u32,
    }

    impl HealthMetrics {
        fn calculate_adjusted_quorum(&self, base_quorum_bps: u32) -> u32 {
            // Adjust quorum based on health metrics
            let repayment_factor = self.repayment_rate;
            let default_penalty = (self.default_count * 500).min(3000); // up to -30%

            let mut adjusted = base_quorum_bps;

            // Decrease quorum if repayment rate is high
            if repayment_factor > 8000 {
                adjusted = (adjusted * 9000) / 10000; // 10% reduction
            }

            // Increase quorum if defaults are high
            adjusted = adjusted.saturating_add(default_penalty);

            // Clamp between 30% and 80%
            adjusted.max(3000).min(8000)
        }
    }

    fn setup_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    #[test]
    fn test_dynamic_quorum_high_repayment_rate() {
        let _env = setup_env();
        let base_quorum = 5000; // 50%

        let metrics = HealthMetrics {
            repayment_rate: 9500, // 95% repayment
            default_count: 0,
            active_loans: 50,
            total_vouchers: 100,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        assert!(adjusted < base_quorum);
        assert_eq!(adjusted, 4500); // 10% reduction
    }

    #[test]
    fn test_dynamic_quorum_high_default_count() {
        let _env = setup_env();
        let base_quorum = 5000; // 50%

        let metrics = HealthMetrics {
            repayment_rate: 7000,
            default_count: 10,
            active_loans: 50,
            total_vouchers: 100,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        assert!(adjusted > base_quorum);
    }

    #[test]
    fn test_dynamic_quorum_healthy_ecosystem() {
        let _env = setup_env();
        let base_quorum = 5000;

        let metrics = HealthMetrics {
            repayment_rate: 9000, // 90% repayment
            default_count: 1,
            active_loans: 100,
            total_vouchers: 200,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        // Should be reduced due to high repayment rate, then minimal penalty
        assert!(adjusted <= base_quorum);
    }

    #[test]
    fn test_dynamic_quorum_stressed_ecosystem() {
        let _env = setup_env();
        let base_quorum = 5000;

        let metrics = HealthMetrics {
            repayment_rate: 5000, // 50% repayment
            default_count: 20,
            active_loans: 30,
            total_vouchers: 100,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        // Should increase due to high defaults
        assert!(adjusted > base_quorum);
    }

    #[test]
    fn test_dynamic_quorum_respects_minimum_floor() {
        let _env = setup_env();
        let base_quorum = 5000;

        let metrics = HealthMetrics {
            repayment_rate: 10000, // 100% repayment
            default_count: 0,
            active_loans: 1000,
            total_vouchers: 10000,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        assert!(adjusted >= 3000); // 30% minimum
    }

    #[test]
    fn test_dynamic_quorum_respects_maximum_ceiling() {
        let _env = setup_env();
        let base_quorum = 5000;

        let metrics = HealthMetrics {
            repayment_rate: 1000, // 10% repayment
            default_count: 100,
            active_loans: 10,
            total_vouchers: 50,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        assert!(adjusted <= 8000); // 80% maximum
    }

    #[test]
    fn test_dynamic_quorum_zero_defaults() {
        let _env = setup_env();
        let base_quorum = 5000;

        let metrics = HealthMetrics {
            repayment_rate: 8000,
            default_count: 0,
            active_loans: 100,
            total_vouchers: 200,
        };

        let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
        assert_eq!(adjusted, 4500);
    }

    #[test]
    fn test_dynamic_quorum_multiple_defaults_cap() {
        let _env = setup_env();
        let base_quorum = 5000;

        let metrics1 = HealthMetrics {
            repayment_rate: 5000,
            default_count: 5,
            active_loans: 50,
            total_vouchers: 100,
        };

        let metrics2 = HealthMetrics {
            repayment_rate: 5000,
            default_count: 10,
            active_loans: 50,
            total_vouchers: 100,
        };

        let adjusted1 = metrics1.calculate_adjusted_quorum(base_quorum);
        let adjusted2 = metrics2.calculate_adjusted_quorum(base_quorum);

        assert!(adjusted2 > adjusted1);
        assert!(adjusted2 <= 8000); // Still respects cap
    }

    #[test]
    fn test_dynamic_quorum_progressive_adjustment() {
        let _env = setup_env();
        let base_quorum = 5000;

        let mut previous_quorum = base_quorum;
        for default_count in 1..=7 {
            let metrics = HealthMetrics {
                repayment_rate: 5000,
                default_count,
                active_loans: 50,
                total_vouchers: 100,
            };

            let adjusted = metrics.calculate_adjusted_quorum(base_quorum);
            assert!(adjusted > previous_quorum || adjusted == 8000);
            previous_quorum = adjusted;
        }
    }

    #[test]
    fn test_dynamic_quorum_combined_factors() {
        let _env = setup_env();
        let base_quorum = 5000;

        // Low repayment + high defaults = high quorum
        let stressed = HealthMetrics {
            repayment_rate: 3000,
            default_count: 15,
            active_loans: 20,
            total_vouchers: 100,
        };

        // High repayment + low defaults = low quorum
        let healthy = HealthMetrics {
            repayment_rate: 9500,
            default_count: 0,
            active_loans: 200,
            total_vouchers: 300,
        };

        let stressed_quorum = stressed.calculate_adjusted_quorum(base_quorum);
        let healthy_quorum = healthy.calculate_adjusted_quorum(base_quorum);

        assert!(stressed_quorum > healthy_quorum);
    }

    #[test]
    fn test_dynamic_quorum_extreme_repayment_rate() {
        let _env = setup_env();
        let base_quorum = 5000;

        let excellent = HealthMetrics {
            repayment_rate: 10000, // 100%
            default_count: 0,
            active_loans: 500,
            total_vouchers: 1000,
        };

        let adjusted = excellent.calculate_adjusted_quorum(base_quorum);
        assert_eq!(adjusted, 4500);
    }

    #[test]
    fn test_dynamic_quorum_initial_ecosystem() {
        let _env = setup_env();
        let base_quorum = 5000;

        let initial = HealthMetrics {
            repayment_rate: 5000, // Unknown/neutral
            default_count: 0,
            active_loans: 1,
            total_vouchers: 10,
        };

        let adjusted = initial.calculate_adjusted_quorum(base_quorum);
        // Should remain close to base without strong signals
        assert!(adjusted >= 4500 && adjusted <= 5500);
    }
}
