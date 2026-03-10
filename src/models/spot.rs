use serde::{Deserialize, Serialize};

use super::grid::{find_band, grid_to_latlon, wspr_bands, BandDef};

// ---------------------------------------------------------------------------
// Wire-format structs returned from server functions (SSR + WASM)
// ---------------------------------------------------------------------------

/// A single decoded WSPR spot with all fields from the `wspr_spots` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WsprSpot {
    /// Unix epoch seconds of the WSPR two-minute window start.
    pub window_start_unix: i64,
    /// HHMM UTC string, e.g. `"1234"`.
    pub time_utc: String,
    /// Signal-to-noise ratio in dB (typically -30 to +10).
    pub snr_db: i32,
    /// Time offset from the nominal window start in seconds.
    pub dt_sec: f32,
    /// Decoded carrier frequency in Hz.
    pub freq_hz: f64,
    /// Full decoded WSPR message, e.g. `"K1ABC FN42 33"`.
    pub message: String,
    /// Extracted callsign.
    pub callsign: String,
    /// Maidenhead grid locator (4 or 6 chars) or empty for type-2 messages.
    pub grid: String,
    /// Transmitted power in dBm.
    pub power_dbm: i32,
    /// Frequency drift in Hz/min.
    pub drift: i32,
    /// Sync vector quality metric.
    pub sync_quality: f32,
    /// Decode pass number (1 = direct, 3 = OSD).
    pub npass: u8,
    /// OSD pass on which the decode succeeded.
    pub osd_pass: u8,
    /// Minimum hard-decision count.
    pub nhardmin: i32,
    /// Number of decoder iterations.
    pub decode_cycles: u32,
    /// Number of candidate messages explored.
    pub candidates: u32,
    /// Fano metric for the decoded path.
    pub nfano: i32,
    /// Great-circle distance from the configured home QTH to this spot's grid
    /// in kilometres.  `None` when no home QTH is configured or the spot has
    /// no grid.
    pub distance_km: Option<f64>,
}

/// Lightweight map marker data: only the fields needed to render a spot on
/// the world map, plus pre-computed lat/lon and band metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSpot {
    /// Unix epoch seconds of the WSPR window start.
    pub window_start_unix: i64,
    /// Callsign of the transmitting station.
    pub callsign: String,
    /// Maidenhead grid locator (only spots with a non-empty grid are included).
    pub grid: String,
    /// Latitude of the grid centre in decimal degrees.
    pub lat: f64,
    /// Longitude of the grid centre in decimal degrees.
    pub lon: f64,
    /// Carrier frequency in Hz.
    pub freq_hz: f64,
    /// SNR in dB.
    pub snr_db: i32,
    /// Transmitted power in dBm.
    pub power_dbm: i32,
    /// Human-readable band name, e.g. `"20m"`.  Empty if not a standard band.
    pub band_name: String,
    /// CSS colour string for the map marker, e.g. `"#2979FF"`.
    pub band_color: String,
    /// Great-circle distance from the configured home QTH in kilometres.
    /// `None` when no home QTH is configured.
    pub distance_km: Option<f64>,
}

/// Aggregate statistics over a set of spots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotStats {
    /// Total number of decoded spots in the time window.
    pub total_spots: u64,
    /// Number of unique callsigns heard.
    pub unique_callsigns: u64,
    /// Number of unique grid squares reported.
    pub unique_grids: u64,
    /// Oldest spot timestamp (Unix seconds).  `0` when no spots exist.
    pub oldest_unix: i64,
    /// Most recent spot timestamp (Unix seconds).  `0` when no spots exist.
    pub newest_unix: i64,
}

/// Per-band summary returned by the bands API endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandInfo {
    /// Band name, e.g. `"20m"`.
    pub name: String,
    /// Nominal WSPR dial frequency in Hz.
    pub dial_hz: u64,
    /// Map-marker CSS colour for this band.
    pub color: String,
    /// Number of spots on this band in the queried time window.
    pub spot_count: u64,
}

/// Public server configuration forwarded to the browser.
///
/// Contains only non-sensitive values that the client needs in order to
/// render the map correctly (home QTH coordinates, time window, band palette).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConfig {
    /// Receiver QTH grid square, if configured.
    pub my_grid: Option<String>,
    /// Pre-computed latitude of the QTH grid centre.
    pub my_lat: Option<f64>,
    /// Pre-computed longitude of the QTH grid centre.
    pub my_lon: Option<f64>,
    /// Default time window in hours.
    pub time_window_hours: u32,
    /// Full band palette so the client can render the legend without
    /// duplicating the band definitions.
    pub bands: Vec<BandInfo>,
}

