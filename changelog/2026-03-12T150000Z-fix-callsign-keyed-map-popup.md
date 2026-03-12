# Fix: callsign-keyed map popup on table row click

**Date:** 2026-03-12T15:00:00Z

## Problem

When two stations shared the same 4-character Maidenhead grid square (e.g. W4EO
and WA3DNM both in FN20), clicking either row in the spot table would open the
popup for whichever marker `markerLayer.eachLayer` happened to visit last —
not necessarily the one the user clicked.

## Root Cause

`wsprMap.highlight(grid)` in `public/map.js` matched markers only by grid
square and called `layer.openPopup()` on **every** match.  With co-located
stations this opened both popups, leaving the last one on top regardless of
which row was clicked.

The Rust callback chain passed only the grid string — callsign was discarded
after it was used to build the row key for UI highlighting.

## Fix

### `public/map.js`
- `makeMarker`: added `_callsign: spot.callsign` to each individual
  `L.circleMarker` options object so the callsign is queryable during highlight.
- `highlight(grid, callsign)`: new `callsign` parameter.  For non-cluster
  markers, skips any marker whose `_callsign` does not match (case-insensitive).
  Cluster markers (`_isCluster: true`) are still opened as a fallback when the
  grid has not yet expanded to individual markers.

### `src/components/spot_table.rs`
- `on_row_select` callback type changed from `Callback<Option<String>>` to
  `Callback<Option<(String, String)>>` where the tuple is `(grid, callsign)`.
- Click handler now emits `grid_opt.map(|g| (g, spot.callsign.clone()))`.

### `src/app.rs`
- `selected_grid` signal type updated from `RwSignal<Option<String>>` to
  `RwSignal<Option<(String, String)>>`.

### `src/components/map.rs`
- `selected_grid` prop type updated to `Option<Signal<Option<(String, String)>>>`.
- Effect destructures `(grid, callsign)` and passes both to the JS bridge.
- `call_js_highlight_grid(grid, callsign)` now uses `hl_fn.call2(...)` to
  forward the callsign to `wsprMap.highlight`.

## Behaviour After Fix

- Clicking W4EO opens W4EO's popup even when WA3DNM shares the same grid.
- When the grid is still in cluster mode the cluster popup opens as before.
