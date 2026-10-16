//! Issue #1909 — `slash`, `auto_slash` and `claim_expired_loan` return typed
//! `ContractError`s instead of panicking through `expect()` / `assert!`.
//!
//! Each failure path is checked through the `try_*` client, which surfaces the
//! contract error itself rather than an opaque aborted transaction.

#![cfg(test)]

use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, String, Vec,
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

/// Opens an active loan for `borrower` backed by a single voucher.
fn open_loan(s: &Setup, borrower: &Address) {
    let voucher = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &10_000_000);
    s.client
        .vouch(&voucher, borrower, &10_000_000, &s.token, &None);
    s.client.request_loan(
        borrower,
        &5_000_000,
        &10_000_000,
        &String::from_str(&s.env, "slash result test"),
        &s.token,
    );
}

// ── slash ─────────────────────────────────────────────────────────────────────

#[test]
fn test_slash_without_active_loan_returns_no_active_loan() {
    let s = setup();
    let borrower = Address::generate(&s.env);

    assert_eq!(
        s.client.try_slash(&s.admins, &borrower),
        Err(Ok(ContractError::NoActiveLoan)),
    );
}

#[test]
fn test_slash_while_paused_returns_contract_paused() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    s.client.emergency_pause(&s.admins);

    assert_eq!(
        s.client.try_slash(&s.admins, &borrower),
        Err(Ok(ContractError::ContractPaused)),
    );
}

// ── auto_slash ────────────────────────────────────────────────────────────────

#[test]
fn test_auto_slash_without_active_loan_returns_no_active_loan() {
    let s = setup();
    let borrower = Address::generate(&s.env);

    assert_eq!(
        s.client.try_auto_slash(&borrower),
        Err(Ok(ContractError::NoActiveLoan)),
    );
}

#[test]
fn test_auto_slash_before_grace_period_returns_grace_period_not_passed() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    // The loan was just opened, so its deadline plus grace period is still ahead.
    assert_eq!(
        s.client.try_auto_slash(&borrower),
        Err(Ok(ContractError::GracePeriodNotPassed)),
    );
}

// ── claim_expired_loan ────────────────────────────────────────────────────────

#[test]
fn test_claim_expired_loan_without_active_loan_returns_no_active_loan() {
    let s = setup();
    let borrower = Address::generate(&s.env);

    assert_eq!(
        s.client.try_claim_expired_loan(&borrower),
        Err(Ok(ContractError::NoActiveLoan)),
    );
}

#[test]
fn test_claim_expired_loan_before_deadline_returns_loan_not_expired() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    open_loan(&s, &borrower);

    assert_eq!(
        s.client.try_claim_expired_loan(&borrower),
        Err(Ok(ContractError::LoanNotExpired)),
    );
}
