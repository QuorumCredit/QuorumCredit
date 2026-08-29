//! Webhook retry logic with exponential backoff and circuit breaker.
//!
//! This module provides utilities for reliable webhook delivery with automatic
//! retry logic. Failed deliveries are retried with exponential backoff until
//! max retries are exhausted.
//!
//! ## Circuit Breaker (Issue #110)
//!
//! A `CircuitBreaker` wraps each webhook endpoint's delivery state. When
//! consecutive failures exceed `failure_threshold`, the circuit **opens** and
//! all delivery attempts are skipped for a `cooldown_secs` period. After the
//! cooldown the circuit transitions to **half-open**: a single probe attempt
//! is allowed. A successful probe closes the circuit; a failed probe reopens
//! it and resets the cooldown.
//!
//! ```text
//!  Closed ──(failures >= threshold)──► Open ──(cooldown elapsed)──► HalfOpen
//!    ▲                                                                    │
//!    └────────────────────(probe succeeds)──────────────────────────────┘
//!                              └──(probe fails)──► Open (reset cooldown)
//! ```

use soroban_sdk::{contracttype, String};

#[cfg(test)]
use soroban_sdk::Env;

/// Webhook retry configuration
pub const DEFAULT_MAX_RETRIES: u32 = 5;
pub const INITIAL_BACKOFF_SECS: u64 = 1;
pub const MAX_BACKOFF_SECS: u64 = 16;

// ── Issue #110: Circuit breaker constants ─────────────────────────────────────

/// Number of consecutive failures before the circuit opens.
pub const DEFAULT_FAILURE_THRESHOLD: u32 = 3;

/// Seconds the circuit stays open before transitioning to half-open.
pub const DEFAULT_COOLDOWN_SECS: u64 = 60;

/// Webhook event types
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebhookEventType {
    LoanRequested,
    LoanRepaid,
    LoanDefaulted,
    VouchCreated,
    VouchWithdrawn,
}

// ── Issue #110: Circuit breaker state ────────────────────────────────────────

/// Operational state of a circuit breaker.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed — deliveries proceed normally.
    Closed,
    /// Circuit is open — deliveries are skipped until cooldown elapses.
    Open,
    /// Cooldown elapsed — one probe attempt is permitted.
    HalfOpen,
}

/// Circuit breaker for a single webhook endpoint.
///
/// Tracks consecutive delivery failures and gates delivery attempts based on
/// the current circuit state.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreaker {
    /// Current circuit state.
    pub state: CircuitState,
    /// Consecutive failure count since the last successful delivery.
    pub consecutive_failures: u32,
    /// Number of consecutive failures required to open the circuit.
    pub failure_threshold: u32,
    /// Seconds the circuit remains open before transitioning to half-open.
    pub cooldown_secs: u64,
    /// Timestamp when the circuit was last opened (0 if never opened).
    pub opened_at: u64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default thresholds.
    pub fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            opened_at: 0,
            cooldown_secs: DEFAULT_COOLDOWN_SECS,
        }
    }

    /// Create a circuit breaker with custom thresholds.
    pub fn with_config(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            failure_threshold,
            opened_at: 0,
            cooldown_secs,
        }
    }

    /// Returns `true` if a delivery attempt should proceed right now.
    ///
    /// - `Closed`   → always allow.
    /// - `Open`     → deny unless the cooldown has elapsed (transitions to `HalfOpen`).
    /// - `HalfOpen` → allow one probe attempt.
    pub fn allow_request(&mut self, current_timestamp: u64) -> bool {
        if self.state == CircuitState::Closed {
            return true;
        }
        if self.state == CircuitState::Open {
            if current_timestamp >= self.opened_at + self.cooldown_secs {
                self.state = CircuitState::HalfOpen;
                return true;
            }
            return false;
        }
        // HalfOpen — allow the probe
        true
    }

    /// Record a successful delivery.
    ///
    /// Closes the circuit and resets the consecutive failure counter.
    pub fn record_success(&mut self) {
        self.state = CircuitState::Closed;
        self.consecutive_failures = 0;
    }

    /// Record a failed delivery.
    ///
    /// Increments the failure counter. If the counter reaches `failure_threshold`
    /// (or the circuit was half-open), the circuit opens and the cooldown starts.
    pub fn record_failure(&mut self, current_timestamp: u64) {
        self.consecutive_failures += 1;
        let should_open = self.state == CircuitState::HalfOpen
            || self.consecutive_failures >= self.failure_threshold;
        if should_open {
            self.state = CircuitState::Open;
            self.opened_at = current_timestamp;
        }
    }

    /// Returns `true` when the circuit is open and the cooldown has **not** elapsed.
    pub fn is_open(&self, current_timestamp: u64) -> bool {
        self.state == CircuitState::Open
            && current_timestamp < self.opened_at + self.cooldown_secs
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Webhook retry state — now circuit-breaker aware.
///
/// Before calling `should_retry`, callers **must** check
/// `circuit_breaker.allow_request(now)`. If the circuit is open the delivery
/// should be skipped without incrementing `retry_count`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WebhookRetryState {
    /// Unique webhook ID
    pub webhook_id: u64,
    /// Event type
    pub event_type: WebhookEventType,
    /// Webhook URL (stored off-chain)
    pub url: String,
    /// Payload (stored off-chain)
    pub payload: String,
    /// Current retry count
    pub retry_count: u32,
    /// Maximum retries allowed
    pub max_retries: u32,
    /// Timestamp of last retry attempt
    pub last_retry_timestamp: u64,
    /// Next scheduled retry timestamp
    pub next_retry_timestamp: u64,
    /// Whether delivery succeeded
    pub delivered: bool,
    /// Circuit breaker for this webhook endpoint (Issue #110).
    pub circuit_breaker: CircuitBreaker,
}

