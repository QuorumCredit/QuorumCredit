//! Issue #2072 — `slash` must not discard the circuit-breaker result.
//!
//! When the breaker engages after a slash, the activation is recorded in the
//! history and no failure event is published. A failure of the breaker itself is
//! published as a `("circuit_breaker", "failed")` event.

#![cfg(test)]

use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token::StellarAssetClient,
    Address, Env, IntoVal, String, Symbol, Val, Vec,
};

struct Setup {
    env: Env,
    client: QuorumCreditContractClient<'static>,
    token: Address,
    admins: Vec<Address>,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let deployer = Address::generate(&env);
    let admin = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(&env, &token).mint(&contract_id, &1_000_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.initialize(&deployer, &admins, &1u32, &token);

    // Start at t=120 so vouches pass the minimum vouch age.
    env.ledger().with_mut(|l| l.timestamp = 120);

    Setup {
        env,
        client,
        token,
        admins,
    }
}

fn open_loan(s: &Setup, borrower: &Address) {
    let voucher = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &10_000_000);
    s.client
        .vouch(&voucher, borrower, &10_000_000, &s.token, &None);
    s.client.request_loan(
        borrower,
        &5_000_000,
        &10_000_000,
        &String::from_str(&s.env, "slash circuit breaker result test"),
        &s.token,
    );
}

/// True when any event emitted by the last invocation has the given two-symbol topics.
fn has_event(env: &Env, topic0: &str, topic1: &str) -> bool {
    let expected0: Val = Symbol::new(env, topic0).into_val(env);
    let expected1: Val = Symbol::new(env, topic1).into_val(env);
    env.events().all().iter().any(|(_, topics, _)| {
        topics.len() == 2 && topics.get(0) == Some(expected0) && topics.get(1) == Some(expected1)
    })
}

#[test]
fn test_slash_that_trips_breaker_emits_no_failed_event() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    // Any default rate at or above a 1 bps threshold trips the breaker on this slash.
    let mut cfg = s.client.get_config();
    cfg.default_rate_threshold = 1;
    s.client.set_config(&s.admins, &cfg);

    // The breaker has a 1-hour anti-thrash cooldown measured from the last trigger
    // (0 when never triggered), so move past it to make the first trigger eligible.
    s.env.ledger().with_mut(|l| l.timestamp = 10_000);

    s.client.slash(&s.admins, &borrower);

    // The breaker engaged, so it is recorded in history.
    assert_eq!(s.client.get_circuit_breaker_history().len(), 1);
    // And no failure is reported.
    assert!(!has_event(&s.env, "circuit_breaker", "failed"));
}

#[test]
fn test_slash_below_threshold_emits_no_failed_event() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    // Threshold of 0 disables the breaker entirely, so it cannot trigger.
    let mut cfg = s.client.get_config();
    cfg.default_rate_threshold = 0;
    s.client.set_config(&s.admins, &cfg);

    s.client.slash(&s.admins, &borrower);

    assert_eq!(s.client.get_circuit_breaker_history().len(), 0);
    assert!(!has_event(&s.env, "circuit_breaker", "failed"));
}
