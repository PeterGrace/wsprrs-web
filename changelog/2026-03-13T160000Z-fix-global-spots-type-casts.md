# Fix: global spots table query type mismatch

**Date:** 2026-03-13T16:00:00Z

## Problem

`get_global_spots` returned `ServerError("global spots query")` while
`get_global_map_spots` (and therefore map markers) succeeded.  The table below
the map showed no data in global mode.

The root cause was a type mismatch between the actual ClickHouse column types in
the `global_spots` table and the types declared in `GlobalSpotRow`.  The
`clickhouse-rs` crate uses `RowBinaryWithNamesAndTypes` format, which encodes
types explicitly; if the received type does not match the Rust field type the
deserializer returns an error.

`query_global_map_spots` (the map query) only selects `snr`, `power`,
`frequency`, `callsign`, `grid`, `reporter`, `reporter_grid`, and `timestamp` —
columns whose types happened to match.  `query_global_spots` additionally
selects `spot_id`, `drift`, `distance`, `azimuth`, `band`, `version`, and
`code`.  Depending on the wsprnet schema version, these columns may be stored as
`Int8`, `UInt32`, etc. rather than the `Int32`/`UInt64` the Rust struct expects.

The underlying ClickHouse error was invisible to the client because
`anyhow::Error::to_string()` returns only the outermost context string
(`"global spots query"`), hiding the chain.  The server-side log at
`tracing::error!("get_global_spots query failed: {e:#}")` shows the full chain.

## Fix

Added explicit casts to every integer column in the `query_global_spots` SELECT
list, consistent with how `timestamp` is already cast via
`toInt64(toUnixTimestamp(timestamp))`:

| Column     | Cast applied              |
|------------|---------------------------|
| `spot_id`  | `toUInt64(spot_id)`       |
| `snr`      | `toInt32(snr)`            |
| `power`    | `toInt32(power)`          |
| `drift`    | `toInt32(drift)`          |
| `distance` | `toInt32(distance)`       |
| `azimuth`  | `toInt32(azimuth)`        |
| `band`     | `toInt32(band)`           |
| `code`     | `toInt32(code)`           |

This makes the query robust against all known `global_spots` schema variants
without changing the `GlobalSpotRow` struct or the deserialization path.
