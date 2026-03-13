/// Latitude/longitude coordinate pair in decimal degrees (WGS-84).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    /// Decimal degrees, -90 to +90 (south negative).
    pub lat: f64,
    /// Decimal degrees, -180 to +180 (west negative).
    pub lon: f64,
}

/// Static descriptor for a standard WSPR amateur radio band.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BandDef {
    /// Human-readable band name, e.g. `"20m"`.
    pub name: &'static str,
    /// Nominal WSPR dial frequency in Hz.
    pub dial_hz: u64,
    /// CSS hex color string for map markers, derived algorithmically from band
    /// position (see [`wspr_bands`]).
    pub color: String,
}

/// Raw band definitions: `(name, dial_hz_in_Hz)`.
///
/// Colors are not stored here; they are computed by [`wspr_bands`] using an
/// HSL hue sweep so that each band gets a distinct, evenly-spaced color
/// regardless of how many bands are defined.
const BAND_RAW: &[(&str, u64)] = &[
    ("2200m", 136_000),
    ("630m", 474_200),
    ("160m", 1_836_600),
    ("80m", 3_592_600),
    ("60m", 5_287_200),
    ("40m", 7_038_600),
    ("30m", 10_138_700),
    ("20m", 14_095_600),
    ("17m", 18_104_600),
    ("15m", 21_094_600),
    ("12m", 24_924_600),
    ("10m", 28_124_600),
    ("6m", 50_293_000),
    ("4m", 70_091_000),
    ("2m", 144_489_000),
    ("70cm", 432_300_000),
    ("23cm", 1_296_500_000),
];

/// Golden angle in degrees: 360° × (1 − 1/φ), where φ is the golden ratio.
///
/// Successive multiples of this angle modulo 360° produce a sequence that is
/// maximally spread around the hue wheel — no two consecutive values are ever
/// close to each other, regardless of how many bands are defined.
const GOLDEN_ANGLE_DEG: f64 = 137.507_764_050_187_35;

/// Starting hue offset in degrees.
///
/// Shifting away from 0° (pure red) avoids having the first band land on a hue
/// that is very close to common UI chrome colors.
const HUE_OFFSET_DEG: f64 = 30.0;

/// Return the canonical list of WSPR bands with algorithmically derived colors.
///
/// Colors are computed once and cached via [`OnceLock`].  Each band's hue is
/// `(i × GOLDEN_ANGLE_DEG + HUE_OFFSET_DEG) mod 360°`, which distributes
/// colors as far apart as possible on the hue wheel for any number of bands.
/// Saturation is fixed at 100% and lightness at 55% for legibility on the dark
/// map background.
///
/// # Panics
///
/// Never panics; color computation is infallible for the fixed input range.
pub fn wspr_bands() -> &'static [BandDef] {
    use std::sync::OnceLock;
    static BANDS: OnceLock<Vec<BandDef>> = OnceLock::new();
    BANDS.get_or_init(|| {
        BAND_RAW
            .iter()
            .enumerate()
            .map(|(i, &(name, dial_hz))| {
                let hue = (i as f64 * GOLDEN_ANGLE_DEG + HUE_OFFSET_DEG) % 360.0;
                BandDef {
                    name,
                    dial_hz,
                    color: hsl_to_hex(hue, 1.0, 0.55),
                }
            })
            .collect()
    })
}

/// Convert an HSL color to a CSS hex string (`"#RRGGBB"`).
///
/// # Arguments
///
/// * `hue`        — degrees, 0–360
/// * `saturation` — 0.0–1.0
/// * `lightness`  — 0.0–1.0
fn hsl_to_hex(hue: f64, saturation: f64, lightness: f64) -> String {
    // Chroma
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    // Intermediate value X depends on which 60° sector the hue falls in.
    let h_sector = hue / 60.0;
    let x = c * (1.0 - (h_sector % 2.0 - 1.0).abs());
    let m = lightness - c / 2.0;

    let (r1, g1, b1) = match h_sector as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x), // sector 5 (300°–360°)
    };

    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Maximum frequency deviation (Hz) from a dial frequency before a spot is
/// considered to be on that band.
const BAND_TOLERANCE_HZ: f64 = 10_000.0;

/// Compute the great-circle distance in kilometres between two WGS-84
/// coordinates using the haversine formula.
///
/// # Arguments
///
/// * `lat1`, `lon1` — origin in decimal degrees
/// * `lat2`, `lon2` — destination in decimal degrees
///
/// # Examples
///
/// ```
/// use wsprrs_web::models::grid::haversine_km;
/// // Roughly New York to London: ~5,570 km
/// let d = haversine_km(40.7, -74.0, 51.5, -0.1);
/// assert!((d - 5_570.0).abs() < 50.0, "d={d}");
/// ```
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371.0; // mean Earth radius, km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().asin()
}

