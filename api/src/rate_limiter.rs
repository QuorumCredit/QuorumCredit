use async_trait::async_trait;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Tier
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Tier {
    Free,
    Pro,
    Enterprise { requests_per_minute: u64, burst: u64 },
}

impl Tier {
    pub fn requests_per_minute(&self) -> u64 {
        match self {
            Tier::Free => 100,
            Tier::Pro => 1000,
            Tier::Enterprise { requests_per_minute, .. } => *requests_per_minute,
        }
    }

    pub fn burst(&self) -> u64 {
        match self {
            Tier::Free => 10,
            Tier::Pro => 50,
            Tier::Enterprise { burst, .. } => *burst,
        }
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EndpointLimit {
    pub requests_per_minute: u64,
    pub burst: u64,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub tier: Tier,
    pub endpoint_overrides: HashMap<String, EndpointLimit>,
    pub per_chain_overrides: HashMap<String, HashMap<String, EndpointLimit>>,
}

impl RateLimitConfig {
    pub fn new(tier: Tier) -> Self {
        Self { tier, endpoint_overrides: HashMap::new(), per_chain_overrides: HashMap::new() }
    }

    pub fn with_endpoint_override(mut self, path: &str, limit: EndpointLimit) -> Self {
        self.endpoint_overrides.insert(path.to_string(), limit);
        self
    }

    /// Returns (requests_per_minute, burst) for the given endpoint path.
    pub fn limits_for(&self, path: &str) -> (u64, u64) {
        if let Some(ov) = self.endpoint_overrides.get(path) {
            (ov.requests_per_minute, ov.burst)
        } else {
            (self.tier.requests_per_minute(), self.tier.burst())
        }
    }

    /// Returns (requests_per_minute, burst) for the given endpoint path and optional chain id.
    /// Chain-specific overrides take precedence over global endpoint overrides.
    pub fn limits_for_chain(&self, path: &str, chain_id: Option<&str>) -> (u64, u64) {
        if let Some(chain) = chain_id {
            if let Some(chain_map) = self.per_chain_overrides.get(chain) {
                if let Some(ov) = chain_map.get(path) {
                    return (ov.requests_per_minute, ov.burst);
                }
            }
        }

        self.limits_for(path)
    }

    pub fn with_per_chain_override(mut self, chain: &str, path: &str, limit: EndpointLimit) -> Self {
        let entry = self.per_chain_overrides.entry(chain.to_string()).or_insert_with(HashMap::new);
        entry.insert(path.to_string(), limit);
        self
    }
}

// ---------------------------------------------------------------------------
// Store trait — allows injecting a real Redis or an in-memory fake for tests
// ---------------------------------------------------------------------------

pub struct RateLimitResult {
    pub allowed: bool,
    pub limit: u64,
    pub remaining: u64,
    pub reset_after_secs: u64,
}

#[async_trait]
pub trait RateLimitStore: Send + Sync {
    /// Token bucket check. Returns Ok(result) or Err(reason).
    async fn check_token_bucket(
        &self,
        key: &str,
        capacity: u64,
        refill_rate_per_minute: u64,
    ) -> Result<RateLimitResult, String>;
}

// ---------------------------------------------------------------------------
// Redis-backed store
// ---------------------------------------------------------------------------

pub struct RedisStore {
    client: redis::Client,
}

impl RedisStore {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        Ok(Self { client: redis::Client::open(redis_url)? })
    }
}

/// Atomic token-bucket via Lua — avoids TOCTOU races on the bucket state.
const TOKEN_BUCKET_LUA: &str = r#"
local key        = KEYS[1]
local capacity   = tonumber(ARGV[1])
local rpm        = tonumber(ARGV[2])
local now_ms     = tonumber(ARGV[3])

local data       = redis.call('HMGET', key, 'tokens', 'last_ms')
local tokens     = tonumber(data[1])
local last_ms    = tonumber(data[2])

if tokens == nil then
    tokens  = capacity
    last_ms = now_ms
end

-- Refill proportionally to elapsed time
local elapsed   = now_ms - last_ms
local refill    = math.floor(elapsed * rpm / 60000)
tokens          = math.min(capacity, tokens + refill)
if refill > 0 then
    last_ms = last_ms + math.floor(refill * 60000 / rpm)
end

local allowed  = 0
local remaining = tokens
if tokens > 0 then
    allowed   = 1
    tokens    = tokens - 1
    remaining = tokens
end

redis.call('HMSET', key, 'tokens', tokens, 'last_ms', last_ms)
redis.call('EXPIRE', key, 120)

return {allowed, remaining}
"#;

#[async_trait]
impl RateLimitStore for RedisStore {
    async fn check_token_bucket(
        &self,
        key: &str,
        capacity: u64,
        refill_rate_per_minute: u64,
    ) -> Result<RateLimitResult, String> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| e.to_string())?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let result: Vec<i64> = redis::Script::new(TOKEN_BUCKET_LUA)
            .key(key)
            .arg(capacity)
            .arg(refill_rate_per_minute)
            .arg(now_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| e.to_string())?;

