//! Tests for Issue #1179: Vouch Audit Trail write-side logging (`vouch.rs`)
//! and read-side entrypoints (`lib.rs`: `get_vouch_audit_trail`,
//! `get_vouch_audit_trail_page`).
#![cfg(test)]

use crate::types::{VouchAuditEvent, VouchAuditEventType};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

fn setup(env: &Env) -> (Address, Address) {
    env.mock_all_auths();
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

/// Vouch → increase → decrease → withdraw should produce, in order, an audit
/// trail of Created / StakeIncreased / StakeDecreased / Withdrawn events.
#[test]
fn test_vouch_audit_trail_reflects_events_in_order() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

    client.vouch(&voucher, &borrower, &1_000_000, &token, &None);
    client.increase_stake(&voucher, &borrower, &500_000);
    client.decrease_stake(&voucher, &borrower, &300_000);
    client.withdraw_vouch(&voucher, &borrower);

    let timestamp = env.ledger().timestamp();
    let mut expected_events: Vec<VouchAuditEvent> = Vec::new(&env);
    expected_events.push_back(VouchAuditEvent {
        event_type: VouchAuditEventType::Created,
        timestamp,
        amount: 1_000_000,
        resulting_stake: 1_000_000,
    });
    expected_events.push_back(VouchAuditEvent {
        event_type: VouchAuditEventType::StakeIncreased,
        timestamp,
        amount: 500_000,
        resulting_stake: 1_500_000,
    });
    expected_events.push_back(VouchAuditEvent {
        event_type: VouchAuditEventType::StakeDecreased,
        timestamp,
        amount: 300_000,
        resulting_stake: 1_200_000,
    });
    expected_events.push_back(VouchAuditEvent {
        event_type: VouchAuditEventType::Withdrawn,
        timestamp,
        amount: 1_200_000,
        resulting_stake: 0,
    });
    let expected_trail = crate::audit::format_audit_trail(&env, &expected_events);

    let actual_trail = client.get_vouch_audit_trail(&borrower, &voucher, &token);
    assert_eq!(actual_trail, expected_trail);

    // The withdrawal removes the vouch record itself, but the audit trail
    // must remain readable — it isn't derived from the live Vouches list.
    assert!(!client.vouch_exists(&voucher, &borrower));

    let report = client.export_vouch_audit_report(&borrower, &voucher, &token);
    let expected_report = crate::audit::format_audit_report(&env, &expected_events);
    assert_eq!(report, expected_report);
}

/// `get_vouch_audit_trail_page` should return the correct slice for a
/// history longer than one page.
#[test]
fn test_vouch_audit_trail_page_pagination() {
    let env = Env::default();
    let (contract_id, token) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

    client.vouch(&voucher, &borrower, &1_000_000, &token, &None);
    let mut resulting_stake = 1_000_000i128;
    for _ in 0..5 {
        client.increase_stake(&voucher, &borrower, &100_000);
        resulting_stake += 100_000;
    }

    // 6 total events: 1 Created + 5 StakeIncreased.
    let timestamp = env.ledger().timestamp();
    let mut all_events: Vec<VouchAuditEvent> = Vec::new(&env);
    all_events.push_back(VouchAuditEvent {
        event_type: VouchAuditEventType::Created,
        timestamp,
        amount: 1_000_000,
        resulting_stake: 1_000_000,
    });
    let mut running_stake = 1_000_000i128;
    for _ in 0..5 {
        running_stake += 100_000;
        all_events.push_back(VouchAuditEvent {
            event_type: VouchAuditEventType::StakeIncreased,
            timestamp,
            amount: 100_000,
            resulting_stake: running_stake,
        });
    }
    assert_eq!(running_stake, resulting_stake);

    let page1 = client.get_vouch_audit_trail_page(&borrower, &voucher, &token, &0, &4);
    assert_eq!(page1.len(), 4);
    for i in 0..4u32 {
        let expected = crate::audit::format_audit_event(&env, &all_events.get(i).unwrap());
        assert_eq!(page1.get(i).unwrap(), expected);
    }

    let page2 = client.get_vouch_audit_trail_page(&borrower, &voucher, &token, &4, &4);
    assert_eq!(page2.len(), 2);
    for i in 0..2u32 {
        let expected = crate::audit::format_audit_event(&env, &all_events.get(4 + i).unwrap());
        assert_eq!(page2.get(i).unwrap(), expected);
    }
}
