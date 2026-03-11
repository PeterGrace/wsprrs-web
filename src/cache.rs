/// Server-side TTL cache for ClickHouse query results.
///
/// [`TtlCache`] is a generic write-through cache backed by a `tokio` read-write
/// lock.  Entries expire after a configurable [`Duration`]; stale entries are
/// evicted lazily on the next read rather than by a background sweeper.
///
/// [`QueryCache`] composes five `TtlCache` instances, all shared across every
/// concurrent user so that an identical query from userA and userB hits
/// ClickHouse only once per TTL window:
///
/// | Query              | Cache key                      | TTL       |
/// |--------------------|--------------------------------|-----------|
/// | `get_public_config`| `()`                           | 5 minutes |
/// | `get_stats`        | `(since_rounded, until_rounded)`| 60 seconds |
/// | `get_map_spots`    | `SpotFilter` (normalised)      | 60 seconds |
/// | `get_spots`        | `SpotFilter` (normalised)      | 60 seconds |
///
/// Timestamp fields inside a [`SpotFilter`] key are rounded to the nearest
/// 60 seconds before lookup (via [`QueryCache::normalize_filter_key`]) so that
/// requests whose `since_unix` differs by only a few seconds still share the
/// same entry.
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::models::{MapSpot, PublicConfig, SpotFilter, SpotStats, WsprSpot};

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

/// Shared cache for all ClickHouse queries, keyed so that identical requests
/// from different users share a single cache entry.
///
/// Inject this as an `Arc<QueryCache>` Axum extension and retrieve it in
/// server functions via `expect_context::<Arc<QueryCache>>()`.
pub struct QueryCache {
    /// Cache for `get_public_config()`.  TTL: 5 minutes.
    pub config: TtlCache<(), PublicConfig>,
    /// Cache for `get_stats(since, until)`.  Key timestamps are rounded to the
    /// nearest 60 seconds before lookup.  TTL: 60 seconds.
    pub stats: TtlCache<(i64, i64), SpotStats>,
    /// Cache for `get_map_spots(filter)`.  Key is a timestamp-normalised clone
    /// of the `SpotFilter`.  TTL: 60 seconds.
    pub map_spots: TtlCache<SpotFilter, Vec<MapSpot>>,
    /// Cache for `get_spots(filter)`.  Key is a timestamp-normalised clone of
    /// the `SpotFilter`.  TTL: 60 seconds.
    pub spots: TtlCache<SpotFilter, Vec<WsprSpot>>,
}

impl QueryCache {
    /// Construct a new cache with the default TTLs.
    pub fn new() -> Self {
        Self {
            config: TtlCache::new(Duration::from_secs(300)),
            stats: TtlCache::new(Duration::from_secs(60)),
            map_spots: TtlCache::new(Duration::from_secs(60)),
            spots: TtlCache::new(Duration::from_secs(60)),
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

    /// Return a copy of `filter` suitable for use as a cache key.
    ///
    /// * `None` timestamps are resolved to `default_since` / `None` (no upper
    ///   bound) before rounding so that the "default view" always maps to the
    ///   same key regardless of the exact wall-clock second at request time.
    /// * Both `since_unix` and `until_unix` are rounded to the nearest 60-second
    ///   boundary so requests within the same minute share an entry.
    pub fn normalize_filter_key(filter: &SpotFilter, default_since: i64) -> SpotFilter {
        SpotFilter {
            callsign: filter.callsign.clone(),
            grid: filter.grid.clone(),
            band_hz: filter.band_hz,
            snr_min: filter.snr_min,
            power_max: filter.power_max,
            since_unix: Some(Self::round_ts(
                filter.since_unix.unwrap_or(default_since),
            )),
            until_unix: filter.until_unix.map(Self::round_ts),
            limit: filter.limit,
            offset: filter.offset,
            grid_only: filter.grid_only,
        }
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience type alias used in `main.rs` and `server_fns.rs`.
pub type SharedQueryCache = Arc<QueryCache>;
