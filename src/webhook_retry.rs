//! Webhook retry logic with exponential backoff.
//!
//! This module provides utilities for reliable webhook delivery with automatic
//! retry logic. Failed deliveries are retried with exponential backoff until
//! max retries are exhausted.

use soroban_sdk::{contracttype, Address, Env, String, Vec};

/// Webhook retry configuration
pub const DEFAULT_MAX_RETRIES: u32 = 5;
pub const INITIAL_BACKOFF_SECS: u64 = 1;
pub const MAX_BACKOFF_SECS: u64 = 16;

/// Webhook event types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebhookEventType {
    LoanRequested,
    LoanRepaid,
    LoanDefaulted,
    VouchCreated,
    VouchWithdrawn,
}

/// Webhook retry state
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
}

impl WebhookRetryState {
    /// Create a new webhook retry state
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
        }
    }

    /// Calculate next retry delay using exponential backoff
    /// Delay = min(INITIAL_BACKOFF_SECS * 2^retry_count, MAX_BACKOFF_SECS)
    pub fn calculate_next_backoff(&self) -> u64 {
        let backoff = INITIAL_BACKOFF_SECS * (1 << self.retry_count);
        if backoff > MAX_BACKOFF_SECS {
            MAX_BACKOFF_SECS
        } else {
            backoff
        }
    }

    /// Check if retry should be attempted
    pub fn should_retry(&self, current_timestamp: u64) -> bool {
        !self.delivered
            && self.retry_count < self.max_retries
            && current_timestamp >= self.next_retry_timestamp
    }

    /// Mark retry attempt and schedule next retry
    pub fn mark_retry_attempt(&mut self, current_timestamp: u64) {
        self.retry_count += 1;
        self.last_retry_timestamp = current_timestamp;
        if self.retry_count < self.max_retries {
            let backoff = self.calculate_next_backoff();
            self.next_retry_timestamp = current_timestamp + backoff;
        }
    }

    /// Mark delivery as successful
    pub fn mark_delivered(&mut self, current_timestamp: u64) {
        self.delivered = true;
        self.last_retry_timestamp = current_timestamp;
    }

    /// Check if max retries exhausted
    pub fn is_exhausted(&self) -> bool {
        !self.delivered && self.retry_count >= self.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_retry_state_creation() {
        let state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            String::from_slice(&Env::default(), "https://example.com/webhook"),
            String::from_slice(&Env::default(), "{}"),
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
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            String::from_slice(&Env::default(), "https://example.com/webhook"),
            String::from_slice(&Env::default(), "{}"),
            1000,
        );

        // First retry: 1s
        assert_eq!(state.calculate_next_backoff(), 1);

        state.retry_count = 1;
        // Second retry: 2s
        assert_eq!(state.calculate_next_backoff(), 2);

        state.retry_count = 2;
        // Third retry: 4s
        assert_eq!(state.calculate_next_backoff(), 4);

        state.retry_count = 3;
        // Fourth retry: 8s
        assert_eq!(state.calculate_next_backoff(), 8);

        state.retry_count = 4;
        // Fifth retry: 16s (capped at MAX_BACKOFF_SECS)
        assert_eq!(state.calculate_next_backoff(), 16);

        state.retry_count = 5;
        // Sixth retry: 16s (capped)
        assert_eq!(state.calculate_next_backoff(), 16);
    }

    #[test]
    fn test_should_retry() {
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            String::from_slice(&Env::default(), "https://example.com/webhook"),
            String::from_slice(&Env::default(), "{}"),
            1000,
        );

        // Should retry at next_retry_timestamp
        assert!(state.should_retry(1001));

        // Should not retry before next_retry_timestamp
        assert!(!state.should_retry(1000));

        // Mark as delivered
        state.mark_delivered(1001);
        assert!(!state.should_retry(1001));
    }

    #[test]
    fn test_mark_retry_attempt() {
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            String::from_slice(&Env::default(), "https://example.com/webhook"),
            String::from_slice(&Env::default(), "{}"),
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
        let mut state = WebhookRetryState::new(
            1,
            WebhookEventType::LoanRequested,
            String::from_slice(&Env::default(), "https://example.com/webhook"),
            String::from_slice(&Env::default(), "{}"),
            1000,
        );

        assert!(!state.is_exhausted());

        state.retry_count = DEFAULT_MAX_RETRIES;
        assert!(state.is_exhausted());

        state.mark_delivered(1000);
        assert!(!state.is_exhausted());
    }
}