impl WebhookRetryState {
    /// Create a new webhook retry state with a fresh circuit breaker.
    pub fn new(
        webhook_id: u64,
        event_type: WebhookEventType,
        url: String,
        payload: String,
        current_timestamp: u64,
    ) -> Self {
        Self {
            webhook_id,
            event_type,
            url,
            payload,
            retry_count: 0,
            max_retries: DEFAULT_MAX_RETRIES,
            last_retry_timestamp: current_timestamp,
            next_retry_timestamp: current_timestamp + INITIAL_BACKOFF_SECS,
            delivered: false,
            circuit_breaker: CircuitBreaker::new(),
        }
    }

    /// Calculate next retry delay using exponential backoff.
    /// Delay = min(INITIAL_BACKOFF_SECS * 2^retry_count, MAX_BACKOFF_SECS)
    pub fn calculate_next_backoff(&self) -> u64 {
        let backoff = INITIAL_BACKOFF_SECS * (1 << self.retry_count);
        if backoff > MAX_BACKOFF_SECS {
            MAX_BACKOFF_SECS
        } else {
            backoff
        }
    }

    /// Check if retry should be attempted at `current_timestamp`.
    ///
    /// Returns `false` when:
    /// - already delivered,
    /// - max retries exhausted,
    /// - not yet time for the next retry, **or**
    /// - the circuit breaker is open (Issue #110).
    pub fn should_retry(&mut self, current_timestamp: u64) -> bool {
        if self.delivered || self.retry_count >= self.max_retries {
            return false;
        }
        if current_timestamp < self.next_retry_timestamp {
            return false;
        }
        // Gate on circuit breaker — this may transition Open → HalfOpen
        self.circuit_breaker.allow_request(current_timestamp)
    }

    /// Mark a failed retry attempt.
    ///
    /// Increments retry_count, schedules the next attempt, and notifies the
    /// circuit breaker so it can track consecutive failures.
    pub fn mark_retry_attempt(&mut self, current_timestamp: u64) {
        self.retry_count += 1;
        self.last_retry_timestamp = current_timestamp;
        if self.retry_count < self.max_retries {
            let backoff = self.calculate_next_backoff();
            self.next_retry_timestamp = current_timestamp + backoff;
        }
        // Inform the circuit breaker about the failure (Issue #110)
        self.circuit_breaker.record_failure(current_timestamp);
    }

    /// Mark delivery as successful.
    ///
    /// Also closes the circuit breaker so future deliveries are not blocked.
    pub fn mark_delivered(&mut self, current_timestamp: u64) {
        self.delivered = true;
        self.last_retry_timestamp = current_timestamp;
        // Reset circuit breaker on success (Issue #110)
        self.circuit_breaker.record_success();
    }

