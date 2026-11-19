//! Tests for the staking pool yield and unstake accounting (issues #2056-#2058).

#![cfg(test)]

use crate::types::{DataKey, StakingPool};
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, Env, Vec,
};

const DAY: u64 = 24 * 60 * 60;

/// Deploys and initializes the contract with one admin (threshold 1) and a
/// stellar asset token. Returns `(env, contract_id, admin, token)`.
fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let deployer = Address::generate(&env);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, QuorumCreditContract);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.initialize(
        &deployer,
        &Vec::from_array(&env, [admin.clone()]),
        &1u32,
        &token,
    );

    (env, contract_id, admin, token)
}

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| li.timestamp = timestamp);
}

fn set_yield_reserve(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &amount);
    });
}

fn yield_reserve(env: &Env, contract_id: &Address) -> i128 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap_or(0)
    })
}

// ── #2056: claim_yield must not reset the pending-unstake timer ──────────────

#[test]
fn test_claim_yield_does_not_delay_queued_unstake() {
    let (env, contract_id, admin, token) = setup();
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let signers = Vec::from_array(&env, [admin.clone()]);

    let pool_id = client.create_staking_pool(&signers, &token);

    let staker = Address::generate(&env);
    let stake: i128 = 1_000;
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&staker, &stake);
    // The contract must hold enough tokens to pay out the yield below.
    asset.mint(&contract_id, &100);

    let t0: u64 = 1_000_000;
    set_time(&env, t0);
    client.stake_capital(&pool_id, &staker, &stake);

    set_yield_reserve(&env, &contract_id, 10_000);
    client.distribute_yield(&signers, &pool_id, &100);

    // Queue the full exit at t0.
    client.queue_unstake(&pool_id, &staker, &stake);

    // Halfway through the delay the staker claims yield.
    set_time(&env, t0 + DAY / 2);
    let claimed = client.claim_yield(&pool_id, &staker);
    assert_eq!(claimed, 100);

    // 24h after the queue time the withdrawal must still be processable.
    set_time(&env, t0 + DAY);
    let returned = client.process_unstake(&pool_id, &staker);
    assert_eq!(returned, stake);
}

// ── #2057: rounding remainder must not be debited from the reserve ───────────

#[test]
fn test_distribute_yield_reserve_debit_matches_harvested_rewards() {
    let (env, contract_id, admin, token) = setup();
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let signers = Vec::from_array(&env, [admin.clone()]);

    let pool_id = client.create_staking_pool(&signers, &token);

    // Large total stake so the per-token delta is not exact.
    let staker = Address::generate(&env);
    let stake: i128 = 1_000_000_000_000_000_007;
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&staker, &stake);
    set_time(&env, 1_000_000);
    client.stake_capital(&pool_id, &staker, &stake);

    let reserve_before: i128 = 10_000_000;
    set_yield_reserve(&env, &contract_id, reserve_before);

    // delta = floor(1e7 * 1e12 / stake) = 9, which only distributes 9_000_000.
    let yield_amount: i128 = 10_000_000;
    let expected_harvest: i128 = 9_000_000;
    client.distribute_yield(&signers, &pool_id, &yield_amount);

    // Only what stakers can harvest leaves the reserve; the remainder stays.
    assert_eq!(
        yield_reserve(&env, &contract_id),
        reserve_before - expected_harvest
    );
    assert_eq!(
        client.get_staking_pool(&pool_id).total_yield_distributed,
        expected_harvest
    );

    // The staker's harvested reward equals the reserve debit.
    asset.mint(&contract_id, &expected_harvest);
    let claimed = client.claim_yield(&pool_id, &staker);
    assert_eq!(claimed, expected_harvest);
}

#[test]
fn test_distribute_yield_below_precision_refunds_whole_yield() {
    let (env, contract_id, admin, token) = setup();
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let signers = Vec::from_array(&env, [admin.clone()]);

    let pool_id = client.create_staking_pool(&signers, &token);

    let staker = Address::generate(&env);
    let stake: i128 = 1_000_000_000_000_000_007;
    StellarAssetClient::new(&env, &token).mint(&staker, &stake);
    set_time(&env, 1_000_000);
    client.stake_capital(&pool_id, &staker, &stake);

    let reserve_before: i128 = 10_000;
    set_yield_reserve(&env, &contract_id, reserve_before);

    // 1_000 * 1e12 / stake rounds to a delta of zero, so nothing is distributed.
    client.distribute_yield(&signers, &pool_id, &1_000);

    assert_eq!(yield_reserve(&env, &contract_id), reserve_before);
    assert_eq!(
        client.get_staking_pool(&pool_id).total_yield_distributed,
        0
    );
}

// ── #2058: accumulator overflow must be reported, not saturated ──────────────

fn set_accumulator(env: &Env, contract_id: &Address, pool_id: u64, value: i128) {
    env.as_contract(contract_id, || {
        let mut pool: StakingPool = env
            .storage()
            .persistent()
            .get(&DataKey::StakingPool(pool_id))
            .unwrap();
        pool.yield_per_token_scaled = value;
        env.storage()
            .persistent()
            .set(&DataKey::StakingPool(pool_id), &pool);
    });
}

#[test]
fn test_distribute_yield_at_accumulator_max_returns_overflow() {
    let (env, contract_id, admin, token) = setup();
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let signers = Vec::from_array(&env, [admin.clone()]);

    let pool_id = client.create_staking_pool(&signers, &token);

    let staker = Address::generate(&env);
    let stake: i128 = 1_000;
    StellarAssetClient::new(&env, &token).mint(&staker, &stake);
    set_time(&env, 1_000_000);
    client.stake_capital(&pool_id, &staker, &stake);

    let reserve_before: i128 = 10_000;
    set_yield_reserve(&env, &contract_id, reserve_before);
    set_accumulator(&env, &contract_id, pool_id, i128::MAX - 1);

    // delta = 10 * 1e12 / 1000 = 1e10, which pushes the accumulator past i128::MAX.
    let result = client.try_distribute_yield(&signers, &pool_id, &10);
    assert!(matches!(result, Err(Ok(ContractError::ArithmeticOverflow))));

    // A failed distribution must not debit the reserve.
    assert_eq!(yield_reserve(&env, &contract_id), reserve_before);
    assert_eq!(
        client.get_staking_pool(&pool_id).yield_per_token_scaled,
        i128::MAX - 1
    );
}

#[test]
fn test_apply_loss_at_accumulator_min_returns_overflow() {
    let (env, contract_id, admin, token) = setup();
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let signers = Vec::from_array(&env, [admin.clone()]);

    let pool_id = client.create_staking_pool(&signers, &token);

    let staker = Address::generate(&env);
    let stake: i128 = 1_000;
    StellarAssetClient::new(&env, &token).mint(&staker, &stake);
    set_time(&env, 1_000_000);
    client.stake_capital(&pool_id, &staker, &stake);

    set_accumulator(&env, &contract_id, pool_id, i128::MIN + 1);

    // loss_delta = 10 * 1e12 / 1000 = 1e10, which underflows the accumulator.
    let result = client.try_apply_staking_pool_loss(&signers, &pool_id, &10);
    assert!(matches!(result, Err(Ok(ContractError::ArithmeticOverflow))));

    // The pool's stake must be unchanged after the rejected loss.
    assert_eq!(client.get_staking_pool(&pool_id).total_staked, stake);
}
