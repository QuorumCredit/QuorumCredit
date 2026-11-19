//! Issue #2060 — `record_recurring_payment_failure` caller authorization.
//!
//! Verifies that only the registered keeper or a registered admin may record
//! a failed recurring-payment attempt. A third party must be refused and the
//! borrower's schedule must be left untouched.

#![cfg(test)]

use crate::types::RecurringPaymentConfig;
use crate::{DataKey, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup(env: &Env) -> (Address, QuorumCreditContractClient<'_>, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    (contract_id, client, admin)
}

/// Seed an active recurring-payment schedule for `borrower` directly in storage,
/// so the authorization check is exercised in isolation from loan setup.
fn seed_schedule(env: &Env, contract_id: &Address, borrower: &Address) {
    let token = Address::generate(env);
    let config = RecurringPaymentConfig {
        borrower: borrower.clone(),
        token,
        amount: 1_000,
        frequency_secs: 86_400,
        start_date: 0,
        next_payment_due: 0,
        active: true,
        success_count: 0,
        failure_count: 0,
        retry_count: 0,
    };
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::RecurringPayment(borrower.clone()), &config);
    });
}

fn failure_count(env: &Env, contract_id: &Address, borrower: &Address) -> u32 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get::<DataKey, RecurringPaymentConfig>(&DataKey::RecurringPayment(borrower.clone()))
            .unwrap()
            .failure_count
    })
}

#[test]
fn third_party_cannot_record_recurring_payment_failure() {
    let env = Env::default();
    let (contract_id, client, _admin) = setup(&env);
    let borrower = Address::generate(&env);
    let stranger = Address::generate(&env);
    seed_schedule(&env, &contract_id, &borrower);

    let result = client.try_record_recurring_payment_failure(&stranger, &borrower);
    assert!(result.is_err(), "third party must be refused");
    assert_eq!(failure_count(&env, &contract_id, &borrower), 0);
}

#[test]
fn third_party_refused_even_when_keeper_is_registered() {
    let env = Env::default();
    let (contract_id, client, admin) = setup(&env);
    let borrower = Address::generate(&env);
    let keeper = Address::generate(&env);
    let stranger = Address::generate(&env);
    seed_schedule(&env, &contract_id, &borrower);

    client.set_recurring_payment_keeper(&Vec::from_array(&env, [admin]), &keeper);

    assert!(client
        .try_record_recurring_payment_failure(&stranger, &borrower)
        .is_err());
    assert_eq!(failure_count(&env, &contract_id, &borrower), 0);
}

#[test]
fn registered_keeper_can_record_failure() {
    let env = Env::default();
    let (contract_id, client, admin) = setup(&env);
    let borrower = Address::generate(&env);
    let keeper = Address::generate(&env);
    seed_schedule(&env, &contract_id, &borrower);

    client.set_recurring_payment_keeper(&Vec::from_array(&env, [admin]), &keeper);

    let retries = client.record_recurring_payment_failure(&keeper, &borrower);
    assert_eq!(retries, 1);
    assert_eq!(failure_count(&env, &contract_id, &borrower), 1);
}

#[test]
fn admin_can_record_failure() {
    let env = Env::default();
    let (contract_id, client, admin) = setup(&env);
    let borrower = Address::generate(&env);
    seed_schedule(&env, &contract_id, &borrower);

    let retries = client.record_recurring_payment_failure(&admin, &borrower);
    assert_eq!(retries, 1);
    assert_eq!(failure_count(&env, &contract_id, &borrower), 1);
}

#[test]
fn no_keeper_registered_means_only_admin_can_record() {
    let env = Env::default();
    let (contract_id, client, _admin) = setup(&env);
    let borrower = Address::generate(&env);
    let random = Address::generate(&env);
    seed_schedule(&env, &contract_id, &borrower);

    assert!(client
        .try_record_recurring_payment_failure(&random, &borrower)
        .is_err());
    assert_eq!(failure_count(&env, &contract_id, &borrower), 0);
}
