# 2026-03-10T00:00:00 — Initial Implementation

## Overview

First full implementation of the WSPR Visualizer web interface.  Built on
Leptos 0.8 (nightly) with Axum SSR and WASM hydration.

## Architecture

```
Browser (Leptos WASM + Leaflet.js)
  └── /pkg/wsprrs-web.js + /pkg/wsprrs-web_bg.wasm
        ↕  JSON  (Leptos server functions over HTTP POST)
        ↕  SSE   (/api/stream — live spot stream)
Axum SSR server (wsprrs-web binary)
        ↕  ClickHouse HTTP RowBinary
ClickHouse (wspr_spots table, database "wsprrs")
```

## Files Created / Modified

### Source

| File | Description |
|---|---|
| `src/error.rs` | `AppError` with `thiserror`, `IntoResponse` impl |
| `src/config.rs` | Environment-variable config loader (SSR only) |
| `src/models/grid.rs` | Maidenhead → lat/lon conversion + `find_band()` |
| `src/models/spot.rs` | Shared structs: `WsprSpot`, `MapSpot`, `SpotStats`, `BandInfo`, `PublicConfig`; SSR-only `*Row` ClickHouse types |
| `src/models/filter.rs` | `SpotFilter` — all query constraints in one struct |
| `src/db/queries.rs` | Five ClickHouse query functions (map spots, spot list, stats, band counts, callsign autocomplete) |
| `src/server_fns.rs` | Five Leptos `#[server]` functions wrapping the query layer |
| `src/components/map.rs` | `<WorldMap>` — Leaflet bridge via `js_sys::Reflect` |
| `src/components/filter_panel.rs` | `<FilterPanel>` — callsign, grid, band, SNR, time, live toggle |
| `src/components/spot_table.rs` | `<SpotTable>` — paginated table with row-click selection |
| `src/components/stats_bar.rs` | `<StatsBar>` — total/unique/window summary |
| `src/components/live_badge.rs` | `<LiveBadge>` — SSE connection status indicator |
| `src/sse.rs` | Client-side `EventSource` wrapper (hydrate only) |
| `src/app.rs` | `shell()`, `App`, `HomePage` — full reactive layout |
| `src/lib.rs` | Crate root with feature-gated module declarations |
| `src/main.rs` | Axum SSR server with `Extension`-based state injection |
| `style/main.scss` | SDR/waterfall-inspired dark theme; Exo 2 + Inter + JetBrains Mono |
| `public/map.js` | Leaflet initialisation, band-coloured markers, great-circle lines |

### Configuration

| File | Description |
|---|---|
| `Cargo.toml` | Added deps: clickhouse, serde, chrono, thiserror, tower-http, tracing, async-stream, web-sys, js-sys |
| `.env` | Local dev defaults (ClickHouse URL, QTH grid, time window) |
| `.gitignore` | Added `.env` |

## Key Design Decisions

- **State injection**: ClickHouse client and config are injected as Axum
  `Extension` layers rather than a custom `AppState` struct.  Leptos server
  functions access them via `expect_context::<T>()` and the SSE handler uses
  `Extension<T>` extractors.  This avoids `FromRef` trait complexity.

- **Marker colour by band**: `find_band()` maps carrier frequency → standard
  WSPR band within ±10 kHz tolerance.  Each band has a distinct CSS colour
  constant in `WSPR_BANDS`.  `MapSpot` carries `band_name` and `band_color`
  pre-computed server-side.

- **Great-circle lines**: `public/map.js` implements the intermediate-point
  formula from Ed Williams' Aviation Formulary.  Lines are rendered with 60
  interpolated points per arc, styled as dashed `L.polyline` in the band's
  colour at 35% opacity.  Only drawn when `WSPR_MY_GRID` is set in env.

- **Deduplication**: `deduplicateSpots()` in `map.js` renders at most one
  marker per (grid, band) pair, keeping the highest-SNR instance to avoid
  marker clutter over the same grid square.

- **SSE live stream**: `/api/stream` polls ClickHouse every 120 s (aligned
  to WSPR windows).  New spots are pushed as `event: spots` JSON arrays.
  Heartbeats keep the connection alive during quiet windows.

- **Default time window**: 1 hour, configurable via `WSPR_TIME_WINDOW_HOURS`.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `WSPR_CLICKHOUSE_URL` | `http://10.174.3.247:8123` | ClickHouse server URL |
| `WSPR_CLICKHOUSE_DB` | `wsprrs` | Database name |
| `WSPR_CLICKHOUSE_TABLE` | `wspr_spots` | Table name |
| `WSPR_MY_GRID` | — | Home QTH grid (enables great-circle lines) |
| `WSPR_TIME_WINDOW_HOURS` | `1` | Default page-load time window |
| `LEPTOS_SITE_ADDR` | `127.0.0.1:3000` | Server listen address |

## Build

```bash
# Development (hot-reload)
cargo leptos watch

# Release
cargo leptos build --release
```

## Tests

```bash
cargo test --features ssr
```

Unit tests cover Maidenhead grid conversion and band-frequency matching.
