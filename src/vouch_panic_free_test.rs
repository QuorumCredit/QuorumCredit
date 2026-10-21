//! Issue #1910 — `src/vouch.rs` production paths return typed errors instead of
//! panicking through `unwrap()` / `expect()`.
//!
//! The replaced calls index into a vector after a successful lookup on that same
//! vector, or inside a loop bounded by `len()`. Through the public API those
//! `get()` calls cannot return `None`, so they cannot be driven to their failure
//! branch directly. These tests therefore cover the reachable typed-error paths
//! of the same entrypoints: a caller that is not a voucher, a borrower with no
//! vouches, and a batch with mismatched inputs. Each must return a
//! `ContractError`, never abort with a generic panic.

#![cfg(test)]

use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

struct Setup {
    env: Env,
    client: QuorumCreditContractClient<'static>,
    token: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let deployer = Address::generate(&env);
    let admin = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin.clone()]);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(&env, &token).mint(&contract_id, &100_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.initialize(&deployer, &admins, &1u32, &token);

    // Start at t=120 so vouches pass the minimum vouch age.
    env.ledger().with_mut(|l| l.timestamp = 120);

    Setup { env, client, token }
}

/// Creates a vouch from a fresh voucher for `borrower` and returns the voucher.
fn vouch_for(s: &Setup, borrower: &Address) -> Address {
    let voucher = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &10_000_000);
    s.client
        .vouch(&voucher, borrower, &1_000_000, &s.token, &None);
    voucher
}

#[test]
fn test_increase_stake_by_non_voucher_returns_voucher_not_found() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    vouch_for(&s, &borrower);

    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client.try_increase_stake(&stranger, &borrower, &1_000),
        Err(Ok(ContractError::VoucherNotFound)),
    );
}

#[test]
fn test_increase_stake_for_borrower_without_vouches_returns_no_vouches() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.client.try_increase_stake(&stranger, &borrower, &1_000),
        Err(Ok(ContractError::NoVouchesForBorrower)),
    );
}

#[test]
fn test_decrease_stake_by_non_voucher_returns_voucher_not_found() {
    let s = setup();
    let borrower = Address::generate(&s.env);
    vouch_for(&s, &borrower);

    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client.try_decrease_stake(&stranger, &borrower, &1_000),
        Err(Ok(ContractError::VoucherNotFound)),
    );
}

#[test]
fn test_batch_vouch_with_mismatched_lengths_returns_typed_error() {
    let s = setup();
    let voucher = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &10_000_000);

    let borrowers = Vec::from_array(
        &s.env,
        [Address::generate(&s.env), Address::generate(&s.env)],
    );
    let stakes = Vec::from_array(&s.env, [1_000_000i128]);

    assert_eq!(
        s.client
            .try_batch_vouch(&voucher, &borrowers, &stakes, &s.token, &None),
        Err(Ok(ContractError::InsufficientFunds)),
    );
}
