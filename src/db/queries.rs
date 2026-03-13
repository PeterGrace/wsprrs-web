/// ClickHouse query functions used by server functions and the SSE handler.
///
/// All functions accept a `&clickhouse::Client` (cheap to clone, internally
/// `Arc`-backed) and return `anyhow::Result` so call sites can use `?`.
///
/// # SQL injection safety
///
/// Numeric values (timestamps, SNR, power, band frequency, limit, offset) are
/// formatted directly into the SQL string — they are typed and cannot carry
/// SQL injection payloads.
///
/// String values (callsign, grid, reporter) are sanitised by
/// [`sanitise_locator`] / [`sanitise_callsign`] before being embedded, keeping
/// only characters that are valid in those fields.
use anyhow::Context;

use crate::models::{
    filter::SpotFilter,
    grid::wspr_bands,
    spot::{
        GlobalMapSpotRow, GlobalSpot, GlobalSpotRow, MapSpot, MapSpotRow, SpotStats, SpotStatsRow,
        WsprSpot, WsprSpotRow,
    },
};

/// Single-column row used for callsign / reporter autocomplete queries.
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct CallsignRow {
    callsign: String,
}

// ---------------------------------------------------------------------------
// String sanitisers
// ---------------------------------------------------------------------------

/// Remove any character from a callsign that is not alphanumeric or `/`.
///
/// WSPR callsigns consist of letters, digits, and at most one `/` for
/// portable/portable-style suffixes.  This is sufficient to prevent SQL
/// injection when the value is embedded in a LIKE clause.
fn sanitise_callsign(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/')
        .take(20)
        .collect()
}

/// Remove any character from a grid locator that is not alphanumeric.
///
/// Maidenhead locators are purely alphanumeric (letters and digits only).
fn sanitise_locator(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect()
}

// ---------------------------------------------------------------------------
// Core queries — local (wspr_spots)
// ---------------------------------------------------------------------------

/// Fetch lightweight map-marker data for all spots in the given filter window.
///
/// Only spots with a non-empty grid field are returned; those without a grid
/// cannot be plotted on the map.
///
/// # Arguments
///
/// * `client`           — ClickHouse HTTP client
/// * `filter`           — query constraints (time window, callsign, band, …)
/// * `table`            — fully-qualified table name, e.g. `"wspr_spots"`
/// * `default_since`    — fallback `since_unix` when `filter.since_unix` is `None`
/// * `ignore_callsigns` — server-configured callsigns to exclude (case-insensitive)
/// * `spot_limit`       — default and maximum row count (`WSPR_SPOT_LIMIT`)
pub async fn query_map_spots(
    client: &clickhouse::Client,
    filter: &SpotFilter,
    table: &str,
    default_since: i64,
    ignore_callsigns: &[String],
    spot_limit: u32,
) -> anyhow::Result<Vec<MapSpot>> {
    let since = filter.since_unix.unwrap_or(default_since);
    let until = filter.until_unix.unwrap_or(i64::MAX);
    let limit = filter.limit.unwrap_or(spot_limit).min(spot_limit);

    let mut sql = format!(
        "SELECT window_start_unix, callsign, grid, freq_hz, snr_db, power_dbm \
         FROM {table} \
         WHERE grid != '' \
           AND window_start_unix >= {since} \
           AND window_start_unix <= {until}"
    );

    append_ignore_callsigns(&mut sql, ignore_callsigns);
    append_shared_filters(&mut sql, filter);

    sql.push_str(&format!(" ORDER BY window_start_unix DESC LIMIT {limit}"));

    let rows: Vec<MapSpotRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("map spots query")?;

    Ok(rows
        .into_iter()
        .filter_map(Option::<MapSpot>::from)
        .collect())
}

