//! API metrics collection and Prometheus export.
//!
//! Provides per-endpoint request counting, latency distribution tracking,
//! and error rate tracking, with a Prometheus text-format exporter.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Latency histogram bucket upper bounds, in seconds.
const LATENCY_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Per-endpoint metrics: request counts, latency histogram, and error counts.
#[derive(Debug, Default)]
struct EndpointMetrics {
    requests: AtomicU64,
    errors: AtomicU64,
    /// Cumulative count of observations per bucket (non-cumulative storage).
    buckets: Mutex<[u64; LATENCY_BUCKETS.len()] >,
    latency_sum_micros: AtomicU64,
    latency_count: AtomicU64,
}

impl EndpointMetrics {
    fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            buckets: Mutex::new([0u64; LATENCY_BUCKETS.len()]),
            latency_sum_micros: AtomicU64::new(0),
            latency_count: AtomicU64::new(0),
        }
    }

    fn observe(&self, latency: Duration, is_error: bool) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        if is_error {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }

        let seconds = latency.as_secs_f64();
        let micros = latency.as_micros() as u64;
        self.latency_sum_micros.fetch_add(micros, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut buckets) = self.buckets.lock() {
            for (i, bound) in LATENCY_BUCKETS.iter().enumerate() {
                if seconds <= *bound {
                    buckets[i] += 1;
                    break;
                }
            }
        }
    }
}

/// Registry of per-endpoint API metrics.
#[derive(Debug, Default)]
pub struct ApiMetrics {
    endpoints: Mutex<HashMap<String, EndpointMetrics>>,
}

impl ApiMetrics {
    /// Create an empty metrics registry.
    pub fn new() -> Self {
        Self {
            endpoints: Mutex::new(HashMap::new()),
        }
    }

    /// Record a completed request for the given endpoint.
    ///
    /// `endpoint` should be a stable label such as `"GET /v1/users/:id"`.
    pub fn record(&self, endpoint: &str, latency: Duration, is_error: bool) {
        let mut endpoints = match self.endpoints.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        endpoints
            .entry(endpoint.to_string())
            .or_insert_with(EndpointMetrics::new)
            .observe(latency, is_error);
    }

    /// Render all collected metrics in Prometheus text exposition format.
    pub fn export_prometheus(&self) -> String {
        let endpoints = match self.endpoints.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let mut out = String::new();

        out.push_str("# HELP api_requests_total Total number of API requests per endpoint.\n");
        out.push_str("# TYPE api_requests_total counter\n");
        for (endpoint, m) in endpoints.iter() {
            let _ = writeln!(
                out,
                "api_requests_total{{endpoint=\"{}\"}} {}",
                escape_label(endpoint),
                m.requests.load(Ordering::Relaxed)
            );
        }

        out.push_str("# HELP api_errors_total Total number of API errors per endpoint.\n");
        out.push_str("# TYPE api_errors_total counter\n");
        for (endpoint, m) in endpoints.iter() {
            let _ = writeln!(
                out,
                "api_errors_total{{endpoint=\"{}\"}} {}",
                escape_label(endpoint),
                m.errors.load(Ordering::Relaxed)
            );
        }

        out.push_str("# HELP api_request_duration_seconds API request latency per endpoint.\n");
        out.push_str("# TYPE api_request_duration_seconds histogram\n");
        for (endpoint, m) in endpoints.iter() {
            let label = escape_label(endpoint);
            let buckets = match m.buckets.lock() {
                Ok(guard) => *guard,
                Err(poisoned) => *poisoned.into_inner(),
            };
            let mut cumulative = 0u64;
            for (i, bound) in LATENCY_BUCKETS.iter().enumerate() {
                cumulative += buckets[i];
                let _ = writeln!(
                    out,
                    "api_request_duration_seconds_bucket{{endpoint=\"{}\",le=\"{}\"}} {}",
                    label, bound, cumulative
                );
            }
            let total = m.latency_count.load(Ordering::Relaxed);
            let _ = writeln!(
                out,
                "api_request_duration_seconds_bucket{{endpoint=\"{}\",le=\"+Inf\"}} {}",
                label, total
            );
            let sum_seconds = m.latency_sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            let _ = writeln!(
                out,
                "api_request_duration_seconds_sum{{endpoint=\"{}\"}} {}",
                label, sum_seconds
            );
            let _ = writeln!(
                out,
                "api_request_duration_seconds_count{{endpoint=\"{}\"}} {}",
                label, total
            );
        }

        out
    }
}

/// Escape a Prometheus label value per the text exposition format.
fn escape_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_exports_per_endpoint() {
        let metrics = ApiMetrics::new();
        metrics.record("GET /v1/users", Duration::from_millis(3), false);
        metrics.record("GET /v1/users", Duration::from_millis(20), true);
        metrics.record("POST /v1/users", Duration::from_millis(150), false);

        let out = metrics.export_prometheus();
        assert!(out.contains("api_requests_total{endpoint=\"GET /v1/users\"} 2"));
        assert!(out.contains("api_errors_total{endpoint=\"GET /v1/users\"} 1"));
        assert!(out.contains("api_request_duration_seconds_count{endpoint=\"POST /v1/users\"} 1"));
        assert!(out.contains("api_request_duration_seconds_bucket{endpoint=\"GET /v1/users\",le=\"+Inf\"} 2"));
    }
}
