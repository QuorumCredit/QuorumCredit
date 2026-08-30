/// Economic Model Simulation Testing Module (Issue #1184)
/// This module implements Monte Carlo simulation for loan portfolio analysis,
/// including stress testing under various economic scenarios.

use crate::errors::ContractError;
use crate::types::{LoanRecord, LoanStatus};
use soroban_sdk::{contracttype, xdr::ToXdr, BytesN, Env, Vec};

/// Default number of Monte Carlo simulations
pub const DEFAULT_SIMULATION_COUNT: u32 = 10_000;

/// Risk metrics thresholds, in basis points (0-10000). Soroban's WASM target
/// is deterministic but does not guarantee bit-identical floating-point
/// results across host implementations, so on-chain-reachable modules use
/// fixed-point integers throughout rather than `f64`/`f32`.
pub const VAR_CONFIDENCE_LEVEL_BPS: u32 = 9_500; // 95% confidence level
pub const CVAR_CONFIDENCE_LEVEL_BPS: u32 = 9_500; // Expected Shortfall at 95% confidence

/// Simulation parameters for Monte Carlo analysis
#[derive(Clone, Debug)]
#[contracttype]
pub struct SimulationParams {
    /// Default rate (probability of default) in basis points (0-10000)
    pub default_rate_bps: u32,
    /// Interest rate for loans in basis points (0-10000)
    pub interest_rate_bps: u32,
    /// Recovery rate (percentage of defaulted loan recovered) in basis points (0-10000)
    pub recovery_rate_bps: u32,
    /// Number of simulations to run
    pub simulation_count: u32,
    /// Initial portfolio value
    pub portfolio_value: i128,
}

/// Results of a single Monte Carlo simulation
#[derive(Clone, Debug)]
#[contracttype]
pub struct SimulationResult {
    /// Portfolio value at end of simulation period
    pub end_value: i128,
    /// Total interest collected
    pub interest_collected: i128,
    /// Total losses from defaults
    pub default_losses: i128,
    /// Number of defaulted loans
    pub defaults_count: u32,
}

/// Summary statistics from Monte Carlo simulations
#[derive(Clone, Debug)]
#[contracttype]
pub struct PortfolioStressTestResult {
    /// Value at Risk at 95% confidence
    pub var_95: i128,
    /// Expected Shortfall (CVaR) at 95% confidence
    pub cvar_95: i128,
    /// Mean portfolio value
    pub mean_value: i128,
    /// Minimum portfolio value observed
    pub min_value: i128,
    /// Maximum portfolio value observed
    pub max_value: i128,
    /// Standard deviation of outcomes
    pub std_dev: i128,
    /// Probability of portfolio loss
    pub loss_probability: u32, // in basis points (0-10000)
    /// Maximum loss scenario
    pub max_loss: i128,
    /// Simulation count used
    pub simulation_count: u32,
}

/// Pseudo-random number generator using linear congruential method.
/// Deterministic for a given seed, which is necessary but not sufficient
/// for safe use: the seed itself must come from a source the party who
/// benefits from the simulation's outcome cannot choose or predict ahead
/// of time (see [`derive_seed`]). `SimpleRng::new` is `pub` for direct use
/// in tests only; on-chain code should go through [`run_monte_carlo_simulation`],
/// which derives its seed internally rather than accepting one.
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    /// Initialize RNG with a seed
    pub fn new(seed: u64) -> Self {
        SimpleRng {
            state: seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407),
        }
    }

    /// Generate next random number between 0 and 1 (scaled to u32)
    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    /// Generate random number between 0.0 and 1.0 (approximated)
    pub fn next_f64(&mut self) -> u64 {
        self.next_u32() as u64
    }
}

