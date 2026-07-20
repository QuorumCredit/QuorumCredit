//! Slash authorization tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_borrower_with_loan(
    env: &Env, client: &QuorumCreditContractClient, token: &Address, admin: &Address
) -> (Address, Address) {
    let voucher = Address::generate(env);
    let borrower = Address::generate(env);
    fund_address(env, admin, token, &voucher, 50_000_000);
    client.vouch(&voucher, &borrower, &10_000_000i128, token, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    client.request_loan(&borrower, &1_000_000i128, &10_000_000i128,
        &soroban_sdk::String::from_str(env, "slash auth"), token);
    (voucher, borrower)
}

#[test]
fn test_only_voucher_can_vote_slash() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let (_voucher, borrower) = setup_borrower_with_loan(&env, &client, &token, &admin);

    // A random address has no vouch and must not be able to vote
    let outsider = Address::generate(&env);
    let result = client.try_vote_slash(&outsider, &borrower, &true);
    assert!(result.is_err(), "outsider slash vote must be rejected");
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}
