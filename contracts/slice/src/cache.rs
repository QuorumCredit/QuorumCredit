//! Slice verification caching.
//!
//! Provides a TTL-based cache for slice verification results so that repeated
//! verifications of unchanged slices do not incur redundant verification cost.
//! Entries are invalidated whenever the underlying slice changes, and cache
//! statistics (hits, misses, evictions) are tracked for observability.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default time-to-live for cached verification results.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// A cached verification result together with its insertion metadata.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Whether the slice passed verification.
    verified: bool,
    /// Fingerprint of the slice contents at the time of verification.
    fingerprint: u64,
    /// When the entry was inserted, used for TTL expiry.
    inserted_at: Instant,
}

/// Cumulative statistics describing cache behaviour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Number of lookups served from a valid cache entry.
    pub hits: u64,
    /// Number of lookups that could not be served from the cache.
    pub misses: u64,
    /// Number of entries removed because they expired or were invalidated.
    pub evictions: u64,
}

impl CacheStats {
    /// Total number of lookups recorded.
    pub fn lookups(&self) -> u64 {
        self.hits + self.misses
    }

    /// Cache hit ratio in the range `0.0..=1.0` (0.0 when no lookups occurred).
    pub fn hit_ratio(&self) -> f64 {
        let lookups = self.lookups();
        if lookups == 0 {
            0.0
        } else {
            self.hits as f64 / lookups as f64
        }
    }
}

/// TTL-based cache for slice verification results.
///
/// The cache is keyed by slice identifier and stores the verification outcome
/// along with a fingerprint of the slice contents. A cached result is only
/// reused when the fingerprint still matches and the entry has not expired.
#[derive(Debug)]
pub struct VerificationCache {
    entries: HashMap<String, CacheEntry>,
    ttl: Duration,
    stats: CacheStats,
}

impl Default for VerificationCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_TTL)
    }
}

impl VerificationCache {
    /// Create a cache with the given time-to-live for each entry.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            stats: CacheStats::default(),
        }
    }

    /// Configured time-to-live for cached entries.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Update the configured time-to-live.
    ///
    /// Existing entries keep their original insertion time and are therefore
    /// re-evaluated against the new duration on the next lookup.
    pub fn set_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }

    /// Number of entries currently held by the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache currently holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Snapshot of the accumulated cache statistics.
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Reset the accumulated cache statistics.
    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }

    /// Look up a cached verification result for `slice_id`.
    ///
    /// Returns `Some(verified)` when a non-expired entry exists whose
    /// fingerprint matches `fingerprint`. Expired or stale entries are evicted
    /// and reported as a miss.
    pub fn get(&mut self, slice_id: &str, fingerprint: u64) -> Option<bool> {
        let expired = match self.entries.get(slice_id) {
            Some(entry) => {
                entry.inserted_at.elapsed() >= self.ttl || entry.fingerprint != fingerprint
            }
            None => {
                self.stats.misses += 1;
                return None;
            }
        };

        if expired {
            self.entries.remove(slice_id);
            self.stats.evictions += 1;
            self.stats.misses += 1;
            return None;
        }

        self.stats.hits += 1;
        self.entries.get(slice_id).map(|entry| entry.verified)
    }

    /// Store a verification result for `slice_id`.
    pub fn insert(&mut self, slice_id: &str, fingerprint: u64, verified: bool) {
        self.entries.insert(
            slice_id.to_string(),
            CacheEntry {
                verified,
                fingerprint,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Invalidate the cached result for a single slice.
    ///
    /// Returns `true` when an entry was present and removed.
    pub fn invalidate(&mut self, slice_id: &str) -> bool {
        if self.entries.remove(slice_id).is_some() {
            self.stats.evictions += 1;
            true
        } else {
            false
        }
    }

    /// Invalidate every cached verification result.
    pub fn invalidate_all(&mut self) {
        self.stats.evictions += self.entries.len() as u64;
        self.entries.clear();
    }

    /// Remove all entries whose TTL has elapsed.
    ///
    /// Returns the number of entries evicted.
    pub fn purge_expired(&mut self) -> usize {
        let ttl = self.ttl;
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.inserted_at.elapsed() < ttl);
        let evicted = before - self.entries.len();
        self.stats.evictions += evicted as u64;
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_and_reuses_matching_fingerprint() {
        let mut cache = VerificationCache::new(Duration::from_secs(60));
        cache.insert("slice-1", 42, true);

        assert_eq!(cache.get("slice-1", 42), Some(true));
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 0);
    }

    #[test]
    fn invalidates_on_slice_change() {
        let mut cache = VerificationCache::new(Duration::from_secs(60));
        cache.insert("slice-1", 42, true);

        // Fingerprint changed: the slice was modified.
        assert_eq!(cache.get("slice-1", 43), None);
        assert_eq!(cache.stats().evictions, 1);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn expires_after_ttl() {
        let mut cache = VerificationCache::new(Duration::from_millis(0));
        cache.insert("slice-1", 42, true);

        assert_eq!(cache.get("slice-1", 42), None);
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn explicit_invalidation_removes_entry() {
        let mut cache = VerificationCache::new(Duration::from_secs(60));
        cache.insert("slice-1", 42, true);

        assert!(cache.invalidate("slice-1"));
        assert!(!cache.invalidate("slice-1"));
        assert_eq!(cache.get("slice-1", 42), None);
    }

    #[test]
    fn tracks_hit_ratio() {
        let mut cache = VerificationCache::new(Duration::from_secs(60));
        cache.insert("slice-1", 1, true);

        let _ = cache.get("slice-1", 1);
        let _ = cache.get("missing", 1);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.lookups(), 2);
        assert!((stats.hit_ratio() - 0.5).abs() < f64::EPSILON);
    }
}