/// Derive a Monte Carlo seed from ledger state and the simulation's own
/// parameters, rather than accepting one from the caller.
///
/// This closes the seed-grinding exposure described in issue #1476: a
/// caller cannot choose which ledger sequence its invocation executes in,
/// so it has no lever to search for a seed that shifts `var_95`/`cvar_95`
/// in its favor within a single call. Identical params on the same ledger
/// always derive the same seed, so results stay reproducible.
fn derive_seed(env: &Env, params: &SimulationParams) -> u64 {
    let ledger_seq = env.ledger().sequence();
    let payload = (
        params.default_rate_bps,
        params.interest_rate_bps,
        params.recovery_rate_bps,
        params.simulation_count,
        params.portfolio_value,
        ledger_seq,
    );
    let encoded = payload.to_xdr(env);
    let hash: BytesN<32> = env.crypto().sha256(&encoded).into();
    let bytes = hash.to_array();
    u64::from_be_bytes(bytes[0..8].try_into().unwrap())
}

/// Run Monte Carlo simulation for portfolio stress testing.
///
/// The seed is derived internally from ledger state (see [`derive_seed`])
/// rather than taken as a parameter, so no caller — on-chain or off — can
/// supply or bias the randomness behind the reported risk metrics.
pub fn run_monte_carlo_simulation(
    env: &Env,
    params: &SimulationParams,
) -> Result<PortfolioStressTestResult, ContractError> {
    if params.simulation_count == 0 {
        return Err(ContractError::InvalidInput);
    }

    let mut results: Vec<i128> = Vec::new(env);
    let mut losses_count = 0u32;
    let mut total_interest = 0i128;
    let mut total_defaults = 0i128;

    let mut rng = SimpleRng::new(derive_seed(env, params));

    // Run simulations
    for _ in 0..params.simulation_count {
        let sim_result = simulate_single_period(params, &mut rng);

        results.push_back(sim_result.end_value);
        total_interest = total_interest.saturating_add(sim_result.interest_collected);
        total_defaults = total_defaults.saturating_add(sim_result.default_losses);

        if sim_result.end_value < params.portfolio_value {
            losses_count = losses_count.saturating_add(1);
        }
    }

    // Calculate statistics
    calculate_portfolio_metrics(&results, params, total_interest, total_defaults, losses_count)
}

/// Simulate a single period for the portfolio
fn simulate_single_period(
    params: &SimulationParams,
    rng: &mut SimpleRng,
) -> SimulationResult {
    let default_threshold = (params.default_rate_bps as u64 * 1_000_000) / 10_000;

    let mut end_value = params.portfolio_value;
    let mut interest_collected = 0i128;
    let mut default_losses = 0i128;
    let mut defaults = 0u32;

    // Simulate ~100 loans per portfolio for reasonable distribution
    let loan_count = 100u32;
    let loan_value = params.portfolio_value / (loan_count as i128);

    for _ in 0..loan_count {
        let random_val = rng.next_f64();

        if random_val < default_threshold {
            // Loan defaults
            defaults = defaults.saturating_add(1);
            let recovery = (loan_value as u128)
                .saturating_mul(params.recovery_rate_bps as u128)
                / 10_000;
            let loss = loan_value.saturating_sub(recovery as i128);
            default_losses = default_losses.saturating_add(loss);
            end_value = end_value.saturating_sub(loss);
        } else {
            // Loan pays interest
            let interest = (loan_value as u128)
                .saturating_mul(params.interest_rate_bps as u128)
                / 10_000;
            interest_collected = interest_collected.saturating_add(interest as i128);
            end_value = end_value.saturating_add(interest as i128);
        }
    }

    SimulationResult {
        end_value,
        interest_collected,
        default_losses,
        defaults_count: defaults,
    }
}

