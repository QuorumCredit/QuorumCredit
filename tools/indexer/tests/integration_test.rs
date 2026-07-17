use anyhow::Result;
use quorum_credit_indexer::db::{Event, Store};
use quorum_credit_indexer::indexer::{Indexer, IndexerConfig};
use quorum_credit_indexer::metrics::IndexerMetrics;
use quorum_credit_indexer::rpc::{MockRpc, SorobanEvent};
use std::sync::Arc;

fn make_vouch_event(ledger: u32, voucher: &str, borrower: &str, stake: i128, tx_hash: &str) -> SorobanEvent {
    SorobanEvent {
        ledger,
        ledger_closed_at: "2025-01-01T00:00:00Z".to_string(),
        contract_id: "CAQLW6YI4Q7VJUP3K6NQ4QJ4Q7VJUP3K6NQ4QJ4Q7".to_string(),
        topic: vec![
            "AAAADwAAAAV2b3VjaA==".to_string(),
            "AAAADwAAAAZjcmVhdGU=".to_string(),
        ],
        value: serde_json::json!({
            "voucher": voucher,
            "borrower": borrower,
            "stake_stroops": stake.to_string(),
            "token": "C".to_string()
        }).to_string(),
        tx_hash: tx_hash.to_string(),
        in_successful_contract_call: Some(true),
    }
}

fn make_loan_event(ledger: u32, borrower: &str, amount: i128, tx_hash: &str) -> SorobanEvent {
    SorobanEvent {
        ledger,
        ledger_closed_at: "2025-01-01T00:00:00Z".to_string(),
        contract_id: "CAQLW6YI4Q7VJUP3K6NQ4QJ4Q7VJUP3K6NQ4QJ4Q7".to_string(),
        topic: vec![
            "AAAADwAAAAVsb2Fu".to_string(),
            "AAAADwAAAAZyZXF1ZXN0".to_string(),
        ],
        value: serde_json::json!({
            "borrower": borrower,
            "amount_stroops": amount.to_string(),
            "token": "C".to_string()
        }).to_string(),
        tx_hash: tx_hash.to_string(),
        in_successful_contract_call: Some(true),
    }
}

fn make_repay_event(ledger: u32, borrower: &str, amount: i128, tx_hash: &str) -> SorobanEvent {
    SorobanEvent {
        ledger,
        ledger_closed_at: "2025-01-01T00:00:00Z".to_string(),
        contract_id: "CAQLW6YI4Q7VJUP3K6NQ4QJ4Q7VJUP3K6NQ4QJ4Q7".to_string(),
        topic: vec![
            "AAAADwAAAAVsb2Fu".to_string(),
            "AAAADwAAAAVyZXBheQ==".to_string(),
        ],
        value: serde_json::json!({
            "borrower": borrower,
            "payment_stroops": amount.to_string(),
        }).to_string(),
        tx_hash: tx_hash.to_string(),
        in_successful_contract_call: Some(true),
    }
}

