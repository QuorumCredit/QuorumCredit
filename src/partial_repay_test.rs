//! Partial repayment tests — I4 must hold throughout.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_active_loan(env: &Env, client: &QuorumCreditContractClient, token: &Address, admin: &Address)
    -> (Address, Address)
{
    let voucher = Address::generate(env);
    let borrower = Address::generate(env);
    fund_address(env, admin, token, &voucher, 50_000_000);
    fund_address(env, admin, token, &borrower, 50_000_000);
    client.vouch(&voucher, &borrower, &10_000_000i128, token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    client.request_loan(&borrower, &2_000_000i128, &10_000_000i128,
        &soroban_sdk::String::from_str(env, "partial repay"), token);
    (voucher, borrower)
}

#[test]
fn test_partial_repay_i4_holds() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let (_voucher, borrower) = setup_active_loan(&env, &client, &token, &admin);

    // Pay half first
    client.repay(&borrower, &1_000_000i128);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // Pay remainder + yield
    let yield_amount = 2_000_000i128 * 200 / 10_000;
    client.repay(&borrower, &(1_000_000 + yield_amount));
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
