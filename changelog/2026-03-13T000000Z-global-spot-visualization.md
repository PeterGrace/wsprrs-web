# Global WSPR Spot Visualization

**Date:** 2026-03-13T00:00:00Z

## Summary

Adds a **Local Receive / Global** source toggle, a reporter callsign filter
(global mode only), and dynamic home-QTH centering based on the reporter's
grid when that filter is active.

## Motivation

The app previously visualized only personal WSPR receive data from the local
`wspr_spots` table.  A new `global_spots` table is being populated with
worldwide spot data from the global WSPR network.  This release surfaces that
data through the same UI with minimal friction: a two-button source toggle and
an optional reporter filter are the only additions required of the user.

---

## Changed Files

### `src/models/filter.rs`

- Added `SpotSource` enum (`Local` | `Global`, derives `Default = Local`).
- Added `source: SpotSource` and `reporter: Option<String>` fields to
  `SpotFilter`.
- Updated `Default` impl: `source = SpotSource::Local`, `reporter = None`.

### `src/models/spot.rs`

- `MapSpot`: added `reporter: Option<String>` and `reporter_grid: Option<String>`
  (both `None` for local spots; populated for global spots so the JS popup can
  show "Reported by: W3POG (FN20)").
- `GlobalSpot`: new wire struct with all `global_spots` columns plus computed
  `distance_km`, `band_name`, `band_color`.
- `GlobalSpotRow` (SSR-only): ClickHouse row type for full global spot records.
- `GlobalMapSpotRow` (SSR-only): lightweight row for global map queries.
- `AnySpot` enum (`Local(WsprSpot)` | `Global(GlobalSpot)`): tagged union for
  the spot table, allowing a single `SpotTable` component to handle both
  sources.
- `From<GlobalSpotRow> -> GlobalSpot` and `From<GlobalMapSpotRow> -> Option<MapSpot>`
  conversions; frequency is treated as MHz (wsprnet convention) and multiplied
  by 1 000 000 when stored in `MapSpot.freq_hz`.

### `src/models/mod.rs`

- Re-exports `AnySpot`, `GlobalSpot`, `SpotSource`.

### `src/config.rs`

- Added `global_table: String` (from `WSPR_GLOBAL_TABLE` env var, default
  `"global_spots"`).

### `src/db/queries.rs`

- `query_global_map_spots()` — lightweight global map query with reporter
  filter support; returns `Vec<MapSpot>`.
- `query_global_spots()` — full global spot records with pagination.
- `query_reporter_suggestions()` — autocomplete for reporter callsigns.
- `append_reporter_filter()` — LIKE/NOT LIKE clause for `reporter` column.
- `append_global_shared_filters()` — like `append_shared_filters()` but uses
  `frequency`/`snr`/`power` column names and the integer `band` column for
  band filtering (via wsprnet band index) instead of frequency proximity.

### `src/cache.rs`

- Added `global_map_spots` and `global_spots` `TtlCache` entries (60 s TTL).
- `normalize_filter_key()` now preserves `source` and `reporter` so local/
  global queries and reporter-filtered queries never share a cache entry.
- Imports `GlobalSpot`.

### `src/server_fns.rs`

- `get_global_map_spots(filter)` — fetches `Vec<MapSpot>` from global table
  with home-QTH distance computation and 60 s caching.
- `get_global_spots(filter)` — fetches `Vec<GlobalSpot>` from global table
  with 60 s caching.
- `get_reporter_suggestions(prefix)` — uncached reporter autocomplete.

### `src/components/filter_panel.rs`

- **Source toggle** (two buttons: "Local Receive" / "Global") added above the
  Callsign input.  Switching to Local clears `filter.reporter` and the
  autocomplete prefix.
- **Reporter input** (shown only in Global mode via `<Show>`): plain text
  input linked to a `<datalist>` fed by `get_reporter_suggestions()`.
  Supports `!` negation prefix; the `!` is stripped before querying
  suggestions.
- Reset button now also clears `reporter_prefix`.

### `src/components/spot_table.rs`

- `SpotTable` now accepts `spots: Signal<Vec<AnySpot>>` and
  `is_global: Signal<bool>`.
- When `is_global` is `false`: renders 9 local columns (UTC, Callsign, Grid,
  Dist km, Freq MHz, SNR dB, Pwr dBm, Drift, Sync).
- When `is_global` is `true`: renders 11 global columns (UTC, Callsign, Grid,
  Reporter, Rptr Grid, Band, Freq MHz, SNR dB, Pwr dBm, Dist CH km, Azimuth).
