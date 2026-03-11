# Callsign Ignore List

**Date:** 2026-03-11T14:00:00

## Summary

Added server-side callsign ignore list, allowing one or more callsigns to be
excluded from all query results via an environment variable.  This is useful
for hiding a local receive station's own callsign from the web interface.

## Configuration

Set `WSPR_IGNORE_CALLSIGNS` in `.env` (or the process environment) to a
comma-separated list of callsigns to suppress:

```
WSPR_IGNORE_CALLSIGNS=W3POG
WSPR_IGNORE_CALLSIGNS=W3POG,N0CALL,KD9FOO
```

- Values are trimmed of whitespace and normalised to uppercase at startup.
- The variable is optional; omitting it (or leaving it empty) disables the
  filter entirely.

## Files Changed

| File | Change |
|------|--------|
| `src/config.rs` | Added `ignore_callsigns: Vec<String>` field; parsed from `WSPR_IGNORE_CALLSIGNS` |
| `src/db/queries.rs` | Added `append_ignore_callsigns()` helper; threaded `ignore_callsigns: &[String]` through all six public query functions |
| `src/server_fns.rs` | Passes `&config.ignore_callsigns` to every query call |
| `src/main.rs` | Passes `config.ignore_callsigns.clone()` into `spot_poll_task`; SSE live stream now also filters ignored callsigns |
| `.env` | Added `WSPR_IGNORE_CALLSIGNS=W3POG` |

## Configurable Spot Limit

Added `WSPR_SPOT_LIMIT` (default: `5000`) to replace the previous hardcoded
`500` row limits in `query_spots` and `query_new_spots`.  The value acts as
both the default (when no limit is specified in the filter) and the hard cap
on caller-supplied limits.

```
# .env
WSPR_SPOT_LIMIT=5000
```

All three spot-returning queries (`query_map_spots`, `query_spots`,
`query_new_spots`) now respect this setting.

## Implementation Details

- Filtering occurs at the **SQL level** in ClickHouse (`upper(callsign) NOT IN (...)`)
  so no ignored spots are transferred over the network.
- Applied to: map spots, table spots, aggregate stats, per-band counts,
  callsign autocomplete suggestions, and SSE live-stream events.
- Each callsign is run through the existing `sanitise_callsign()` function
  before embedding in SQL to prevent injection.
