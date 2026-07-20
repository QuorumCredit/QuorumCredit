//! Bug-condition regression tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_invariants_hold_after_vouch_to_self_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let actor = Address::generate(&env);
    fund_address(&env, &admin, &token, &actor, 10_000_000);
    // Self-vouch must be rejected
    let result = client.try_vouch(&actor, &actor, &1_000_000i128, &token, &None);
    assert!(result.is_err(), "self-vouch should be rejected");
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
