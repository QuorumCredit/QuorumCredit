use crate::db::{Event, Store};
use crate::metrics::IndexerMetrics;
use crate::rpc::{SorobanEvent, SorobanRpc};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

pub struct IndexerConfig {
    pub rpc_url: String,
    pub contract_id: String,
    pub db_path: String,
    pub metrics_port: u16,
    pub poll_interval_ms: u64,
    pub retention_window_ledgers: u32,
    pub backfill_chunk_size: u32,
    pub deploy_ledger: Option<u32>,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            contract_id: String::new(),
            db_path: "indexer.db".to_string(),
            metrics_port: 9090,
            poll_interval_ms: 5000,
            retention_window_ledgers: 15000,
            backfill_chunk_size: 100,
            deploy_ledger: None,
        }
    }
}

pub struct Indexer {
    config: IndexerConfig,
    store: Arc<Store>,
    rpc: Box<dyn SorobanRpc>,
    metrics: Arc<IndexerMetrics>,
}

impl Indexer {
    pub fn new(
        config: IndexerConfig,
        store: Store,
        rpc: Box<dyn SorobanRpc>,
        metrics: Arc<IndexerMetrics>,
    ) -> Self {
        Self {
            config,
            store: Arc::new(store),
            rpc,
            metrics,
        }
    }

    pub fn get_store(&self) -> &Store {
        &self.store
    }

    pub async fn run_one_poll(&self) -> Result<bool> {
        self.poll_events().await
    }

    pub async fn initialize(&self) -> Result<()> {
        let current = self.rpc.get_latest_ledger().await?;
        info!(latest_ledger = current.sequence, "Connected to Soroban RPC");

        let stored = self.store.get_last_ledger().await?;
        match stored {
            Some(ledger) => {
                info!(last_ledger = ledger, "Resuming from stored cursor");
                let gap = current.sequence.saturating_sub(ledger);
                if gap > self.config.retention_window_ledgers {
                    warn!(
                        gap,
                        retention_window = self.config.retention_window_ledgers,
                        "Gap exceeds retention window — backfill required"
                    );
                    self.metrics.gaps_detected.inc();
                    self.backfill(ledger, current.sequence).await?;
                } else if gap > 0 {
                    info!(gap, "Catch-up required");
                }
            }
            None => {
                info!("No stored cursor found — starting fresh");
                let start = self.config.deploy_ledger.unwrap_or(current.sequence.saturating_sub(1000));
                info!(start_ledger = start, "Initial start ledger");
                self.store.set_last_ledger(start.saturating_sub(1)).await?;
            }
        }

        let all_events = self.store.get_all_events().await?;
        if !all_events.is_empty() {
            self.metrics.rebuild_from_events(&all_events);
            info!(event_count = all_events.len(), "Rebuilt metrics from event store");
        }

        Ok(())
    }

