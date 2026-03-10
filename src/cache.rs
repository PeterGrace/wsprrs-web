/// Server-side TTL cache for ClickHouse query results.
///
/// [`TtlCache`] is a generic write-through cache backed by a `tokio` read-write
/// lock.  Entries expire after a configurable [`Duration`]; stale entries are
/// evicted lazily on the next read rather than by a background sweeper.
///
/// [`QueryCache`] composes three `TtlCache` instances covering the three
/// aggregate queries whose results are identical across all users:
///
/// | Query              | Cache key         | Default TTL |
/// |--------------------|-------------------|-------------|
/// | `get_public_config`| `()`              | 5 minutes   |
/// | `get_stats`        | `(since, until)`  | 30 seconds  |
/// | `get_band_counts`  | `since`           | 30 seconds  |
///
/// Filter-specific queries (`get_map_spots`, `get_spots`) are **not** cached
/// here because their key space is unbounded and hit rates are unpredictable.
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::models::{BandInfo, PublicConfig, SpotStats};

// ---------------------------------------------------------------------------
// Generic TTL cache
// ---------------------------------------------------------------------------

/// A thread-safe key-value cache where entries expire after a fixed TTL.
///
/// Internally uses a `tokio::sync::RwLock` so that concurrent reads do not
/// block each other; a write lock is only taken on cache misses (to insert)
/// and never held during the actual query.
pub struct TtlCache<K, V> {
    store: RwLock<HashMap<K, (V, Instant)>>,
    ttl: Duration,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new cache whose entries expire after `ttl`.
    pub fn new(ttl: Duration) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Return a cached value for `key` if one exists and has not expired.
    pub async fn get(&self, key: &K) -> Option<V> {
        let store = self.store.read().await;
        store.get(key).and_then(|(v, inserted_at)| {
            if inserted_at.elapsed() < self.ttl {
                Some(v.clone())
            } else {
                None
            }
        })
    }

    /// Insert or replace the value for `key`, resetting its expiry timer.
    pub async fn set(&self, key: K, value: V) {
        let mut store = self.store.write().await;
        store.insert(key, (value, Instant::now()));
    }
}

// ---------------------------------------------------------------------------
// Composed query cache
// ---------------------------------------------------------------------------

/// Shared cache for the three aggregate queries that return identical results
/// for all users on the same time window.
///
/// Inject this as an `Arc<QueryCache>` Axum extension and retrieve it in
/// server functions via `expect_context::<Arc<QueryCache>>()`.
pub struct QueryCache {
    /// Cache for `get_public_config()`.  TTL: 5 minutes.
    pub config: TtlCache<(), PublicConfig>,
    /// Cache for `get_stats(since, until)`.  Key timestamps are rounded to the
    /// nearest 60 seconds before lookup so that requests within the same minute
    /// share an entry.  TTL: 30 seconds.
    pub stats: TtlCache<(i64, i64), SpotStats>,
    /// Cache for `get_band_counts(since)`.  Key rounded to nearest 60 s.
    /// TTL: 30 seconds.
    pub band_counts: TtlCache<i64, Vec<BandInfo>>,
}

impl QueryCache {
    /// Construct a new cache with the default TTLs.
    pub fn new() -> Self {
        Self {
            config: TtlCache::new(Duration::from_secs(300)),
            stats: TtlCache::new(Duration::from_secs(30)),
            band_counts: TtlCache::new(Duration::from_secs(30)),
        }
    }

    /// Round a Unix timestamp down to the nearest 60-second boundary.
    ///
    /// Used as the cache key component for time-windowed queries so that all
    /// requests whose `since` / `until` fall within the same minute share a
    /// single cache entry.
    pub fn round_ts(ts: i64) -> i64 {
        (ts / 60) * 60
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience type alias used in `main.rs` and `server_fns.rs`.
pub type SharedQueryCache = Arc<QueryCache>;
