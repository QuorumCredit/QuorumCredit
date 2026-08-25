//! Issue #1179: Vouch Audit Trail.
//!
//! Records a bounded, append-only audit trail of `VouchAuditEvent`s for each
//! (borrower, voucher, token) vouch relationship — creation, stake increases,
//! stake decreases, and withdrawals — and formats it for the read-side
//! entrypoints in `lib.rs` (`get_vouch_audit_trail`, `get_vouch_audit_trail_page`,
//! `export_vouch_audit_report`).
//!
//! The hot window is bounded and archived using the same cutover strategy as
//! `VouchHistory` in `vouch.rs` (Issue #1146): once the hot window reaches
//! `VOUCH_AUDIT_TRAIL_ARCHIVE_TRIGGER_ENTRIES`, the oldest entries are moved
//! into an `ArchivedVouchAuditTrail` batch, bringing the hot window back down
//! to `MAX_HOT_VOUCH_AUDIT_TRAIL_ENTRIES`.

extern crate alloc;

use soroban_sdk::{Address, Env, String as SorobanString, Vec};

use crate::errors::ContractError;
use crate::helpers::paginate_vec;
use crate::types::{
    DataKey, VouchAuditEvent, VouchAuditEventType, MAX_HOT_VOUCH_AUDIT_TRAIL_ENTRIES,
    VOUCH_AUDIT_TRAIL_ARCHIVE_TRIGGER_ENTRIES,
};

/// Append a new audit event to the (borrower, voucher, token) hot window,
/// cutting the oldest entries over into an archive batch once the window
/// reaches `VOUCH_AUDIT_TRAIL_ARCHIVE_TRIGGER_ENTRIES`.
pub fn log_vouch_audit_event(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
    event_type: VouchAuditEventType,
    amount: i128,
    resulting_stake: i128,
) -> Result<(), ContractError> {
    let key = DataKey::VouchAuditTrail(borrower.clone(), voucher.clone(), token.clone());
    let mut trail: Vec<VouchAuditEvent> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));

    trail.push_back(VouchAuditEvent {
        event_type,
        timestamp: env.ledger().timestamp(),
        amount,
        resulting_stake,
    });

    if trail.len() >= VOUCH_AUDIT_TRAIL_ARCHIVE_TRIGGER_ENTRIES {
        let overflow = trail.len() - MAX_HOT_VOUCH_AUDIT_TRAIL_ENTRIES;
        let mut archived_batch: Vec<VouchAuditEvent> = Vec::new(env);
        for _ in 0..overflow {
            archived_batch.push_back(trail.get(0).unwrap());
            trail.remove(0);
        }

        let count_key =
            DataKey::VouchAuditTrailArchiveCount(borrower.clone(), voucher.clone(), token.clone());
        let batch_id: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        env.storage().persistent().set(
            &DataKey::ArchivedVouchAuditTrail(
                borrower.clone(),
                voucher.clone(),
                token.clone(),
                batch_id,
            ),
            &archived_batch,
        );
        env.storage().persistent().set(&count_key, &(batch_id + 1));
    }

    env.storage().persistent().set(&key, &trail);
    Ok(())
}

/// Read the bounded "hot" audit-trail window for (borrower, voucher, token).
pub fn get_vouch_audit_trail_events(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
) -> Vec<VouchAuditEvent> {
    env.storage()
        .persistent()
        .get(&DataKey::VouchAuditTrail(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
        ))
        .unwrap_or(Vec::new(env))
}

/// Number of archive batches created so far for this relationship's audit trail.
pub fn get_vouch_audit_trail_archive_count(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::VouchAuditTrailArchiveCount(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
        ))
        .unwrap_or(0)
}