    async fn backfill(&self, from_ledger: u32, to_ledger: u32) -> Result<()> {
        info!(from = from_ledger, to = to_ledger, "Starting backfill");
        let mut current = from_ledger + 1;
        let chunk = self.config.backfill_chunk_size;

        while current <= to_ledger {
            let end = (current + chunk - 1).min(to_ledger);
            info!(current, end, "Backfill chunk");

            match self
                .rpc
                .get_events(current, chunk, None, &self.config.contract_id)
                .await
            {
                Ok(resp) => {
                    for event in &resp.events {
                        if self.process_event(event).await? {
                            self.metrics.backfill_events_total.inc();
                        }
                    }
                    self.store.set_last_ledger(end).await?;
                }
                Err(e) => {
                    error!(error = %e, current, "Backfill RPC error — retrying after delay");
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }

            current = end + 1;
        }

        info!("Backfill complete");
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.poll_events().await {
                Ok(true) => {}
                Ok(false) => {
                    sleep(Duration::from_millis(self.config.poll_interval_ms)).await;
                }
                Err(e) => {
                    error!(error = %e, "Poll error — retrying after delay");
                    self.metrics
                        .errors_total
                        .with_label_values(&["poll_error"])
                        .inc();
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }
    }

    async fn poll_events(&self) -> Result<bool> {
        let last_ledger = self
            .store
            .get_last_ledger()
            .await?
            .unwrap_or(0);

        let latest = self.rpc.get_latest_ledger().await?;
        let latest_seq = latest.sequence;

        if last_ledger >= latest_seq {
            return Ok(false);
        }

        if let Some(ref hash) = latest.hash {
            if let Some((stored_seq, stored_hash)) = self.store.get_latest_sequence_with_hash().await? {
                if stored_seq == latest_seq && stored_hash != *hash {
                    warn!(
                        stored_seq,
                        stored_hash,
                        actual_hash = hash,
                        "Ledger reorg detected at sequence {}",
                        stored_seq
                    );
                    self.metrics.reorgs_detected.inc();
                    let deleted = self
                        .store
                        .rollback_from_ledger(stored_seq, &stored_hash, hash)
                        .await?;
                    info!(deleted, "Rolled back events after reorg");
                    let all = self.store.get_all_events().await?;
                    self.metrics.rebuild_from_events(&all);

                    self.store.store_ledger_hash(latest_seq, hash).await?;
                    return Ok(false);
                }
            }

            self.store.store_ledger_hash(latest_seq, hash).await?;
        }

        let gap = latest_seq.saturating_sub(last_ledger);
        if gap > self.config.retention_window_ledgers {
            warn!(
                gap,
                retention_window = self.config.retention_window_ledgers,
                "Gap exceeds retention window during poll — triggering backfill"
            );
            self.metrics.gaps_detected.inc();
            self.backfill(last_ledger, latest_seq).await?;
            return Ok(true);
        }

        let start = last_ledger + 1;
        let limit: u32 = 200;
        let mut cursor: Option<String> = None;
        let mut total = 0usize;

        loop {
            let resp = self
                .rpc
                .get_events(start, limit, cursor.as_deref(), &self.config.contract_id)
                .await
                .context("Failed to fetch events")?;

            for event in &resp.events {
                if self.process_event(event).await? {
                    total += 1;
                }
            }

            if resp.events.len() < limit as usize || total >= gap as usize {
                break;
            }

            cursor = resp
                .events
                .last()
                .map(|e| format!("{}-{}", e.ledger, e.tx_hash));
        }

        if total > 0 {
            let new_last = start + (total as u32).saturating_sub(1);
            self.store.set_last_ledger(new_last).await?;
        } else {
            self.store.set_last_ledger(latest_seq).await?;
        }

        Ok(total > 0)
    }

    async fn process_event(&self, event: &SorobanEvent) -> Result<bool> {
        let (category, action, value_json) = decode_event(event);

        let ev = Event {
            id: None,
            ledger: event.ledger,
            ledger_closed_at: event.ledger_closed_at.clone(),
            tx_hash: event.tx_hash.clone(),
            contract_id: event.contract_id.clone(),
            category,
            action,
            value_json,
            raw_topics: Some(serde_json::to_string(&event.topic)?),
            raw_value: Some(event.value.clone()),
        };

        let inserted = self.store.insert_event(&ev).await?;
        if inserted {
            self.metrics.record_event(&ev);
        }
        Ok(inserted)
    }
}

fn decode_address(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn decode_event(event: &SorobanEvent) -> (String, String, String) {
    let topic0 = event.topic.first().map(|s| s.as_str()).unwrap_or("");
    let topic1 = event.topic.get(1).map(|s| s.as_str()).unwrap_or("");

    let cat = match topic0 {
        "AAAADwAAAAV2b3VjaA==" => "vouch",
        "AAAADwAAAAVsb2Fu" => "loan",
        "AAAADwAAAAVhZG1pbg==" => "admin",
        "AAAADwAAAAhjb250cmFjdA==" => "contract",
        _ => topic0,
    };

    let action = match topic1 {
        "AAAADwAAAAZpbml0" => "init",
        "AAAADwAAAAZjcmVhdGU=" => "create",
        "AAAADwAAAAhpbmNyZWFzZQ==" => "increase",
        "AAAADwAAAAhkZWNyZWFzZQ==" => "decrease",
        "AAAADwAAAAh3aXRoZHJhdw==" => "withdraw",
        "AAAADwAAAAZyZXF1ZXN0" => "request",
        "AAAADwAAAAVyZXBheQ==" => "repay",
        "AAAADwAAAAVzbGFzaA==" => "slash",
        "AAAADwAAAAZjb25maWc=" => "config",
        "AAAADwAAAAVwYXVzZQ==" => "pause",
        "AAAADwAAAAd1bnBhdXNl" => "unpause",
        _ => topic1,
    };

    let value_json = simplify_value(&event.value, cat, action);

    (cat.to_string(), action.to_string(), value_json)
}

fn simplify_value(value_b64: &str, category: &str, action: &str) -> String {
    let engine = base64::engine::general_purpose::STANDARD;
    let raw = match engine.decode(value_b64) {
        Ok(bytes) => bytes,
        Err(_) => return serde_json::json!({"raw": value_b64}).to_string(),
    };

    let hex_str = hex::encode(&raw);

    match (category, action) {
        ("vouch", "create" | "increase" | "decrease" | "withdraw") => {
            serde_json::json!({
                "voucher": decode_address(&raw[..32.min(raw.len())]),
                "borrower": decode_address(&raw[32..64.min(raw.len())]),
                "stake_stroops": i128_from_hex(hex_str.as_str()),
                "token": "C".to_string()
            })
            .to_string()
        }
        ("loan", "request") => {
            serde_json::json!({
                "borrower": decode_address(&raw[..32.min(raw.len())]),
                "amount_stroops": i128_from_hex(hex_str.as_str()),
                "threshold_stroops": 0i128,
                "loan_purpose": "decoded",
                "token": "C".to_string()
            })
            .to_string()
        }
        ("loan", "repay" | "slash") => {
            serde_json::json!({
                "borrower": decode_address(&raw[..32.min(raw.len())]),
                "payment_stroops": i128_from_hex(hex_str.as_str()),
                "total_slashed_stroops": i128_from_hex(hex_str.as_str())
            })
            .to_string()
        }
        _ => {
            serde_json::json!({
                "raw_hex": hex_str,
                "raw_b64": value_b64
            })
            .to_string()
        }
    }
}

fn i128_from_hex(hex_str: &str) -> i128 {
    if hex_str.len() >= 16 {
        if let Ok(byte_slice) = hex::decode(&hex_str[hex_str.len().saturating_sub(32)..]) {
            let mut arr = [0u8; 16];
            let len = byte_slice.len().min(16);
            arr[16 - len..].copy_from_slice(&byte_slice[..len]);
            return i128::from_be_bytes(arr);
        }
    }
    0
}

