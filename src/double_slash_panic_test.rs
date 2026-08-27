#![cfg(test)]
//! Issue #1434: double-slash safety.
//!
//! Slashing the same borrower / voucher twice must return a clean
//! `ContractError` — it must not panic, and it must not deduct stake a
//! second time (no double accounting on `total_slashed`).
//!
//! Env / admin setup follows the pattern in `slash_appeal_test.rs`.

use crate::governance::{execute_slash_vote, vote_slash};
use crate::loan::request_loan;
use crate::types::{DataKey, SlashRecord};
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

fn first_slash(
    env: &Env,
    borrower: &Address,
    voucher1: &Address,
    voucher2: &Address,
    token: &Address,
) -> SlashRecord {
    vouch(env, voucher1.clone(), borrower.clone(), 1_000, token.clone())
        .expect("vouch1 failed");
    vouch(env, voucher2.clone(), borrower.clone(), 2_000, token.clone())
        .expect("vouch2 failed");
    request_loan(env, borrower.clone(), 3_000, 86400, String::new(env))
        .expect("request_loan failed");

    vote_slash(env, voucher1.clone(), borrower.clone(), true).expect("vote1 failed");
    vote_slash(env, voucher2.clone(), borrower.clone(), true).expect("vote2 failed");
    execute_slash_vote(env, borrower.clone()).expect("first execute_slash_vote failed");

    env.storage()
        .persistent()
        .get(&DataKey::SlashAudit(borrower.clone()))
        .expect("slash record not found after first slash")
}

/// Re-executing the slash for an already-slashed borrower returns an error.
#[test]
fn test_double_execute_slash_vote_returns_error_not_panic() {
    let (env, _admin, borrower, voucher1, voucher2, token) = setup_test_env();

    let first = first_slash(&env, &borrower, &voucher1, &voucher2, &token);
    assert!(first.total_slashed > 0, "first slash should deduct stake");

    // Second execution must be a clean Err (no panic).
    let second = execute_slash_vote(&env, borrower.clone());
    assert!(
        second.is_err(),
        "second execute_slash_vote must return ContractError, got Ok"
    );
}

/// A second slash attempt must not increase `total_slashed`.
#[test]
fn test_double_slash_does_not_double_deduct() {
    let (env, _admin, borrower, voucher1, voucher2, token) = setup_test_env();

    let first = first_slash(&env, &borrower, &voucher1, &voucher2, &token);
    let slashed_after_first = first.total_slashed;

    let _ = execute_slash_vote(&env, borrower.clone());

    let after: SlashRecord = env
        .storage()
        .persistent()
        .get(&DataKey::SlashAudit(borrower.clone()))
        .expect("slash record missing after second attempt");

    assert_eq!(
        after.total_slashed, slashed_after_first,
        "total_slashed must be unchanged by a rejected second slash"
    );
}

/// Re-voting after a completed slash also does not re-open the flow.
#[test]
fn test_revote_after_slash_is_rejected() {
    let (env, _admin, borrower, voucher1, voucher2, token) = setup_test_env();

    let _ = first_slash(&env, &borrower, &voucher1, &voucher2, &token);

    let revote = vote_slash(&env, voucher1.clone(), borrower.clone(), true)
        .and_then(|_| execute_slash_vote(&env, borrower.clone()));
    assert!(
        revote.is_err(),
        "re-voting and re-executing after a completed slash must be rejected"
    );
}
