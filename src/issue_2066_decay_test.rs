//! Issue #2066: reputation decay must clamp and convert safely instead of wrapping.

#![cfg(test)]

use crate::credit_score::apply_reputation_decay;
use crate::types::{Config, CreditScore, CreditTier, DataKey};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::{Address as _, Ledger}, token::StellarAssetClient, Address, Env, Vec};

const SECS_PER_MONTH: u64 = 30 * 24 * 60 * 60;

fn initialized_contract(env: &Env) -> Address {
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token = env.register_stellar_asset_contract_v2(admin).address();
    let contract_id = env.register(QuorumCreditContract, ());
    StellarAssetClient::new(env, &token).mint(&contract_id, &1_000_000);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1u32, &token);
    contract_id
}

fn set_decay_and_score(env: &Env, contract_id: &Address, borrower: &Address, decay_per_month: u32, score: u32) {
    env.as_contract(contract_id, || {
        let mut cfg: Config = env.storage().instance().get(&DataKey::Config).expect("config");
        cfg.score_decay_per_month = decay_per_month;
        env.storage().instance().set(&DataKey::Config, &cfg);

        let record = CreditScore {
            score,
            tier: CreditTier::Good,
            last_updated: 0,
            last_decay_timestamp: 0,
            total_loans: 0,
            successful_repayments: 0,
            defaults: 0,
            total_borrowed: 0,
            total_repaid: 0,
            account_age: 0,
            voucher_count: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(borrower.clone()), &record);
    });
}

fn read_score(env: &Env, contract_id: &Address, borrower: &Address) -> u32 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get::<_, CreditScore>(&DataKey::CreditScore(borrower.clone()))
            .expect("score")
            .score
    })
}

#[test]
fn test_decay_with_maximum_configured_rate_clamps_to_zero_without_wrapping() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = initialized_contract(&env);
    let borrower = Address::generate(&env);

    // u32::MAX bps per month, one month elapsed. The raw product would exceed 10,000 bps.
    set_decay_and_score(&env, &contract_id, &borrower, u32::MAX, 700);
    env.ledger().with_mut(|l| l.timestamp = 120 + SECS_PER_MONTH);

    let result = env.as_contract(&contract_id, || apply_reputation_decay(&env, &borrower));
    assert!(result.is_ok(), "decay must succeed and clamp, not error or wrap");

    // Clamped to 100%: the score drops to zero and must not jump upward.
    assert_eq!(read_score(&env, &contract_id, &borrower), 0);
}

#[test]
fn test_decay_normal_rate_is_unchanged() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = initialized_contract(&env);
    let borrower = Address::generate(&env);

    // 100 bps per month on a score of 700, one month elapsed: 700 * 9900 / 10000 = 693.
    set_decay_and_score(&env, &contract_id, &borrower, 100, 700);
    env.ledger().with_mut(|l| l.timestamp = 120 + SECS_PER_MONTH);

    let result = env.as_contract(&contract_id, || apply_reputation_decay(&env, &borrower));
    assert!(result.is_ok());
    assert_eq!(read_score(&env, &contract_id, &borrower), 693);
}
