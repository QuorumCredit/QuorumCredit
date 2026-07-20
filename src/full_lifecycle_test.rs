//! Full lifecycle integration test: vouch → request_loan → repay.
//! `verify_invariants` is called after every state-changing operation.
#![cfg(test)]
#![allow(unused_imports)]

use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_full_lifecycle_vouch_loan_repay() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 100_000_000);
    fund_address(&env, &admin, &token, &borrower, 50_000_000);

    let stake: i128 = 10_000_000;
    let loan_amount: i128 = 1_000_000;

    // 1. Vouch
    client.vouch(&voucher, &borrower, &stake, &token, &None);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // 2. Advance past min vouch age
    env.ledger()
        .with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);

    // 3. Request loan
    client.request_loan(
        &borrower,
        &loan_amount,
        &stake,
        &soroban_sdk::String::from_str(&env, "business"),
        &token,
    );
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // 4. Repay
    let yield_amount = loan_amount * 200 / 10_000;
    client.repay(&borrower, &(loan_amount + yield_amount));
    // Vouches cleared after full repayment
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_full_lifecycle_multiple_vouchers() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &borrower, 50_000_000);

    let mut vouchers = soroban_sdk::Vec::new(&env);
    for _ in 0..3u32 {
        let v = Address::generate(&env);
        fund_address(&env, &admin, &token, &v, 20_000_000);
        client.vouch(&v, &borrower, &5_000_000i128, &token, &None);
        vouchers.push_back(v);
    }
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    env.ledger()
        .with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);

    client.request_loan(
        &borrower,
        &2_000_000i128,
        &15_000_000i128,
        &soroban_sdk::String::from_str(&env, "multi-voucher"),
        &token,
    );
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    let yield_amount = 2_000_000i128 * 200 / 10_000;
    client.repay(&borrower, &(2_000_000 + yield_amount));
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
