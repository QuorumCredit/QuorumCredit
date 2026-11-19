//! Issue #2074 — `repay` must not discard badge-mint or credit-score errors.
//!
//! Neither failure aborts the repayment: the loan is still settled. Each failure
//! is published as an event so the caller and indexers can see it.

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

/// Opens an active loan for `borrower` and returns nothing; the borrower holds the principal.
fn open_loan(s: &Setup, borrower: &Address) {
    let voucher = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &10_000_000);
    s.client
        .vouch(&voucher, borrower, &10_000_000, &s.token, &None);
    s.client.request_loan(
        borrower,
        &5_000_000,
        &10_000_000,
        &String::from_str(&s.env, "repay result events test"),
        &s.token,
    );
}

/// Repays the full outstanding balance, funding the borrower for any yield owed.
fn repay_in_full(s: &Setup, borrower: &Address) {
    let loan = s.client.get_loan(borrower).unwrap();
    let owed = loan.amount + loan.total_yield - loan.amount_repaid;
    StellarAssetClient::new(&s.env, &s.token).mint(borrower, &owed);
    s.client.repay(borrower, &owed);
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
fn test_repay_emits_mint_failed_for_borrower_without_badge_eligibility() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    // A first-time borrower has no credit score yet, so the badge mint fails.
    repay_in_full(&s, &borrower);

    assert!(has_event(&s.env, "reputation", "mint_failed"));
    // The repayment itself succeeded and settled the loan.
    assert!(s.client.get_loan(&borrower).unwrap().repaid);
}

#[test]
fn test_repay_emits_upd_fail_when_credit_score_update_fails() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    // Disabling credit scoring makes update_credit_score fail after repayment.
    let mut cfg = s.client.get_credit_score_config_view();
    cfg.enabled = false;
    s.client.set_credit_score_config(&s.admins, &cfg);

    repay_in_full(&s, &borrower);

    assert!(has_event(&s.env, "credit", "upd_fail"));
    // A failed score update must not undo the repayment.
    assert!(s.client.get_loan(&borrower).unwrap().repaid);
}

#[test]
fn test_repay_emits_no_upd_fail_when_credit_score_update_succeeds() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    repay_in_full(&s, &borrower);

    assert!(!has_event(&s.env, "credit", "upd_fail"));
}
