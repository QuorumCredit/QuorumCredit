//! Event sourcing for the API audit trail.
//!
//! Provides an append-only [`EventStore`] that records every API operation as
//! an immutable [`Event`], supports replaying the recorded history to rebuild
//! state, and exposes a query interface for inspecting the audit trail.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A single immutable record of an API operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Monotonically increasing sequence number assigned on append.
    pub sequence: u64,
    /// Logical stream the event belongs to (e.g. the resource or aggregate id).
    pub stream: String,
    /// Operation that produced the event (e.g. `"create"`, `"update"`).
    pub operation: String,
    /// Serialized payload describing the operation.
    pub payload: String,
    /// Unix timestamp (seconds) when the event was recorded.
    pub timestamp: u64,
}

/// Filter used by [`EventStore::query`] to select events from the audit trail.
#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    /// Restrict results to a single stream when set.
    pub stream: Option<String>,
    /// Restrict results to a single operation when set.
    pub operation: Option<String>,
    /// Only return events with `timestamp >= since` when set.
    pub since: Option<u64>,
    /// Only return events with `timestamp <= until` when set.
    pub until: Option<u64>,
    /// Maximum number of events to return (applied after ordering).
    pub limit: Option<usize>,
}

impl EventQuery {
    /// Returns `true` when `event` matches every constraint in this query.
    pub fn matches(&self, event: &Event) -> bool {
        if let Some(stream) = &self.stream {
            if &event.stream != stream {
                return false;
            }
        }
        if let Some(operation) = &self.operation {
            if &event.operation != operation {
                return false;
            }
        }
        if let Some(since) = self.since {
            if event.timestamp < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if event.timestamp > until {
                return false;
            }
        }
        true
    }
}

/// Append-only store of [`Event`]s forming the API audit trail.
///
/// Cloning an `EventStore` shares the same underlying log, so it can be handed
/// to request handlers while remaining queryable elsewhere.
#[derive(Debug, Clone, Default)]
pub struct EventStore {
    inner: Arc<Mutex<EventStoreInner>>,
}

#[derive(Debug, Default)]
struct EventStoreInner {
    events: Vec<Event>,
    next_sequence: u64,
}

impl EventStore {
    /// Creates an empty event store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an event for an API operation and returns the stored record.
    ///
    /// The sequence number and timestamp are assigned by the store.
    pub fn append(
        &self,
        stream: impl Into<String>,
        operation: impl Into<String>,
        payload: impl Into<String>,
    ) -> Event {
        let mut inner = self.inner.lock().expect("event store mutex poisoned");
        let event = Event {
            sequence: inner.next_sequence,
            stream: stream.into(),
            operation: operation.into(),
            payload: payload.into(),
            timestamp: now_secs(),
        };
        inner.next_sequence += 1;
        inner.events.push(event.clone());
        event
    }

    /// Returns every event in append order.
    pub fn all(&self) -> Vec<Event> {
        self.inner
            .lock()
            .expect("event store mutex poisoned")
            .events
            .clone()
    }

    /// Returns the number of recorded events.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("event store mutex poisoned")
            .events
            .len()
    }

    /// Returns `true` when no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Queries the audit trail, returning matching events in append order.
    pub fn query(&self, query: &EventQuery) -> Vec<Event> {
        let inner = self.inner.lock().expect("event store mutex poisoned");
        let mut results: Vec<Event> = inner
            .events
            .iter()
            .filter(|event| query.matches(event))
            .cloned()
            .collect();
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }
        results
    }

    /// Replays the audit trail in append order, invoking `apply` for each event.
    ///
    /// This rebuilds derived state from the recorded history. When `query` is
    /// provided only matching events are replayed.
    pub fn replay<F>(&self, query: Option<&EventQuery>, mut apply: F)
    where
        F: FnMut(&Event),
    {
        let events = match query {
            Some(query) => self.query(query),
            None => self.all(),
        };
        for event in &events {
            apply(event);
        }
    }

    /// Replays the audit trail and folds it into an accumulator.
    ///
    /// Useful for reconstructing aggregate state from the event history.
    pub fn replay_fold<T, F>(&self, query: Option<&EventQuery>, init: T, mut fold: F) -> T
    where
        F: FnMut(T, &Event) -> T,
    {
        let events = match query {
            Some(query) => self.query(query),
            None => self.all(),
        };
        events.iter().fold(init, |acc, event| fold(acc, event))
    }

    /// Groups the audit trail by stream, preserving append order within each.
    pub fn by_stream(&self) -> HashMap<String, Vec<Event>> {
        let mut grouped: HashMap<String, Vec<Event>> = HashMap::new();
        for event in self.all() {
            grouped.entry(event.stream.clone()).or_default().push(event);
        }
        grouped
    }
}

/// Returns the current Unix timestamp in seconds.
fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_assigns_sequence_and_records_event() {
        let store = EventStore::new();
        let first = store.append("users/1", "create", "{\"name\":\"a\"}");
        let second = store.append("users/1", "update", "{\"name\":\"b\"}");
        assert_eq!(first.sequence, 0);
        assert_eq!(second.sequence, 1);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn query_filters_by_stream_and_operation() {
        let store = EventStore::new();
        store.append("users/1", "create", "{}");
        store.append("users/2", "create", "{}");
        store.append("users/1", "delete", "{}");

        let query = EventQuery {
            stream: Some("users/1".to_string()),
            ..Default::default()
        };
        assert_eq!(store.query(&query).len(), 2);

        let query = EventQuery {
            operation: Some("create".to_string()),
            ..Default::default()
        };
        assert_eq!(store.query(&query).len(), 2);
    }

    #[test]
    fn replay_rebuilds_state_in_order() {
        let store = EventStore::new();
        store.append("counter", "increment", "1");
        store.append("counter", "increment", "2");

        let total = store.replay_fold(None, 0u64, |acc, event| {
            acc + event.payload.parse::<u64>().unwrap()
        });
        assert_eq!(total, 3);
    }
}
