//! Webhook subscription registry (Issue #111).
//!
//! Manages per-subject webhook subscriptions and enforces a maximum subscription
//! count per caller to prevent registry spam and unbounded storage growth.
//!
//! ## Subscription Limit
//!
//! Each `(subject, caller)` pair may register at most `MAX_WEBHOOKS_PER_SUBJECT`
//! webhooks. Attempting to register beyond this limit returns
//! [`crate::errors::ContractError::WebhookLimitExceeded`].
//!
//! The default limit is **10** subscriptions per subject per caller. Admins may
//! override this limit via [`WebhookRegistry::set_limit`].
//!
//! See also: `docs/webhook-signature-verification-guide.md`.

use soroban_sdk::{contracttype, Address, Env, String, Vec};
use crate::types::DataKey;

// ── Issue #111: Per-subject webhook subscription limit ───────────────────────

/// Default maximum number of webhook subscriptions allowed per (subject, caller) pair.
///
/// This limit exists to prevent a single caller from monopolising contract
/// storage and to bound the cost of iterating over subscriptions during
/// event dispatch.
pub const MAX_WEBHOOKS_PER_SUBJECT: u32 = 10;

// ─────────────────────────────────────────────────────────────────────────────

/// A single webhook subscription record.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WebhookSubscription {
    /// Address that registered this subscription.
    pub owner: Address,
    /// Subject identifier (e.g. borrower address string or event topic).
    pub subject: String,
    /// Delivery URL for the webhook payload.
    pub url: String,
    /// Timestamp when this subscription was created.
    pub created_at: u64,
    /// Whether this subscription is currently active.
    pub active: bool,
}

/// Storage key for the webhook subscription list.
///
/// Keyed by `(subject_str, owner)` → `Vec<WebhookSubscription>`.
#[contracttype]
#[derive(Clone, Debug)]
pub enum WebhookRegistryKey {
    /// All subscriptions for a given (subject, owner) pair.
    Subscriptions(String, Address),
}

/// Webhook registry — manages subscription storage and limit enforcement.
pub struct WebhookRegistry;

impl WebhookRegistry {
    // ── Registration ──────────────────────────────────────────────────────────

    /// Register a new webhook subscription for `owner` on `subject`.
    ///
    /// # Errors
    ///
    /// Returns `Err(WebhookLimitExceeded)` when the caller already has
    /// `MAX_WEBHOOKS_PER_SUBJECT` (or the admin-configured limit) active
    /// subscriptions for `subject`.
    ///
    /// Returns `Err(DuplicateVouch)` (reused as a generic "duplicate" sentinel)
    /// when an identical URL is already registered for the same subject/owner.
    pub fn register(
        env: &Env,
        owner: Address,
        subject: String,
        url: String,
    ) -> Result<(), WebhookRegistryError> {
        owner.require_auth();

        let key = WebhookRegistryKey::Subscriptions(subject.clone(), owner.clone());
        let mut subs: Vec<WebhookSubscription> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));

        // Enforce per-subject limit (Issue #111)
        let limit = Self::get_limit(env);
        let active_count = subs.iter().filter(|s| s.active).count() as u32;
        if active_count >= limit {
            return Err(WebhookRegistryError::LimitExceeded);
        }

        // Reject duplicate URLs for the same subject/owner
        if subs.iter().any(|s| s.active && s.url == url) {
            return Err(WebhookRegistryError::DuplicateUrl);
        }

        let now = env.ledger().timestamp();
        subs.push_back(WebhookSubscription {
            owner: owner.clone(),
            subject: subject.clone(),
            url,
            created_at: now,
            active: true,
        });

        env.storage().persistent().set(&key, &subs);
        Ok(())
    }

    /// Unregister a specific URL subscription for `owner` on `subject`.
    ///
    /// Marks the matching subscription inactive rather than removing it,
    /// preserving the audit trail.
    ///
    /// # Errors
    ///
    /// Returns `Err(NotFound)` when no active subscription with `url` exists
    /// for the given subject/owner.
    pub fn unregister(
        env: &Env,
        owner: Address,
        subject: String,
        url: String,
    ) -> Result<(), WebhookRegistryError> {
        owner.require_auth();

        let key = WebhookRegistryKey::Subscriptions(subject.clone(), owner.clone());
        let mut subs: Vec<WebhookSubscription> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));

        let mut found = false;
        let mut updated: Vec<WebhookSubscription> = Vec::new(env);
        for mut sub in subs.iter() {
            if sub.active && sub.url == url {
                sub.active = false;
                found = true;
            }
            updated.push_back(sub);
        }

        if !found {
            return Err(WebhookRegistryError::NotFound);
        }

        env.storage().persistent().set(&key, &updated);
        Ok(())
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    /// Return all **active** subscriptions for `(subject, owner)`.
    pub fn get_subscriptions(
        env: &Env,
        owner: Address,
        subject: String,
    ) -> Vec<WebhookSubscription> {
        let key = WebhookRegistryKey::Subscriptions(subject, owner);
        let subs: Vec<WebhookSubscription> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));

        let mut active: Vec<WebhookSubscription> = Vec::new(env);
        for sub in subs.iter() {
            if sub.active {
                active.push_back(sub);
            }
        }
        active
    }

    /// Return the current per-subject limit (default or admin-configured).
    pub fn get_limit(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::WebhookLimit)
            .unwrap_or(MAX_WEBHOOKS_PER_SUBJECT)
    }

    // ── Admin ─────────────────────────────────────────────────────────────────

    /// Override the per-subject webhook limit.
    ///
    /// Must be called by an admin (caller must separately enforce auth before
    /// invoking this helper).
    ///
    /// # Panics
    ///
    /// Panics if `limit` is 0.
    pub fn set_limit(env: &Env, limit: u32) {
        assert!(limit > 0, "webhook limit must be at least 1");
        env.storage()
            .instance()
            .set(&DataKey::WebhookLimit, &limit);
    }
}

