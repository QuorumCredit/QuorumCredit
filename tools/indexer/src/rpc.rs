use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SorobanEvent {
    pub ledger: u32,
    #[serde(rename = "ledgerClosedAt")]
    pub ledger_closed_at: String,
    #[serde(rename = "contractId")]
    pub contract_id: String,
    pub topic: Vec<String>,
    pub value: String,
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    #[serde(rename = "inSuccessfulContractCall")]
    pub in_successful_contract_call: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestLedger {
    pub sequence: u32,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEventsResponse {
    pub events: Vec<SorobanEvent>,
    #[serde(rename = "latestLedger")]
    pub latest_ledger: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest<T: Serialize> {
    jsonrpc: String,
    id: u64,
    method: String,
    params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[async_trait]
pub trait SorobanRpc: Send + Sync {
    async fn get_latest_ledger(&self) -> Result<LatestLedger>;
    async fn get_events(
        &self,
        start_ledger: u32,
        limit: u32,
        cursor: Option<&str>,
        contract_id: &str,
    ) -> Result<GetEventsResponse>;
}

pub struct LiveRpc {
    client: reqwest::Client,
    url: String,
}

impl LiveRpc {
    pub fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    async fn call<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: T,
    ) -> Result<R> {
        let body = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: method.to_string(),
            params,
        };

        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .context("RPC request failed")?;

        let text = resp.text().await.context("Failed to read RPC response")?;
        let json: JsonRpcResponse<R> =
            serde_json::from_str(&text).context("Failed to parse RPC response")?;

        if let Some(err) = json.error {
            anyhow::bail!("RPC error {}: {}", err.code, err.message);
        }

        json.result.context("RPC returned null result")
    }
}

#[async_trait]
impl SorobanRpc for LiveRpc {
    async fn get_latest_ledger(&self) -> Result<LatestLedger> {
        #[derive(Serialize)]
        struct EmptyParams {}
        self.call("getLatestLedger", EmptyParams {}).await
    }

    async fn get_events(
        &self,
        start_ledger: u32,
        limit: u32,
        cursor: Option<&str>,
        contract_id: &str,
    ) -> Result<GetEventsResponse> {
        #[derive(Serialize)]
        struct GetEventsParams<'a> {
            #[serde(rename = "startLedger")]
            start_ledger: u32,
            filters: Vec<EventFilter<'a>>,
            limit: u32,
            cursor: Option<&'a str>,
        }

        #[derive(Serialize)]
        struct EventFilter<'a> {
            #[serde(rename = "type")]
            filter_type: &'a str,
            #[serde(rename = "contractIds")]
            contract_ids: Vec<&'a str>,
        }

        let params = GetEventsParams {
            start_ledger,
            filters: vec![EventFilter {
                filter_type: "contract",
                contract_ids: vec![contract_id],
            }],
            limit,
            cursor,
        };

        self.call("getEvents", params).await
    }
}

pub struct MockRpc {
    pub events: std::sync::Mutex<Vec<SorobanEvent>>,
    pub latest_ledger: std::sync::Mutex<u32>,
    pub latest_hash: std::sync::Mutex<String>,
    pub cursor: std::sync::Mutex<u32>,
}

impl MockRpc {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
            latest_ledger: std::sync::Mutex::new(0),
            latest_hash: std::sync::Mutex::new("genesis-hash".to_string()),
            cursor: std::sync::Mutex::new(0),
        }
    }

    pub fn push_event(&self, event: SorobanEvent) {
        let mut events = self.events.lock().unwrap();
        let ledger = event.ledger;
        events.push(event);
        let mut ll = self.latest_ledger.lock().unwrap();
        if ledger > *ll {
            *ll = ledger;
        }
    }

    pub fn set_latest_ledger(&self, seq: u32) {
        let mut ll = self.latest_ledger.lock().unwrap();
        *ll = seq;
    }

    pub fn set_ledger_hash(&self, hash: &str) {
        let mut h = self.latest_hash.lock().unwrap();
        *h = hash.to_string();
    }

    pub fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }

    pub fn clone_box(&self) -> Box<dyn SorobanRpc> {
        Box::new(MockRpc {
            events: std::sync::Mutex::new(self.events.lock().unwrap().clone()),
            latest_ledger: std::sync::Mutex::new(*self.latest_ledger.lock().unwrap()),
            latest_hash: std::sync::Mutex::new(self.latest_hash.lock().unwrap().clone()),
            cursor: std::sync::Mutex::new(*self.cursor.lock().unwrap()),
        })
    }
}

#[async_trait]
impl SorobanRpc for MockRpc {
    async fn get_latest_ledger(&self) -> Result<LatestLedger> {
        let seq = *self.latest_ledger.lock().unwrap();
        let hash = self.latest_hash.lock().unwrap().clone();
        Ok(LatestLedger {
            sequence: seq,
            hash: Some(hash),
        })
    }

    async fn get_events(
        &self,
        start_ledger: u32,
        limit: u32,
        _cursor: Option<&str>,
        _contract_id: &str,
    ) -> Result<GetEventsResponse> {
        let events = self.events.lock().unwrap();
        let filtered: Vec<SorobanEvent> = events
            .iter()
            .filter(|e| e.ledger >= start_ledger)
            .take(limit as usize)
            .cloned()
            .collect();
        let latest = *self.latest_ledger.lock().unwrap();

        if filtered.is_empty() && start_ledger <= latest {
            return Ok(GetEventsResponse {
                events: vec![],
                latest_ledger: latest,
            });
        }

        Ok(GetEventsResponse {
            events: filtered,
            latest_ledger: latest,
        })
    }
}
