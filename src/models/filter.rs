use serde::{Deserialize, Serialize};

/// Whether to query personal receive data or the global WSPR spot network.
///
/// The `Default` variant is `Local` so that existing callers (and the UI
/// on first render) see personal data without any extra configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SpotSource {
    /// Personal receiver data from the `wspr_spots` table.
    #[default]
    Local,
    /// Worldwide spot data from the `global_spots` table.
    Global,
}

/// Query filter passed to all spot-fetching server functions.
///
/// All fields are optional; missing values mean "no constraint".  The struct
/// is serialised by Leptos server-function machinery for the client→server
/// round-trip, so all types must implement `Serialize + Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpotFilter {
    /// Data source: personal receive data (`Local`) or global network (`Global`).
    ///
    /// Defaults to `SpotSource::Local` when the field is absent from
    /// deserialized JSON (e.g. a request from an older client build that
    /// predates this field).
    #[serde(default)]
    pub source: SpotSource,

    /// Filter by callsign prefix (case-insensitive, trailing wildcard applied).
    /// A leading `!` negates the match (e.g. `"!K1ABC"` → exclude K1ABC%).
    pub callsign: Option<String>,

    /// Filter by reporter callsign (global mode only).
    /// A leading `!` negates the match (e.g. `"!W3POG"` → exclude W3POG%).
    pub reporter: Option<String>,

    /// Filter by grid prefix, e.g. `"FN20"` matches `"FN20"` and `"FN20eg"`.
    pub grid: Option<String>,

    /// Nominal WSPR dial frequency in Hz that identifies the desired band.
    /// When set, only spots within ±10 kHz of this value are returned (local),
    /// or the matching `band` integer column is used (global).
    pub band_hz: Option<u64>,

    /// Minimum SNR in dB (inclusive).
    pub snr_min: Option<i32>,

    /// Maximum transmitted power in dBm (inclusive).
    pub power_max: Option<i32>,

    /// Start of the time window as Unix epoch seconds.
    pub since_unix: Option<i64>,

    /// End of the time window as Unix epoch seconds.
    pub until_unix: Option<i64>,

    /// Maximum rows to return.  `None` defers to the server-configured cap
    /// (`WSPR_SPOT_LIMIT` for local, `WSPR_GLOBAL_SPOT_LIMIT` for global).
    pub limit: Option<u32>,

    /// Row offset for pagination.
    pub offset: Option<u32>,

    /// When `true`, exclude spots that have an empty grid field.
    pub grid_only: Option<bool>,
}

impl Default for SpotFilter {
    /// Sensible defaults: local source, no constraints.
    /// `limit: None` defers to the server-configured cap so that
    /// `WSPR_SPOT_LIMIT` / `WSPR_GLOBAL_SPOT_LIMIT` are the single source
    /// of truth for the maximum row count.
    fn default() -> Self {
        Self {
            source: SpotSource::Local,
            callsign: None,
            reporter: None,
            grid: None,
            band_hz: None,
            snr_min: None,
            power_max: None,
            since_unix: None,
            until_unix: None,
            limit: None,
            offset: Some(0),
            grid_only: Some(false),
        }
    }
}
