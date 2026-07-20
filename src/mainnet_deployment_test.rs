//! Mainnet deployment smoke tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{setup_env, verify_invariants};
use soroban_sdk::Env;

#[test]
fn test_mainnet_config_invariants_hold_on_init() {
    let env = Env::default();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
