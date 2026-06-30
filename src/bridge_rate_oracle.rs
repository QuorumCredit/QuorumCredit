//! Decentralized bridge rate oracle (#973/#90).
//!
//! Cross-chain settlement needs an exchange rate that no single party can
//! dictate. Rather than trust one feed, a set of admin-registered oracle
//! operators each push their own observation of a rate feed. A read only
//! succeeds once a quorum of *fresh* submissions exists, and the value returned
//! is their median — so a minority of stale or manipulated feeds cannot move
//! the aggregate.

use crate::{helpers, ContractError};
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Submissions older than this are ignored when aggregating a rate.
pub const MAX_RATE_AGE_SECS: u64 = 60 * 60;
/// Minimum number of fresh submissions required before a rate is readable.
pub const DEFAULT_RATE_QUORUM: u32 = 3;

/// One operator's observation of a feed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateSubmission {
    pub operator: Address,
    pub rate: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum RateOracleKey {
    /// The set of addresses allowed to submit rates.
    Operators,
    /// Minimum fresh submissions required to read a feed.
    Quorum,
    /// Operators that have submitted to a feed (per feed id).
    Submitters(u32),
    /// A single operator's submission for a feed.
    Submission(u32, Address),
}

fn operators(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&RateOracleKey::Operators)
        .unwrap_or_else(|| Vec::new(env))
}

/// Required quorum, falling back to [`DEFAULT_RATE_QUORUM`] when unset.
pub fn rate_quorum(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&RateOracleKey::Quorum)
        .unwrap_or(DEFAULT_RATE_QUORUM)
}

/// Register an address as an authorized rate oracle operator.
pub fn register_rate_oracle(
    env: Env,
    admin_signers: Vec<Address>,
    operator: Address,
) -> Result<(), ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);
    let mut current = operators(&env);
    if current.iter().any(|a| a == operator) {
        return Err(ContractError::RateOracleAlreadyRegistered);
    }
    current.push_back(operator.clone());
    env.storage()
        .persistent()
        .set(&RateOracleKey::Operators, &current);
    env.events().publish(
        (
            soroban_sdk::symbol_short!("rate_orc"),
            soroban_sdk::symbol_short!("register"),
        ),
        operator,
    );
    Ok(())
}

/// Remove an address from the set of authorized rate oracle operators.
pub fn remove_rate_oracle(
    env: Env,
    admin_signers: Vec<Address>,
    operator: Address,
) -> Result<(), ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);
    let current = operators(&env);
    let mut next = Vec::new(&env);
    let mut found = false;
    for existing in current.iter() {
        if existing == operator {
            found = true;
        } else {
            next.push_back(existing);
        }
    }
    if !found {
        return Err(ContractError::RateOracleNotFound);
    }
    env.storage()
        .persistent()
        .set(&RateOracleKey::Operators, &next);
    env.events().publish(
        (
            soroban_sdk::symbol_short!("rate_orc"),
            soroban_sdk::symbol_short!("remove"),
        ),
        operator,
    );
    Ok(())
}

/// Set the minimum number of fresh submissions required to read a rate.
pub fn set_rate_quorum(
    env: Env,
    admin_signers: Vec<Address>,
    quorum: u32,
) -> Result<(), ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);
    if quorum == 0 {
        return Err(ContractError::RateQuorumNotMet);
    }
    env.storage()
        .persistent()
        .set(&RateOracleKey::Quorum, &quorum);
    Ok(())
}

pub fn is_rate_oracle(env: Env, operator: Address) -> bool {
    operators(&env).iter().any(|a| a == operator)
}

/// Submit (or overwrite) this operator's observation for a feed.
pub fn submit_bridge_rate(
    env: Env,
    operator: Address,
    feed_id: u32,
    rate: i128,
) -> Result<(), ContractError> {
    operator.require_auth();
    if !operators(&env).iter().any(|a| a == operator) {
        return Err(ContractError::RateOracleUnauthorized);
    }
    if rate <= 0 {
        return Err(ContractError::InvalidRateValue);
    }

    let submission = RateSubmission {
        operator: operator.clone(),
        rate,
        timestamp: env.ledger().timestamp(),
    };
    env.storage().persistent().set(
        &RateOracleKey::Submission(feed_id, operator.clone()),
        &submission,
    );

    let submitters_key = RateOracleKey::Submitters(feed_id);
    let mut submitters: Vec<Address> = env
        .storage()
        .persistent()
        .get(&submitters_key)
        .unwrap_or_else(|| Vec::new(&env));
    if !submitters.iter().any(|a| a == operator) {
        submitters.push_back(operator.clone());
        env.storage().persistent().set(&submitters_key, &submitters);
    }

    env.events().publish(
        (
            soroban_sdk::symbol_short!("rate_orc"),
            soroban_sdk::symbol_short!("submit"),
        ),
        (feed_id, operator, rate),
    );
    Ok(())
}

