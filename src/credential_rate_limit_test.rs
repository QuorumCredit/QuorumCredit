//! Issues #1603 / #1797: tests for per-holder verification rate limiting.

use crate::credential_batch::{
    create_batch_operation, execute_batch_operation, BatchOperation,
};
use crate::credential_rate_limit::*;
use crate::errors::ContractError;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

#[contract]
pub struct RateLimitDummy;

#[contractimpl]
impl RateLimitDummy {
    pub fn noop() {}
}

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RateLimitDummy, ());
    (env, contract_id)
}

fn drain(env: &Env, holder: &Address, attempts: u32) {
    for _ in 0..attempts {
        consume_verification_attempt(env, holder).unwrap();
    }
}

#[test]
fn defaults_apply_until_a_holder_configures_a_limit() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let holder = Address::generate(&env);
        let config = get_verification_rate_limit(&env, &holder);
        assert_eq!(config.limit, DEFAULT_VERIFICATION_LIMIT);
        assert_eq!(config.window_secs, DEFAULT_VERIFICATION_WINDOW_SECS);

        drain(&env, &holder, DEFAULT_VERIFICATION_LIMIT);
        assert_eq!(
            consume_verification_attempt(&env, &holder),
            Err(ContractError::CredentialRateLimitExceeded)
        );
    });
}

#[test]
fn configured_limit_caps_attempts_and_resets_bucket() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let holder = Address::generate(&env);
        set_verification_rate_limit(&env, &holder, 3).unwrap();
        assert_eq!(get_available_attempts(&env, &holder), 3);

        assert_eq!(consume_verification_attempt(&env, &holder), Ok(2));
        assert_eq!(consume_verification_attempt(&env, &holder), Ok(1));
        assert_eq!(consume_verification_attempt(&env, &holder), Ok(0));
        assert_eq!(
            consume_verification_attempt(&env, &holder),
            Err(ContractError::CredentialRateLimitExceeded)
        );

        // Setting a limit again starts a fresh, full bucket.
        set_verification_rate_limit(&env, &holder, 3).unwrap();
        assert_eq!(get_available_attempts(&env, &holder), 3);
    });
}

#[test]
fn tokens_refill_gradually_and_fully_after_one_window() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        env.ledger().set_timestamp(10_000);
        let waiter = Address::generate(&env);
        set_verification_rate_limit(&env, &waiter, 4).unwrap();
        drain(&env, &waiter, 4);

        // Window is 3600s with capacity 4, so one token arrives every 900s.
        env.ledger().set_timestamp(10_000 + 900);
        assert_eq!(get_available_attempts(&env, &waiter), 1);
        env.ledger().set_timestamp(10_000 + 1_800);
        assert_eq!(get_available_attempts(&env, &waiter), 2);

        // A full window after the drain, the holder has the whole quota back.
        env.ledger().set_timestamp(10_000 + 3_600);
        assert_eq!(get_available_attempts(&env, &waiter), 4);
    });
}

#[test]
fn spending_a_refilled_token_moves_the_refill_clock() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        env.ledger().set_timestamp(10_000);
        let holder = Address::generate(&env);
        set_verification_rate_limit(&env, &holder, 4).unwrap();
        drain(&env, &holder, 4);

        env.ledger().set_timestamp(10_000 + 900);
        assert_eq!(consume_verification_attempt(&env, &holder), Ok(0));
        assert_eq!(
            consume_verification_attempt(&env, &holder),
            Err(ContractError::CredentialRateLimitExceeded)
        );

        // The refill clock moved to 900s when the token was spent. Measured from
        // there, 2700s earns three more tokens, not four.
        env.ledger().set_timestamp(10_000 + 3_600);
        assert_eq!(get_available_attempts(&env, &holder), 3);
    });
}

#[test]
fn fractional_refill_is_not_lost_between_checks() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        env.ledger().set_timestamp(0);
        let holder = Address::generate(&env);
        set_verification_rate_limit(&env, &holder, 4).unwrap();
        drain(&env, &holder, 4);

        // Check every 500s. The earned tokens add up over successive checks even
        // though no single interval earns a whole token by itself.
        env.ledger().set_timestamp(500);
        assert_eq!(get_available_attempts(&env, &holder), 0);
        env.ledger().set_timestamp(1_000);
        assert_eq!(get_available_attempts(&env, &holder), 1);
    });
}

#[test]
fn holders_are_limited_independently() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        set_verification_rate_limit(&env, &a, 1).unwrap();
        set_verification_rate_limit(&env, &b, 1).unwrap();
        drain(&env, &a, 1);
        assert_eq!(
            consume_verification_attempt(&env, &a),
            Err(ContractError::CredentialRateLimitExceeded)
        );
        assert_eq!(consume_verification_attempt(&env, &b), Ok(0));
    });
}

#[test]
fn invalid_limits_and_windows_are_rejected() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let holder = Address::generate(&env);
        assert_eq!(
            set_verification_rate_limit(&env, &holder, 0),
            Err(ContractError::InvalidRateLimit)
        );
        assert_eq!(
            set_verification_rate_limit(&env, &holder, MAX_VERIFICATION_LIMIT + 1),
            Err(ContractError::InvalidRateLimit)
        );
        assert_eq!(
            set_verification_window(&env, &holder, MIN_VERIFICATION_WINDOW_SECS - 1),
            Err(ContractError::InvalidRateLimitWindow)
        );
        assert_eq!(
            set_verification_window(&env, &holder, MAX_VERIFICATION_WINDOW_SECS + 1),
            Err(ContractError::InvalidRateLimitWindow)
        );
    });
}

#[test]
fn window_change_alters_refill_speed() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        env.ledger().set_timestamp(0);
        let holder = Address::generate(&env);
        set_verification_rate_limit(&env, &holder, 4).unwrap();
        set_verification_window(&env, &holder, 400).unwrap();
        drain(&env, &holder, 4);

        // A 400s window with capacity 4 refills one token every 100s.
        env.ledger().set_timestamp(100);
        assert_eq!(get_available_attempts(&env, &holder), 1);
    });
}

#[test]
fn batch_can_set_rate_limits_atomically() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let ops = Vec::from_array(
            &env,
            [
                BatchOperation::SetVerificationRateLimit(a.clone(), 5),
                BatchOperation::SetVerificationRateLimit(b.clone(), 0),
            ],
        );
        // The second limit is invalid, so the whole batch is refused at creation.
        assert_eq!(
            create_batch_operation(&env, ops),
            Err(ContractError::InvalidRateLimit)
        );

        let ops = Vec::from_array(
            &env,
            [
                BatchOperation::SetVerificationRateLimit(a.clone(), 5),
                BatchOperation::SetVerificationRateLimit(b.clone(), 7),
            ],
        );
        let id = create_batch_operation(&env, ops).unwrap();
        execute_batch_operation(&env, id).unwrap();
        assert_eq!(get_verification_rate_limit(&env, &a).limit, 5);
        assert_eq!(get_verification_rate_limit(&env, &b).limit, 7);
    });
}
