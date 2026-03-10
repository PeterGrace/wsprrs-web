# WSPR Visualizer (wsprrs-web) — Project Memory

## Stack
- Leptos 0.8.0 nightly + Axum 0.8 SSR + WASM hydration
- ClickHouse HTTP client (`clickhouse = "0.12"`)
- Leaflet 1.9.4 (CDN, vanilla JS bridge via `js_sys::Reflect`)
- SCSS: dark SDR theme, Exo 2 / Inter / JetBrains Mono fonts

## Key Paths
- `src/models/grid.rs` — Maidenhead conversion, `WSPR_BANDS` constants
- `src/models/spot.rs` — `WsprSpot`, `MapSpot`, SSR-only `*Row` ClickHouse types
- `src/db/queries.rs` — all SQL (dynamic string building with sanitised string inputs)
- `src/server_fns.rs` — Leptos `#[server]` wrappers
- `src/main.rs` — Axum setup; uses `Extension` not `AppState` for DB/config
- `public/map.js` — `window.wsprMap.{init,update,highlight}` JS API

## Architecture Notes
- ClickHouse + config injected as Axum `Extension` layers (NOT custom AppState)
  → avoids FromRef complexity; SSE handler uses `Extension<T>` extractors
  → Leptos server fns use `expect_context::<T>()` from context closure
- `AppState` / `FromRef` approach was abandoned due to leptos_axum 0.8 API surface
- SSE at `/api/stream`: polls every 120s, emits `event: spots` JSON arrays

## ClickHouse Schema
- Database: `wsprrs`, Table: `wspr_spots`
- Server: `http://10.174.3.247:8123` (no auth)
- Key fields: `window_start_unix i64`, `callsign String`, `grid String`,
  `freq_hz f64`, `snr_db i32`, `power_dbm i32`

## User Preferences
- Home QTH: `FN20eg` (lat≈40.271°N, lon≈-75.625°W)
- Default time window: 1 hour
- Great-circle lines: enabled when `WSPR_MY_GRID` set

## Build Commands
```bash
cargo leptos watch          # dev hot-reload
cargo test --features ssr   # unit tests
cargo build --features ssr  # SSR binary only
cargo build --features hydrate --target wasm32-unknown-unknown  # WASM only
```

## Resource Pattern
- Use `LocalResource::new(move || async_fn(...))` (single-arg form), NOT `Resource::new(source, fetcher)`
- `LocalResource` skips SSR data embedding — correct for live dashboards; avoids
  "reading resource outside <Suspense/>" warnings and hydration mismatches
- `.get()` still returns `Option<Result<T, ServerFnError>>`; use `.and_then(|r| r.ok())`
- `.refetch()` still works on `LocalResource`
- Never add explicit `Resource<T>` / `LocalResource<T>` type annotations — let inference work

## Known Leptos 0.8 Gotchas
- `Callback<In>::run(input)` not `.call(input)`
- Explicit `Resource<T>` annotation strips `Result` wrapper — let type inference work
- `For` component inside `Show::fallback=move || view!{}` causes parse errors;
  use `{move || if cond { ... }.into_any() else { ... }.into_any()}` instead
- `#[prop(optional)]` on `Option<T>` props: pass `T` at call site, not `Some(T)`
