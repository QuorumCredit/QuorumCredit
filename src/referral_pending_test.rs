//! Issue #2062 — skipped referral rewards are recorded and claimable.
//!
//! Verifies that when the yield reserve cannot cover a referral reward:
//!   - a `referral/skipped` event is emitted with the amounts,
//!   - the unpaid amount is recorded as a pending reward,
//!   - the pending reward can be claimed once the reserve is replenished,
//!   - a claim while the reserve is still short fails and keeps the balance.

#![cfg(test)]

use crate::referral::distribute_referral_reward;
use crate::types::{DataKey, DEFAULT_REFERRAL_BONUS_BPS};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    symbol_short, testutils::Address as _, token::StellarAssetClient, Address, Env, Symbol,
    TryFromVal, Vec,
};

fn setup(env: &Env) -> (Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);

    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &100_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    (contract_id, token_id.address())
}

fn expected_reward(interest: i128) -> i128 {
    interest * DEFAULT_REFERRAL_BONUS_BPS as i128 / 10_000
}

#[test]
fn short_reserve_emits_skipped_event_with_amounts() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);
    let interest: i128 = 1_000_000;
    let reward = expected_reward(interest);
    let reserve: i128 = reward - 1;

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &reserve);

        distribute_referral_reward(&env, &borrower, &token, interest).unwrap();
    });

    let events = env.events().all();
    let (_, topics, _) = events.last().unwrap();
    let kind: Symbol = Symbol::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(kind, symbol_short!("skipped"));
}

#[test]
fn short_reserve_records_pending_reward_and_pays_nothing() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);
    let interest: i128 = 1_000_000;
    let reward = expected_reward(interest);
    let reserve: i128 = reward - 1;

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &reserve);

        distribute_referral_reward(&env, &borrower, &token, interest).unwrap();

        let reserve_after: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap();
        assert_eq!(reserve_after, reserve, "reserve must not be debited");
    });

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    assert_eq!(client.get_pending_referral_reward(&referrer), reward);
}

#[test]
fn pending_reward_claimable_once_reserve_covers_it() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);
    let interest: i128 = 1_000_000;
    let reward = expected_reward(interest);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &(reward - 1));
        distribute_referral_reward(&env, &borrower, &token, interest).unwrap();
        // Replenish the reserve.
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &(reward * 2));
    });

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let paid = client.claim_pending_referral_reward(&referrer, &token);
    assert_eq!(paid, reward);
    assert_eq!(client.get_pending_referral_reward(&referrer), 0);
}

#[test]
fn claim_fails_and_keeps_balance_while_reserve_still_short() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);
    let interest: i128 = 1_000_000;
    let reward = expected_reward(interest);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &0_i128);
        distribute_referral_reward(&env, &borrower, &token, interest).unwrap();
    });

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let result = client.try_claim_pending_referral_reward(&referrer, &token);
    assert!(result.is_err());
    assert_eq!(client.get_pending_referral_reward(&referrer), reward);
}
