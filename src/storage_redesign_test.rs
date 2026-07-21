//! Tests for Issue #1146: paginated storage access, vouch-history archival
//! cutover, withdrawal-queue bounding, and the live `check_invariants` entrypoint.
#![cfg(test)]

use crate::types::{DataKey, QueuedWithdrawal};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

fn setup(env: &Env) -> (Address, Address) {
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &1_000_000_000_000);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());
    (contract_id, token_id.address())
}

#[test]
fn test_get_vouches_page_paginates() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);

    for _ in 0..5 {
        let voucher = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);
        client.vouch(&voucher, &borrower, &1_000_000, &token, &None);
    }

    let (page1, cursor1) = client.get_vouches_page(&borrower, &0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(cursor1, Some(2));

    let (page2, cursor2) = client.get_vouches_page(&borrower, &2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(cursor2, Some(4));

    let (page3, cursor3) = client.get_vouches_page(&borrower, &4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(cursor3, None);

    let (page_empty, cursor_empty) = client.get_vouches_page(&borrower, &5, &2);
    assert_eq!(page_empty.len(), 0);
    assert_eq!(cursor_empty, None);
}

#[test]
fn test_get_voucher_history_page_paginates() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let voucher = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&voucher, &50_000_000);

    for _ in 0..3 {
        let borrower = Address::generate(&env);
        client.vouch(&voucher, &borrower, &1_000_000, &token, &None);
    }

    let (page, cursor) = client.get_voucher_history_page(&voucher, &0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(cursor, Some(2));

    let (page2, cursor2) = client.get_voucher_history_page(&voucher, &2, &2);
    assert_eq!(page2.len(), 1);
    assert_eq!(cursor2, None);
}

#[test]
fn test_withdrawal_queue_cap_enforced() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);
    client.vouch(&voucher, &borrower, &5_000_000, &token, &None);
    StellarAssetClient::new(&env, &token).mint(&borrower, &1_000_000);
    client.request_loan(
        &borrower,
        &1_000_000,
        &5_000_000,
        &soroban_sdk::String::from_str(&env, "test"),
        &token,
    );

    // Pre-fill the queue directly to MAX_WITHDRAWAL_QUEUE_SIZE without needing
    // hundreds of real vouches, then confirm the next real request is rejected.
    env.as_contract(&contract_id, || {
        let mut queue: Vec<QueuedWithdrawal> = Vec::new(&env);
        for i in 0..crate::types::MAX_WITHDRAWAL_QUEUE_SIZE {
            queue.push_back(QueuedWithdrawal {
                voucher: Address::generate(&env),
                token: token.clone(),
                requested_at: i as u64,
                partial: false,
                priority_fee: 0,
            });
        }
        env.storage()
            .persistent()
            .set(&DataKey::WithdrawalQueue(borrower.clone()), &queue);
    });

    let result = client.try_withdraw_vouch(&voucher, &borrower);
    assert!(result.is_err(), "queue at cap should reject new requests");
}

#[test]
fn test_withdrawal_queue_page_bounds_read() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let mut queue: Vec<QueuedWithdrawal> = Vec::new(&env);
        for i in 0..10u32 {
            queue.push_back(QueuedWithdrawal {
                voucher: Address::generate(&env),
                token: token.clone(),
                requested_at: i as u64,
                partial: false,
                priority_fee: 0,
            });
        }
        env.storage()
            .persistent()
            .set(&DataKey::WithdrawalQueue(borrower.clone()), &queue);
    });

    let (page, cursor) = client.get_withdrawal_queue_page(&borrower, &0, &4);
    assert_eq!(page.len(), 4);
    assert_eq!(cursor, Some(4));
}

#[test]
fn test_vouch_history_archival_cutover() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher, &100_000_000);
    client.vouch(&voucher, &borrower, &1_000_000, &token, &None);

    // Each set_vouch_expiry call appends one VouchHistory entry. Combined with
    // the initial "created" entry from vouch(), 29 more calls crosses the
    // VOUCH_HISTORY_ARCHIVE_TRIGGER_ENTRIES (30) threshold and triggers a
    // cutover down to MAX_HOT_VOUCH_HISTORY_ENTRIES (20).
    for i in 1..30u64 {
        client.set_vouch_expiry(&voucher, &borrower, &(1_000_000 + i), &token);
    }

    let hot = client.get_vouch_history(&borrower, &voucher, &token);
    assert_eq!(
        hot.len() as u32,
        crate::types::MAX_HOT_VOUCH_HISTORY_ENTRIES,
        "hot window should be cut back down to the target size"
    );

    let archive_count = client.get_vouch_history_archive_count(&borrower, &voucher, &token);
    assert!(archive_count > 0, "overflow entries should have been archived");

    let archived = client.get_archived_vouch_history_batch(&borrower, &voucher, &token, &0);
    assert!(!archived.is_empty(), "archive batch 0 should contain the oldest entries");
}

#[test]
fn test_borrower_list_page_and_count() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    for _ in 0..3 {
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);
        client.vouch(&voucher, &borrower, &1_000_000, &token, &None);
    }

    assert_eq!(client.get_borrower_count(), 3);
    let (page, cursor) = client.get_borrower_list_page(&0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(cursor, Some(2));
}

#[test]
fn test_check_invariants_passes_on_healthy_state() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);
    client.vouch(&voucher, &borrower, &5_000_000, &token, &None);

    let borrowers = Vec::from_array(&env, [borrower]);
    let result = client.try_check_invariants(&borrowers);
    assert!(result.is_ok(), "healthy state should pass all invariants");
}

#[test]
fn test_check_invariants_fails_on_corrupted_config() {
    let env = Env::default();
    let (contract_id, _token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    env.as_contract(&contract_id, || {
        let mut cfg: crate::types::Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("config");
        cfg.yield_bps = 20_000; // invalid: > 10_000
        env.storage().instance().set(&DataKey::Config, &cfg);
    });

    let result = client.try_check_invariants(&Vec::new(&env));
    assert!(result.is_err(), "corrupted config should fail check_invariants");
}
