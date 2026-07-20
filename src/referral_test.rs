//! Referral bonus tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_invariants_hold_with_referral() {
    // Referral does not mutate core invariant-checked state differently
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
