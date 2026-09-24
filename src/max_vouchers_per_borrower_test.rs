//! Issue #1420 — Max Vouchers Per Borrower Tests
//!
//! Exercises the `MaxVouchersPerBorrowerExceeded` guard in `validate_vouch`
//! and `batch_vouch` (`src/vouch.rs`):
//!   - Vouching succeeds up to exactly `max_vouchers_per_borrower`.
//!   - The (max+1)-th vouch is rejected with `MaxVouchersPerBorrowerExceeded`.
//!   - `batch_vouch` reports per-item `success: false` with the right
//!     `error_code` for the entry that would exceed the cap, without aborting
//!     the whole batch.
//!   - The configurable default matches `DEFAULT_MAX_VOUCHERS_PER_BORROWER`.

#![cfg(test)]

use crate::types::{DataKey, DEFAULT_MAX_VOUCHERS_PER_BORROWER};
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

    // Fund the contract
    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &100_000_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    // Disable cooldown so vouchers can vouch freely without time advances
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::VouchCooldownSecs, &0u64);
    });

    env.ledger().with_mut(|l| l.timestamp = 120);

    (contract_id, token_id.address(), admin, deployer)
}

fn admin_signers(env: &Env, admin: &Address) -> Vec<Address> {
    Vec::from_array(env, [admin.clone()])
}

/// Create `n` fresh vouchers each funded with `balance` tokens.
fn create_vouchers(env: &Env, token_addr: &Address, n: usize, balance: i128) -> Vec<Address> {
    let mut vouchers = Vec::new(env);
    for _ in 0..n {
        let v = Address::generate(env);
        StellarAssetClient::new(env, token_addr).mint(&v, &balance);
        vouchers.push_back(v);
    }
    vouchers
}

// ── Test 1: vouching succeeds up to max_vouchers_per_borrower ─────────────────

#[test]
fn test_vouching_succeeds_up_to_max() {
    let env = Env::default();
    let (contract_id, token_addr, admin, _) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Set a small cap for practical testing
    let max: u32 = 3;
    client.set_max_vouchers_per_loan(&admin_signers(&env, &admin), &max);

    let borrower = Address::generate(&env);
    let vouchers = create_vouchers(&env, &token_addr, max as usize, 2_000_000);

    for v in vouchers.iter() {
        let result = client.try_vouch(&v, &borrower, &1_000_000, &token_addr, &None);
        assert!(
            result.is_ok(),
            "Vouch #{} (up to max={}) should succeed, got: {:?}",
            vouchers.iter().position(|x| x == v).unwrap() + 1,
            max,
            result
        );
    }

    // Verify exactly `max` vouches are stored
    env.as_contract(&contract_id, || {
        let vouches: Vec<crate::types::VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));
        assert_eq!(
            vouches.len(),
            max,
            "Exactly max={} vouches should be stored",
            max
        );
    });
}

// ── Test 2: (max+1)-th vouch is rejected with MaxVouchersPerBorrowerExceeded ──

#[test]
fn test_vouching_over_max_is_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, admin, _) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let max: u32 = 3;
    client.set_max_vouchers_per_loan(&admin_signers(&env, &admin), &max);

    let borrower = Address::generate(&env);
    let vouchers = create_vouchers(&env, &token_addr, (max + 1) as usize, 2_000_000);

    // Fill up to max
    for i in 0..max as usize {
        client.vouch(&vouchers.get(i as u32).unwrap(), &borrower, &1_000_000, &token_addr, &None);
    }

    // The (max+1)-th voucher attempt must be rejected
    let extra = vouchers.get(max).unwrap();
    let result = client.try_vouch(&extra, &borrower, &1_000_000, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::MaxVouchersPerBorrowerExceeded)),
        "The (max+1)-th vouch must be rejected with MaxVouchersPerBorrowerExceeded"
    );
}

// ── Test 3: batch_vouch reports per-item error when cap is exceeded ───────────

#[test]
fn test_batch_vouch_reports_per_item_error_on_cap_exceeded() {
    let env = Env::default();
    let (contract_id, token_addr, admin, _) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let max: u32 = 2;
    client.set_max_vouchers_per_loan(&admin_signers(&env, &admin), &max);

    let borrower = Address::generate(&env);
    let voucher_batch = Address::generate(&env);
    StellarAssetClient::new(&env, &token_addr).mint(&voucher_batch, &20_000_000);

    // Pre-fill the borrower with max vouches from different vouchers
    let fillers = create_vouchers(&env, &token_addr, max as usize, 2_000_000);
    for v in fillers.iter() {
        client.vouch(&v, &borrower, &1_000_000, &token_addr, &None);
    }

    // Now batch_vouch: borrower2 (ok), borrower (over cap)
    let borrower2 = Address::generate(&env);
    let borrowers = Vec::from_array(&env, [borrower2.clone(), borrower.clone()]);
    let stakes = Vec::from_array(&env, [1_000_000i128, 1_000_000i128]);

    let results = client.batch_vouch(&voucher_batch, &borrowers, &stakes, &token_addr, &None);

    assert_eq!(results.len(), 2, "batch_vouch must return a result for each entry");

    let r0 = results.get(0).unwrap();
    assert!(r0.success, "borrower2 (under cap) should succeed in batch");
    assert!(r0.error_code.is_none());

    let r1 = results.get(1).unwrap();
    assert!(!r1.success, "borrower (at cap) should fail in batch");
    assert_eq!(
        r1.error_code,
        Some(ContractError::MaxVouchersPerBorrowerExceeded as u32),
        "error_code should be MaxVouchersPerBorrowerExceeded"
    );
}

// ── Test 4: DEFAULT_MAX_VOUCHERS_PER_BORROWER is the default cap ──────────────

#[test]
fn test_default_max_vouchers_per_borrower_value() {
    let env = Env::default();
    let (contract_id, _token_addr, _admin, _) = setup(&env);

    env.as_contract(&contract_id, || {
        let stored: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxVouchersPerBorrower)
            .unwrap_or(DEFAULT_MAX_VOUCHERS_PER_BORROWER);
        assert_eq!(
            stored,
            DEFAULT_MAX_VOUCHERS_PER_BORROWER,
            "Default max_vouchers_per_borrower should match DEFAULT_MAX_VOUCHERS_PER_BORROWER ({})",
            DEFAULT_MAX_VOUCHERS_PER_BORROWER
        );
    });
}
