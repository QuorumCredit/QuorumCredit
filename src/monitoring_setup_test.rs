//! Monitoring / observability smoke tests.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{setup_env, verify_invariants};
use soroban_sdk::Env;

#[test]
fn test_slash_treasury_zero_on_init() {
    let env = Env::default();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    // SlashTreasury starts at 0 — I6 must hold immediately
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
