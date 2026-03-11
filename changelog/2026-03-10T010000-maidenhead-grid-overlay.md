# Maidenhead Grid Overlay

**Date:** 2026-03-10

## Summary

Added a configurable Maidenhead grid overlay to the Leaflet map.  The overlay
is zoom-adaptive: at low zoom it draws field boundaries with 2-char labels,
at medium zoom it draws 4-char square boundaries, and at high zoom it adds
6-char subsquare boundaries.  The overlay is togglable from both a sidebar
checkbox and the built-in Leaflet layer control (the layers icon in the map
corner).

## Changes

### `public/map.js`

- **Added** grid step constants: `FIELD_LON/LAT_STEP`, `SQUARE_LON/LAT_STEP`,
  `SUBSQ_LON/LAT_STEP`.
- **Added** `snapDown(v, step)` — integer-index-safe floor-to-multiple-of-step
  helper; avoids floating-point drift in grid line loops.
- **Added** `cellToField(lat, lon)` — encodes SW corner to 2-char Maidenhead
  field label (e.g. `"FN"`).
- **Added** `cellToSquare(lat, lon)` — encodes SW corner to 4-char square label
  (e.g. `"FN20"`).
- **Added** `cellToSubsquare(lat, lon)` — encodes SW corner to 6-char
  subsquare label (e.g. `"FN20aa"`); subsquare indices computed as
  `floor((lonNorm % 2) * 12)` and `floor((latNorm % 1) * 24)` (24 cells per
  2° lon / 1° lat square).
- **Added** `drawGridLevel(latStep, lonStep, lineStyle, labelFn, showLabels)` —
  draws horizontal and vertical polylines spanning the visible viewport
  (O(rows + cols) elements, not O(rows × cols)), plus optional cell-centre
  labels.
- **Added** `drawGrid()` — orchestrates three grid levels based on zoom:
  - Field boundaries always; opacity reduced at zoom > 4.
  - Square boundaries at zoom ≥ 3; labels at zoom ≥ 7.
  - Subsquare boundaries at zoom ≥ 9; labels at zoom ≥ 13.
- **Added** `onViewChange()` — merged zoom/pan handler that redraws both spots
  and grid, replacing the previous separate `zoomend` listener.
- **Modified** `init()`:
  - Creates `gridLayer = L.layerGroup()` (initially not added to map).
  - Registers `L.control.layers` with `"Maidenhead Grid"` as an overlay,
    providing a built-in Leaflet toggle in the map corner.
  - Listens to `overlayadd` / `overlayremove` to set `gridOverlayEnabled` and
    redraw or clear.
  - Changed `map.on("zoomend", ...)` to `map.on("zoomend moveend", onViewChange)`.
- **Added** `wsprMap.setGridOverlay(enabled)` — public API called from
  Rust/WASM; adds or removes `gridLayer` from the map, which fires the
  `overlayadd`/`overlayremove` events to keep state consistent.

### `src/components/map.rs`

- **Added** `grid_overlay: Signal<bool>` prop (defaults to `false`).
- **Added** `Effect` that calls `call_js_set_grid_overlay(bool)` whenever the
  signal changes.
- **Added** `call_js_set_grid_overlay(enabled: bool)` — JS bridge calling
  `window.wsprMap.setGridOverlay(enabled)`.

### `src/components/filter_panel.rs`

- **Added** `grid_overlay: RwSignal<bool>` prop.
- **Added** "Map" section heading in the sidebar.
- **Added** "Maidenhead grid" checkbox that writes to `grid_overlay`.

### `src/app.rs`

- **Added** `grid_overlay: RwSignal<bool>` signal (default `false`).
- Passed `grid_overlay` to `FilterPanel` and `WorldMap`.

### `style/main.scss`

- **Added** `.grid-label` — monospace teal label centred via
  `transform: translate(-50%, -50%)`, with semi-transparent dark background,
  `pointer-events: none` so labels never intercept map clicks, light-mode
  override.

## Grid level reference

| Zoom | Lines drawn            | Labels shown |
|------|------------------------|--------------|
| 1–2  | Field (20° × 10°)      | Field (2-char) |
| 3–4  | Field + Square         | Field only |
| 5–6  | Field + Square         | none |
| 7–8  | Field + Square         | Square (4-char) |
| 9–12 | Field + Square + Subsq | Square |
| ≥ 13 | Field + Square + Subsq | Subsquare (6-char) |
