//! Issue #2063 — checked addition in `deposit_to_treasury`.
//!
//! Verifies that a deposit which would overflow the treasury balance returns
//! `ArithmeticError` and leaves the stored balance unchanged, and that a normal
//! deposit still succeeds.

#![cfg(test)]

use crate::community_treasury::{deposit_to_treasury, get_treasury_balance};
use crate::types::DataKey;
use crate::{ContractError, QuorumCreditContract};
use soroban_sdk::Env;

#[test]
fn deposit_overflowing_treasury_returns_arithmetic_error() {
    let env = Env::default();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    let near_max = i128::MAX - 10;

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TreasuryBalance, &near_max);

        let result = deposit_to_treasury(&env, 100);
        assert_eq!(result, Err(ContractError::ArithmeticError));

        assert_eq!(
            get_treasury_balance(&env),
            near_max,
            "balance must not change on overflow"
        );
    });
}

#[test]
fn deposit_at_exact_max_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, QuorumCreditContract);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TreasuryBalance, &(i128::MAX - 100));

        assert_eq!(deposit_to_treasury(&env, 100), Ok(()));
        assert_eq!(get_treasury_balance(&env), i128::MAX);
    });
}

#[test]
fn normal_deposit_updates_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, QuorumCreditContract);

    env.as_contract(&contract_id, || {
        deposit_to_treasury(&env, 1_000).unwrap();
        deposit_to_treasury(&env, 500).unwrap();
        assert_eq!(get_treasury_balance(&env), 1_500);
    });
}