/// Find the standard WSPR band closest to `freq_hz`, within
/// [`BAND_TOLERANCE_HZ`].
///
/// Returns `None` if no band matches (i.e. the frequency is not a recognised
/// WSPR allocation).
///
/// # Examples
///
/// ```
/// use wsprrs_web::models::grid::find_band;
/// let band = find_band(14_095_800.0).expect("should match 20m");
/// assert_eq!(band.name, "20m");
/// ```
pub fn find_band(freq_hz: f64) -> Option<&'static BandDef> {
    wspr_bands()
        .iter()
        .filter(|b| (freq_hz - b.dial_hz as f64).abs() < BAND_TOLERANCE_HZ)
        .min_by(|a, b| {
            let da = (freq_hz - a.dial_hz as f64).abs();
            let db = (freq_hz - b.dial_hz as f64).abs();
            // Unwrap is safe: neither value is NaN given our filter above.
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Convert a 4- or 6-character Maidenhead grid locator to the **centre**
/// latitude/longitude of that grid cell.
///
/// # Arguments
///
/// * `grid` — Maidenhead locator, 4 chars (field + square) or 6 chars
///   (field + square + subsquare).  Case-insensitive.
///
/// # Returns
///
/// `Some(LatLon)` on success, `None` if `grid` is empty, the wrong length,
/// or contains out-of-range characters.
///
/// # Examples
///
/// ```
/// use wsprrs_web::models::grid::grid_to_latlon;
/// // FN20 covers roughly New York / New England
/// let ll = grid_to_latlon("FN20").expect("valid 4-char grid");
/// assert!((ll.lat - 40.5).abs() < 0.01);
/// assert!((ll.lon - (-75.0)).abs() < 0.01);
///
/// // 6-char adds subsquare precision (~12 km)
/// let ll6 = grid_to_latlon("FN20eg").expect("valid 6-char grid");
/// assert!((ll6.lat - 40.271).abs() < 0.01);
/// ```
pub fn grid_to_latlon(grid: &str) -> Option<LatLon> {
    if grid.len() < 4 {
        return None;
    }

    // Normalise to uppercase for consistent parsing.
    let grid = grid.to_uppercase();
    let b = grid.as_bytes();

    // --- Field pair (characters 1–2): letters A–R, each 20° lon × 10° lat ---
    let f_lon = b[0].checked_sub(b'A')?;
    let f_lat = b[1].checked_sub(b'A')?;
    if f_lon > 17 || f_lat > 17 {
        return None;
    }

    // --- Square pair (characters 3–4): digits 0–9, each 2° lon × 1° lat ---
    let s_lon = b[2].checked_sub(b'0')?;
    let s_lat = b[3].checked_sub(b'0')?;
    if s_lon > 9 || s_lat > 9 {
        return None;
    }

    let mut lon = -180.0 + f_lon as f64 * 20.0 + s_lon as f64 * 2.0;
    let mut lat = -90.0 + f_lat as f64 * 10.0 + s_lat as f64 * 1.0;

    if grid.len() >= 6 {
        // --- Subsquare pair (characters 5–6): letters A–X, each 5' lon × 2.5' lat ---
        let ss_lon = b[4].checked_sub(b'A')?;
        let ss_lat = b[5].checked_sub(b'A')?;
        if ss_lon > 23 || ss_lat > 23 {
            return None;
        }
        // Each subsquare is 2°/24 = 5' in longitude and 1°/24 = 2.5' in latitude.
        lon += ss_lon as f64 * (2.0 / 24.0) + (1.0 / 24.0);
        lat += ss_lat as f64 * (1.0 / 24.0) + (0.5 / 24.0);
    } else {
        // Centre of the 4-char grid cell: +1° lon, +0.5° lat
        lon += 1.0;
        lat += 0.5;
    }

    Some(LatLon { lat, lon })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_fn20_center() {
        let ll = grid_to_latlon("FN20").expect("FN20 is valid");
        // FN20: lon base = -180 + 5*20 + 2*2 + 1 = -75°, lat base = -90 + 13*10 + 0*1 + 0.5 = 40.5°
        assert!((ll.lon - (-75.0)).abs() < 0.001, "lon={}", ll.lon);
        assert!((ll.lat - 40.5).abs() < 0.001, "lat={}", ll.lat);
    }

    #[test]
    fn grid_fn20eg_six_char() {
        let ll = grid_to_latlon("FN20eg").expect("FN20eg is valid");
        // Should be more precise than the 4-char version
        assert!((ll.lon - (-75.0)).abs() < 1.0);
        assert!((ll.lat - 40.5).abs() < 1.0);
    }

    #[test]
    fn grid_case_insensitive() {
        let upper = grid_to_latlon("FN20EG").expect("upper");
        let lower = grid_to_latlon("fn20eg").expect("lower");
        assert!((upper.lat - lower.lat).abs() < 1e-10);
        assert!((upper.lon - lower.lon).abs() < 1e-10);
    }

    #[test]
    fn grid_empty_returns_none() {
        assert!(grid_to_latlon("").is_none());
        assert!(grid_to_latlon("FN").is_none());
    }

    #[test]
    fn grid_out_of_range_returns_none() {
        // 'S' is out of range for field characters (A-R = 0-17)
        assert!(grid_to_latlon("SN20").is_none());
    }

    #[test]
    fn find_band_20m() {
        let b = find_band(14_095_800.0).expect("should match 20m");
        assert_eq!(b.name, "20m");
    }

    #[test]
    fn haversine_ny_to_london() {
        // New York (40.7°N, 74.0°W) → London (51.5°N, 0.1°W) ≈ 5,570 km
        let d = haversine_km(40.7, -74.0, 51.5, -0.1);
        assert!((d - 5_570.0).abs() < 50.0, "d={d}");
    }

    #[test]
    fn haversine_same_point() {
        let d = haversine_km(40.5, -75.0, 40.5, -75.0);
        assert!(d < 1e-6, "d={d}");
    }

    #[test]
    fn find_band_out_of_tolerance() {
        // 100 MHz is not a WSPR band
        assert!(find_band(100_000_000.0).is_none());
    }
}