/// Read one archived audit-trail batch. `batch_id` ranges over
/// `0..get_vouch_audit_trail_archive_count(...)`, oldest batch first.
pub fn get_archived_vouch_audit_trail_batch(
    env: &Env,
    borrower: &Address,
    voucher: &Address,
    token: &Address,
    batch_id: u32,
) -> Vec<VouchAuditEvent> {
    env.storage()
        .persistent()
        .get(&DataKey::ArchivedVouchAuditTrail(
            borrower.clone(),
            voucher.clone(),
            token.clone(),
            batch_id,
        ))
        .unwrap_or(Vec::new(env))
}

fn event_type_label(event_type: &VouchAuditEventType) -> &'static str {
    match event_type {
        VouchAuditEventType::Created => "created",
        VouchAuditEventType::StakeIncreased => "stake_increased",
        VouchAuditEventType::StakeDecreased => "stake_decreased",
        VouchAuditEventType::Withdrawn => "withdrawn",
    }
}

/// Format a single audit event as a human-readable line, e.g.:
/// `"[1700000000] created amount=1000000 resulting_stake=1000000"`.
pub fn format_audit_event(env: &Env, event: &VouchAuditEvent) -> SorobanString {
    let line = alloc::format!(
        "[{}] {} amount={} resulting_stake={}",
        event.timestamp,
        event_type_label(&event.event_type),
        event.amount,
        event.resulting_stake,
    );
    SorobanString::from_slice(env, &line)
}

/// Format the entire hot-window audit trail for a vouch as a single
/// newline-separated `String`, oldest event first.
pub fn format_audit_trail(env: &Env, events: &Vec<VouchAuditEvent>) -> SorobanString {
    let mut report = alloc::string::String::new();
    for (i, event) in events.iter().enumerate() {
        if i > 0 {
            report.push('\n');
        }
        report.push_str(&alloc::format!(
            "[{}] {} amount={} resulting_stake={}",
            event.timestamp,
            event_type_label(&event.event_type),
            event.amount,
            event.resulting_stake,
        ));
    }
    SorobanString::from_slice(env, &report)
}

/// Format a compliance/transparency report covering the full hot-window
/// audit trail for a (borrower, voucher, token) vouch relationship.
pub fn format_audit_report(env: &Env, events: &Vec<VouchAuditEvent>) -> SorobanString {
    let mut report = alloc::string::String::new();
    report.push_str("Vouch Audit Trail Report\n");
    report.push_str(&alloc::format!("Total events: {}\n", events.len()));
    for event in events.iter() {
        report.push_str(&alloc::format!(
            "[{}] {} amount={} resulting_stake={}\n",
            event.timestamp,
            event_type_label(&event.event_type),
            event.amount,
            event.resulting_stake,
        ));
    }
    SorobanString::from_slice(env, &report)
}

/// Paginated read of the hot-window audit trail, formatted one event per
/// `String`. Bounds the read to `[offset, offset+limit)` instead of
/// formatting the full window.
pub fn get_vouch_audit_trail_page_formatted(
    env: &Env,
    events: &Vec<VouchAuditEvent>,
    offset: u32,
    limit: u32,
) -> Vec<SorobanString> {
    let (page, _cursor) = paginate_vec(env, events, offset, limit);
    let mut formatted: Vec<SorobanString> = Vec::new(env);
    for event in page.iter() {
        formatted.push_back(format_audit_event(env, &event));
    }
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup_contract(env: &Env) -> Address {
        env.mock_all_auths();
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        contract_id
    }

    #[test]
    fn append_and_read_hot_window() {
        let env = Env::default();
        let contract_id = setup_contract(&env);
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract_id, || {
            log_vouch_audit_event(
                &env,
                &borrower,
                &voucher,
                &token,
                VouchAuditEventType::Created,
                1_000,
                1_000,
            )
            .unwrap();

            let events = get_vouch_audit_trail_events(&env, &borrower, &voucher, &token);
            assert_eq!(events.len(), 1);
            assert_eq!(events.get(0).unwrap().event_type, VouchAuditEventType::Created);
        });
    }
}
