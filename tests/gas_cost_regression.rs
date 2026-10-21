mod errors {
    #[derive(Debug, Eq, PartialEq)]
    pub enum ContractError {
        InvalidInput,
        NotFound,
        RegressionDetected,
    }
}

mod types {
    use soroban_sdk::{contracttype, Symbol};

    #[contracttype]
    pub enum DataKey {
        Custom(Symbol),
    }
}

#[path = "../src/gas_cost_regression.rs"]
mod gas_cost_regression;

use gas_cost_regression::{
    get_gas_baseline, measure_gas_cost, set_gas_baseline, REGRESSION_THRESHOLD_BPS,
};
use soroban_sdk::{Env, String};

const BASELINE_GAS: u64 = 10_000;

#[test]
fn accepts_gas_cost_at_regression_threshold() {
    let env = Env::default();
    let operation = String::from_str(&env, "loan_issue");
    let threshold_increase = BASELINE_GAS * REGRESSION_THRESHOLD_BPS as u64 / 10_000;

    assert_eq!(
        REGRESSION_THRESHOLD_BPS, 1_000,
        "the regression limit should remain at 10%"
    );
    set_gas_baseline(&env, operation.clone(), BASELINE_GAS)
        .expect("baseline setup should succeed");

    measure_gas_cost(
        &env,
        operation,
        BASELINE_GAS + threshold_increase,
        String::from_str(&env, "test-build"),
    )
    .expect("gas cost exactly at the threshold should not trigger a regression");
}

#[test]
fn rejects_gas_cost_above_regression_threshold() {
    let env = Env::default();
    let operation = String::from_str(&env, "loan_issue");
    let threshold_increase = BASELINE_GAS * REGRESSION_THRESHOLD_BPS as u64 / 10_000;
    set_gas_baseline(&env, operation.clone(), BASELINE_GAS)
        .expect("baseline setup should succeed");

    let result = measure_gas_cost(
        &env,
        operation.clone(),
        BASELINE_GAS + threshold_increase + 1,
        String::from_str(&env, "test-build"),
    );

    assert!(
        matches!(result, Err(errors::ContractError::RegressionDetected)),
        "gas cost above the threshold should return RegressionDetected"
    );
    let baseline = get_gas_baseline(&env, &operation)
        .expect("the regression should preserve the operation baseline");
    assert!(baseline.regression_detected, "the baseline should record the regression");
    assert_eq!(
        baseline.regression_percentage_bps,
        REGRESSION_THRESHOLD_BPS + 1,
        "the baseline should record the measured regression percentage"
    );
}
