//! Governance tests: slash vote flow, config updates.
//! `verify_invariants` is called after every state-changing operation.
#![cfg(test)]
#![allow(unused_imports)]

use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Helper: set up a borrower with an outstanding active loan.
fn setup_active_loan(
    env: &Env,
    client: &QuorumCreditContractClient,
    token: &Address,
    admin: &Address,
    contract_id: &Address,
) -> (Address, Address) {
    let voucher = Address::generate(env);
    let borrower = Address::generate(env);
    fund_address(env, admin, token, &voucher, 50_000_000);
    fund_address(env, admin, token, &borrower, 20_000_000);

    client.vouch(&voucher, &borrower, &10_000_000i128, token, &None);
    env.ledger()
        .with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);
    client.request_loan(
        &borrower,
        &1_000_000i128,
        &10_000_000i128,
        &soroban_sdk::String::from_str(env, "governance test"),
        token,
    );
    (voucher, borrower)
}

#[test]
fn test_invariants_hold_after_slash_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let (voucher, borrower) = setup_active_loan(&env, &client, &token, &admin, &contract_id);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();

    // Cast a slash vote — invariants must still hold
    client.vote_slash(&voucher, &borrower, &true);
    verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
}

#[test]
fn test_invariants_hold_after_config_update() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

    // Update yield_bps to 300 (3%) — still within [0, 10_000]
    client.update_config(&admins, &Some(300i128), &None);
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();

    // Update slash_bps to 4000 (40%)
    client.update_config(&admins, &None, &Some(4000i128));
    verify_invariants(&env, &contract_id, &token, &[]).unwrap();
}