- Row key uses `(timestamp_unix, callsign)` for both variants.
- `format_unix_hhmm()` helper converts Unix timestamp to `HHMM` UTC string
  for global spot time display.

### `src/app.rs`

- `map_spots_resource`: routes between `get_map_spots` and
  `get_global_map_spots` based on `debounced_filter.source`.
- `spots_resource`: routes between `get_spots` and `get_global_spots`,
  wrapping results in `AnySpot::Local` / `AnySpot::Global`.
- `reporter_home_override` derived signal: when in Global mode with a
  non-negated reporter filter, extracts `reporter_grid` from the first
  resolved map spot and calls `grid_to_latlon()` to produce an
  `Option<(f64, f64)>` override.
- `config_json` derived signal: injects the override lat/lon into a mutable
  copy of `PublicConfig` before serialization, causing the Leaflet map to
  re-center on the reporter's QTH.  `my_grid` is cleared in override mode
  so the home marker popup does not show the original receiver's grid.
- `is_global` derived signal passed to `SpotTable` as a new `is_global` prop.

### `public/map.js`

- `init()`: detects changes in home lat/lon (reporter QTH override) by
  comparing `newHome` against `homeLatLon`.  When a change is detected and
  the map exists, the old home marker is removed and recreated at the new
  position.  This handles the QTH override being applied, changed, or
  cleared without requiring a page reload.
- `buildPopup()`: appends a "Reporter: W3POG (FN20)" row when `spot.reporter`
  is present, using the new optional `reporter` / `reporter_grid` fields on
  `MapSpot`.

### `src/lib.rs` / `src/main.rs`

- Added `#![recursion_limit = "512"]` to both crate roots; required by the
  Leptos `view!` macro's deeply-nested type machinery now that the component
  tree is larger.

### `src/sse.rs`

- Fixed `clippy::doc_overindented_list_items` lint on a continuation line
  in the `start_sse` doc comment (pre-existing style issue, surfaced by the
  new `-D warnings` run against the hydrate target).

---

## Data Flow (reporter QTH override)

```
filter.source == Global && filter.reporter == "W3POG"
  ↓
get_global_map_spots(filter)  →  ClickHouse global_spots WHERE reporter LIKE 'W3POG%'
  ↓
Vec<MapSpot> { reporter_grid: Some("FN20"), ... }
  ↓
reporter_home_override signal  →  grid_to_latlon("FN20")  →  Some((lat, lon))
  ↓
config_json signal  →  PublicConfig { my_lat: lat, my_lon: lon, my_grid: None }
  ↓
call_js_init_map(configJson, spotsJson)
  ↓
map.js init():  homeChanged == true  →  remove old marker, create new at FN20
  ↓
drawSpots(spots):  great-circle lines originate from FN20
```

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `WSPR_GLOBAL_TABLE` | `global_spots` | ClickHouse table name for global spots |

All existing environment variables remain unchanged.

---

## Schema Notes

The `global_spots` table is expected to have the following columns:

| Column | Type | Notes |
|---|---|---|
| `timestamp` | DateTime | Two-minute window start (UTC) |
| `spot_id` | UInt64 | Unique spot identifier |
| `reporter` | String | Reporting station callsign |
| `reporter_grid` | String | Reporting station Maidenhead grid |
| `callsign` | String | Transmitting station callsign |
| `grid` | String | Transmitting station Maidenhead grid |
| `frequency` | Float64 | Carrier frequency **in MHz** |
| `snr` | Int32 | Signal-to-noise ratio (dB) |
| `power` | Int32 | Transmitted power (dBm) |
| `drift` | Int32 | Frequency drift (Hz/min) |
| `distance` | Int32 | Reporter-to-transmitter distance (km) |
| `azimuth` | Int32 | Azimuth from reporter to transmitter (degrees) |
| `band` | Int32 | wsprnet band index (0 = 2200 m, 7 = 20 m, …) |
| `version` | String | WSPR software version |
| `code` | Int32 | Decode quality code |

Adjust `GlobalSpotRow` / `GlobalMapSpotRow` field types in
`src/models/spot.rs` if your schema uses different ClickHouse types.

---

## Verification Checklist

- [x] `cargo build --features ssr` — no warnings
- [x] `cargo clippy --features ssr -- -D warnings` — clean
- [x] `cargo clippy --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings` — clean
- [x] `cargo fmt --check` — formatted
- [x] `cargo leptos build` — both SSR binary and WASM bundle compile
