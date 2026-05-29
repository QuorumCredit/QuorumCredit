#[cfg(test)]
mod tests {
    use crate::benchmarks::*;

    #[test]
    fn test_performance_targets_exist() {
        let targets = PerformanceTargets::default();
        assert!(targets.vouch_max_gas > 0);
        assert!(targets.request_loan_max_gas > 0);
        assert!(targets.repay_max_gas > 0);
        assert!(targets.slash_max_gas > 0);
    }

    #[test]
    fn test_check_performance_target_vouch() {
        let targets = PerformanceTargets::default();
        let result = BenchmarkResult {
            operation: "vouch",
            gas_used: 40_000,
            execution_time_ms: 10,
        };
        assert!(check_performance_target(&result, &targets));
    }

    #[test]
    fn test_check_performance_target_exceeds() {
        let targets = PerformanceTargets::default();
        let result = BenchmarkResult {
            operation: "vouch",
            gas_used: 60_000,
            execution_time_ms: 15,
        };
        assert!(!check_performance_target(&result, &targets));
    }

    #[test]
    fn test_check_performance_target_request_loan() {
        let targets = PerformanceTargets::default();
        let result = BenchmarkResult {
            operation: "request_loan",
            gas_used: 90_000,
            execution_time_ms: 20,
        };
        assert!(check_performance_target(&result, &targets));
    }

    #[test]
    fn test_check_performance_target_repay() {
        let targets = PerformanceTargets::default();
        let result = BenchmarkResult {
            operation: "repay",
            gas_used: 75_000,
            execution_time_ms: 15,
        };
        assert!(check_performance_target(&result, &targets));
    }

    #[test]
    fn test_check_performance_target_slash() {
        let targets = PerformanceTargets::default();
        let result = BenchmarkResult {
            operation: "slash",
            gas_used: 55_000,
            execution_time_ms: 12,
        };
        assert!(check_performance_target(&result, &targets));
    }
}