    /// Check if max retries exhausted.
    pub fn is_exhausted(&self) -> bool {
        !self.delivered && self.retry_count >= self.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_url(env: &Env) -> String {
        String::from_slice(env, "https://example.com/webhook")
    }
    fn make_payload(env: &Env) -> String {
        String::from_slice(env, "{}")
    }

    #[test]
    fn test_webhook_retry_state_creation() {
        let env = Env::default();
        let state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        assert_eq!(state.webhook_id, 1);
        assert_eq!(state.retry_count, 0);
        assert_eq!(state.max_retries, DEFAULT_MAX_RETRIES);
        assert!(!state.delivered);
        assert_eq!(state.next_retry_timestamp, 1001);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let env = Env::default();
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        assert_eq!(state.calculate_next_backoff(), 1);  // retry 0 → 1s
        state.retry_count = 1;
        assert_eq!(state.calculate_next_backoff(), 2);  // retry 1 → 2s
        state.retry_count = 2;
        assert_eq!(state.calculate_next_backoff(), 4);  // retry 2 → 4s
        state.retry_count = 3;
        assert_eq!(state.calculate_next_backoff(), 8);  // retry 3 → 8s
        state.retry_count = 4;
        assert_eq!(state.calculate_next_backoff(), 16); // retry 4 → 16s (cap)
        state.retry_count = 5;
        assert_eq!(state.calculate_next_backoff(), 16); // retry 5 → 16s (cap)
    }

    #[test]
    fn test_should_retry() {
        let env = Env::default();
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        assert!(state.should_retry(1001));   // at next_retry_timestamp
        assert!(!state.should_retry(1000));  // before next_retry_timestamp

        state.mark_delivered(1001);
        assert!(!state.should_retry(1001));  // after delivery
    }

    #[test]
    fn test_mark_retry_attempt() {
        let env = Env::default();
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        state.mark_retry_attempt(1001);
        assert_eq!(state.retry_count, 1);
        assert_eq!(state.last_retry_timestamp, 1001);
        assert_eq!(state.next_retry_timestamp, 1003); // 1001 + 2s backoff

        state.mark_retry_attempt(1003);
        assert_eq!(state.retry_count, 2);
        assert_eq!(state.next_retry_timestamp, 1007); // 1003 + 4s backoff
    }

    #[test]
    fn test_is_exhausted() {
        let env = Env::default();
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        assert!(!state.is_exhausted());
        state.retry_count = DEFAULT_MAX_RETRIES;
        assert!(state.is_exhausted());
        state.mark_delivered(1000);
        assert!(!state.is_exhausted());
    }

    // ── Issue #110: Circuit breaker tests ─────────────────────────────────────

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new();
        for i in 0..DEFAULT_FAILURE_THRESHOLD {
            cb.record_failure(1000 + i as u64);
        }
        assert!(matches!(cb.state, CircuitState::Open));
    }

    #[test]
    fn test_circuit_breaker_blocks_when_open() {
        let mut cb = CircuitBreaker::with_config(1, 60);
        cb.record_failure(1000);
        assert!(matches!(cb.state, CircuitState::Open));

        // blocked before cooldown
        assert!(!cb.allow_request(1050));
        // half-open after cooldown
        assert!(cb.allow_request(1061));
        assert!(matches!(cb.state, CircuitState::HalfOpen));
    }

    #[test]
    fn test_circuit_breaker_closes_on_probe_success() {
        let mut cb = CircuitBreaker::with_config(1, 60);
        cb.record_failure(1000);
        cb.allow_request(1061); // → half-open
        cb.record_success();
        assert!(matches!(cb.state, CircuitState::Closed));
        assert_eq!(cb.consecutive_failures, 0);
    }

    #[test]
    fn test_circuit_breaker_reopens_on_probe_failure() {
        let mut cb = CircuitBreaker::with_config(1, 60);
        cb.record_failure(1000);
        cb.allow_request(1061); // → half-open
        cb.record_failure(1061); // probe fails → back to open
        assert!(matches!(cb.state, CircuitState::Open));
        assert_eq!(cb.opened_at, 1061);
    }

    #[test]
    fn test_should_retry_blocked_when_circuit_open() {
        let env = Env::default();
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        // Force the circuit open
        state.circuit_breaker = CircuitBreaker::with_config(1, 3600);
        state.circuit_breaker.record_failure(1000);

        // should_retry must return false even if the backoff has elapsed
        state.next_retry_timestamp = 1001;
        assert!(!state.should_retry(2000));
    }

    #[test]
    fn test_mark_delivered_resets_circuit() {
        let env = Env::default();
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            make_url(&env),
            make_payload(&env),
            1000,
        );

        // Fail enough to open the circuit
        for _ in 0..DEFAULT_FAILURE_THRESHOLD {
            state.mark_retry_attempt(1001);
        }
        assert!(matches!(state.circuit_breaker.state, CircuitState::Open));

        // Successful delivery should close the circuit
        state.mark_delivered(2000);
        assert!(matches!(state.circuit_breaker.state, CircuitState::Closed));
        assert_eq!(state.circuit_breaker.consecutive_failures, 0);
    }
}
