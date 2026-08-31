//! Issue #1420 — Duplicate Vouch Rejection Tests
//!
//! Exercises the `DuplicateVouch` guard in `validate_vouch` and `batch_vouch`
//! (`src/vouch.rs`):
//!   - A second vouch from the same (voucher, token) pair for the same borrower
//!     is rejected with `DuplicateVouch`.
//!   - A different token from the same voucher for the same borrower is allowed.
//!   - `batch_vouch` reports per-item `success: false` with `DuplicateVouch`
//!     error_code for the duplicate entry, without aborting the whole batch.

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

    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &50_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    // Disable cooldown
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::VouchCooldownSecs, &0u64);
    });

    env.ledger().with_mut(|l| l.timestamp = 120);

    let borrower = Address::generate(env);
    let voucher = Address::generate(env);
    StellarAssetClient::new(env, &token_id.address()).mint(&voucher, &20_000_000);

    (contract_id, token_id.address(), admin, borrower, voucher)
}

fn admin_signers(env: &Env, admin: &Address) -> Vec<Address> {
    Vec::from_array(env, [admin.clone()])
}

// ── Test 1: second vouch from same (voucher, token) pair is rejected ──────────

#[test]
fn test_duplicate_vouch_same_token_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // First vouch — should succeed
    client.vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);

    // Second vouch from the same voucher for the same borrower with the same token
    let result = client.try_vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::DuplicateVouch)),
        "Duplicate (voucher, token) pair must be rejected with DuplicateVouch"
    );
}

// ── Test 2: a different token from the same voucher for the same borrower is ok

#[test]
fn test_different_token_from_same_voucher_is_allowed() {
    let env = Env::default();
    let (contract_id, token_addr, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Register a second token
    let token2_id = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &token2_id.address()).mint(&contract_id, &50_000_000);
    StellarAssetClient::new(&env, &token2_id.address()).mint(&voucher, &10_000_000);
    client.add_allowed_token(&admin_signers(&env, &admin), &token2_id.address()).unwrap();

    // First vouch with token1 — ok
    client.vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);

    // Second vouch with token2 for the same borrower from the same voucher — allowed
    let result = client.try_vouch(&voucher, &borrower, &1_000_000, &token2_id.address(), &None);
    assert!(
        result.is_ok(),
        "Same voucher with a different token must be allowed for the same borrower, got: {:?}",
        result
    );
}

// ── Test 3: batch_vouch reports per-item error for duplicate entry ─────────────

#[test]
fn test_batch_vouch_reports_per_item_error_on_duplicate() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Pre-voucher: establish existing vouch from `voucher` for `borrower`
    client.vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);

    // batch_vouch: new_borrower (ok), borrower (duplicate)
    let new_borrower = Address::generate(&env);
    let borrowers = Vec::from_array(&env, [new_borrower.clone(), borrower.clone()]);
    let stakes = Vec::from_array(&env, [1_000_000i128, 1_000_000i128]);

    let results = client.batch_vouch(&voucher, &borrowers, &stakes, &token_addr, &None);

    assert_eq!(results.len(), 2, "batch_vouch must return one result per entry");

    let r0 = results.get(0).unwrap();
    assert!(r0.success, "new_borrower (no existing vouch) should succeed in batch");
    assert!(r0.error_code.is_none());

    let r1 = results.get(1).unwrap();
    assert!(!r1.success, "duplicate borrower should fail in batch");
    assert_eq!(
        r1.error_code,
        Some(ContractError::DuplicateVouch as u32),
        "error_code should be DuplicateVouch"
    );
}

// ── Test 4: duplicate rejection leaves no additional state changes ────────────

#[test]
fn test_duplicate_rejection_leaves_no_extra_state() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // First vouch succeeds
    client.vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);

    let tc = soroban_sdk::token::Client::new(&env, &token_addr);
    let voucher_balance_after_first = tc.balance(&voucher);

    // Second (duplicate) vouch must fail
    let _ = client.try_vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);

    // Token balance must not have changed further
    assert_eq!(
        tc.balance(&voucher),
        voucher_balance_after_first,
        "Voucher token balance must not change after DuplicateVouch rejection"
    );

    // Still only one vouch record in storage
    env.as_contract(&contract_id, || {
        let vouches: Vec<crate::types::VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));
        assert_eq!(
            vouches.len(),
            1,
            "There must be exactly 1 vouch record after a duplicate rejection"
        );
    });
}
