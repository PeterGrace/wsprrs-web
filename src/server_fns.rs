/// Leptos server functions for all data-fetching operations.
///
/// Each `#[server]` function runs exclusively on the server (SSR binary) but
/// its signature is compiled into the WASM bundle so the browser can call it
/// over the wire.  The ClickHouse client and server config are retrieved from
/// the Leptos reactive context, which is populated in `main.rs` via
/// `leptos_routes_with_context`.
use leptos::prelude::*;

use crate::models::{MapSpot, PublicConfig, SpotFilter, SpotStats, WsprSpot};

// ---------------------------------------------------------------------------
// Server function: public configuration
// ---------------------------------------------------------------------------

/// Return the public server configuration (QTH coords, time window, bands).
///
/// Called once on page load so the WASM client knows the home QTH and can
/// set the default time-window filter.  Result is cached for 5 minutes since
/// the configuration never changes at runtime.
#[server]
pub async fn get_public_config() -> Result<PublicConfig, ServerFnError> {
    use std::sync::Arc;

    use crate::cache::SharedQueryCache;
    use crate::config::Config;

    let cache = expect_context::<SharedQueryCache>();

    if let Some(cached) = cache.config.get(&()).await {
        return Ok(cached);
    }

    let config = expect_context::<Arc<Config>>();

    let cfg = PublicConfig::new_without_counts(
        config.my_grid.clone(),
        config.time_window_hours,
        config.detail_zoom,
    );

    cache.config.set((), cfg.clone()).await;
    Ok(cfg)
}

// ---------------------------------------------------------------------------
// Server function: map spots
// ---------------------------------------------------------------------------

/// Fetch lightweight map-marker data for the given filter.
///
/// Only spots that carry a valid Maidenhead grid are returned; type-2 messages
/// (no grid) are excluded because they cannot be plotted.  Results are cached
/// for 60 seconds, shared across all users, keyed on a timestamp-normalised
/// copy of the filter so that the common "default view" is only queried once
/// per minute regardless of how many browsers are open.
#[server]
pub async fn get_map_spots(filter: SpotFilter) -> Result<Vec<MapSpot>, ServerFnError> {
    use std::sync::Arc;

    use crate::cache::SharedQueryCache;
    use crate::config::Config;
    use crate::db::queries;
    use crate::models::grid::{grid_to_latlon, haversine_km};

    let config = expect_context::<Arc<Config>>();
    let default_since =
        chrono::Utc::now().timestamp() - config.time_window_hours as i64 * 3600;

    let cache = expect_context::<SharedQueryCache>();
    let cache_key = crate::cache::QueryCache::normalize_filter_key(&filter, default_since);

    if let Some(cached) = cache.map_spots.get(&cache_key).await {
        return Ok(cached);
    }

    let client = expect_context::<clickhouse::Client>();
    let home = config.my_grid.as_deref().and_then(grid_to_latlon);

    let mut spots: Vec<MapSpot> = match queries::query_map_spots(
        &client,
        &filter,
        &config.clickhouse_table,
        default_since,
        &config.ignore_callsigns,
        config.spot_limit,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("get_map_spots query failed: {e:#}");
            return Err(ServerFnError::ServerError(e.to_string()));
        }
    };

    if let Some(home) = home {
        for spot in &mut spots {
            spot.distance_km = Some(haversine_km(home.lat, home.lon, spot.lat, spot.lon));
        }
    }

    cache.map_spots.set(cache_key, spots.clone()).await;
    Ok(spots)
}

// ---------------------------------------------------------------------------
// Server function: spot list
// ---------------------------------------------------------------------------

/// Fetch full spot records with all columns, paginated.
///
/// Results are cached for 60 seconds, shared across all users, keyed on a
/// timestamp-normalised copy of the filter (same strategy as `get_map_spots`).
#[server]
pub async fn get_spots(filter: SpotFilter) -> Result<Vec<WsprSpot>, ServerFnError> {
    use std::sync::Arc;

    use crate::cache::SharedQueryCache;
    use crate::config::Config;
    use crate::db::queries;
    use crate::models::grid::{grid_to_latlon, haversine_km};

    let config = expect_context::<Arc<Config>>();
    let default_since =
        chrono::Utc::now().timestamp() - config.time_window_hours as i64 * 3600;

    let cache = expect_context::<SharedQueryCache>();
    let cache_key = crate::cache::QueryCache::normalize_filter_key(&filter, default_since);

    if let Some(cached) = cache.spots.get(&cache_key).await {
        return Ok(cached);
    }

    let client = expect_context::<clickhouse::Client>();
    let home = config.my_grid.as_deref().and_then(grid_to_latlon);

    let mut spots: Vec<WsprSpot> = match queries::query_spots(
        &client,
        &filter,
        &config.clickhouse_table,
        default_since,
        &config.ignore_callsigns,
        config.spot_limit,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("get_spots query failed: {e:#}");
            return Err(ServerFnError::ServerError(e.to_string()));
        }
    };

    if let Some(home) = home {
        for spot in &mut spots {
            if spot.grid.is_empty() {
                continue;
            }
            if let Some(ll) = grid_to_latlon(&spot.grid) {
                spot.distance_km = Some(haversine_km(home.lat, home.lon, ll.lat, ll.lon));
            }
        }
    }

    cache.spots.set(cache_key, spots.clone()).await;
    Ok(spots)
}

// ---------------------------------------------------------------------------
// Server function: aggregate statistics
// ---------------------------------------------------------------------------

/// Return aggregate statistics (total spots, unique callsigns, unique grids)
/// for the given time range.  Results are cached for 30 seconds keyed on
/// timestamps rounded to the nearest minute.
#[server]
pub async fn get_stats(since_unix: i64, until_unix: i64) -> Result<SpotStats, ServerFnError> {
    use std::sync::Arc;

    use crate::cache::SharedQueryCache;
    use crate::config::Config;
    use crate::db::queries;

    let cache = expect_context::<SharedQueryCache>();
    let cache_key = (
        crate::cache::QueryCache::round_ts(since_unix),
        crate::cache::QueryCache::round_ts(until_unix),
    );

    if let Some(cached) = cache.stats.get(&cache_key).await {
        return Ok(cached);
    }

    let config = expect_context::<Arc<Config>>();
    let client = expect_context::<clickhouse::Client>();

    let result: SpotStats = match queries::query_stats(
        &client,
        &config.clickhouse_table,
        since_unix,
        until_unix,
        &config.ignore_callsigns,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("get_stats query failed: {e:#}");
            return Err(ServerFnError::ServerError(e.to_string()));
        }
    };

    cache.stats.set(cache_key, result.clone()).await;
    Ok(result)
}

// ---------------------------------------------------------------------------
// Server function: per-band counts
// ---------------------------------------------------------------------------
// Server function: callsign autocomplete
// ---------------------------------------------------------------------------

/// Return up to 20 callsigns that start with `prefix`.
#[server]
pub async fn get_callsign_suggestions(
    prefix: String,
) -> Result<Vec<String>, ServerFnError> {
    use std::sync::Arc;

    use crate::config::Config;
    use crate::db::queries;

    let config = expect_context::<Arc<Config>>();
    let client = expect_context::<clickhouse::Client>();

    queries::query_callsign_suggestions(&client, &config.clickhouse_table, &prefix, &config.ignore_callsigns)
        .await
        .map_err(|e| {
            tracing::error!("get_callsign_suggestions query failed: {e:#}");
            ServerFnError::ServerError(e.to_string())
        })
}
