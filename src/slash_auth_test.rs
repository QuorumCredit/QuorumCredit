#![cfg(test)]
//! Issue #1434: slash authorization coverage.
//!
//! Verifies that a slash can only be driven through the sanctioned
//! governance / admin paths and that unauthorized callers are rejected
//! cleanly (a `ContractError`, never a panic or a silent stake deduction).
//!
//! Env / admin setup follows the pattern in `slash_appeal_test.rs`.

use crate::governance::{execute_slash_vote, vote_slash};
use crate::loan::request_loan;
use crate::types::DataKey;
use crate::vouch::vouch;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup_test_env() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::new();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let deployer = Address::random(&env);
    let admin = Address::random(&env);
    let borrower = Address::random(&env);
    let voucher1 = Address::random(&env);
    let voucher2 = Address::random(&env);
    let token = Address::random(&env);

    crate::QuorumCreditContract::initialize(
        env.clone(),
        deployer.clone(),
        vec![&env, admin.clone()],
        1,
        token.clone(),
    )
    .expect("initialize failed");

    (env, admin, borrower, voucher1, voucher2, token)
}

/// A slash cannot be executed before the voucher quorum has voted for it.
#[test]
fn test_slash_requires_quorum_vote() {
    let (env, _admin, borrower, voucher1, voucher2, token) = setup_test_env();

    vouch(&env, voucher1.clone(), borrower.clone(), 1000, token.clone())
        .expect("vouch1 failed");
    vouch(&env, voucher2.clone(), borrower.clone(), 2000, token.clone())
        .expect("vouch2 failed");
    request_loan(&env, borrower.clone(), 3000, 86400, String::new(&env))
        .expect("request_loan failed");

    // No votes cast yet — execution must be rejected, not silently applied.
    let result = execute_slash_vote(&env, borrower.clone());
    assert!(
        result.is_err(),
        "execute_slash_vote must fail without a passing quorum vote"
    );

    // And no slash record / stake deduction should have been written.
    let record = env
        .storage()
        .persistent()
        .get::<DataKey, crate::types::SlashRecord>(&DataKey::SlashAudit(borrower.clone()));
    assert!(record.is_none(), "no slash record should exist before a valid slash");
}

/// Only an actual voucher on the loan may cast a slash vote.
#[test]
fn test_non_voucher_cannot_vote_slash() {
    let (env, _admin, borrower, voucher1, _voucher2, token) = setup_test_env();
    let stranger = Address::random(&env);

    vouch(&env, voucher1.clone(), borrower.clone(), 1000, token.clone())
        .expect("vouch1 failed");
    request_loan(&env, borrower.clone(), 1000, 86400, String::new(&env))
        .expect("request_loan failed");

    let result = vote_slash(&env, stranger.clone(), borrower.clone(), true);
    assert!(
        result.is_err(),
        "a non-voucher must not be able to cast a slash vote"
    );
}

/// Voting to slash a borrower with no outstanding loan is rejected.
#[test]
fn test_cannot_slash_without_active_loan() {
    let (env, _admin, borrower, voucher1, _voucher2, token) = setup_test_env();

    vouch(&env, voucher1.clone(), borrower.clone(), 1000, token.clone())
        .expect("vouch1 failed");

    // No loan requested — there is nothing to slash against.
    let result = vote_slash(&env, voucher1.clone(), borrower.clone(), true)
        .and_then(|_| execute_slash_vote(&env, borrower.clone()));
    assert!(
        result.is_err(),
        "slash flow must fail when the borrower has no active loan"
    );
}
