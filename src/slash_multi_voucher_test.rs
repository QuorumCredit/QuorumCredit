//! Multi-voucher slash tests — I1 and I6 must hold after slash.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_slash_with_multiple_vouchers_invariants_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    let mut vouchers = Vec::new(&env);
    for _ in 0..3u32 {
        let v = Address::generate(&env);
        fund_address(&env, &admin, &token, &v, 20_000_000);
        client.vouch(&v, &borrower, &5_000_000i128, &token, &None);
        vouchers.push_back(v);
    }

    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    client.request_loan(&borrower, &2_000_000i128, &15_000_000i128,
        &soroban_sdk::String::from_str(&env, "multi-slash"), &token);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // All three vouchers vote to slash
    for i in 0..vouchers.len() {
        client.vote_slash(&vouchers.get(i).unwrap(), &borrower, &true);
        verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
    }

    // Attempt to execute slash
    let _ = client.try_execute_slash_vote(&borrower);
    // After slash vouches are cleared
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

use soroban_sdk::Vec;
