//! Loan purpose is persisted correctly.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_loan_purpose_stored() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);

    client.vouch(&voucher, &borrower, &10_000_000i128, &token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    let purpose = soroban_sdk::String::from_str(&env, "buy equipment");
    client.request_loan(&borrower, &1_000_000i128, &10_000_000i128, &purpose, &token);

    let loan = client.get_loan(&borrower).expect("loan should exist");
    assert_eq!(loan.loan_purpose, purpose, "loan purpose mismatch");
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
