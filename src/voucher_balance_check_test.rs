//! Issue #1419 — Voucher Balance Check Tests
//!
//! Exercises the `InsufficientVoucherBalance` guard in `validate_vouch`
//! (`src/vouch.rs`): when `token_client.balance(voucher) < stake` the vouch
//! must be rejected *before* any state is mutated or tokens are transferred.

#![cfg(test)]

use crate::types::DataKey;
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, Vec,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, crate::QuorumCreditContract);

    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    // Disable cooldown for clarity
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::VouchCooldownSecs, &0u64);
    });

    env.ledger().with_mut(|l| l.timestamp = 120);

    (contract_id, token_id.address(), admin, deployer)
}

// ── Test 1: voucher with insufficient balance is rejected ─────────────────────

#[test]
fn test_insufficient_voucher_balance_is_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);
    // Mint only 100_000 — less than the 500_000 we will attempt to stake
    StellarAssetClient::new(&env, &token_addr).mint(&voucher, &100_000);

    let result = client.try_vouch(&voucher, &borrower, &500_000, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::InsufficientVoucherBalance)),
        "Voucher with insufficient balance must be rejected with InsufficientVoucherBalance"
    );
}

// ── Test 2: voucher with exact balance is accepted ────────────────────────────

#[test]
fn test_exact_balance_is_accepted() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);
    // Mint exactly 500_000 — equal to the stake amount
    StellarAssetClient::new(&env, &token_addr).mint(&voucher, &500_000);

    let result = client.try_vouch(&voucher, &borrower, &500_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "Voucher with exact balance must be accepted, got: {:?}",
        result
    );
}

// ── Test 3: insufficient balance leaves no partial state changes ──────────────

#[test]
fn test_insufficient_balance_leaves_no_state_changes() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);
    // Give the voucher only 200_000 but attempt to stake 1_000_000
    StellarAssetClient::new(&env, &token_addr).mint(&voucher, &200_000);

    let tc = soroban_sdk::token::Client::new(&env, &token_addr);
    let voucher_balance_before = tc.balance(&voucher);
    let contract_balance_before = tc.balance(&contract_id);

    let _ = client.try_vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);

    // No Vouches entry should have been created
    env.as_contract(&contract_id, || {
        let vouches: Option<Vec<crate::types::VouchRecord>> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()));
        assert!(
            vouches.is_none() || vouches.unwrap().is_empty(),
            "No Vouches entry should exist after InsufficientVoucherBalance rejection"
        );

        // No LastVouchTimestamp should have been written
        let ts: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::LastVouchTimestamp(voucher.clone()));
        assert!(
            ts.is_none(),
            "LastVouchTimestamp must not be set after InsufficientVoucherBalance rejection"
        );
    });

    // Token balances must be unchanged
    assert_eq!(
        tc.balance(&voucher),
        voucher_balance_before,
        "Voucher token balance must not change after InsufficientVoucherBalance rejection"
    );
    assert_eq!(
        tc.balance(&contract_id),
        contract_balance_before,
        "Contract token balance must not change after InsufficientVoucherBalance rejection"
    );
}

// ── Test 4: zero-balance voucher is rejected ──────────────────────────────────

#[test]
fn test_zero_balance_voucher_is_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    // voucher has NO tokens at all
    let voucher = Address::generate(&env);

    let result = client.try_vouch(&voucher, &borrower, &1, &token_addr, &None);
    // stake > 0 passes require_positive_amount but balance check must fire
    assert_eq!(
        result,
        Err(Ok(ContractError::InsufficientVoucherBalance)),
        "Zero-balance voucher must be rejected with InsufficientVoucherBalance"
    );
}
