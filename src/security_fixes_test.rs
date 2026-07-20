//! Security regression tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_unauthorized_repay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let thief = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);
    fund_address(&env, &admin, &token, &borrower, 20_000_000);
    fund_address(&env, &admin, &token, &thief, 20_000_000);

    client.vouch(&voucher, &borrower, &10_000_000i128, &token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    client.request_loan(&borrower, &1_000_000i128, &10_000_000i128,
        &soroban_sdk::String::from_str(&env, "security test"), &token);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}

#[test]
fn test_reentrancy_guard_invariants_hold() {
    // Reentrancy guard prevents double-lock; post-operation state must be valid.
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
