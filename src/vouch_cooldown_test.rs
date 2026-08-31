//! Issue #1418 — Vouch Cooldown Enforcement Tests
//!
//! Exercises the cooldown logic in `validate_vouch` (`src/vouch.rs`):
//!   - `VouchCooldownActive` is returned when the same voucher tries to vouch
//!     again within `vouch_cooldown_secs` of the previous vouch.
//!   - A vouch is allowed once the cooldown window has fully elapsed.
//!   - A first-ever vouch (no prior `LastVouchTimestamp`) is never blocked.
//!   - An admin-approved cooldown bypass lets a voucher skip the window.
//!   - Setting `vouch_cooldown_secs = 0` disables the cooldown entirely.

#![cfg(test)]

use crate::types::{CooldownBypassRequest, DataKey, DEFAULT_VOUCH_COOLDOWN_SECS};
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, Vec,
};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns (contract_id, token_addr, admin, borrower, voucher).
/// The ledger timestamp starts at 1_000_000 so the first vouch clears
/// MIN_VOUCH_AGE (60 s) and so that advancing by DEFAULT_VOUCH_COOLDOWN_SECS
/// stays in clearly positive territory.
fn setup(env: &Env) -> (Address, Address, Address, Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);

    // Fund the contract so yield payouts don't fail later in other tests
    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    // Start ledger well past MIN_VOUCH_AGE
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let borrower = Address::generate(env);
    let voucher = Address::generate(env);
    // Give the voucher enough tokens for multiple vouches in these tests
    StellarAssetClient::new(env, &token_id.address()).mint(&voucher, &20_000_000);

    (contract_id, token_id.address(), admin, borrower, voucher)
}

// ── Test 1: second vouch within cooldown window is rejected ───────────────────

#[test]
fn test_cooldown_rejects_second_vouch_within_window() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);

    // First vouch — should succeed and record LastVouchTimestamp
    client.vouch(&voucher, &borrower1, &1_000_000, &token_addr, &None);

    // Second vouch within the 24-hour window (ledger timestamp unchanged)
    let result = client.try_vouch(&voucher, &borrower2, &1_000_000, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::VouchCooldownActive)),
        "Expected VouchCooldownActive when the voucher re-vouches within the cooldown window"
    );

    // Verify that no partial state was written for borrower2
    env.as_contract(&contract_id, || {
        let vouches: Option<Vec<crate::types::VouchRecord>> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower2.clone()));
        assert!(
            vouches.is_none() || vouches.unwrap().is_empty(),
            "No Vouches entry should exist for borrower2 after a rejected vouch"
        );
    });
}

// ── Test 2: vouch is allowed once the cooldown window has fully elapsed ────────

#[test]
fn test_cooldown_allows_vouch_after_window_elapses() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);

    client.vouch(&voucher, &borrower1, &1_000_000, &token_addr, &None);

    // Advance time by DEFAULT_VOUCH_COOLDOWN_SECS + 1 so the window has passed
    env.ledger()
        .with_mut(|l| l.timestamp += DEFAULT_VOUCH_COOLDOWN_SECS + 1);

    // Should succeed — the cooldown has elapsed
    let result = client.try_vouch(&voucher, &borrower2, &1_000_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "Vouch should be allowed after the cooldown window has elapsed, got: {:?}",
        result
    );
}

// ── Test 3: first-ever vouch (no prior timestamp) is never blocked ────────────

#[test]
fn test_first_ever_vouch_is_not_blocked_by_cooldown() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Verify there is no LastVouchTimestamp for this fresh voucher address
    env.as_contract(&contract_id, || {
        let stored: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::LastVouchTimestamp(voucher.clone()));
        assert!(
            stored.is_none(),
            "New voucher must have no LastVouchTimestamp before their first vouch"
        );
    });

    // The very first vouch must not be blocked regardless of what the cooldown is set to
    let result = client.try_vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "First-ever vouch must never be blocked by cooldown, got: {:?}",
        result
    );
}

// ── Test 4: approved cooldown bypass lets the voucher skip the window ─────────

#[test]
fn test_approved_bypass_allows_vouch_before_cooldown_expires() {
    let env = Env::default();
    let (contract_id, token_addr, admin, _borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);

    // First vouch sets LastVouchTimestamp
    client.vouch(&voucher, &borrower1, &1_000_000, &token_addr, &None);

    // Inject an approved CooldownBypassRequest for (borrower2, voucher) directly.
    // This mirrors what would happen after request_cooldown_bypass + vote_bypass
    // reach the 2/3 admin approval threshold.
    env.as_contract(&contract_id, || {
        let key = DataKey::CooldownBypass(borrower2.clone(), voucher.clone());
        let request = CooldownBypassRequest {
            voucher: voucher.clone(),
            borrower: borrower2.clone(),
            reason: soroban_sdk::String::from_str(&env, "emergency re-vouch"),
            requested_at: env.ledger().timestamp(),
            approvers: Vec::from_array(&env, [admin.clone()]),
            approved: true,
        };
        env.storage().persistent().set(&key, &request);
    });

    // The second vouch should now succeed even though the cooldown has not elapsed
    let result = client.try_vouch(&voucher, &borrower2, &1_000_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "Approved bypass must allow vouching before the cooldown expires, got: {:?}",
        result
    );
}

// ── Test 5: vouch_cooldown_secs = 0 disables cooldown entirely ────────────────

#[test]
fn test_zero_cooldown_secs_disables_cooldown() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Disable the cooldown by writing 0 directly to instance storage
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::VouchCooldownSecs, &0u64);
    });

    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    let borrower3 = Address::generate(&env);

    // Multiple vouches in the same ledger timestamp — all should succeed
    client.vouch(&voucher, &borrower1, &1_000_000, &token_addr, &None);
    client.vouch(&voucher, &borrower2, &1_000_000, &token_addr, &None);
    client.vouch(&voucher, &borrower3, &1_000_000, &token_addr, &None);
}
