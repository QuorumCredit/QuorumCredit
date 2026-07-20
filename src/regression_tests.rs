//! Regression tests for fixed bugs.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Regression: vouch followed by immediate withdraw_vouch (no active loan) must leave
/// contract in a consistent state.
#[test]
fn test_regression_withdraw_vouch_after_vouch() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 20_000_000);

    client.vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // Advance past cooldown/lock period
    env.ledger().with_mut(|l| {
        l.timestamp = crate::types::MIN_VOUCH_LOCK_PERIOD + 10;
    });

    let _ = client.try_withdraw_vouch(&voucher, &borrower, &token);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}

/// Regression: increase_stake then decrease_stake must not corrupt balance.
#[test]
fn test_regression_increase_then_decrease_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);

    client.vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    client.increase_stake(&voucher, &borrower, &2_000_000i128, &token);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);
    // decrease_stake back to original level (must stay above min_stake)
    let _ = client.try_decrease_stake(&voucher, &borrower, &5_000_000i128, &token);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