/// Fetch full spot records with all ClickHouse columns.
///
/// Supports pagination via `filter.limit` / `filter.offset`.
///
/// # Arguments
///
/// * `ignore_callsigns` — server-configured callsigns to exclude (case-insensitive)
/// * `spot_limit`       — default and maximum row count (`WSPR_SPOT_LIMIT`)
pub async fn query_spots(
    client: &clickhouse::Client,
    filter: &SpotFilter,
    table: &str,
    default_since: i64,
    ignore_callsigns: &[String],
    spot_limit: u32,
) -> anyhow::Result<Vec<WsprSpot>> {
    let since = filter.since_unix.unwrap_or(default_since);
    let until = filter.until_unix.unwrap_or(i64::MAX);
    let limit = filter.limit.unwrap_or(spot_limit).min(spot_limit);
    let offset = filter.offset.unwrap_or(0);

    let mut sql = format!(
        "SELECT window_start_unix, time_utc, snr_db, dt_sec, freq_hz, message, \
                callsign, grid, power_dbm, drift, sync_quality, npass, osd_pass, \
                nhardmin, decode_cycles, candidates, nfano \
         FROM {table} \
         WHERE window_start_unix >= {since} \
           AND window_start_unix <= {until}"
    );

    if filter.grid_only.unwrap_or(false) {
        sql.push_str(" AND grid != ''");
    }

    append_ignore_callsigns(&mut sql, ignore_callsigns);
    append_shared_filters(&mut sql, filter);

    sql.push_str(&format!(
        " ORDER BY window_start_unix DESC LIMIT {limit} OFFSET {offset}"
    ));

    let rows: Vec<WsprSpotRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("spots query")?;

    Ok(rows.into_iter().map(WsprSpot::from).collect())
}

/// Return aggregate statistics (total spots, unique callsigns, unique grids,
/// time range) over the specified window.
///
/// # Arguments
///
/// * `ignore_callsigns` — server-configured callsigns to exclude from counts
pub async fn query_stats(
    client: &clickhouse::Client,
    table: &str,
    since_unix: i64,
    until_unix: i64,
    ignore_callsigns: &[String],
) -> anyhow::Result<SpotStats> {
    let mut sql = format!(
        "SELECT \
            count()                         AS total_spots, \
            uniqExact(callsign)             AS unique_callsigns, \
            uniqExactIf(grid, grid != '')   AS unique_grids, \
            min(window_start_unix)          AS oldest_unix, \
            max(window_start_unix)          AS newest_unix \
         FROM {table} \
         WHERE window_start_unix >= {since_unix} \
           AND window_start_unix <= {until_unix}"
    );
    append_ignore_callsigns(&mut sql, ignore_callsigns);

    let rows: Vec<SpotStatsRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("stats query")?;

    Ok(rows
        .into_iter()
        .next()
        .map(SpotStats::from)
        .unwrap_or(SpotStats {
            total_spots: 0,
            unique_callsigns: 0,
            unique_grids: 0,
            oldest_unix: 0,
            newest_unix: 0,
        }))
}

/// Return up to 20 callsigns that start with `prefix` (case-insensitive).
///
/// Used for autocomplete in the filter panel.  Ignored callsigns are excluded
/// so they do not appear in suggestions.
///
/// # Arguments
///
/// * `ignore_callsigns` — server-configured callsigns to exclude from results
pub async fn query_callsign_suggestions(
    client: &clickhouse::Client,
    table: &str,
    prefix: &str,
    ignore_callsigns: &[String],
) -> anyhow::Result<Vec<String>> {
    let safe = sanitise_callsign(prefix);
    if safe.is_empty() {
        return Ok(vec![]);
    }
    let like_pat = format!("{}%", safe.to_uppercase());

    let mut sql = format!(
        "SELECT DISTINCT callsign \
         FROM {table} \
         WHERE upper(callsign) LIKE '{like_pat}'"
    );
    append_ignore_callsigns(&mut sql, ignore_callsigns);
    sql.push_str(" ORDER BY callsign LIMIT 20");

    let rows: Vec<CallsignRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("callsign suggestions query")?;

    Ok(rows.into_iter().map(|r| r.callsign).collect())
}

