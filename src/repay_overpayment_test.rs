//! Overpayment protection tests — I4 must catch over-repayment.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_overpayment_rejected_or_i4_holds() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);
    fund_address(&env, &admin, &token, &borrower, 50_000_000);

    client.vouch(&voucher, &borrower, &10_000_000i128, &token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    let loan_amount = 1_000_000i128;
    client.request_loan(&borrower, &loan_amount, &10_000_000i128,
        &soroban_sdk::String::from_str(&env, "overpay test"), &token);

    // Try to pay 10x the owed amount
    let gross_overrepayment = loan_amount * 10;
    let result = client.try_repay(&borrower, &gross_overrepayment);
    if result.is_ok() {
        // If the contract accepted it, I4 must still hold
        verify_invariants(&env, &contract_id, &token, &[]).unwrap();
    } else {
        // Contract correctly rejected it; state must still be consistent
        verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
    }
}
