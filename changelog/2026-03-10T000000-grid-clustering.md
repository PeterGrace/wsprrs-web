# Grid-Square Cluster Markers

**Date:** 2026-03-10

## Summary

Replaced the static per-(grid, band) deduplication in `public/map.js` with a
zoom-aware clustering system.  Multiple stations that share a 4-character
Maidenhead grid square are now aggregated into a single marker until the user
zooms in far enough that individual stations can be displayed distinctly.

## Changes

### `public/map.js`

- **Added** `currentSpots` module-level variable to retain the last dataset so
  that `zoomend` events can trigger a redraw without a new server round-trip.
- **Added** `groupByGrid(spots)` — groups a flat spot array into a
  `Map<grid, spot[]>` keyed on the 4-char grid locator.
- **Added** `bestPerCallsign(spots)` — reduces a grid's spots to one
  representative per callsign (highest SNR), replacing the old
  `deduplicateSpots` (which deduped per grid+band).
- **Added** `gridCenter(grid)` — computes the geographic centre `[lat, lon]` of
  a 4-char Maidenhead square; used for stable cluster marker placement.
- **Added** `gridPixelWidth(lat, lon)` — returns the pixel width of a grid
  square at the current Leaflet zoom level using `map.project()`.
- **Added** `makeClusterMarker(grid, spots, lat, lon)` — creates an
  `L.divIcon`-backed `L.Marker` with a station-count badge and a popup listing
  all unique callsigns sorted by SNR.
- **Added** `buildClusterPopup(grid, spots)` — HTML builder for the cluster
  popup.
- **Added** `addHomeLine(destLat, destLon, color)` — extracted great-circle
  line drawing into a helper to support both individual and cluster modes.
- **Modified** `drawSpots(spots)` — now iterates over `groupByGrid` results;
  for each grid it calls `gridPixelWidth` and either expands to per-callsign
  circle markers (`>= EXPAND_THRESHOLD_PX = 80 px`) or renders a cluster badge.
- **Modified** `init()` — stores spots in `currentSpots` and registers a
  `map.on("zoomend")` listener to redraw on zoom.
- **Modified** `update()` — stores spots in `currentSpots` before drawing.
- **Modified** `highlight()` — works correctly for both cluster and individual
  markers; pans to the marker's `getLatLng()` regardless of type.
- **Removed** `deduplicateSpots` (superseded by `groupByGrid` + `bestPerCallsign`).

### `style/main.scss`

- **Added** `.grid-cluster` — styles for the cluster DivIcon:
  circle with `--cluster-color` CSS custom property, monospace count text,
  drop shadow, hover scale transition.

## Behaviour

| Zoom | Grid pixel width | Rendering |
|------|-----------------|-----------|
| ≤ 5  | < 80 px         | Cluster badge with station count |
| ≥ 6  | ≥ 80 px         | One circle marker per unique callsign |

The threshold of 80 px was chosen so that individual 5 px radius markers
(10 px diameter) within a single grid square have comfortable visual spacing
before expansion occurs.  The exact zoom crossover varies slightly with
latitude due to Mercator projection stretching.
