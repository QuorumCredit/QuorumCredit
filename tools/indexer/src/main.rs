mod db;
mod indexer;
mod metrics;
mod rpc;

use crate::indexer::{Indexer, IndexerConfig};
use crate::metrics::IndexerMetrics;
use crate::rpc::LiveRpc;
use anyhow::Result;
use axum::http::Method;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "quorum-credit-indexer", about = "Persistent indexed store for QuorumCredit Soroban events")]
struct Cli {
    #[arg(long, default_value = "https://soroban-testnet.stellar.org")]
    rpc_url: String,

    #[arg(long)]
    contract_id: String,

    #[arg(long, default_value = "indexer.db")]
    db_path: String,

    #[arg(long, default_value_t = 9090)]
    metrics_port: u16,

    #[arg(long, default_value_t = 5000)]
    poll_interval_ms: u64,

    #[arg(long, default_value_t = 15000)]
    retention_window: u32,

    #[arg(long)]
    deploy_ledger: Option<u32>,

    /// Issue #1146: one-shot mode. Instead of starting the long-running
    /// daemon, print every distinct borrower address ever observed in the
    /// indexed event history to stdout (one per line, or JSON with
    /// `--export-format json`) and exit. Used by `scripts/backup.sh` to
    /// derive its snapshot address set instead of a manually-maintained
    /// `BORROWER_ADDRESSES` env var.
    #[arg(long)]
    export_addresses: bool,

    #[arg(long, default_value = "text")]
    export_format: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    if cli.export_addresses {
        let store = db::Store::open(Path::new(&cli.db_path))?;
        let addresses = store.distinct_borrower_addresses().await?;
        if cli.export_format == "json" {
            println!("{}", serde_json::to_string(&addresses)?);
        } else {
            for address in &addresses {
                println!("{address}");
            }
        }
        return Ok(());
    }

    info!(
        rpc_url = %cli.rpc_url,
        contract_id = %cli.contract_id,
        db_path = %cli.db_path,
        "Starting QuorumCredit Indexer"
    );

    let config = IndexerConfig {
        rpc_url: cli.rpc_url.clone(),
        contract_id: cli.contract_id.clone(),
        db_path: cli.db_path.clone(),
        metrics_port: cli.metrics_port,
        poll_interval_ms: cli.poll_interval_ms,
        retention_window_ledgers: cli.retention_window,
        backfill_chunk_size: 100,
        deploy_ledger: cli.deploy_ledger,
    };

    let store = db::Store::open(Path::new(&cli.db_path))?;
    let rpc = Box::new(LiveRpc::new(&cli.rpc_url));
    let metrics = IndexerMetrics::new();
    let metrics = Arc::new(metrics);

    let indexer = Indexer::new(config, store, rpc, metrics.clone());
    indexer.initialize().await?;

    let metrics_srv = start_metrics_server(cli.metrics_port, metrics.clone());

    tokio::select! {
        _ = indexer.run() => {},
        _ = metrics_srv => {},
    }

    Ok(())
}

async fn start_metrics_server(port: u16, metrics: Arc<IndexerMetrics>) {
    let app_state = metrics;

    let cors = CorsLayer::new()
        .allow_methods([Method::GET])
        .allow_origin(Any);

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .layer(cors)
        .with_state(app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!(address = %addr, "Metrics HTTP server starting");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, "Failed to bind metrics server");
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!(error = %e, "Metrics server error");
    }
}

async fn metrics_handler(
    axum::extract::State(metrics): axum::extract::State<Arc<IndexerMetrics>>,
) -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metrics.registry.gather(), &mut buffer) {
        return format!("# Error encoding metrics: {}\n", e);
    }
    String::from_utf8(buffer).unwrap_or_else(|_| "# Error: invalid UTF-8 in metrics\n".to_string())
}
