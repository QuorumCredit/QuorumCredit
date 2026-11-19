//! Issue #2075 — the lazy default check on `get_loan` / `get_loan_by_id` must not
//! fail silently.
//!
//! Expected outcomes (`NoActiveLoan` for a settled or unknown loan) stay quiet. Any
//! other error is published as a `("lazy_default", "check_failed")` event.

#![cfg(test)]

use crate::types::LoanStatus;
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

    Setup { env, client, token }
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
        &String::from_str(&s.env, "lazy default result test"),
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
fn test_get_loan_by_id_marks_past_deadline_loan_defaulted_without_failure_event() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    let loan = s.client.get_loan(&borrower).unwrap();
    // Move past the deadline with the loan still unpaid.
    s.env
        .ledger()
        .with_mut(|l| l.timestamp = loan.deadline + 1);

    let refreshed = s.client.get_loan_by_id(&loan.id).unwrap();

    assert_eq!(refreshed.status, LoanStatus::Defaulted);
    assert!(!has_event(&s.env, "lazy_default", "check_failed"));
}

#[test]
fn test_get_loan_by_id_for_unknown_loan_emits_no_failure_event() {
    let s = setup();

    // NoActiveLoan is the expected miss for an unknown id and must stay quiet.
    assert!(s.client.get_loan_by_id(&999_999u64).is_none());
    assert!(!has_event(&s.env, "lazy_default", "check_failed"));
}

#[test]
fn test_get_loan_on_active_loan_emits_no_failure_event() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    let loan = s.client.get_loan(&borrower).unwrap();

    assert_eq!(loan.status, LoanStatus::Active);
    assert!(!has_event(&s.env, "lazy_default", "check_failed"));
}
