use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::Mutex;
use reqwest::Client;
use thiserror::Error;
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[derive(Error, Debug)]
pub enum WebhookError {
    #[error("Failed to send webhook: {0}")]
    SendError(String),
    #[error("Invalid webhook URL")]
    InvalidUrl,
    #[error("Webhook not found")]
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscription {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub retry_count: u32,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub webhook_id: String,
    pub event_id: String,
    pub status: DeliveryStatus,
    pub timestamp: DateTime<Utc>,
    pub response_code: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeliveryStatus {
    Pending,
    Success,
    Failed,
    Retrying,
}

pub struct WebhookManager {
    subscriptions: Arc<Mutex<Vec<WebhookSubscription>>>,
    deliveries: Arc<Mutex<Vec<WebhookDelivery>>>,
    client: Client,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            deliveries: Arc::new(Mutex::new(Vec::new())),
            client: Client::new(),
        }
    }

    pub async fn subscribe(
        &self,
        url: String,
        events: Vec<String>,
        secret: Option<String>,
    ) -> Result<WebhookSubscription, WebhookError> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(WebhookError::InvalidUrl);
        }

        let subscription = WebhookSubscription {
            id: Uuid::new_v4().to_string(),
            url,
            events,
            secret,
            active: true,
            created_at: Utc::now(),
            retry_count: 0,
            max_retries: 5,
        };

        let mut subs = self.subscriptions.lock().await;
        subs.push(subscription.clone());

        tracing::info!(
            webhook_id = %subscription.id,
            url = %subscription.url,
            events = ?subscription.events,
            secret_configured = subscription.secret.is_some(),
            "Webhook subscription created"
        );

        Ok(subscription)
    }

    pub async fn unsubscribe(&self, webhook_id: &str) -> Result<(), WebhookError> {
        let mut subs = self.subscriptions.lock().await;
        if let Some(pos) = subs.iter().position(|s| s.id == webhook_id) {
            subs.remove(pos);
            tracing::info!(webhook_id = %webhook_id, "Webhook subscription removed");
            Ok(())
        } else {
            Err(WebhookError::NotFound)
        }
    }

    pub async fn deliver_event(&self, event: WebhookEvent) -> Result<(), WebhookError> {
        let subs = self.subscriptions.lock().await;

        for sub in subs.iter().filter(|s| s.active && s.events.contains(&event.event_type)) {
            let delivery = WebhookDelivery {
                id: Uuid::new_v4().to_string(),
                webhook_id: sub.id.clone(),
                event_id: event.id.clone(),
                status: DeliveryStatus::Pending,
                timestamp: Utc::now(),
                response_code: None,
                error: None,
            };

            let delivery_id = delivery.id.clone();

            let mut deliveries = self.deliveries.lock().await;
            deliveries.push(delivery);
            drop(deliveries);

            self.send_webhook(sub.clone(), event.clone(), delivery_id).await;
        }

        Ok(())
    }

    async fn send_webhook(
        &self,
        subscription: WebhookSubscription,
        event: WebhookEvent,
        delivery_id: String,
    ) {
        let mut attempt = 0;

        loop {
            attempt += 1;
            let body = serde_json::to_vec(&event).unwrap_or_default();
            let mut request = self.client.post(&subscription.url).body(body.clone());

            if let Some(secret) = &subscription.secret {
                let signature = self.sign_payload(secret, &body);
                request = request.header("X-Webhook-Signature", signature);
            }

            match request.send().await {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let delivery_status = if response.status().is_success() {
                        DeliveryStatus::Success
                    } else {
                        DeliveryStatus::Failed
                    };

                    self.update_delivery(
                        &delivery_id,
                        delivery_status,
                        Some(status_code),
                        None,
                    )
                    .await;

                    tracing::info!(
                        delivery_id = %delivery_id,
                        webhook_id = %subscription.id,
                        status = status_code,
                        "Webhook delivered"
                    );
                    break;
                }
                Err(e) => {
                    if attempt <= subscription.max_retries {
                        let delay = self.calculate_backoff(attempt);
                        self.update_delivery(
                            &delivery_id,
                            DeliveryStatus::Retrying,
                            None,
                            Some(format!("{e} (retrying in {delay}s)")),
                        )
                        .await;

                        tracing::warn!(
                            delivery_id = %delivery_id,
                            webhook_id = %subscription.id,
                            attempt = attempt,
                            error = %e,
                            retry_in_secs = delay,
                            "Webhook delivery failed, retrying"
                        );

                        tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                    } else {
                        self.update_delivery(
                            &delivery_id,
                            DeliveryStatus::Failed,
                            None,
                            Some(e.to_string()),
                        )
                        .await;

                        tracing::error!(
                            delivery_id = %delivery_id,
                            webhook_id = %subscription.id,
                            error = %e,
                            "Webhook delivery failed after retries"
                        );
                        break;
                    }
                }
            }
        }
    }

    fn sign_payload(&self, secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .expect("HMAC can initialize with any key length");
        mac.update(body);
        let result = mac.finalize().into_bytes();
        format!("sha256={}", hex::encode(result))
    }

    fn calculate_backoff(&self, attempt: u32) -> u64 {
        let mut delay = 1u64;
        for _ in 0..attempt.saturating_sub(1) {
            delay = delay.saturating_mul(2);
            if delay >= 16 {
                return 16;
            }
        }
        delay
    }

    async fn update_delivery(
        &self,
        delivery_id: &str,
        status: DeliveryStatus,
        response_code: Option<u16>,
        error: Option<String>,
    ) {
        let mut deliveries = self.deliveries.lock().await;
        if let Some(delivery) = deliveries.iter_mut().find(|d| d.id == delivery_id) {
            delivery.status = status;
            delivery.response_code = response_code;
            delivery.error = error;
        }
    }

    pub async fn get_subscriptions(&self) -> Vec<WebhookSubscription> {
        self.subscriptions.lock().await.clone()
    }

    pub async fn get_deliveries(&self) -> Vec<WebhookDelivery> {
        self.deliveries.lock().await.clone()
    }
}

impl Default for WebhookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscribe_webhook() {
        let manager = WebhookManager::new();
        let result = manager
            .subscribe(
                "https://example.com/webhook".to_string(),
                vec!["loan.created".to_string()],
                None,
            )
            .await;

        assert!(result.is_ok());
        let sub = result.unwrap();
        assert_eq!(sub.events, vec!["loan.created"]);
    }

    #[tokio::test]
    async fn test_invalid_webhook_url() {
        let manager = WebhookManager::new();
        let result = manager
            .subscribe(
                "invalid-url".to_string(),
                vec!["loan.created".to_string()],
                None,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unsubscribe_webhook() {
        let manager = WebhookManager::new();
        let sub = manager
            .subscribe(
                "https://example.com/webhook".to_string(),
                vec!["loan.created".to_string()],
                None,
            )
            .await
            .unwrap();

        let result = manager.unsubscribe(&sub.id).await;
        assert!(result.is_ok());

        let subs = manager.get_subscriptions().await;
        assert_eq!(subs.len(), 0);
    }
}