// ── Registry-specific errors ──────────────────────────────────────────────────

/// Errors that can be returned by `WebhookRegistry` operations.
///
/// These are **not** `ContractError` variants — they are returned from the
/// registry helper functions and mapped to `ContractError` at the call site.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WebhookRegistryError {
    /// Caller has reached `MAX_WEBHOOKS_PER_SUBJECT` active subscriptions.
    LimitExceeded,
    /// A subscription with the same URL already exists for this subject/owner.
    DuplicateUrl,
    /// No matching active subscription found.
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_register_and_retrieve() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let subject = String::from_str(&env, "borrower_events");
        let url = String::from_str(&env, "https://example.com/hook1");

        WebhookRegistry::register(&env, owner.clone(), subject.clone(), url.clone()).unwrap();

        let subs = WebhookRegistry::get_subscriptions(&env, owner.clone(), subject.clone());
        assert_eq!(subs.len(), 1);
        assert_eq!(subs.get(0).unwrap().url, url);
    }

    #[test]
    fn test_limit_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let subject = String::from_str(&env, "borrower_events");

        // Register URLs to fill up to the default limit.
        // We use distinct static strings for each slot.
        let urls = [
            "https://example.com/hook0",
            "https://example.com/hook1",
            "https://example.com/hook2",
            "https://example.com/hook3",
            "https://example.com/hook4",
            "https://example.com/hook5",
            "https://example.com/hook6",
            "https://example.com/hook7",
            "https://example.com/hook8",
            "https://example.com/hook9",
        ];
        // MAX_WEBHOOKS_PER_SUBJECT == 10, matching the array above
        for url_str in urls.iter() {
            let url = String::from_str(&env, url_str);
            WebhookRegistry::register(&env, owner.clone(), subject.clone(), url).unwrap();
        }

        // One more should be rejected
        let overflow_url = String::from_str(&env, "https://example.com/overflow");
        let err =
            WebhookRegistry::register(&env, owner.clone(), subject.clone(), overflow_url)
                .unwrap_err();
        assert_eq!(err, WebhookRegistryError::LimitExceeded);
    }

    #[test]
    fn test_duplicate_url_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let subject = String::from_str(&env, "borrower_events");
        let url = String::from_str(&env, "https://example.com/hook1");

        WebhookRegistry::register(&env, owner.clone(), subject.clone(), url.clone()).unwrap();
        let err =
            WebhookRegistry::register(&env, owner.clone(), subject.clone(), url.clone())
                .unwrap_err();
        assert_eq!(err, WebhookRegistryError::DuplicateUrl);
    }

    #[test]
    fn test_unregister() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let subject = String::from_str(&env, "borrower_events");
        let url = String::from_str(&env, "https://example.com/hook1");

        WebhookRegistry::register(&env, owner.clone(), subject.clone(), url.clone()).unwrap();
        WebhookRegistry::unregister(&env, owner.clone(), subject.clone(), url.clone()).unwrap();

        let subs = WebhookRegistry::get_subscriptions(&env, owner.clone(), subject.clone());
        assert_eq!(subs.len(), 0);
    }

    #[test]
    fn test_custom_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let subject = String::from_str(&env, "borrower_events");

        // Set a lower limit of 2
        WebhookRegistry::set_limit(&env, 2);
        assert_eq!(WebhookRegistry::get_limit(&env), 2);

        WebhookRegistry::register(
            &env,
            owner.clone(),
            subject.clone(),
            String::from_str(&env, "https://example.com/hook1"),
        )
        .unwrap();
        WebhookRegistry::register(
            &env,
            owner.clone(),
            subject.clone(),
            String::from_str(&env, "https://example.com/hook2"),
        )
        .unwrap();

        let err = WebhookRegistry::register(
            &env,
            owner.clone(),
            subject.clone(),
            String::from_str(&env, "https://example.com/hook3"),
        )
        .unwrap_err();
        assert_eq!(err, WebhookRegistryError::LimitExceeded);
    }
}