/// Calculate risk metrics from simulation results
fn calculate_portfolio_metrics(
    results: &Vec<i128>,
    params: &SimulationParams,
    total_interest: i128,
    total_defaults: i128,
    losses_count: u32,
) -> Result<PortfolioStressTestResult, ContractError> {
    if results.is_empty() {
        return Err(ContractError::InvalidInput);
    }

    let mut sorted_results = results.clone();
    // Simple bubble sort for small arrays
    for i in 0..sorted_results.len() {
        for j in i + 1..sorted_results.len() {
            if sorted_results.get(i).unwrap() > sorted_results.get(j).unwrap() {
                let temp = sorted_results.get(i).unwrap();
                sorted_results.set(i, sorted_results.get(j).unwrap());
                sorted_results.set(j, temp);
            }
        }
    }

    let mean_value = results.iter().fold(0i128, |acc, val| {
        acc.saturating_add(val)
    }) / results.len() as i128;

    // Calculate VaR (Value at Risk) at 95% confidence, using fixed-point
    // basis-point arithmetic throughout (no f64 — see module docs).
    let var_index = (results.len() as u64 * (10_000 - VAR_CONFIDENCE_LEVEL_BPS) as u64 / 10_000) as usize;
    let var_95 = sorted_results.get(var_index).unwrap_or(sorted_results.get(0).unwrap());

    // Calculate CVaR (Conditional Value at Risk / Expected Shortfall)
    let cvar_index = (results.len() as u64 * (10_000 - CVAR_CONFIDENCE_LEVEL_BPS) as u64 / 10_000) as usize;
    let cvar_95 = sorted_results
        .iter()
        .take(cvar_index + 1)
        .fold(0i128, |acc, val| acc.saturating_add(val))
        / (cvar_index as i128 + 1);

    let min_value = sorted_results.get(0).unwrap();
    let max_value = sorted_results.get(sorted_results.len() - 1).unwrap();

    // Calculate standard deviation
    let variance = results
        .iter()
        .fold(0i128, |acc, val| {
            let diff = val - mean_value;
            acc.saturating_add(diff.saturating_mul(diff))
        })
        / results.len() as i128;

    let std_dev = if variance > 0 {
        // Approximate square root using integer arithmetic
        integer_sqrt(variance as u128) as i128
    } else {
        0
    };

    let loss_probability = (losses_count as u128 * 10_000 / params.simulation_count as u128) as u32;
    let max_loss = min_value.saturating_sub(params.portfolio_value);

    Ok(PortfolioStressTestResult {
        var_95,
        cvar_95,
        mean_value,
        min_value,
        max_value,
        std_dev,
        loss_probability,
        max_loss,
        simulation_count: params.simulation_count,
    })
}

/// Integer square root using binary search
fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }

    let mut x0 = n;
    let mut x1 = (x0 + 1) / 2;

    while x1 < x0 {
        x0 = x1;
        x1 = (x0 + n / x0) / 2;
    }

    x0
}

