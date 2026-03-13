# Fix: global query UInt32/i64 type mismatch and add row-count logging

**Date:** 2026-03-13T15:20:00Z

## Problems

1. **Type mismatch** — `toUnixTimestamp()` returns `UInt32` in ClickHouse, but
   `GlobalMapSpotRow.timestamp_unix` and `GlobalSpotRow.timestamp_unix` are declared
   as `i64` in Rust.  ClickHouse 26.x RowBinary deserialization is strict about types;
   this could cause silent failures or incorrect values.

2. **i64::MAX upper bound** — when `filter.until_unix` is `None`, the query included
   `toUnixTimestamp(timestamp) <= 9223372036854775807` (`i64::MAX`).  Comparing a
   `UInt32` column to a literal larger than `UInt32::MAX` triggers unexpected
   type-coercion behaviour in ClickHouse 26.x.

## Fixes

- Changed `toUnixTimestamp(timestamp)` → `toInt64(toUnixTimestamp(timestamp))` in
  both `query_global_map_spots` and `query_global_spots` SELECT lists, so the
  emitted column type matches the Rust `i64` field.
- Removed the unconditional upper-bound `<= i64::MAX` clause; the upper bound is
  now only appended when `filter.until_unix` is explicitly set.
- Added `tracing::info!` logs at query entry (table, since, limit) and on
  completion (row count returned) to distinguish data-absence from render bugs.
