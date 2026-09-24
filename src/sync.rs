//! Differential Sync API (issue #1568)
//!
//! Provides change tracking, a cursor-based sync endpoint, delta encoding of
//! the response payload, and lightweight sync performance metrics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// A single tracked change to a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Monotonic version assigned when the change was recorded.
    pub version: u64,
    /// Identifier of the affected record.
    pub key: String,
    /// Encoded value for the record (empty string means deletion).
    pub value: String,
}

/// Delta-encoded representation of a set of changes.
///
/// Instead of repeating the full key for every change, the first change carries
/// the key and subsequent changes only carry the value when the key is
/// unchanged. A `None` key means "same key as the previous entry".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaEntry {
    pub version: u64,
    pub key: Option<String>,
    pub value: String,
}

/// Response returned by the sync endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncResponse {
    /// Cursor the client should send on the next request.
    pub cursor: u64,
    /// Whether more changes are available beyond this response.
    pub has_more: bool,
    /// Delta-encoded changes since the requested cursor.
    pub changes: Vec<DeltaEntry>,
}

/// Aggregated sync performance counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncMetrics {
    /// Number of sync requests served.
    pub requests: u64,
    /// Total number of changes returned across all requests.
    pub changes_returned: u64,
    /// Total time spent serving sync requests, in microseconds.
    pub total_micros: u64,
}

impl SyncMetrics {
    /// Average number of changes returned per request.
    pub fn avg_changes(&self) -> f64 {
        if self.requests == 0 {
            0.0
        } else {
            self.changes_returned as f64 / self.requests as f64
        }
    }

    /// Average request latency in microseconds.
    pub fn avg_micros(&self) -> f64 {
        if self.requests == 0 {
            0.0
        } else {
            self.total_micros as f64 / self.requests as f64
        }
    }
}

/// Tracks changes and serves differential sync requests.
#[derive(Debug)]
pub struct SyncStore {
    /// Monotonic version counter; every recorded change increments it.
    version: AtomicU64,
    /// Changes indexed by version for ordered range scans.
    changes: Mutex<BTreeMap<u64, Change>>,
    /// Performance counters.
    metrics: Mutex<SyncMetrics>,
}

impl Default for SyncStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncStore {
    /// Create an empty sync store.
    pub fn new() -> Self {
        SyncStore {
            version: AtomicU64::new(0),
            changes: Mutex::new(BTreeMap::new()),
            metrics: Mutex::new(SyncMetrics::default()),
        }
    }

    /// Record a change and return the version assigned to it.
    pub fn record(&self, key: impl Into<String>, value: impl Into<String>) -> u64 {
        let version = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        let change = Change {
            version,
            key: key.into(),
            value: value.into(),
        };
        self.changes.lock().unwrap().insert(version, change);
        version
    }

    /// Current cursor (latest recorded version).
    pub fn cursor(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Serve a sync request for all changes after `cursor`.
    ///
    /// Returns at most `limit` changes and delta-encodes the payload.
    pub fn sync(&self, cursor: u64, limit: usize) -> SyncResponse {
        let started = Instant::now();

        let changes = self.changes.lock().unwrap();
        let mut selected: Vec<Change> = changes
            .range((cursor + 1)..)
            .take(limit)
            .map(|(_, c)| c.clone())
            .collect();

        let has_more = selected.len() == limit
            && changes
                .range((cursor + 1)..)
                .nth(limit)
                .is_some();

        let next_cursor = selected.last().map(|c| c.version).unwrap_or(cursor);
        let delta = encode_delta(&mut selected);
        drop(changes);

        let elapsed = started.elapsed().as_micros() as u64;
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.requests += 1;
            metrics.changes_returned += delta.len() as u64;
            metrics.total_micros += elapsed;
        }

        SyncResponse {
            cursor: next_cursor,
            has_more,
            changes: delta,
        }
    }

    /// Snapshot of the accumulated sync performance metrics.
    pub fn metrics(&self) -> SyncMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

/// Delta-encode a list of changes, omitting repeated keys.
fn encode_delta(changes: &mut [Change]) -> Vec<DeltaEntry> {
    let mut out = Vec::with_capacity(changes.len());
    let mut last_key: Option<String> = None;
    for change in changes.iter_mut() {
        let key = if last_key.as_deref() == Some(change.key.as_str()) {
            None
        } else {
            last_key = Some(change.key.clone());
            Some(change.key.clone())
        };
        out.push(DeltaEntry {
            version: change.version,
            key,
            value: change.value.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_monotonic_versions() {
        let store = SyncStore::new();
        assert_eq!(store.record("a", "1"), 1);
        assert_eq!(store.record("b", "2"), 2);
        assert_eq!(store.cursor(), 2);
    }

    #[test]
    fn sync_returns_changes_after_cursor() {
        let store = SyncStore::new();
        store.record("a", "1");
        store.record("b", "2");
        let resp = store.sync(1, 10);
        assert_eq!(resp.cursor, 2);
        assert!(!resp.has_more);
        assert_eq!(resp.changes.len(), 1);
        assert_eq!(resp.changes[0].key.as_deref(), Some("b"));
    }

    #[test]
    fn delta_encoding_omits_repeated_keys() {
        let store = SyncStore::new();
        store.record("a", "1");
        store.record("a", "2");
        let resp = store.sync(0, 10);
        assert_eq!(resp.changes[0].key.as_deref(), Some("a"));
        assert_eq!(resp.changes[1].key, None);
    }

    #[test]
    fn tracks_metrics() {
        let store = SyncStore::new();
        store.record("a", "1");
        store.sync(0, 10);
        let metrics = store.metrics();
        assert_eq!(metrics.requests, 1);
        assert_eq!(metrics.changes_returned, 1);
    }
}
