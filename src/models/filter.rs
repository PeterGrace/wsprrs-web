use serde::{Deserialize, Serialize};

/// Query filter passed to all spot-fetching server functions.
///
/// All fields are optional; missing values mean "no constraint".  The struct
/// is serialised by Leptos server-function machinery for the client→server
/// round-trip, so all types must implement `Serialize + Deserialize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotFilter {
    /// Filter by callsign prefix (case-insensitive, trailing wildcard applied).
    pub callsign: Option<String>,

    /// Filter by grid prefix, e.g. `"FN20"` matches `"FN20"` and `"FN20eg"`.
    pub grid: Option<String>,

    /// Nominal WSPR dial frequency in Hz that identifies the desired band.
    /// When set, only spots within ±10 kHz of this value are returned.
    pub band_hz: Option<u64>,

    /// Minimum SNR in dB (inclusive).
    pub snr_min: Option<i32>,

    /// Maximum transmitted power in dBm (inclusive).
    pub power_max: Option<i32>,

    /// Start of the time window as Unix epoch seconds.
    pub since_unix: Option<i64>,

    /// End of the time window as Unix epoch seconds.
    pub until_unix: Option<i64>,

    /// Maximum rows to return (hard-capped at 5 000 in query layer).
    pub limit: Option<u32>,

    /// Row offset for pagination.
    pub offset: Option<u32>,

    /// When `true`, exclude spots that have an empty grid field.
    pub grid_only: Option<bool>,
}

impl Default for SpotFilter {
    /// Sensible defaults: no constraints, limit 500, last-hour window is set
    /// by the query layer based on server config rather than in the filter.
    fn default() -> Self {
        Self {
            callsign: None,
            grid: None,
            band_hz: None,
            snr_min: None,
            power_max: None,
            since_unix: None,
            until_unix: None,
            limit: Some(500),
            offset: Some(0),
            grid_only: Some(false),
        }
    }
}
