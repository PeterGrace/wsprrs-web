# Time Window Dropdown Defaults to Server Config

**Date:** 2026-03-11T16:00:00Z

## Problem

`WSPR_TIME_WINDOW_HOURS` was set to `24` in the deployment config, but the
time-window filter dropdown always initialised to "1 hour" on page load.  The
server used the env var as a query fallback, but the frontend had no knowledge
of it and used a hardcoded 1-hour default.

## Changes

### `src/app.rs`

- Added `default_window_secs: Signal<Option<i64>>` derived from
  `config_resource`, converting `PublicConfig.time_window_hours → seconds`.
- Passed the new signal to `FilterPanel` as the `default_window_secs` prop.

### `src/components/filter_panel.rs`

- Replaced hardcoded `DEFAULT_WINDOW_SECS: i64 = 3_600` with
  `DEFAULT_WINDOW_SECS_FALLBACK` (same value, used only while config is
  still loading).
- Added `default_window_secs: Signal<Option<i64>>` prop to `FilterPanel`.
- Added an `Effect` with a `config_applied` boolean guard that fires once
  when the config resolves: snaps `window_secs` and `filter.since_unix` to
  the server-configured value.
- Updated the Reset button to reset to `default_window_secs` (falling back
  to 1 hour) instead of the old hardcoded constant.

## Behaviour After Fix

| Stage | Dropdown shows |
|---|---|
| Initial render (config loading) | 1 hour (fallback) |
| After config resolves | Matches `WSPR_TIME_WINDOW_HOURS` (e.g. 24 hours) |
| After Reset | Matches `WSPR_TIME_WINDOW_HOURS` |
