# Fix: spot visibility regression and map init effect cleanup

**Date:** 2026-03-13T12:00:00Z

## Problems fixed

### 1. No spots visible on first load / after filter change (`SpotFilter.source` missing `#[serde(default)]`)

The `source: SpotSource` field added to `SpotFilter` in the global-mode feature had no
`#[serde(default)]` attribute.  Serde requires all non-`Option` struct fields to be present in
the JSON payload; if the field is absent the entire deserialization fails with a "missing field"
error.

This caused **every server-function call to fail** whenever the client and server were built from
different git commits (e.g. a freshly-deployed server binary paired with a cached WASM bundle from
the previous release).  The `ServerFnError` propagated silently: `spots_json` stayed `""`, the
map showed nothing, and switching between Local and Global modes appeared to do nothing because
both queries returned errors.

**Fix:** added `#[serde(default)]` to `source: SpotSource` in `src/models/filter.rs`.  Missing
fields now default to `SpotSource::Local`, matching the `Default` impl and restoring backward
compatibility with any older serialized filter payloads.

### 2. Redundant double map redraw on every spots update (`map.rs` init effect)

The init `Effect` in `WorldMap` tracked **both** `config_json` and `spots_json`.  Because the
update `Effect` also tracked `spots_json`, every spot change caused two full `drawSpots()` calls:
one from `init()` and one from `update()`.  More subtly, calling `init()` on every spot change
was semantically wrong — `init()` is responsible for map creation and home-marker lifecycle; those
operations belong only on config changes.

**Fix:** changed the init effect to read `spots_json` with `.get_untracked()` so it no longer
creates a reactive dependency on spots.  The effect now fires exclusively when `config_json`
changes (home QTH updates, reporter-override changes).  The update effect remains the sole
reactive consumer of `spots_json`, eliminating the redundant redraw.

## Files changed

- `src/models/filter.rs` — `#[serde(default)]` on `SpotFilter::source`
- `src/components/map.rs` — `spots_for_init.get_untracked()` in the init effect