pub fn query_rate_submission(
    env: Env,
    feed_id: u32,
    operator: Address,
) -> Option<RateSubmission> {
    env.storage()
        .persistent()
        .get(&RateOracleKey::Submission(feed_id, operator))
}

/// Collect the fresh, still-authorized submissions for a feed.
fn fresh_rates(env: &Env, feed_id: u32) -> Vec<i128> {
    let now = env.ledger().timestamp();
    let authorized = operators(env);
    let submitters: Vec<Address> = env
        .storage()
        .persistent()
        .get(&RateOracleKey::Submitters(feed_id))
        .unwrap_or_else(|| Vec::new(env));

    let mut rates = Vec::new(env);
    for operator in submitters.iter() {
        // Drop operators that were de-registered after submitting.
        if !authorized.iter().any(|a| a == operator) {
            continue;
        }
        if let Some(submission) = env
            .storage()
            .persistent()
            .get::<_, RateSubmission>(&RateOracleKey::Submission(feed_id, operator))
        {
            if now.saturating_sub(submission.timestamp) <= MAX_RATE_AGE_SECS {
                rates.push_back(submission.rate);
            }
        }
    }
    rates
}

/// Ascending insertion sort. Operator sets are tiny, so O(n^2) is fine and
/// avoids pulling the standard library into the contract build.
fn sorted_ascending(env: &Env, values: &Vec<i128>) -> Vec<i128> {
    let mut sorted = Vec::new(env);
    for value in values.iter() {
        let mut index = 0u32;
        while index < sorted.len() && sorted.get(index).unwrap() <= value {
            index += 1;
        }
        sorted.insert(index, value);
    }
    sorted
}

/// Return the decentralized median rate for a feed, or an error if fewer than
/// `rate_quorum` fresh submissions exist.
pub fn get_bridge_rate(env: Env, feed_id: u32) -> Result<i128, ContractError> {
    let rates = fresh_rates(&env, feed_id);
    let count = rates.len();
    if count < rate_quorum(&env) {
        return Err(ContractError::RateQuorumNotMet);
    }
    let sorted = sorted_ascending(&env, &rates);
    let mid = count / 2;
    let median = if count % 2 == 1 {
        sorted.get(mid).unwrap()
    } else {
        // Average the two central values without overflowing i128.
        let low = sorted.get(mid - 1).unwrap();
        let high = sorted.get(mid).unwrap();
        low + (high - low) / 2
    };
    Ok(median)
}

