# Band Dropdown: Static Band List, Remove Spot Count

**Date:** 2026-03-11T15:00:00Z

## Change

The band filter dropdown now shows all known WSPR bands statically rather than only bands that
currently have spots. The `(n spots)` label suffix has been removed.

**Before:** dropdown populated dynamically from a live ClickHouse query, showing only bands with
spots and labelling each with a count, e.g. `20m (145 spots)`.

**After:** dropdown populated from the static `wspr_bands()` table on page load. All standard
WSPR bands are always present. Selecting a band that happens to have no data simply returns
an empty result — no different from any other zero-result filter.

## Rationale

The dynamic approach had two correctness problems:

1. **Stale filter options:** Band presence was gated on a live count. With a 60-second cache TTL,
   a band that received its first spot could be invisible in the dropdown for up to 60 seconds
   (false negative). Conversely, a band whose spots fell outside a narrowed time window would
   linger in the dropdown until the cache expired (false positive).

2. **Misleading counts:** The count reflected the full, unfiltered dataset for the time window.
   When other filters (callsign, grid, SNR, etc.) were active the number bore no relation to
   how many spots would actually appear after selecting that band.

The simplest correct solution is to treat band selection as a pure filter predicate — the
dropdown offers all valid choices and the query result handles the "no data" case naturally.

## What Was Removed

- `query_band_counts` function (`src/db/queries.rs`)
- `get_band_counts` server function (`src/server_fns.rs`)
- `bands_resource` reactive resource and its `refetch()` calls (`src/app.rs`)
- `band_counts` TTL cache entry (`src/cache.rs`)
- `spot_count` field on `BandInfo` (`src/models/spot.rs`)
- `FreqCountRow` ClickHouse row type (`src/models/spot.rs`)

## What Replaced It

`bands_signal` in `src/app.rs` is now derived from `config_resource` (the existing
`PublicConfig` fetch), which already carries the full static band list via
`PublicConfig::new_without_counts`. No new network round-trip is required.

## Files Changed

- `src/components/filter_panel.rs`
- `src/app.rs`
- `src/server_fns.rs`
- `src/db/queries.rs`
- `src/cache.rs`
- `src/models/spot.rs`