impl PublicConfig {
    /// Build a `PublicConfig` with zero spot counts (used when band counts are
    /// not yet available, e.g. on initial page load).
    pub fn new_without_counts(
        my_grid: Option<String>,
        time_window_hours: u32,
    ) -> Self {
        let (my_lat, my_lon) = my_grid
            .as_deref()
            .and_then(grid_to_latlon)
            .map(|ll| (Some(ll.lat), Some(ll.lon)))
            .unwrap_or((None, None));

        let bands = wspr_bands()
            .iter()
            .map(|b| BandInfo {
                name: b.name.to_string(),
                dial_hz: b.dial_hz,
                color: b.color.to_string(),
                spot_count: 0,
            })
            .collect();

        Self { my_grid, my_lat, my_lon, time_window_hours, bands }
    }
}

// ---------------------------------------------------------------------------
// SSR-only: raw ClickHouse row types and conversions
// ---------------------------------------------------------------------------

/// Raw deserialization target for a `wspr_spots` row read from ClickHouse.
///
/// Field names MUST match the ClickHouse column names exactly so that the
/// `clickhouse::Row` derive can map them via serde.
#[cfg(feature = "ssr")]
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
pub struct WsprSpotRow {
    pub window_start_unix: i64,
    pub time_utc: String,
    pub snr_db: i32,
    pub dt_sec: f32,
    pub freq_hz: f64,
    pub message: String,
    pub callsign: String,
    pub grid: String,
    pub power_dbm: i32,
    pub drift: i32,
    pub sync_quality: f32,
    pub npass: u8,
    pub osd_pass: u8,
    pub nhardmin: i32,
    pub decode_cycles: u32,
    pub candidates: u32,
    pub nfano: i32,
}

#[cfg(feature = "ssr")]
impl From<WsprSpotRow> for WsprSpot {
    fn from(r: WsprSpotRow) -> Self {
        Self {
            window_start_unix: r.window_start_unix,
            time_utc: r.time_utc,
            snr_db: r.snr_db,
            dt_sec: r.dt_sec,
            freq_hz: r.freq_hz,
            message: r.message,
            callsign: r.callsign,
            grid: r.grid,
            power_dbm: r.power_dbm,
            drift: r.drift,
            sync_quality: r.sync_quality,
            npass: r.npass,
            osd_pass: r.osd_pass,
            nhardmin: r.nhardmin,
            decode_cycles: r.decode_cycles,
            candidates: r.candidates,
            nfano: r.nfano,
            // Filled in by the server function after the query returns,
            // once the home QTH coordinates are available.
            distance_km: None,
        }
    }
}

/// Lightweight row used by the map query (fewer columns → less network traffic).
#[cfg(feature = "ssr")]
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
pub struct MapSpotRow {
    pub window_start_unix: i64,
    pub callsign: String,
    pub grid: String,
    pub freq_hz: f64,
    pub snr_db: i32,
    pub power_dbm: i32,
}

#[cfg(feature = "ssr")]
impl From<MapSpotRow> for Option<MapSpot> {
    /// Returns `None` when the grid is empty or does not parse as a valid
    /// Maidenhead locator (type-2 WSPR messages have no grid).
    fn from(r: MapSpotRow) -> Self {
        if r.grid.is_empty() {
            return None;
        }
        let ll = grid_to_latlon(&r.grid)?;
        let (band_name, band_color) = band_info_for(r.freq_hz);
        Some(MapSpot {
            window_start_unix: r.window_start_unix,
            callsign: r.callsign,
            grid: r.grid,
            lat: ll.lat,
            lon: ll.lon,
            freq_hz: r.freq_hz,
            snr_db: r.snr_db,
            power_dbm: r.power_dbm,
            band_name,
            band_color,
            // Filled in by the server function after the query returns.
            distance_km: None,
        })
    }
}

/// Row type for the stats aggregate query.
#[cfg(feature = "ssr")]
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
pub struct SpotStatsRow {
    pub total_spots: u64,
    pub unique_callsigns: u64,
    pub unique_grids: u64,
    pub oldest_unix: i64,
    pub newest_unix: i64,
}

#[cfg(feature = "ssr")]
impl From<SpotStatsRow> for SpotStats {
    fn from(r: SpotStatsRow) -> Self {
        Self {
            total_spots: r.total_spots,
            unique_callsigns: r.unique_callsigns,
            unique_grids: r.unique_grids,
            oldest_unix: r.oldest_unix,
            newest_unix: r.newest_unix,
        }
    }
}

/// Row type used when counting spots per frequency bucket.
#[cfg(feature = "ssr")]
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
pub struct FreqCountRow {
    pub freq_hz: f64,
    pub cnt: u64,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Return `(band_name, band_color)` for a given carrier frequency.
///
/// Falls back to `("unknown", "#808080")` when no standard band matches.
pub fn band_info_for(freq_hz: f64) -> (String, String) {
    find_band(freq_hz)
        .map(|b: &BandDef| (b.name.to_string(), b.color.to_string()))
        .unwrap_or_else(|| ("unknown".to_string(), "#808080".to_string()))
}
