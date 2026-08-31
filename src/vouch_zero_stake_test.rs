//! Issue #1419 — Zero/Negative Stake Rejection Tests
//!
//! Exercises the `require_positive_amount` guard in `validate_vouch`
//! (`src/vouch.rs`).  Any stake ≤ 0 must be rejected before any state
//! is mutated or tokens are transferred.

#![cfg(test)]

use crate::types::DataKey;
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, Vec,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, Address, Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, crate::QuorumCreditContract);

    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    env.ledger().with_mut(|l| l.timestamp = 120);

    let borrower = Address::generate(env);
    let voucher = Address::generate(env);
    StellarAssetClient::new(env, &token_id.address()).mint(&voucher, &10_000_000);

    (contract_id, token_id.address(), admin, borrower, voucher)
}

// ── Test 1: stake == 0 is rejected with InsufficientFunds ────────────────────

#[test]
fn test_zero_stake_is_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let result = client.try_vouch(&voucher, &borrower, &0, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::InsufficientFunds)),
        "stake == 0 must be rejected with InsufficientFunds"
    );
}

// ── Test 2: negative stake is rejected with InsufficientFunds ────────────────

#[test]
fn test_negative_stake_is_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let result = client.try_vouch(&voucher, &borrower, &-1, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::InsufficientFunds)),
        "Negative stake must be rejected with InsufficientFunds"
    );
}

// ── Test 3: zero stake leaves no state changes ────────────────────────────────

#[test]
fn test_zero_stake_leaves_no_state() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Attempt the zero-stake vouch — must fail
    let _ = client.try_vouch(&voucher, &borrower, &0, &token_addr, &None);

    env.as_contract(&contract_id, || {
        // No Vouches entry should have been created
        let vouches: Option<Vec<crate::types::VouchRecord>> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()));
        assert!(
            vouches.is_none() || vouches.unwrap().is_empty(),
            "No Vouches entry should exist after a rejected zero-stake vouch"
        );

        // No LastVouchTimestamp should have been recorded
        let ts: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::LastVouchTimestamp(voucher.clone()));
        assert!(
            ts.is_none(),
            "LastVouchTimestamp must not be set after a rejected zero-stake vouch"
        );
    });
}

// ── Test 4: negative stake leaves no state changes ────────────────────────────

#[test]
fn test_negative_stake_leaves_no_state() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher_balance_before = {
        let tc = soroban_sdk::token::Client::new(&env, &token_addr);
        tc.balance(&voucher)
    };

    let _ = client.try_vouch(&voucher, &borrower, &-1_000_000, &token_addr, &None);

    env.as_contract(&contract_id, || {
        let vouches: Option<Vec<crate::types::VouchRecord>> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()));
        assert!(
            vouches.is_none() || vouches.unwrap().is_empty(),
            "No Vouches entry should exist after a rejected negative-stake vouch"
        );
    });

    // Token balance must be unchanged
    let tc = soroban_sdk::token::Client::new(&env, &token_addr);
    assert_eq!(
        tc.balance(&voucher),
        voucher_balance_before,
        "Voucher token balance must not change after a rejected negative-stake vouch"
    );
}
