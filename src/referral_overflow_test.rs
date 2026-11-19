//! Issue #2061 — checked arithmetic in `distribute_referral_reward`.
//!
//! Verifies that overflow surfaces as `ArithmeticError` rather than being
//! silently capped, and that an overflow error leaves no partial state
//! (reserve, referrer stats, and referral count are all unchanged).

#![cfg(test)]

use crate::referral::distribute_referral_reward;
use crate::types::{DataKey, DEFAULT_REFERRAL_BONUS_BPS};
use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

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

#[test]
fn reward_multiplication_overflow_returns_arithmetic_error() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &i128::MAX);
        assert!(DEFAULT_REFERRAL_BONUS_BPS > 1);

        // i128::MAX * bonus_bps overflows for any bonus_bps > 1.
        let result = distribute_referral_reward(&env, &borrower, &token, i128::MAX);
        assert_eq!(result, Err(ContractError::ArithmeticError));

        // Nothing was written: the reserve is still the original value.
        let reserve: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap();
        assert_eq!(reserve, i128::MAX);
    });
}

#[test]
fn referral_count_overflow_leaves_no_partial_state() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);
    let reserve_before: i128 = 1_000_000;

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &reserve_before);
        env.storage()
            .persistent()
            .set(&DataKey::ReferralCount(referrer.clone()), &u64::MAX);

        let result = distribute_referral_reward(&env, &borrower, &token, 100_000);
        assert_eq!(result, Err(ContractError::ArithmeticError));

        let reserve: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap();
        assert_eq!(reserve, reserve_before, "reserve must not be debited");

        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ReferralCount(referrer.clone()))
            .unwrap();
        assert_eq!(count, u64::MAX, "referral count must not change");
    });
}

#[test]
fn normal_reward_still_succeeds_after_checked_refactor() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let borrower = Address::generate(&env);
    let referrer = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &1_000_000_i128);

        let result = distribute_referral_reward(&env, &borrower, &token, 100_000);
        assert_eq!(result, Ok(()));

        let reserve: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap();
        let expected_reward = 100_000 * DEFAULT_REFERRAL_BONUS_BPS as i128 / 10_000;
        assert_eq!(reserve, 1_000_000 - expected_reward);
    });
}