/// Fetch spots newer than `after_unix` for the SSE live stream.
///
/// Returns at most `spot_limit` spots to keep individual events small.
///
/// # Arguments
///
/// * `ignore_callsigns` — server-configured callsigns to exclude from results
/// * `spot_limit`       — maximum number of spots to return per poll
pub async fn query_new_spots(
    client: &clickhouse::Client,
    table: &str,
    after_unix: i64,
    ignore_callsigns: &[String],
    spot_limit: u32,
) -> anyhow::Result<Vec<MapSpot>> {
    let mut sql = format!(
        "SELECT window_start_unix, callsign, grid, freq_hz, snr_db, power_dbm \
         FROM {table} \
         WHERE grid != '' \
           AND window_start_unix > {after_unix}"
    );
    append_ignore_callsigns(&mut sql, ignore_callsigns);
    sql.push_str(&format!(
        " ORDER BY window_start_unix DESC LIMIT {spot_limit}"
    ));

    let rows: Vec<MapSpotRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("new spots query")?;

    Ok(rows
        .into_iter()
        .filter_map(Option::<MapSpot>::from)
        .collect())
}

// ---------------------------------------------------------------------------
// Core queries — global (global_spots)
// ---------------------------------------------------------------------------

/// Fetch lightweight map-marker data from the global spot table.
///
/// The `timestamp` DateTime column is converted to Unix seconds via
/// `toUnixTimestamp()`.  Reporter information is included so that the
/// home-QTH override can derive the reporter's grid from the result set.
///
/// # Arguments
///
/// * `client`           — ClickHouse HTTP client
/// * `filter`           — query constraints; `filter.reporter` is applied here
/// * `table`            — global spots table name, e.g. `"global_spots"`
/// * `default_since`    — fallback `since_unix` when `filter.since_unix` is `None`
/// * `ignore_callsigns` — transmitter callsigns to exclude
/// * `spot_limit`       — maximum row count
pub async fn query_global_map_spots(
    client: &clickhouse::Client,
    filter: &SpotFilter,
    table: &str,
    default_since: i64,
    ignore_callsigns: &[String],
    spot_limit: u32,
) -> anyhow::Result<Vec<MapSpot>> {
    let since = filter.since_unix.unwrap_or(default_since);
    let until = filter.until_unix.unwrap_or(i64::MAX);
    let limit = filter.limit.unwrap_or(spot_limit).min(spot_limit);

    let mut sql = format!(
        "SELECT \
          toUnixTimestamp(timestamp) AS timestamp_unix, \
          callsign, grid, reporter, reporter_grid, \
          frequency, snr, power \
         FROM {table} \
         WHERE grid != '' \
           AND toUnixTimestamp(timestamp) >= {since} \
           AND toUnixTimestamp(timestamp) <= {until}"
    );

    append_ignore_callsigns(&mut sql, ignore_callsigns);
    append_reporter_filter(&mut sql, filter);
    append_global_shared_filters(&mut sql, filter);

    sql.push_str(&format!(
        " ORDER BY toUnixTimestamp(timestamp) DESC LIMIT {limit}"
    ));

    let rows: Vec<GlobalMapSpotRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("global map spots query")?;

    Ok(rows
        .into_iter()
        .filter_map(Option::<MapSpot>::from)
        .collect())
}

