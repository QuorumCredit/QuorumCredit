//! API reference smoke tests — verify every public query function returns sensible values.
#![cfg(test)]
#![allow(unused_imports)]
use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_get_config_returns_default_values() {
    let env = Env::default();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let cfg = client.get_config();
    assert_eq!(cfg.yield_bps, crate::types::DEFAULT_YIELD_BPS);
    assert_eq!(cfg.slash_bps, crate::types::DEFAULT_SLASH_BPS);
    assert_eq!(cfg.min_loan_amount, crate::types::DEFAULT_MIN_LOAN_AMOUNT);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_get_admins_returns_initialized_admins() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let admins = client.get_admins();
    assert_eq!(admins.len(), 1);
    assert_eq!(admins.get(0).unwrap(), admin);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}

#[test]
fn test_loan_status_none_for_unknown_borrower() {
    let env = Env::default();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    let status = client.loan_status(&unknown);
    assert_eq!(status, crate::types::LoanStatus::None);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
