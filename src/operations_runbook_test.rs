//! Operations runbook smoke tests — mirrors the runbook walk-through.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_runbook_pause_unpause_cycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

    client.pause(&admins);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
    client.unpause(&admins);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
