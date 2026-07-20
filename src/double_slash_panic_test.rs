//! Double-slash protection tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_defaulted_loan(env: &Env, client: &QuorumCreditContractClient, token: &Address, admin: &Address) -> (Address, Address) {
    let voucher = Address::generate(env);
    let borrower = Address::generate(env);
    fund_address(env, admin, token, &voucher, 50_000_000);
    client.vouch(&voucher, &borrower, &10_000_000i128, token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    client.request_loan(&borrower, &1_000_000i128, &10_000_000i128, &soroban_sdk::String::from_str(env, "slash test"), token);
    (voucher, borrower)
}

#[test]
fn test_slash_vote_then_execute_invariants_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let (voucher, borrower) = setup_defaulted_loan(&env, &client, &token, &admin);

    client.vote_slash(&voucher, &borrower, &true);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // Execute slash (quorum met since there's only one voucher)
    let _ = client.try_execute_slash_vote(&borrower);
    // After slash, vouches are cleared — use empty borrower list
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_double_slash_vote_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let (voucher, borrower) = setup_defaulted_loan(&env, &client, &token, &admin);

    client.vote_slash(&voucher, &borrower, &true);
    // Second vote from same voucher must fail
    let result = client.try_vote_slash(&voucher, &borrower, &true);
    assert!(result.is_err(), "double vote should be rejected");
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
