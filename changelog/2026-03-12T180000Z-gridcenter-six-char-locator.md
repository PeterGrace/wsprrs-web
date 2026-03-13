# Fix: `gridCenter()` now resolves six-character Maidenhead locators

**Date:** 2026-03-12T18:00:00Z

## Problem

`gridCenter()` in `public/map.js` only handled 4-character Maidenhead grid
squares (field + square). When a spot carried a 6-character locator (field +
square + subsquare), the subsquare pair (characters 5–6) was silently ignored
and the marker was placed at the centre of the coarser 4-character cell
(~220 km precision) instead of the subsquare centre (~12 km precision).

8-character locators are still truncated to 6 characters by the database
sanitiser (`sanitise_locator()` in `src/db/queries.rs`), so only 4- and
6-character grids are relevant on the client side.

## Fix (`public/map.js`)

Extended `gridCenter()` to mirror the logic already present in the Rust
`grid_to_latlon()` function (`src/models/grid.rs`):

- If `grid.length >= 6`, decode the subsquare pair (letters A–X, 0–23) and
  apply the offsets:
  - longitude: `ssLon * (2.0 / 24.0) + (1.0 / 24.0)`
  - latitude:  `ssLat * (1.0 / 24.0) + (0.5 / 24.0)`
- Otherwise, fall back to the 4-character cell centre (+1.0° lon, +0.5° lat).

The constants are identical to the Rust implementation, ensuring client and
server agree on marker positions.

## Files changed

- `public/map.js` — `gridCenter()` updated to handle 4- and 6-character locators
