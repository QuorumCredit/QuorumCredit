//! Issue #2104 — Cooldown bypass approvals must expire, be consumed on use,
//! and cap the `reason` length.
//!
//! Drives the real request → vote flow through `crate::cooldown_bypass` and
//! then exercises `validate_vouch` through the public `vouch` entrypoint.

#![cfg(test)]

use crate::cooldown_bypass;
use crate::types::{DataKey, COOLDOWN_BYPASS_VALIDITY_SECS, MAX_COOLDOWN_BYPASS_REASON_LEN};
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, String, Vec,
};

/// Returns (contract_id, token_addr, admin, borrower, voucher).
/// Single admin with threshold 1 so one approval reaches the 2/3 bar.
fn setup(env: &Env) -> (Address, Address, Address, Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let borrower = Address::generate(env);
    let voucher = Address::generate(env);
    StellarAssetClient::new(env, &token_id.address()).mint(&voucher, &20_000_000);

    (contract_id, token_id.address(), admin, borrower, voucher)
}

/// Creates a vouch from `voucher` to `borrower1` so the cooldown clock starts,
/// then requests and admin-approves a bypass for `(voucher, borrower2)`.
fn approve_bypass_for(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    admin: &Address,
    voucher: &Address,
    borrower1: &Address,
    borrower2: &Address,
) {
    let client = QuorumCreditContractClient::new(env, contract_id);
    client.vouch(voucher, borrower1, &1_000_000, token, &None);

    env.as_contract(contract_id, || {
        cooldown_bypass::request_cooldown_bypass(
            env.clone(),
            voucher.clone(),
            borrower2.clone(),
            String::from_str(env, "emergency re-vouch"),
        )
        .unwrap();
        cooldown_bypass::vote_bypass(
            env.clone(),
            admin.clone(),
            voucher.clone(),
            borrower2.clone(),
            true,
        )
        .unwrap();
    });
}

// ── Reason length is capped ───────────────────────────────────────────────────

#[test]
fn test_bypass_reason_over_cap_is_rejected() {
    let env = Env::default();
    let (contract_id, token, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &Address::generate(&env), &1_000_000, &token, &None);

    let long_reason = String::from_bytes(
        &env,
        &vec![b'a'; (MAX_COOLDOWN_BYPASS_REASON_LEN + 1) as usize],
    );
    let result = env.as_contract(&contract_id, || {
        cooldown_bypass::request_cooldown_bypass(
            env.clone(),
            voucher.clone(),
            borrower.clone(),
            long_reason,
        )
    });
    assert_eq!(result, Err(ContractError::CooldownBypassReasonTooLong));
    let _ = admin;
}

#[test]
fn test_bypass_reason_at_cap_is_accepted() {
    let env = Env::default();
    let (contract_id, token, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &Address::generate(&env), &1_000_000, &token, &None);

    let reason = String::from_bytes(
        &env,
        &vec![b'a'; MAX_COOLDOWN_BYPASS_REASON_LEN as usize],
    );
    let result = env.as_contract(&contract_id, || {
        cooldown_bypass::request_cooldown_bypass(env.clone(), voucher.clone(), borrower.clone(), reason)
    });
    assert_eq!(result, Ok(()));
}

// ── Approved bypass expires ───────────────────────────────────────────────────

#[test]
fn test_expired_bypass_does_not_skip_cooldown() {
    let env = Env::default();
    let (contract_id, token, admin, _borrower, voucher) = setup(&env);
    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);

    approve_bypass_for(&env, &contract_id, &token, &admin, &voucher, &borrower1, &borrower2);

    // Advance past the validity window that started at approval.
    env.ledger()
        .with_mut(|l| l.timestamp += COOLDOWN_BYPASS_VALIDITY_SECS + 1);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let result = client.try_vouch(&voucher, &borrower2, &1_000_000, &token, &None);
    assert_eq!(result, Err(Ok(ContractError::VouchCooldownActive)));
}

#[test]
fn test_expired_pending_request_cannot_be_voted_on() {
    let env = Env::default();
    let (contract_id, token, admin, _borrower, voucher) = setup(&env);
    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &borrower1, &1_000_000, &token, &None);

    env.as_contract(&contract_id, || {
        cooldown_bypass::request_cooldown_bypass(
            env.clone(),
            voucher.clone(),
            borrower2.clone(),
            String::from_str(&env, "late"),
        )
        .unwrap();
    });

    env.ledger()
        .with_mut(|l| l.timestamp += COOLDOWN_BYPASS_VALIDITY_SECS + 1);

    let result = env.as_contract(&contract_id, || {
        cooldown_bypass::vote_bypass(
            env.clone(),
            admin.clone(),
            voucher.clone(),
            borrower2.clone(),
            true,
        )
    });
    assert_eq!(result, Err(ContractError::CooldownBypassExpired));
}

// ── Approved bypass is consumed on first use ──────────────────────────────────

#[test]
fn test_bypass_is_single_use() {
    let env = Env::default();
    let (contract_id, token, admin, _borrower, voucher) = setup(&env);
    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    let borrower3 = Address::generate(&env);

    approve_bypass_for(&env, &contract_id, &token, &admin, &voucher, &borrower1, &borrower2);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // First use consumes the waiver.
    assert!(client
        .try_vouch(&voucher, &borrower2, &1_000_000, &token, &None)
        .is_ok());

    env.as_contract(&contract_id, || {
        let key = DataKey::CooldownBypass(borrower2.clone(), voucher.clone());
        assert!(
            !env.storage().persistent().has(&key),
            "bypass record must be removed after it is consumed"
        );
    });

    // Re-vouching for the same borrower inside the window is blocked again.
    // Use a fresh borrower pair that has never been granted a waiver.
    let result = client.try_vouch(&voucher, &borrower3, &1_000_000, &token, &None);
    assert_eq!(result, Err(Ok(ContractError::VouchCooldownActive)));
}
