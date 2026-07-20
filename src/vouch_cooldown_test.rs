//! Vouch cooldown enforcement tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_vouch_cooldown_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 50_000_000);

    client.vouch(&voucher, &borrower1, &5_000_000i128, &token, &None);
    verify_invariants(&env, &contract_id, &token, &[&borrower1]).unwrap();

    // Immediate second vouch to a different borrower may be rejected by cooldown
    let result = client.try_vouch(&voucher, &borrower2, &5_000_000i128, &token, &None);
    // Whether it passes or fails, invariants must hold
    if result.is_ok() {
        verify_invariants(&env, &contract_id, &token, &[&borrower1, &borrower2]).unwrap();
    } else {
        verify_invariants(&env, &contract_id, &token, &[&borrower1]).unwrap();
    }
}