/// Number of fresh submissions currently backing a feed.
pub fn fresh_submission_count(env: Env, feed_id: u32) -> u32 {
    fresh_rates(&env, feed_id).len()
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        contract,
        testutils::{Address as _, Ledger},
        Address,
    };

    #[contract]
    struct StorageContract;

    struct Fixture {
        env: Env,
        contract: Address,
        admins: Vec<Address>,
    }

    impl Fixture {
        fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().set_timestamp(100_000);
            let contract = env.register(StorageContract, ());
            let admin = Address::generate(&env);
            let admins = Vec::from_array(&env, [admin.clone()]);
            env.as_contract(&contract, || {
                let cfg = crate::types::Config {
                    admins: admins.clone(),
                    admin_threshold: 1,
                    admin_whitelist: Vec::new(&env),
                    admin_blacklist: Vec::new(&env),
                    token: Address::generate(&env),
                    allowed_tokens: Vec::new(&env),
                    yield_bps: 200,
                    slash_bps: 5000,
                    max_vouchers: 50,
                    min_loan_amount: 100_000,
                    loan_duration: 86_400,
                    max_loan_to_stake_ratio: 100,
                    grace_period: 3_600,
                    min_vouch_age_secs: 0,
                    prepayment_penalty_bps: 0,
                    liquidity_mining_rate_bps: 0,
                    voting_period_seconds: 3_600,
                    slash_cooldown_seconds: 0,
                    emergency_pause_enabled: false,
                    early_repayment_discount_bps: 0,
                    oracle_address: None,
                    slash_delay_seconds: 0,
                    successor_admin: None,
                    rate_limit_config: crate::types::RateLimitConfig {
                        window_secs: 60,
                        max_calls: 100,
                        enabled: false,
                    },
                    multi_tier_thresholds: None,
                };
                env.storage()
                    .instance()
                    .set(&crate::types::DataKey::Config, &cfg);
            });
            Self {
                env,
                contract,
                admins,
            }
        }

        fn register(&self, operator: &Address) -> Result<(), ContractError> {
            self.env.as_contract(&self.contract, || {
                register_rate_oracle(self.env.clone(), self.admins.clone(), operator.clone())
            })
        }

        fn submit(&self, operator: &Address, feed: u32, rate: i128) -> Result<(), ContractError> {
            self.env.as_contract(&self.contract, || {
                submit_bridge_rate(self.env.clone(), operator.clone(), feed, rate)
            })
        }

        fn get(&self, feed: u32) -> Result<i128, ContractError> {
            self.env
                .as_contract(&self.contract, || get_bridge_rate(self.env.clone(), feed))
        }

        fn registered_operators(&self, n: usize) -> std::vec::Vec<Address> {
            let mut ops = std::vec::Vec::new();
            for _ in 0..n {
                let op = Address::generate(&self.env);
                self.register(&op).unwrap();
                ops.push(op);
            }
            ops
        }
    }

    #[test]
    fn quorum_of_submissions_yields_median() {
        let f = Fixture::new();
        let ops = f.registered_operators(3);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 110).unwrap();
        f.submit(&ops[2], 1, 130).unwrap();
        assert_eq!(f.get(1), Ok(110));
    }

    #[test]
    fn even_count_averages_central_pair() {
        let f = Fixture::new();
        let ops = f.registered_operators(4);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 120).unwrap();
        f.submit(&ops[2], 1, 140).unwrap();
        f.submit(&ops[3], 1, 160).unwrap();
        // median of 120 and 140
        assert_eq!(f.get(1), Ok(130));
    }

    #[test]
    fn below_quorum_is_rejected() {
        let f = Fixture::new();
        let ops = f.registered_operators(3);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 110).unwrap();
        assert_eq!(f.get(1), Err(ContractError::RateQuorumNotMet));
    }

    #[test]
    fn unregistered_operator_cannot_submit() {
        let f = Fixture::new();
        let stranger = Address::generate(&f.env);
        assert_eq!(
            f.submit(&stranger, 1, 100),
            Err(ContractError::RateOracleUnauthorized)
        );
    }

    #[test]
    fn non_positive_rate_is_rejected() {
        let f = Fixture::new();
        let ops = f.registered_operators(1);
        assert_eq!(f.submit(&ops[0], 1, 0), Err(ContractError::InvalidRateValue));
        assert_eq!(f.submit(&ops[0], 1, -5), Err(ContractError::InvalidRateValue));
    }

    #[test]
    fn stale_submissions_are_excluded() {
        let f = Fixture::new();
        let ops = f.registered_operators(3);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 110).unwrap();
        // Advance past the freshness window, then add a third fresh rate.
        f.env
            .ledger()
            .set_timestamp(100_000 + MAX_RATE_AGE_SECS + 1);
        f.submit(&ops[2], 1, 130).unwrap();
        // Only one fresh submission remains.
        assert_eq!(f.get(1), Err(ContractError::RateQuorumNotMet));
    }

    #[test]
    fn resubmission_overwrites_previous_value() {
        let f = Fixture::new();
        let ops = f.registered_operators(3);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 110).unwrap();
        f.submit(&ops[2], 1, 130).unwrap();
        // Operator 0 revises upward; median shifts.
        f.submit(&ops[0], 1, 200).unwrap();
        assert_eq!(f.get(1), Ok(130));
        assert_eq!(
            f.env.as_contract(&f.contract, || fresh_submission_count(
                f.env.clone(),
                1
            )),
            3
        );
    }

    #[test]
    fn feeds_are_independent() {
        let f = Fixture::new();
        let ops = f.registered_operators(3);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 100).unwrap();
        f.submit(&ops[2], 1, 100).unwrap();
        assert_eq!(f.get(2), Err(ContractError::RateQuorumNotMet));
        assert_eq!(f.get(1), Ok(100));
    }

    #[test]
    fn deregistered_operator_drops_out_of_aggregate() {
        let f = Fixture::new();
        let ops = f.registered_operators(3);
        f.submit(&ops[0], 1, 100).unwrap();
        f.submit(&ops[1], 1, 110).unwrap();
        f.submit(&ops[2], 1, 130).unwrap();
        f.env
            .as_contract(&f.contract, || {
                remove_rate_oracle(f.env.clone(), f.admins.clone(), ops[2].clone())
            })
            .unwrap();
        assert_eq!(f.get(1), Err(ContractError::RateQuorumNotMet));
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let f = Fixture::new();
        let op = Address::generate(&f.env);
        f.register(&op).unwrap();
        assert_eq!(
            f.register(&op),
            Err(ContractError::RateOracleAlreadyRegistered)
        );
    }

    #[test]
    fn removing_unknown_operator_is_rejected() {
        let f = Fixture::new();
        let op = Address::generate(&f.env);
        assert_eq!(
            f.env.as_contract(&f.contract, || remove_rate_oracle(
                f.env.clone(),
                f.admins.clone(),
                op
            )),
            Err(ContractError::RateOracleNotFound)
        );
    }
}
