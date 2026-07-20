//! Min loan amount enforcement tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_loan_below_min_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);
    client.vouch(&voucher, &borrower, &10_000_000i128, &token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);

    // DEFAULT_MIN_LOAN_AMOUNT is 100_000; request 1 stroop
    let result = client.try_request_loan(&borrower, &1i128, &10_000_000i128,
        &soroban_sdk::String::from_str(&env, "too small"), &token);
    assert!(result.is_err(), "sub-minimum loan must be rejected");
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