/// Stress test the portfolio across multiple scenarios.
///
/// Each scenario derives its own seed from its own (distinct) parameters
/// via [`run_monte_carlo_simulation`] — there is no caller-supplied seed to
/// grind over here either.
pub fn stress_test_scenarios(
    env: &Env,
    base_params: &SimulationParams,
) -> Result<Vec<PortfolioStressTestResult>, ContractError> {
    let mut results = Vec::new(env);

    // Scenario 1: Base case
    let base_result = run_monte_carlo_simulation(env, base_params)?;
    results.push_back(base_result);

    // Scenario 2: High default environment (2x default rate)
    let mut high_default_params = base_params.clone();
    high_default_params.default_rate_bps = (high_default_params.default_rate_bps as u64 * 2).min(10_000) as u32;
    let high_default_result = run_monte_carlo_simulation(env, &high_default_params)?;
    results.push_back(high_default_result);

    // Scenario 3: Low interest rate environment (50% lower rates)
    let mut low_rate_params = base_params.clone();
    low_rate_params.interest_rate_bps = low_rate_params.interest_rate_bps / 2;
    let low_rate_result = run_monte_carlo_simulation(env, &low_rate_params)?;
    results.push_back(low_rate_result);

    // Scenario 4: Poor recovery (recovery rate halved)
    let mut poor_recovery_params = base_params.clone();
    poor_recovery_params.recovery_rate_bps = poor_recovery_params.recovery_rate_bps / 2;
    let poor_recovery_result = run_monte_carlo_simulation(env, &poor_recovery_params)?;
    results.push_back(poor_recovery_result);

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monte_carlo_simulation() {
        let env = Env::default();
        let params = SimulationParams {
            default_rate_bps: 500,  // 5% default rate
            interest_rate_bps: 1000, // 10% interest rate
            recovery_rate_bps: 5000, // 50% recovery rate
            simulation_count: 1000,
            portfolio_value: 1_000_000,
        };

        let result = run_monte_carlo_simulation(&env, &params).unwrap();

        // Basic sanity checks
        assert!(result.mean_value > 0);
        assert!(result.min_value <= result.mean_value);
        assert!(result.mean_value <= result.max_value);
        assert!(result.loss_probability <= 10_000);
    }

    #[test]
    fn test_stress_test_scenarios() {
        let env = Env::default();
        let params = SimulationParams {
            default_rate_bps: 300,
            interest_rate_bps: 800,
            recovery_rate_bps: 6000,
            simulation_count: 500,
            portfolio_value: 500_000,
        };

        let results = stress_test_scenarios(&env, &params).unwrap();
        assert!(results.len() == 4); // 4 scenarios
    }

    /// Issue #1476: demonstrate the seed-grinding exposure that exists when
    /// a party can choose the raw `SimpleRng` seed directly, and confirm
    /// the public API (which derives its seed from ledger state instead of
    /// accepting one) gives no such lever.
    #[test]
    fn adversarial_seed_search_demonstrates_and_closes_exposure() {
        let env = Env::default();
        let params = SimulationParams {
            default_rate_bps: 500,
            interest_rate_bps: 1000,
            recovery_rate_bps: 5000,
            simulation_count: 50,
            portfolio_value: 1_000_000,
        };

        // Exposure: given direct access to `SimpleRng`, searching over
        // caller-chosen seeds finds a wide spread of `var_95` outcomes,
        // i.e. an attacker with a raw seed parameter could pick whichever
        // seed makes the reported risk look most favorable.
        let mut min_var = i128::MAX;
        let mut max_var = i128::MIN;
        for raw_seed in 0u64..200 {
            let mut rng = SimpleRng::new(raw_seed);
            let mut results: Vec<i128> = Vec::new(&env);
            let mut total_interest = 0i128;
            let mut total_defaults = 0i128;
            let mut losses = 0u32;

            for _ in 0..params.simulation_count {
                let sim_result = simulate_single_period(&params, &mut rng);
                results.push_back(sim_result.end_value);
                total_interest = total_interest.saturating_add(sim_result.interest_collected);
                total_defaults = total_defaults.saturating_add(sim_result.default_losses);
                if sim_result.end_value < params.portfolio_value {
                    losses = losses.saturating_add(1);
                }
            }

            let metrics = calculate_portfolio_metrics(&results, &params, total_interest, total_defaults, losses)
                .unwrap();
            min_var = min_var.min(metrics.var_95);
            max_var = max_var.max(metrics.var_95);
        }
        assert!(
            max_var > min_var,
            "a raw, caller-chosen seed should be able to move var_95 -- this is the exposure issue #1476 documents"
        );

        // Fix: the public API derives its seed from ledger state rather
        // than accepting one, so identical params on the same ledger
        // always agree -- there is no seed argument left to search over.
        let a = run_monte_carlo_simulation(&env, &params).unwrap();
        let b = run_monte_carlo_simulation(&env, &params).unwrap();
        assert_eq!(
            a.var_95, b.var_95,
            "seed derivation must be deterministic per ledger state, not attacker-chosen"
        );
    }
}
