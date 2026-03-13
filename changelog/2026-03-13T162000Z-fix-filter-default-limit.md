# Fix: SpotFilter::default() hardcoded limit=500 overrode server config

**Date:** 2026-03-13T16:20:00Z

## Problem

`SpotFilter::default()` set `limit: Some(500)`.  Because the query layer
computes `filter.limit.unwrap_or(spot_limit).min(spot_limit)`, a client-side
`Some(500)` always wins: `500.min(N) = 500` regardless of how large `N` is.
`WSPR_SPOT_LIMIT` and `WSPR_GLOBAL_SPOT_LIMIT` were therefore completely
ignored for every default (non-paginated) request.

## Fix

Changed `SpotFilter::default()` to `limit: None`.  The query layer's
`unwrap_or(spot_limit)` branch is now reached for default requests, making
the env-var caps the single source of truth.
