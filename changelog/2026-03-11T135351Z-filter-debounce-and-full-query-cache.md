# Filter Debounce + Full Query Cache

**Date:** 2026-03-11T135351Z

## Problem

Two related issues allowed users to inadvertently (or deliberately) flood ClickHouse:

1. **No debounce** — every `on:input` event on text boxes and the SNR slider fired
   an immediate server request.  Typing five characters in the callsign box sent
   ten ClickHouse queries (map_spots + spots × 5 keystrokes).

2. **Partial cache coverage** — `get_stats` and `get_band_counts` were cached for
   30 seconds, but the two most expensive queries (`get_map_spots`, `get_spots`)
   were entirely uncached.  N concurrent users with the same default view each
   generated N independent ClickHouse round-trips per refresh.

## Changes

### `src/models/filter.rs`
- Derived `Eq` and `Hash` on `SpotFilter` so it can serve as a `HashMap` key
  in the new cache entries.

### `src/cache.rs`
- Added `map_spots: TtlCache<SpotFilter, Vec<MapSpot>>` (60 s TTL).
- Added `spots: TtlCache<SpotFilter, Vec<WsprSpot>>` (60 s TTL).
- Bumped `stats` and `band_counts` TTLs from 30 s → 60 s for consistency.
- Added `QueryCache::normalize_filter_key(filter, default_since) -> SpotFilter`
  which resolves `None` timestamps to the server's configured default and rounds
  all timestamps to the nearest 60-second boundary.  This ensures that the
  "default view" (no explicit `since_unix`) maps to the same cache key for all
  users regardless of the exact wall-clock second their request arrived.

### `src/server_fns.rs`
- `get_map_spots`: check/populate `cache.map_spots` using the normalised key.
- `get_spots`: check/populate `cache.spots` using the normalised key.
- Moved `grid_to_latlon` / `haversine_km` imports inside the function bodies to
  eliminate a spurious "unused import" warning in WASM builds (where `#[server]`
  bodies are stripped by the macro).

### `src/app.rs`
- Added `debounced_filter: RwSignal<SpotFilter>`.
- Added a `#[cfg(feature = "hydrate")]` Effect that watches `filter`, cancels
  any pending `setTimeout`, and schedules a 1 000 ms delayed write to
  `debounced_filter`.  Only the settled value after the user stops typing/dragging
  triggers a server request.
- All four `LocalResource` definitions (`map_spots`, `spots`, `stats`, `bands`)
  now read `debounced_filter` instead of `filter`.

## Behaviour After This Change

| Scenario | Before | After |
|---|---|---|
| User types 5-char callsign | 10 CH queries | 1 CH query (after 1 s pause) |
| User drags SNR slider 40 steps | 80 CH queries | 1 CH query |
| 100 users load default view simultaneously | 200 CH queries/min | ≤1 CH query/min |
| Explicit Refresh button | immediate | immediate (bypasses debounce) |
| SSE new-data event | immediate refetch | immediate refetch |

The cache is global to all server users.  Two users with identical normalised
filters share a single cache entry; the second user's request costs zero CH I/O.
