use crate::db::Event;
use prometheus::{
    core::Collector,
    Counter, CounterVec, Gauge, Opts, Registry,
};
use std::collections::HashMap;

pub struct IndexerMetrics {
    pub ledger_height: Gauge,
    pub events_total: CounterVec,
    pub loan_volume_total: CounterVec,
    pub loan_count_total: Counter,
    pub active_loans: Gauge,
    pub slash_events_total: Counter,
    pub slash_amount_total: CounterVec,
    pub vouch_count: Gauge,
    pub gaps_detected: Counter,
    pub reorgs_detected: Counter,
    pub errors_total: CounterVec,
    pub backfill_events_total: Counter,
    pub registry: Registry,
}

fn register<C: Collector + Clone>(registry: &Registry, c: C) -> C {
    registry
        .register(Box::new(c.clone()))
        .expect("metric registration failed");
    c
}

impl IndexerMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let ledger_height = register(
            &registry,
            Gauge::new("qc_indexer_ledger_height", "Last processed ledger sequence").unwrap(),
        );

        let events_total = register(
            &registry,
            CounterVec::new(
                Opts::new("qc_indexer_events_total", "Total events indexed"),
                &["category", "action"],
            )
            .unwrap(),
        );

        let loan_volume_total = register(
            &registry,
            CounterVec::new(
                Opts::new("qc_loan_volume_total", "Total loan amount disbursed (stroops)"),
                &["token"],
            )
            .unwrap(),
        );

        let loan_count_total = register(
            &registry,
            Counter::new("qc_loan_count_total", "Total loans created").unwrap(),
        );

        let active_loans = register(
            &registry,
            Gauge::new("qc_active_loans", "Current active loans").unwrap(),
        );

        let slash_events_total = register(
            &registry,
            Counter::new("qc_slash_events_total", "Total slash events").unwrap(),
        );

        let slash_amount_total = register(
            &registry,
            CounterVec::new(
                Opts::new("qc_slash_amount_total", "Total amount slashed (stroops)"),
                &["token"],
            )
            .unwrap(),
        );

        let vouch_count = register(
            &registry,
            Gauge::new("qc_vouch_count", "Total active vouches").unwrap(),
        );

        let gaps_detected = register(
            &registry,
            Counter::new(
                "qc_indexer_gap_detected_total",
                "Total retention-window gaps detected",
            )
            .unwrap(),
        );

        let reorgs_detected = register(
            &registry,
            Counter::new(
                "qc_indexer_reorgs_detected_total",
                "Total ledger reorgs detected",
            )
            .unwrap(),
        );

        let errors_total = register(
            &registry,
            CounterVec::new(
                Opts::new("qc_indexer_errors_total", "Indexer errors"),
                &["error_code"],
            )
            .unwrap(),
        );

        let backfill_events_total = register(
            &registry,
            Counter::new(
                "qc_indexer_backfill_events_total",
                "Total events indexed during backfill",
            )
            .unwrap(),
        );

        Self {
            ledger_height,
            events_total,
            loan_volume_total,
            loan_count_total,
            active_loans,
            slash_events_total,
            slash_amount_total,
            vouch_count,
            gaps_detected,
            reorgs_detected,
            errors_total,
            backfill_events_total,
            registry,
        }
    }

    pub fn rebuild_from_events(&self, events: &[Event]) {
        let mut event_counts: HashMap<(String, String), f64> = HashMap::new();

        for ev in events {
            *event_counts
                .entry((ev.category.clone(), ev.action.clone()))
                .or_insert(0.0) += 1.0;
        }

        for ((cat, act), cnt) in &event_counts {
            self.events_total
                .with_label_values(&[cat, act])
                .inc_by(*cnt);
        }

        let mut max_ledger: f64 = 0.0;
        let mut loan_count: f64 = 0.0;
        let mut active_loans_val: f64 = 0.0;
        let mut vouch_count_val: f64 = 0.0;
        let mut slash_count: f64 = 0.0;
        let mut loan_volume: f64 = 0.0;
        let mut slash_amount: f64 = 0.0;

        for ev in events {
            max_ledger = max_ledger.max(ev.ledger as f64);

            let val: serde_json::Value =
                serde_json::from_str(&ev.value_json).unwrap_or(serde_json::Value::Null);

            match (ev.category.as_str(), ev.action.as_str()) {
                ("loan", "request") => {
                    loan_count += 1.0;
                    active_loans_val += 1.0;
                    if let Some(amount) = val.get("amount_stroops").and_then(|v| v.as_f64()) {
                        loan_volume += amount;
                    }
                }
                ("loan", "repay") => {
                    active_loans_val -= 1.0;
                }
                ("vouch", "create") => {
                    vouch_count_val += 1.0;
                }
                ("vouch", "withdraw") => {
                    vouch_count_val -= 1.0;
                }
                ("loan", "slash") => {
                    slash_count += 1.0;
                    if let Some(amount) = val.get("total_slashed_stroops").and_then(|v| v.as_f64())
                    {
                        slash_amount += amount;
                    }
                }
                _ => {}
            }
        }

        self.ledger_height.set(max_ledger);
        self.loan_count_total.inc_by(loan_count);
        self.active_loans.set(active_loans_val.max(0.0));
        self.vouch_count.set(vouch_count_val.max(0.0));
        self.slash_events_total.inc_by(slash_count);

        let loan_vol_token = Self::most_common_token(events, "loan");
        self.loan_volume_total
            .with_label_values(&[&loan_vol_token])
            .inc_by(loan_volume);

        let slash_token = Self::most_common_token(events, "loan");
        self.slash_amount_total
            .with_label_values(&[&slash_token])
            .inc_by(slash_amount);
    }

    fn most_common_token(events: &[Event], category: &str) -> String {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for ev in events {
            if ev.category != category {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&ev.value_json) {
                if let Some(token) = val.get("token").and_then(|v| v.as_str()) {
                    *counts.entry(token.to_string()).or_insert(0) += 1;
                }
            }
        }
        counts
            .into_iter()
            .max_by_key(|&(_, c)| c)
            .map(|(t, _)| t)
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn record_event(&self, event: &Event) {
        self.ledger_height.set(event.ledger as f64);
        self.events_total
            .with_label_values(&[&event.category, &event.action])
            .inc();

        let val: serde_json::Value =
            serde_json::from_str(&event.value_json).unwrap_or(serde_json::Value::Null);

        match (event.category.as_str(), event.action.as_str()) {
            ("loan", "request") => {
                self.loan_count_total.inc();
                self.active_loans.inc();
                if let Some(amount) = val.get("amount_stroops").and_then(|v| v.as_f64()) {
                    let token = val
                        .get("token")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    self.loan_volume_total
                        .with_label_values(&[token])
                        .inc_by(amount);
                }
            }
            ("loan", "repay") => {
                self.active_loans.dec();
            }
            ("loan", "slash") => {
                self.slash_events_total.inc();
                if let Some(amount) = val.get("total_slashed_stroops").and_then(|v| v.as_f64()) {
                    self.slash_amount_total
                        .with_label_values(&["unknown"])
                        .inc_by(amount);
                }
            }
            ("vouch", "create") => {
                self.vouch_count.inc();
            }
            ("vouch", "withdraw") => {
                self.vouch_count.dec();
            }
            _ => {}
        }
    }
}
