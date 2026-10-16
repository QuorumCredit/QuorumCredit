//! Issue #1796: tests for credential transactional batching.

use crate::audit_trail_compression::{
    append_credential_audit_entry, hot_entry_count, MAX_AUDIT_EVENT_BYTES, MAX_HOT_AUDIT_ENTRIES,
};
use crate::credential_batch::*;
use crate::errors::ContractError;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

#[contract]
pub struct BatchDummy;

#[contractimpl]
impl BatchDummy {
    pub fn noop() {}
}

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(BatchDummy, ());
    (env, contract_id)
}

fn append_op(env: &Env, holder: &Address, event: &str) -> BatchOperation {
    BatchOperation::AppendAuditEntry(holder.clone(), String::from_str(env, event))
}

#[test]
fn execute_applies_every_operation() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        env.ledger().set_timestamp(1_000);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let ops = Vec::from_array(&env, [append_op(&env, &a, "one"), append_op(&env, &b, "two")]);

        let id = create_batch_operation(&env, ops).unwrap();
        assert_eq!(id, 1);
        assert_eq!(get_batch_status(&env, id).unwrap(), BatchStatus::Pending);
        assert_eq!(hot_entry_count(&env, &a), 0, "creation must not write");

        execute_batch_operation(&env, id).unwrap();
        assert_eq!(hot_entry_count(&env, &a), 1);
        assert_eq!(hot_entry_count(&env, &b), 1);

        let batch = get_batch_operation(&env, id).unwrap();
        assert_eq!(batch.status, BatchStatus::Executed);
        assert_eq!(batch.executed_at, 1_000);
    });
}

#[test]
fn failed_validation_writes_nothing() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        let b = Address::generate(&env);

        // Fill b's trail so that one more append for b cannot fit.
        for _ in 0..MAX_HOT_AUDIT_ENTRIES {
            env.cost_estimate().budget().reset_default();
            append_credential_audit_entry(&env, &b, String::from_str(&env, "e")).unwrap();
        }
        env.cost_estimate().budget().reset_default();

        let ops = Vec::from_array(&env, [append_op(&env, &a, "first"), append_op(&env, &b, "over")]);
        let id = create_batch_operation(&env, ops).unwrap();

        assert_eq!(
            execute_batch_operation(&env, id),
            Err(ContractError::AuditTrailFull)
        );
        // Phase 1 caught the failure before any write, so a's trail is untouched.
        assert_eq!(hot_entry_count(&env, &a), 0);
        assert_eq!(get_batch_status(&env, id).unwrap(), BatchStatus::Pending);
    });
}

#[test]
fn same_holder_capacity_counts_all_pending_appends() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        for _ in 0..(MAX_HOT_AUDIT_ENTRIES - 1) {
            env.cost_estimate().budget().reset_default();
            append_credential_audit_entry(&env, &a, String::from_str(&env, "e")).unwrap();
        }
        env.cost_estimate().budget().reset_default();

        // One slot is free, but the batch wants two appends for the same holder.
        let ops = Vec::from_array(&env, [append_op(&env, &a, "x"), append_op(&env, &a, "y")]);
        let id = create_batch_operation(&env, ops).unwrap();
        assert_eq!(
            execute_batch_operation(&env, id),
            Err(ContractError::AuditTrailFull)
        );
        assert_eq!(hot_entry_count(&env, &a), MAX_HOT_AUDIT_ENTRIES - 1);
    });
}

#[test]
fn batch_cannot_run_twice() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        let id = create_batch_operation(&env, Vec::from_array(&env, [append_op(&env, &a, "once")]))
            .unwrap();
        execute_batch_operation(&env, id).unwrap();
        assert_eq!(
            execute_batch_operation(&env, id),
            Err(ContractError::BatchAlreadyExecuted)
        );
        assert_eq!(hot_entry_count(&env, &a), 1);
    });
}

#[test]
fn create_rejects_empty_oversized_and_invalid_batches() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        assert_eq!(
            create_batch_operation(&env, Vec::new(&env)),
            Err(ContractError::EmptyBatch)
        );

        let mut many = Vec::new(&env);
        for _ in 0..=MAX_BATCH_OPERATIONS {
            many.push_back(append_op(&env, &a, "e"));
        }
        assert_eq!(
            create_batch_operation(&env, many),
            Err(ContractError::BatchTooLarge)
        );

        let big = [b'z'; (MAX_AUDIT_EVENT_BYTES + 1) as usize];
        let bad = Vec::from_array(
            &env,
            [BatchOperation::AppendAuditEntry(a.clone(), String::from_bytes(&env, &big))],
        );
        assert_eq!(
            create_batch_operation(&env, bad),
            Err(ContractError::InvalidAuditEvent)
        );
    });
}

#[test]
fn unknown_batch_reports_not_found() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        assert_eq!(
            execute_batch_operation(&env, 42),
            Err(ContractError::BatchNotFound)
        );
        assert_eq!(get_batch_status(&env, 42), Err(ContractError::BatchNotFound));
    });
}

#[test]
fn ids_increase_per_batch() {
    let (env, cid) = setup();
    env.as_contract(&cid, || {
        let a = Address::generate(&env);
        let first = create_batch_operation(&env, Vec::from_array(&env, [append_op(&env, &a, "1")]))
            .unwrap();
        let second =
            create_batch_operation(&env, Vec::from_array(&env, [append_op(&env, &a, "2")]))
                .unwrap();
        assert_eq!(second, first + 1);
    });
}
