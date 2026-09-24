use soroban_sdk::{contracttype, Address, Env, Vec};

/// Maximum plausible drift (in seconds) between an event timestamp and the
/// current ledger timestamp before the event is considered forward-dated.
const MAX_FUTURE_DRIFT_SECONDS: u64 = 300;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEvent {
    pub timestamp: u64,
    pub actor: Address,
    pub action: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditVerificationReport {
    pub is_complete: bool,
    pub total_events: u32,
    pub timestamp_regressions: Vec<u64>,
    pub future_timestamp_violations: Vec<u64>,
}

/// Verify the completeness and monotonicity of an audit log.
///
/// Flags two classes of timestamp violations:
/// - regressions: an event whose timestamp is earlier than the previous event
/// - forward-dated events: an event whose timestamp is implausibly far ahead of
///   the current ledger timestamp (likely a caller passing a wrong timestamp
///   instead of `env.ledger().timestamp()`)
pub fn verify_audit_log_completeness(
    env: &Env,
    events: &Vec<AuditEvent>,
) -> AuditVerificationReport {
    let mut timestamp_regressions: Vec<u64> = Vec::new(env);
    let mut future_timestamp_violations: Vec<u64> = Vec::new(env);
    let ledger_timestamp = env.ledger().timestamp();
    let max_allowed = ledger_timestamp.saturating_add(MAX_FUTURE_DRIFT_SECONDS);

    let mut last_timestamp: Option<u64> = None;
    let mut is_complete = true;

    for event in events.iter() {
        if let Some(prev) = last_timestamp {
            if event.timestamp < prev {
                timestamp_regressions.push_back(event.timestamp);
                is_complete = false;
            }
        }

        if event.timestamp > max_allowed {
            future_timestamp_violations.push_back(event.timestamp);
            is_complete = false;
        }

        last_timestamp = Some(event.timestamp);
    }

    AuditVerificationReport {
        is_complete,
        total_events: events.len(),
        timestamp_regressions,
        future_timestamp_violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{Address, Env, Vec};

    fn make_event(env: &Env, timestamp: u64) -> AuditEvent {
        AuditEvent {
            timestamp,
            actor: Address::generate(env),
            action: 1,
        }
    }

    #[test]
    fn flags_forward_dated_event() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let mut events: Vec<AuditEvent> = Vec::new(&env);
        events.push_back(make_event(&env, 900));
        events.push_back(make_event(&env, 1_000_000));

        let report = verify_audit_log_completeness(&env, &events);

        assert!(!report.is_complete);
        assert_eq!(report.future_timestamp_violations.len(), 1);
        assert_eq!(report.future_timestamp_violations.get(0).unwrap(), 1_000_000);
        assert_eq!(report.timestamp_regressions.len(), 0);
    }

    #[test]
    fn accepts_events_within_plausible_drift() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let mut events: Vec<AuditEvent> = Vec::new(&env);
        events.push_back(make_event(&env, 900));
        events.push_back(make_event(&env, 1_000));

        let report = verify_audit_log_completeness(&env, &events);

        assert!(report.is_complete);
        assert_eq!(report.future_timestamp_violations.len(), 0);
    }

    #[test]
    fn still_flags_timestamp_regressions() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);

        let mut events: Vec<AuditEvent> = Vec::new(&env);
        events.push_back(make_event(&env, 900));
        events.push_back(make_event(&env, 800));

        let report = verify_audit_log_completeness(&env, &events);

        assert!(!report.is_complete);
        assert_eq!(report.timestamp_regressions.len(), 1);
        assert_eq!(report.timestamp_regressions.get(0).unwrap(), 800);
        assert_eq!(report.future_timestamp_violations.len(), 0);
    }
}
