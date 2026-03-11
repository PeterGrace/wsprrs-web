# SSE Exponential Backoff Reconnection

**Date:** 2026-03-11T12:00:00

## Summary

Added automatic reconnection with exponential backoff to the client-side SSE
connection.  Previously, a network blip would leave the live badge stuck in
`Disconnected` and require the user to manually toggle the Live switch.

## Changes

### `src/components/live_badge.rs`

- Added `Reconnecting(u32)` variant to `LiveState`.  The inner value is the
  1-based reconnect attempt number (informational; currently shown as a generic
  "Reconnecting..." label).
- Updated `css_class` and `label` match arms for the new variant.

### `src/app.rs`

- Added `reconnect_attempt: RwSignal<u32>` to track consecutive failures.
- `on_open` callback now resets `reconnect_attempt` to 0 on successful
  handshake, so the backoff counter clears after each successful session.
- `on_error` callback now:
  1. Reads the current attempt count (untracked, to avoid reactive side-effects).
  2. After `MAX_RECONNECT_ATTEMPTS` (10) failures, transitions to `LiveState::Error`.
  3. Otherwise computes `delay_ms = min(1000 * 2^attempt, 30_000)` and
     schedules a `setTimeout` via `web_sys::Window`.
  4. Sets the badge to `LiveState::Reconnecting(attempt + 1)`.
  5. The timer callback guards on `LiveState::Reconnecting(_)` before
     transitioning to `Connecting`, so a pending timer is harmlessly discarded
     if the user switches live mode off mid-backoff.
- The `Reconnecting` arm in the `Effect` closes any stale `SseHandle` (the
  errored `EventSource`) so it is cleaned up before the fresh connection attempt.
- `Off | Error` arms reset `reconnect_attempt` to 0.

### `style/main.scss`

- Added `&--reconnecting` style block: inherits danger colour (red dot) with a
  slow pulse animation to visually distinguish it from the static `--error`
  state.

## Backoff Schedule

| Attempt | Delay   |
|---------|---------|
| 1       | 1 s     |
| 2       | 2 s     |
| 3       | 4 s     |
| 4       | 8 s     |
| 5       | 16 s    |
| 6–10    | 30 s    |
| > 10    | Error   |

## Pre-existing Clippy Fixes

- `src/app.rs:97` — replaced redundant closure `|| get_public_config()` with
  bare function reference `get_public_config`.
- `src/db/queries.rs` (×2) — replaced `|r| Option::<MapSpot>::from(r)` with
  `Option::<MapSpot>::from`.