/// Fetch full spot records from the global spot table.
///
/// # Arguments
///
/// * `ignore_callsigns` — transmitter callsigns to exclude
/// * `spot_limit`       — default and maximum row count
pub async fn query_global_spots(
    client: &clickhouse::Client,
    filter: &SpotFilter,
    table: &str,
    default_since: i64,
    ignore_callsigns: &[String],
    spot_limit: u32,
) -> anyhow::Result<Vec<GlobalSpot>> {
    let since = filter.since_unix.unwrap_or(default_since);
    let until = filter.until_unix.unwrap_or(i64::MAX);
    let limit = filter.limit.unwrap_or(spot_limit).min(spot_limit);
    let offset = filter.offset.unwrap_or(0);

    let mut sql = format!(
        "SELECT \
          spot_id, \
          toUnixTimestamp(timestamp) AS timestamp_unix, \
          reporter, reporter_grid, snr, frequency, \
          callsign, grid, power, drift, distance, azimuth, \
          band, version, code \
         FROM {table} \
         WHERE toUnixTimestamp(timestamp) >= {since} \
           AND toUnixTimestamp(timestamp) <= {until}"
    );

    if filter.grid_only.unwrap_or(false) {
        sql.push_str(" AND grid != ''");
    }

    append_ignore_callsigns(&mut sql, ignore_callsigns);
    append_reporter_filter(&mut sql, filter);
    append_global_shared_filters(&mut sql, filter);

    sql.push_str(&format!(
        " ORDER BY toUnixTimestamp(timestamp) DESC LIMIT {limit} OFFSET {offset}"
    ));

    let rows: Vec<GlobalSpotRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("global spots query")?;

    Ok(rows.into_iter().map(GlobalSpot::from).collect())
}

/// Return up to 20 reporter callsigns that start with `prefix` (global only).
///
/// Used for reporter autocomplete in the filter panel.  No ignore-list is
/// applied since `ignore_callsigns` targets transmitters, not reporters.
///
/// # Arguments
///
/// * `table`  — global spots table name
/// * `prefix` — reporter callsign prefix to match
pub async fn query_reporter_suggestions(
    client: &clickhouse::Client,
    table: &str,
    prefix: &str,
) -> anyhow::Result<Vec<String>> {
    let safe = sanitise_callsign(prefix);
    if safe.is_empty() {
        return Ok(vec![]);
    }
    let like_pat = format!("{}%", safe.to_uppercase());

    // Alias `reporter` as `callsign` to reuse the existing `CallsignRow` struct.
    let sql = format!(
        "SELECT DISTINCT reporter AS callsign \
         FROM {table} \
         WHERE upper(reporter) LIKE '{like_pat}' \
         ORDER BY reporter LIMIT 20"
    );

    let rows: Vec<CallsignRow> = client
        .query(&sql)
        .fetch_all()
        .await
        .context("reporter suggestions query")?;

    Ok(rows.into_iter().map(|r| r.callsign).collect())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Strip a leading `!` from a filter value and report whether it was present.
///
/// Returns `(exclude, rest)` where `exclude` is `true` when the value started
/// with `!` and `rest` is the remainder without the prefix.
fn parse_exclude_prefix(s: &str) -> (bool, &str) {
    match s.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, s),
    }
}

/// Append a `callsign NOT IN (...)` clause for every entry in `ignore`.
///
/// Each callsign is sanitised through [`sanitise_callsign`] before embedding.
/// Does nothing when `ignore` is empty.
fn append_ignore_callsigns(sql: &mut String, ignore: &[String]) {
    if ignore.is_empty() {
        return;
    }
    let list: Vec<String> = ignore
        .iter()
        .map(|cs| format!("'{}'", sanitise_callsign(cs)))
        .collect();
    sql.push_str(&format!(
        " AND upper(callsign) NOT IN ({})",
        list.join(", ")
    ));
}

