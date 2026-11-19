//! Issue #2073 — `slash` must not discard a failed credit-score update.
//!
//! The slash still succeeds, but a failed `update_credit_score` is published as
//! a `("credit", "upd_fail")` event so the stale score is observable.

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
        &String::from_str(&s.env, "slash credit score failure test"),
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
fn test_slash_emits_upd_fail_event_when_credit_score_update_fails() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    // Disabling credit scoring makes update_credit_score return
    // CreditScoreCalculationFailed, which slash previously discarded silently.
    let mut cfg = s.client.get_credit_score_config_view();
    cfg.enabled = false;
    s.client.set_credit_score_config(&s.admins, &cfg);

    // The slash itself must still succeed.
    s.client.slash(&s.admins, &borrower);

    assert!(has_event(&s.env, "credit", "upd_fail"));
}

#[test]
fn test_slash_emits_no_upd_fail_event_when_credit_score_update_succeeds() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    s.client.slash(&s.admins, &borrower);

    assert!(!has_event(&s.env, "credit", "upd_fail"));
}
