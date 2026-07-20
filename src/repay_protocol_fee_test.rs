//! Protocol fee collection during repayment tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_invariants_hold_when_protocol_fee_configured() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

    let fee_recipient = Address::generate(&env);
    // 50 bps = 0.5% protocol fee
    client.set_protocol_fee(&admins, &50u32);
    client.set_fee_treasury(&admins, &fee_recipient);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);
    fund_address(&env, &admin, &token, &borrower, 50_000_000);

    client.vouch(&voucher, &borrower, &10_000_000i128, &token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    let loan_amount = 1_000_000i128;
    client.request_loan(&borrower, &loan_amount, &10_000_000i128,
        &soroban_sdk::String::from_str(&env, "fee test"), &token);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    let yield_amount = loan_amount * 200 / 10_000;
    client.repay(&borrower, &(loan_amount + yield_amount));
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
