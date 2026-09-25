#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Contract, ContractClient};

fn setup() -> (Env, ContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    (env, client, admin)
}

// -------------------------------------------------------------------------
// Compliance requirements (explicit, testable assertions)
//
// CR-1: Only an authorized admin may initialize the contract.
// CR-2: Contract state must be initialized exactly once (no re-init).
// CR-3: Every state-changing operation must emit an audit event.
// CR-4: Audit events must carry the acting address for traceability.
// CR-5: Compliance status must be queryable from contract state.
// -------------------------------------------------------------------------

#[test]
fn compliance_cr1_admin_authorization_required() {
    let (env, client, admin) = setup();

    // Unauthorized caller must not be able to initialize.
    let attacker = Address::generate(&env);
    let result = client.try_initialize(&attacker);
    assert!(result.is_err(), "CR-1: unauthorized init must fail");

    // Authorized admin succeeds.
    client.initialize(&admin);
    assert_eq!(client.get_admin(), admin, "CR-1: admin must be recorded");
}

#[test]
fn compliance_cr2_single_initialization() {
    let (_env, client, admin) = setup();

    client.initialize(&admin);

    // Re-initialization must be rejected to preserve state integrity.
    let result = client.try_initialize(&admin);
    assert!(result.is_err(), "CR-2: re-initialization must fail");
}

#[test]
fn compliance_cr3_state_change_emits_audit_event() {
    let (env, client, admin) = setup();
    client.initialize(&admin);

    let before = env.events().all().len();
    client.set_value(&admin, 42);
    let after = env.events().all().len();

    assert!(
        after > before,
        "CR-3: state-changing operation must emit an audit event"
    );
}

#[test]
fn compliance_cr4_audit_event_records_actor() {
    let (env, client, admin) = setup();
    client.initialize(&admin);

    client.set_value(&admin, 7);

    let events = env.events().all();
    let last = events.last().expect("CR-4: expected an audit event");
    // The audit event must reference the acting address for traceability.
    assert!(
        last.1.contains(&admin),
        "CR-4: audit event must record the acting address"
    );
}

#[test]
fn compliance_cr5_status_is_queryable() {
    let (_env, client, admin) = setup();
    client.initialize(&admin);

    // Compliance status must be observable from contract state.
    assert!(client.is_compliant(), "CR-5: contract must report compliant");
}