/// Append callsign, grid, band, SNR, and power WHERE clauses for the local
/// `wspr_spots` table column names.
///
/// Callsign and grid values may be prefixed with `!` to negate the match
/// (e.g. `"!K1ABC"` → `NOT LIKE 'K1ABC%'`).
fn append_shared_filters(sql: &mut String, filter: &SpotFilter) {
    if let Some(cs) = &filter.callsign {
        let (exclude, raw) = parse_exclude_prefix(cs);
        let safe = sanitise_callsign(raw);
        if !safe.is_empty() {
            let not = if exclude { "NOT " } else { "" };
            sql.push_str(&format!(" AND upper(callsign) {not}LIKE upper('{safe}%')"));
        }
    }

    if let Some(g) = &filter.grid {
        let (exclude, raw) = parse_exclude_prefix(g);
        let safe = sanitise_locator(raw);
        if !safe.is_empty() {
            let safe_upper = safe.to_uppercase();
            let not = if exclude { "NOT " } else { "" };
            sql.push_str(&format!(" AND upper(grid) {not}LIKE '{safe_upper}%'"));
        }
    }

    if let Some(band_hz) = filter.band_hz {
        sql.push_str(&format!(" AND abs(freq_hz - {band_hz}) < 10000"));
    }

    if let Some(snr_min) = filter.snr_min {
        sql.push_str(&format!(" AND snr_db >= {snr_min}"));
    }

    if let Some(power_max) = filter.power_max {
        sql.push_str(&format!(" AND power_dbm <= {power_max}"));
    }
}

/// Append a reporter LIKE / NOT LIKE clause for the global `global_spots` table.
///
/// The `reporter` field in [`SpotFilter`] supports the same `!` negation prefix
/// as the `callsign` filter.  Does nothing when `filter.reporter` is `None` or
/// resolves to an empty string after sanitisation.
fn append_reporter_filter(sql: &mut String, filter: &SpotFilter) {
    if let Some(rep) = &filter.reporter {
        let (exclude, raw) = parse_exclude_prefix(rep);
        let safe = sanitise_callsign(raw);
        if !safe.is_empty() {
            let not = if exclude { "NOT " } else { "" };
            sql.push_str(&format!(" AND upper(reporter) {not}LIKE upper('{safe}%')"));
        }
    }
}

/// Append callsign, grid, band, SNR, and power WHERE clauses for the global
/// `global_spots` table column names.
///
/// Differences from [`append_shared_filters`]:
/// - Uses `frequency` column (Float64, MHz) for band matching via index
/// - Uses `snr` column (not `snr_db`)
/// - Uses `power` column (not `power_dbm`)
/// - Band filter uses the `band` integer column via wsprnet band index rather
///   than frequency proximity, since the global table stores the pre-computed
///   band number.
fn append_global_shared_filters(sql: &mut String, filter: &SpotFilter) {
    if let Some(cs) = &filter.callsign {
        let (exclude, raw) = parse_exclude_prefix(cs);
        let safe = sanitise_callsign(raw);
        if !safe.is_empty() {
            let not = if exclude { "NOT " } else { "" };
            sql.push_str(&format!(" AND upper(callsign) {not}LIKE upper('{safe}%')"));
        }
    }

    if let Some(g) = &filter.grid {
        let (exclude, raw) = parse_exclude_prefix(g);
        let safe = sanitise_locator(raw);
        if !safe.is_empty() {
            let safe_upper = safe.to_uppercase();
            let not = if exclude { "NOT " } else { "" };
            sql.push_str(&format!(" AND upper(grid) {not}LIKE '{safe_upper}%'"));
        }
    }

    // Band filter: look up the wsprnet band index for the given dial frequency.
    // The global_spots.band column stores the integer band number matching
    // the position of the band in the standard wsprnet enumeration, which
    // aligns with the index in wspr_bands().
    if let Some(band_hz) = filter.band_hz {
        if let Some(idx) = wspr_bands().iter().position(|b| b.dial_hz == band_hz) {
            sql.push_str(&format!(" AND band = {idx}"));
        }
    }

    if let Some(snr_min) = filter.snr_min {
        sql.push_str(&format!(" AND snr >= {snr_min}"));
    }

    if let Some(power_max) = filter.power_max {
        sql.push_str(&format!(" AND power <= {power_max}"));
    }
}
