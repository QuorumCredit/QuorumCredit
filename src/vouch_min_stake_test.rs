//! Issue #1419 — Minimum Stake Validation Tests
//!
//! Exercises the `min_stake` and `effective_min_stake` checks in
//! `validate_vouch` (`src/vouch.rs`):
//!   - A vouch below `min_stake` is rejected with `MinStakeNotMet`.
//!   - A vouch at exactly `min_stake` is accepted.
//!   - A vouch above `min_stake` is accepted.
//!   - A borrower in a high credit tier gets a discounted effective_min_stake
//!     via `credit_score::apply_tier_rewards_to_min_stake`.

#![cfg(test)]

use crate::types::{
    CreditScore, CreditTier, DataKey, DEFAULT_CREDIT_SCORE_CONFIG, DEFAULT_VOUCH_COOLDOWN_SECS,
};
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

    // Disable cooldown for these tests so successive vouches aren't blocked
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

// ── Test 1: stake below min_stake is rejected ─────────────────────────────────

#[test]
fn test_stake_below_min_stake_is_rejected() {
    let env = Env::default();
    let (contract_id, token_addr, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Set min_stake to 500_000 stroops
    client.set_min_stake(&admin_signers(&env, &admin), &500_000);

    // Attempt to vouch with stake below min_stake
    let result = client.try_vouch(&voucher, &borrower, &100_000, &token_addr, &None);
    assert_eq!(
        result,
        Err(Ok(ContractError::MinStakeNotMet)),
        "Stake below min_stake must be rejected with MinStakeNotMet"
    );
}

// ── Test 2: stake at exactly min_stake is accepted ────────────────────────────

#[test]
fn test_stake_at_min_stake_is_accepted() {
    let env = Env::default();
    let (contract_id, token_addr, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Set min_stake to 500_000 stroops
    client.set_min_stake(&admin_signers(&env, &admin), &500_000);

    let result = client.try_vouch(&voucher, &borrower, &500_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "Stake equal to min_stake must be accepted, got: {:?}",
        result
    );
}

// ── Test 3: stake above min_stake is accepted ─────────────────────────────────

#[test]
fn test_stake_above_min_stake_is_accepted() {
    let env = Env::default();
    let (contract_id, token_addr, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    client.set_min_stake(&admin_signers(&env, &admin), &500_000);

    let result = client.try_vouch(&voucher, &borrower, &1_000_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "Stake above min_stake must be accepted, got: {:?}",
        result
    );
}

// ── Test 4: rejection leaves no partial state ─────────────────────────────────

#[test]
fn test_min_stake_rejection_leaves_no_state() {
    let env = Env::default();
    let (contract_id, token_addr, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    client.set_min_stake(&admin_signers(&env, &admin), &1_000_000);

    // Record the voucher token balance before the failed attempt
    let tc = soroban_sdk::token::Client::new(&env, &token_addr);
    let balance_before = tc.balance(&voucher);

    let _ = client.try_vouch(&voucher, &borrower, &500_000, &token_addr, &None);

    env.as_contract(&contract_id, || {
        let vouches: Option<Vec<crate::types::VouchRecord>> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()));
        assert!(
            vouches.is_none() || vouches.unwrap().is_empty(),
            "No Vouches entry should exist after a MinStakeNotMet rejection"
        );
    });

    // Token balance must be unchanged — no tokens moved
    assert_eq!(
        tc.balance(&voucher),
        balance_before,
        "Voucher token balance must not change after a MinStakeNotMet rejection"
    );
}

// ── Test 5: tier-discounted effective_min_stake allows a lower stake ──────────

#[test]
fn test_tier_discount_reduces_effective_min_stake() {
    let env = Env::default();
    let (contract_id, token_addr, admin, borrower, voucher) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Set min_stake to 1_000_000 stroops
    let base_min_stake: i128 = 1_000_000;
    client.set_min_stake(&admin_signers(&env, &admin), &base_min_stake);

    // Manually store a credit score for borrower at the "Excellent" tier.
    // DEFAULT_EXCELLENT_REWARDS has min_stake_reduction_bps = 2000 (20%).
    // effective_min_stake = 1_000_000 - 1_000_000 * 2000 / 10_000 = 800_000.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::CreditScoreConfig, &DEFAULT_CREDIT_SCORE_CONFIG);

        let score = CreditScore {
            score: 900,
            tier: CreditTier::Excellent,
            last_updated: env.ledger().timestamp(),
            last_decay_timestamp: env.ledger().timestamp(),
            total_loans: 5,
            successful_repayments: 5,
            defaults: 0,
            total_borrowed: 5_000_000,
            total_repaid: 5_000_000,
            account_age: 365 * 24 * 60 * 60,
            voucher_count: 3,
            avg_repayment_time: 86_400,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(borrower.clone()), &score);
    });

    // Staking 900_000 (between 800_000 and 1_000_000) should now pass the
    // tier-discounted effective_min_stake of 800_000.
    let result = client.try_vouch(&voucher, &borrower, &900_000, &token_addr, &None);
    assert!(
        result.is_ok(),
        "Tier-discounted effective_min_stake should allow staking 900_000 when base is 1_000_000 and discount is 20%, got: {:?}",
        result
    );
}
