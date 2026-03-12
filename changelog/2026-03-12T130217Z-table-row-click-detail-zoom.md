# Table Row Click — Detail Zoom

**Date:** 2026-03-12T130217Z

## Summary

Clicking a row in the spot table now zooms the map to a configurable "detail zoom" level and opens the station's popup, matching the behaviour of clicking a marker directly on the map.

## Motivation

Previously, clicking a table row called `wsprMap.highlight(grid)` which only panned the map to the selected grid square without changing the zoom level.  Users had to manually zoom in to see station details.  The new behaviour zooms to a useful detail level automatically.

## Changes

### `config.rs`
- Added `detail_zoom: u8` field to `Config`, loaded from `WSPR_DETAIL_ZOOM` (default: `10`).

### `src/models/spot.rs`
- Added `detail_zoom: u8` field to `PublicConfig` so it is serialised and forwarded to the browser.
- Updated `PublicConfig::new_without_counts` to accept a `detail_zoom` parameter.

### `src/server_fns.rs`
- `get_public_config` now passes `config.detail_zoom` into `PublicConfig::new_without_counts`.

### `public/map.js`
- Added module-level `detailZoom` variable (default `10`).
- `init()` now reads `config.detail_zoom` and stores it in `detailZoom`.
- `highlight(grid)` now calls `map.setView(center, Math.max(currentZoom, detailZoom))` so the map zooms to at least `detailZoom` (but never zooms out if the user is already at a higher zoom), then opens all matching popups.  The `gridCenter()` helper is used to compute the target latlng, which works correctly even when the grid is currently rendered as a collapsed cluster marker.

### `.env`
- Documented `WSPR_DETAIL_ZOOM` as a commented-out default.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `WSPR_DETAIL_ZOOM` | `10` | Leaflet zoom level applied on table row click.  Map zooms to `max(currentZoom, detailZoom)`. |

## Behaviour Notes

- If the user is already zoomed in past `WSPR_DETAIL_ZOOM`, the zoom level is unchanged — the map only pans.
- The popup opens for whichever marker type is currently rendered for that grid (cluster or individual), identical to clicking the marker directly.