        let allowed = result[0] == 1;
        let remaining = result[1] as u64;
        let reset_after_secs = (60u64.checked_div(refill_rate_per_minute.max(1)).unwrap_or(60)).max(1);

        Ok(RateLimitResult { allowed, limit: capacity, remaining, reset_after_secs })
    }
}

// ---------------------------------------------------------------------------
// In-memory store — used as fallback and in unit tests (no Redis needed)
// ---------------------------------------------------------------------------

struct BucketState {
    tokens: u64,
    last_refill: Instant,
}

pub struct InMemoryStore {
    buckets: Mutex<HashMap<String, BucketState>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self { buckets: Mutex::new(HashMap::new()) }
    }
}

#[async_trait]
impl RateLimitStore for InMemoryStore {
    async fn check_token_bucket(
        &self,
        key: &str,
        capacity: u64,
        refill_rate_per_minute: u64,
    ) -> Result<RateLimitResult, String> {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();

        let state = buckets.entry(key.to_string()).or_insert(BucketState {
            tokens: capacity,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed_secs = now.duration_since(state.last_refill).as_secs_f64();
        let refill = (elapsed_secs * refill_rate_per_minute as f64 / 60.0) as u64;
        if refill > 0 {
            state.tokens = (state.tokens + refill).min(capacity);
            state.last_refill = now;
        }

        let allowed = state.tokens > 0;
        if allowed {
            state.tokens -= 1;
        }
        let remaining = state.tokens;
        let reset_after_secs = (60u64.checked_div(refill_rate_per_minute.max(1)).unwrap_or(60)).max(1);

        Ok(RateLimitResult { allowed, limit: capacity, remaining, reset_after_secs })
    }
}

// ---------------------------------------------------------------------------
// RateLimiter — combines config + store
// ---------------------------------------------------------------------------

pub struct RateLimiter {
    config: RateLimitConfig,
    store: Arc<dyn RateLimitStore>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig, store: Arc<dyn RateLimitStore>) -> Self {
        Self { config, store }
    }

    /// Convenience constructor that uses an in-memory store (no Redis required).
    pub fn in_memory(config: RateLimitConfig) -> Self {
        Self::new(config, Arc::new(InMemoryStore::new()))
    }

    pub async fn check_rate_limit(&self, api_key: &str, endpoint: &str) -> RateLimitResult {
        self.check_rate_limit_with_chain(api_key, endpoint, None).await
    }

    /// Chain-aware rate limit check. Pass `Some(chain_id)` to scope limits per-chain.
    pub async fn check_rate_limit_with_chain(&self, api_key: &str, endpoint: &str, chain_id: Option<&str>) -> RateLimitResult {
        let (rpm, burst) = self.config.limits_for_chain(endpoint, chain_id);
        let key = match chain_id {
            Some(chain) => format!("rl:{}:{}:{}", api_key, endpoint, chain),
            None => format!("rl:{}:{}", api_key, endpoint),
        };

        match self.store.check_token_bucket(&key, burst, rpm).await {
            Ok(result) => result,
            // Graceful degradation: allow the request when the store is unavailable.
            Err(_) => RateLimitResult {
                allowed: true,
                limit: burst,
                remaining: burst,
                reset_after_secs: 60,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Axum middleware
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RateLimiterState(pub Arc<RateLimiter>);

pub async fn rate_limit_middleware(
    State(rl): State<RateLimiterState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();

    let endpoint = req.uri().path().to_string();

    // Optional chain scoping header. If present, rate limits are applied per-chain.
    let chain_id = req
        .headers()
        .get("x-chain-id")
        .and_then(|v| v.to_str().ok());

    let result = rl.0.check_rate_limit_with_chain(&api_key, &endpoint, chain_id).await;

    if !result.allowed {
        let mut resp = (StatusCode::TOO_MANY_REQUESTS, "Too Many Requests").into_response();
        let headers = resp.headers_mut();
        headers.insert("X-RateLimit-Limit", hval(result.limit));
        headers.insert("X-RateLimit-Remaining", hval(0u64));
        headers.insert("X-RateLimit-Reset", hval(result.reset_after_secs));
        headers.insert("Retry-After", hval(result.reset_after_secs));
        return resp;
    }

    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert("X-RateLimit-Limit", hval(result.limit));
    headers.insert("X-RateLimit-Remaining", hval(result.remaining));
    headers.insert("X-RateLimit-Reset", hval(result.reset_after_secs));
    resp
}

fn hval(n: u64) -> HeaderValue {
    HeaderValue::from_str(&n.to_string()).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn free_config() -> RateLimitConfig {
        RateLimitConfig::new(Tier::Free)
    }

    fn pro_config() -> RateLimitConfig {
        RateLimitConfig::new(Tier::Pro)
    }

    fn enterprise_config(rpm: u64, burst: u64) -> RateLimitConfig {
        RateLimitConfig::new(Tier::Enterprise { requests_per_minute: rpm, burst })
    }

    fn limiter(config: RateLimitConfig) -> RateLimiter {
        RateLimiter::in_memory(config)
    }

    // --- Tier default values -----------------------------------------------

    #[test]
    fn test_free_tier_rpm() {
        assert_eq!(Tier::Free.requests_per_minute(), 100);
    }

    #[test]
    fn test_free_tier_burst() {
        assert_eq!(Tier::Free.burst(), 10);
    }

    #[test]
    fn test_pro_tier_rpm() {
        assert_eq!(Tier::Pro.requests_per_minute(), 1000);
    }

    #[test]
    fn test_pro_tier_burst() {
        assert_eq!(Tier::Pro.burst(), 50);
    }

    #[test]
    fn test_enterprise_tier_custom_rpm() {
        let tier = Tier::Enterprise { requests_per_minute: 5000, burst: 200 };
        assert_eq!(tier.requests_per_minute(), 5000);
    }

    #[test]
    fn test_enterprise_tier_custom_burst() {
        let tier = Tier::Enterprise { requests_per_minute: 5000, burst: 200 };
        assert_eq!(tier.burst(), 200);
    }

    // --- Config endpoint overrides -----------------------------------------

    #[test]
    fn test_endpoint_override_applies() {
        let config = free_config().with_endpoint_override(
            "/heavy",
            EndpointLimit { requests_per_minute: 5, burst: 2 },
        );
        let (rpm, burst) = config.limits_for("/heavy");
        assert_eq!(rpm, 5);
        assert_eq!(burst, 2);
    }

    #[test]
    fn test_default_limits_when_no_override() {
        let config = free_config().with_endpoint_override(
            "/heavy",
            EndpointLimit { requests_per_minute: 5, burst: 2 },
        );
        let (rpm, burst) = config.limits_for("/light");
        assert_eq!(rpm, 100);
        assert_eq!(burst, 10);
    }

    // --- Burst enforcement -------------------------------------------------

    #[tokio::test]
    async fn test_free_burst_first_10_succeed() {
        let rl = limiter(free_config());
        for i in 0..10 {
            let r = rl.check_rate_limit("user1", "/api").await;
            assert!(r.allowed, "request {} should be allowed within burst", i + 1);
        }
    }

    #[tokio::test]
    async fn test_free_burst_11th_fails() {
        let rl = limiter(free_config());
        for _ in 0..10 {
            rl.check_rate_limit("user2", "/api").await;
        }
        let r = rl.check_rate_limit("user2", "/api").await;
        assert!(!r.allowed, "11th rapid request should be denied (burst=10)");
    }

    // --- Remaining header tracking ----------------------------------------

    #[tokio::test]
    async fn test_remaining_decrements() {
        let rl = limiter(free_config());
        let r1 = rl.check_rate_limit("user3", "/api").await;
        let r2 = rl.check_rate_limit("user3", "/api").await;
        assert!(r1.remaining > r2.remaining);
    }

    #[tokio::test]
    async fn test_limit_field_equals_burst() {
        let rl = limiter(free_config());
        let r = rl.check_rate_limit("user4", "/api").await;
        assert_eq!(r.limit, Tier::Free.burst());
    }

    // --- Per-endpoint isolation -------------------------------------------

    #[tokio::test]
    async fn test_per_endpoint_different_buckets() {
        let rl = limiter(free_config());
        // Exhaust bucket on /a
        for _ in 0..10 {
            rl.check_rate_limit("user5", "/a").await;
        }
        // /b bucket should be independent and still allow requests
        let r = rl.check_rate_limit("user5", "/b").await;
        assert!(r.allowed, "/b bucket should be independent of /a");
    }

    #[tokio::test]
    async fn test_per_apikey_different_buckets() {
        let rl = limiter(free_config());
        for _ in 0..10 {
            rl.check_rate_limit("key_a", "/api").await;
        }
        let r = rl.check_rate_limit("key_b", "/api").await;
        assert!(r.allowed, "key_b bucket should be independent of key_a");
    }

    // --- Graceful degradation on store failure ----------------------------

    struct AlwaysFailStore;

    #[async_trait]
    impl RateLimitStore for AlwaysFailStore {
        async fn check_token_bucket(&self, _k: &str, cap: u64, _r: u64) -> Result<RateLimitResult, String> {
            Err("redis connection refused".to_string())
        }
    }

    #[tokio::test]
    async fn test_redis_failure_allows_request() {
        let rl = RateLimiter::new(free_config(), Arc::new(AlwaysFailStore));
        let r = rl.check_rate_limit("user6", "/api").await;
        assert!(r.allowed, "should fail-open when store is unavailable");
    }

    #[tokio::test]
    async fn test_redis_failure_returns_limit_headers() {
        let rl = RateLimiter::new(free_config(), Arc::new(AlwaysFailStore));
        let r = rl.check_rate_limit("user7", "/api").await;
        assert_eq!(r.limit, Tier::Free.burst());
        assert_eq!(r.reset_after_secs, 60);
    }

    // --- Enterprise endpoint override -------------------------------------

    #[tokio::test]
    async fn test_enterprise_endpoint_override_burst() {
        let config = enterprise_config(2000, 100).with_endpoint_override(
            "/sensitive",
            EndpointLimit { requests_per_minute: 10, burst: 2 },
        );
        let rl = limiter(config);
        rl.check_rate_limit("admin", "/sensitive").await;
        rl.check_rate_limit("admin", "/sensitive").await;
        let r = rl.check_rate_limit("admin", "/sensitive").await;
        assert!(!r.allowed, "burst=2 on /sensitive should deny 3rd request");
    }

    // --- Reset-after value ------------------------------------------------

    #[tokio::test]
    async fn test_reset_after_is_nonzero() {
        let rl = limiter(free_config());
        let r = rl.check_rate_limit("user8", "/api").await;
        assert!(r.reset_after_secs > 0);
    }

    // --- Pro tier burst ---------------------------------------------------

    #[tokio::test]
    async fn test_pro_burst_50_requests_succeed() {
        let rl = limiter(pro_config());
        for i in 0..50 {
            let r = rl.check_rate_limit("pro_user", "/api").await;
            assert!(r.allowed, "pro request {} should be within burst=50", i + 1);
        }
    }

    #[tokio::test]
    async fn test_pro_burst_51st_fails() {
        let rl = limiter(pro_config());
        for _ in 0..50 {
            rl.check_rate_limit("pro_user2", "/api").await;
        }
        let r = rl.check_rate_limit("pro_user2", "/api").await;
        assert!(!r.allowed, "51st rapid request should be denied (pro burst=50)");
    }

    // --- Per-chain overrides ---------------------------------------------

    #[tokio::test]
    async fn test_per_chain_override_applies() {
        let config = free_config().with_per_chain_override(
            "chainA",
            "/bridge",
            EndpointLimit { requests_per_minute: 5, burst: 2 },
        );
        let rl = limiter(config);

        // Two rapid requests on chainA should succeed
        let r1 = rl.check_rate_limit_with_chain("key_c", "/bridge", Some("chainA")).await;
        assert!(r1.allowed);
        let r2 = rl.check_rate_limit_with_chain("key_c", "/bridge", Some("chainA")).await;
        assert!(r2.allowed);

        // Third rapid request on chainA should be denied (burst=2)
        let r3 = rl.check_rate_limit_with_chain("key_c", "/bridge", Some("chainA")).await;
        assert!(!r3.allowed, "3rd rapid request on chainA should be denied");

        // Requests on a different chain should be independent
        let r_other = rl.check_rate_limit_with_chain("key_c", "/bridge", Some("chainB")).await;
        assert!(r_other.allowed, "chainB bucket should be independent of chainA");

        // Requests without a chain header should use global limits (free tier burst=10)
        let r_global = rl.check_rate_limit("key_c", "/bridge").await;
        assert!(r_global.allowed, "global bucket should be independent and allow requests");
    }
}