fn make_config() -> IndexerConfig {
    IndexerConfig {
        db_path: String::new(),
        contract_id: "CAQLW6YI4Q7VJUP3K6NQ4QJ4Q7VJUP3K6NQ4QJ4Q7".to_string(),
        deploy_ledger: Some(1),
        retention_window_ledgers: 100,
        backfill_chunk_size: 100,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_restart_mid_backfill() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("restart_test.db");

    let mock_rpc = MockRpc::new();

    mock_rpc.push_event(make_vouch_event(1, "GA", "GB", 100, "tx1"));
    mock_rpc.push_event(make_vouch_event(2, "GC", "GD", 200, "tx2"));
    mock_rpc.push_event(make_vouch_event(3, "GE", "GF", 300, "tx3"));
    mock_rpc.set_ledger_hash("hash-1-3");

    let mut config = make_config();
    config.db_path = db_path.to_string_lossy().to_string();

    let store = Store::open(&db_path)?;
    let indexer = Indexer::new(config, store, mock_rpc.clone_box(), Arc::new(IndexerMetrics::new()));
    indexer.initialize().await?;

    for _ in 0..5 {
        indexer.run_one_poll().await?;
    }

    let events = indexer.get_store().get_all_events().await?;
    assert_eq!(events.len(), 3, "Should have indexed 3 events before restart");

    let cursor = indexer.get_store().get_last_ledger().await?;
    assert_eq!(cursor, Some(3), "Cursor should be at ledger 3");

    drop(indexer);

    let store2 = Store::open(&db_path)?;
    mock_rpc.push_event(make_vouch_event(4, "GG", "GH", 400, "tx4"));
    mock_rpc.push_event(make_vouch_event(5, "GI", "GJ", 500, "tx5"));

    let mut config2 = make_config();
    config2.db_path = db_path.to_string_lossy().to_string();

    let indexer2 = Indexer::new(config2, store2, mock_rpc.clone_box(), IndexerMetrics::new());
    indexer2.initialize().await?;

    for _ in 0..5 {
        indexer2.run_one_poll().await?;
    }

    let events2 = indexer2.get_store().get_all_events().await?;
    assert_eq!(events2.len(), 5, "Should have 5 events after restart and catch-up");

    let cursor2 = indexer2.get_store().get_last_ledger().await?;
    assert_eq!(cursor2, Some(5), "Cursor should be at ledger 5 after restart");

    Ok(())
}

#[tokio::test]
async fn test_reorg_handling() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("reorg_test.db");

    let mock_rpc = MockRpc::new();

    mock_rpc.push_event(make_vouch_event(1, "GA", "GB", 100, "tx1"));
    mock_rpc.push_event(make_vouch_event(2, "GC", "GD", 200, "tx2"));
    mock_rpc.set_ledger_hash("hash-original");

    let mut config = make_config();
    config.db_path = db_path.to_string_lossy().to_string();

    let store = Store::open(&db_path)?;
    let indexer = Indexer::new(config, store, mock_rpc.clone_box(), Arc::new(IndexerMetrics::new()));
    indexer.initialize().await?;

    for _ in 0..3 {
        indexer.run_one_poll().await?;
    }

    let events_before = indexer.get_store().get_all_events().await?;
    assert_eq!(events_before.len(), 2, "Should have 2 events before reorg");
    assert_eq!(indexer.get_store().get_last_ledger().await?, Some(2));

    mock_rpc.clear_events();
    mock_rpc.set_ledger_hash("hash-reorged");

    mock_rpc.push_event(make_vouch_event(1, "GA", "GB", 100, "tx1"));
    mock_rpc.push_event(make_loan_event(2, "BORROWER_X", 5000, "tx-reorg"));
    mock_rpc.set_latest_ledger(2);
    mock_rpc.set_ledger_hash("hash-reorged");

    indexer.run_one_poll().await?;

    let events_after = indexer.get_store().get_all_events().await?;
    assert_eq!(events_after.len(), 2, "Should have 2 events after reorg");

    let loan_events: Vec<&Event> = events_after.iter().filter(|e| e.category == "loan").collect();
    assert_eq!(loan_events.len(), 1, "Should have the reorged loan event");
    assert_eq!(loan_events[0].action, "request");

    Ok(())
}

#[tokio::test]
async fn test_gap_detection_triggers_backfill() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("gap_test.db");

    let mock_rpc = MockRpc::new();

    mock_rpc.push_event(make_vouch_event(10, "GA", "GB", 100, "tx-a"));
    mock_rpc.push_event(make_vouch_event(20, "GC", "GD", 200, "tx-b"));
    mock_rpc.set_ledger_hash("hash-10-20");
    mock_rpc.set_latest_ledger(20);

    let store = Store::open(&db_path)?;
    store.set_last_ledger(1).await?;

    let mut config = make_config();
    config.db_path = db_path.to_string_lossy().to_string();
    config.retention_window_ledgers = 5;

    let indexer = Indexer::new(config, store, mock_rpc.clone_box(), Arc::new(IndexerMetrics::new()));
    indexer.initialize().await?;

    let events = indexer.get_store().get_all_events().await?;
    assert_eq!(events.len(), 2, "Backfill should have indexed 2 events");

    let cursor = indexer.get_store().get_last_ledger().await?;
    assert_eq!(cursor, Some(20), "Cursor should be at the last backfilled ledger");

    Ok(())
}

#[tokio::test]
async fn test_event_dedup() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("dedup_test.db");

    let mock_rpc = MockRpc::new();

    mock_rpc.push_event(make_vouch_event(1, "GA", "GB", 100, "tx1"));
    mock_rpc.set_ledger_hash("hash-dedup");
    mock_rpc.set_latest_ledger(1);

    let mut config = make_config();
    config.db_path = db_path.to_string_lossy().to_string();

    let store = Store::open(&db_path)?;
    let indexer = Indexer::new(config, store, mock_rpc.clone_box(), Arc::new(IndexerMetrics::new()));
    indexer.initialize().await?;

    for _ in 0..3 {
        indexer.run_one_poll().await?;
    }

    let events = indexer.get_store().get_all_events().await?;
    assert_eq!(events.len(), 1, "Duplicate events should be deduplicated");

    Ok(())
}
