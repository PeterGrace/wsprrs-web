/// Paginated WSPR spot table supporting both local (`WsprSpot`) and global
/// (`GlobalSpot`) data sources via the `AnySpot` tagged union.
///
/// When `is_global` is `false` the table renders the 9 local columns (UTC,
/// Callsign, Grid, Dist, Freq, SNR, Pwr, Drift, Sync).  When `is_global` is
/// `true` it renders 11 global columns (UTC, Callsign, Grid, Reporter,
/// Rptr Grid, Band, Freq, SNR, Pwr, Dist CH, Azimuth).
///
/// Clicking a row emits `(grid, callsign)` via `on_row_select` so the
/// `WorldMap` can highlight the exact marker for the selected station.
use leptos::prelude::*;

use crate::models::{AnySpot, GlobalSpot, WsprSpot};

#[component]
pub fn SpotTable(
    /// Spot records to display — may be all `Local` or all `Global` variants.
    spots: Signal<Vec<AnySpot>>,
    /// When `true`, render global columns; otherwise render local columns.
    is_global: Signal<bool>,
    /// Emitted with `(grid, callsign)` when the user clicks a row, or `None`
    /// when the row has no grid information.
    on_row_select: Callback<Option<(String, String)>>,
) -> impl IntoView {
    // Track the selected row by its unique (timestamp, callsign) key.
    let selected_key: RwSignal<Option<(i64, String)>> = RwSignal::new(None);

    view! {
        <div id="spot-table-container">
            {move || {
                let items = spots.get();
                if items.is_empty() {
                    return view! {
                        <p class="empty-state">"No spots match the current filter."</p>
                    }
                    .into_any();
                }

                let global = is_global.get();
                view! {
                    <table id="spot-table">
                        <thead>
                            <tr>
                                {if global {
                                    view! {
                                        <>
                                            <th>"UTC"</th>
                                            <th>"Callsign"</th>
                                            <th>"Grid"</th>
                                            <th>"Reporter"</th>
                                            <th>"Rptr Grid"</th>
                                            <th>"Band"</th>
                                            <th>"Freq (MHz)"</th>
                                            <th>"SNR (dB)"</th>
                                            <th>"Pwr (dBm)"</th>
                                            <th>"Dist CH (km)"</th>
                                            <th>"Azimuth"</th>
                                        </>
                                    }.into_any()
                                } else {
                                    view! {
                                        <>
                                            <th>"UTC"</th>
                                            <th>"Callsign"</th>
                                            <th>"Grid"</th>
                                            <th>"Dist (km)"</th>
                                            <th>"Freq (MHz)"</th>
                                            <th>"SNR (dB)"</th>
                                            <th>"Pwr (dBm)"</th>
                                            <th>"Drift"</th>
                                            <th>"Sync"</th>
                                        </>
                                    }.into_any()
                                }}
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || spots.get()
                                key=|spot| spot_key(spot)
                                children=move |spot| {
                                    match spot {
                                        AnySpot::Local(s) => {
                                            render_local_row(s, selected_key, on_row_select)
                                                .into_any()
                                        }
                                        AnySpot::Global(s) => {
                                            render_global_row(s, selected_key, on_row_select)
                                                .into_any()
                                        }
                                    }
                                }
                            />
                        </tbody>
                    </table>
                }
                .into_any()
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Row renderers
// ---------------------------------------------------------------------------

/// Render a table row for a local `WsprSpot`.
fn render_local_row(
    spot: WsprSpot,
    selected_key: RwSignal<Option<(i64, String)>>,
    on_row_select: Callback<Option<(String, String)>>,
) -> impl IntoView {
    let row_key = (spot.window_start_unix, spot.callsign.clone());
    let grid_opt = if spot.grid.is_empty() {
        None
    } else {
        Some(spot.grid.clone())
    };
    let snr = spot.snr_db;
    let freq_mhz = format!("{:.6}", spot.freq_hz / 1_000_000.0);
    let sync = format!("{:.1}", spot.sync_quality);
    let dist = format_dist(spot.distance_km);

    let is_selected = {
        let key = row_key.clone();
        move || selected_key.get().as_ref() == Some(&key)
    };
    let click_key = row_key.clone();
    let click_selection = grid_opt.map(|g| (g, spot.callsign.clone()));

    view! {
        <tr
            class=move || if is_selected() { "row-selected" } else { "" }
            on:click=move |_| {
                selected_key.set(Some(click_key.clone()));
                on_row_select.run(click_selection.clone());
            }
        >
            <td class="mono">{spot.time_utc.clone()}</td>
            <td class="callsign">{spot.callsign.clone()}</td>
            <td class="mono">{spot.grid.clone()}</td>
            <td class="mono">{dist}</td>
            <td class="mono">{freq_mhz}</td>
            <td class=snr_class(snr)>{snr.to_string()}</td>
            <td class="mono">{spot.power_dbm.to_string()}</td>
            <td class="mono">{spot.drift.to_string()}</td>
            <td class="mono">{sync}</td>
        </tr>
    }
}

/// Render a table row for a global `GlobalSpot`.
fn render_global_row(
    spot: GlobalSpot,
    selected_key: RwSignal<Option<(i64, String)>>,
    on_row_select: Callback<Option<(String, String)>>,
) -> impl IntoView {
    let row_key = (spot.timestamp_unix, spot.callsign.clone());
    let grid_opt = if spot.grid.is_empty() {
        None
    } else {
        Some(spot.grid.clone())
    };
    let snr = spot.snr;
    let freq_mhz = format!("{:.6}", spot.frequency);
    let dist_ch = spot.distance_ch.to_string();
    let time_str = format_unix_hhmm(spot.timestamp_unix);

    let is_selected = {
        let key = row_key.clone();
        move || selected_key.get().as_ref() == Some(&key)
    };
    let click_key = row_key.clone();
    let click_selection = grid_opt.map(|g| (g, spot.callsign.clone()));

    view! {
        <tr
            class=move || if is_selected() { "row-selected" } else { "" }
            on:click=move |_| {
                selected_key.set(Some(click_key.clone()));
                on_row_select.run(click_selection.clone());
            }
        >
            <td class="mono">{time_str}</td>
            <td class="callsign">{spot.callsign.clone()}</td>
            <td class="mono">{spot.grid.clone()}</td>
            <td class="callsign">{spot.reporter.clone()}</td>
            <td class="mono">{spot.reporter_grid.clone()}</td>
            <td class="mono">{spot.band_name.clone()}</td>
            <td class="mono">{freq_mhz}</td>
            <td class=snr_class(snr)>{snr.to_string()}</td>
            <td class="mono">{spot.power.to_string()}</td>
            <td class="mono">{dist_ch}</td>
            <td class="mono">{spot.azimuth.to_string()}</td>
        </tr>
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Build the cache key for a spot: `(timestamp_unix, callsign)`.
fn spot_key(spot: &AnySpot) -> (i64, String) {
    match spot {
        AnySpot::Local(s) => (s.window_start_unix, s.callsign.clone()),
        AnySpot::Global(s) => (s.timestamp_unix, s.callsign.clone()),
    }
}

/// Return a CSS class name based on the SNR value for colour-coding table cells.
fn snr_class(snr: i32) -> &'static str {
    match snr {
        i32::MIN..=-15 => "mono snr-weak",
        -14..=-5 => "mono snr-moderate",
        _ => "mono snr-strong",
    }
}

/// Format an optional distance in km; returns `"-"` when `None`.
fn format_dist(dist: Option<f64>) -> String {
    match dist {
        Some(d) => format!("{:.0}", d),
        None => "-".to_string(),
    }
}

/// Format a Unix timestamp as `"HHMM"` UTC, e.g. `"1430"`.
///
/// Returns `"----"` on conversion failure (should never occur for valid
/// Unix timestamps within the supported range).
fn format_unix_hhmm(unix: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(unix, 0)
        .single()
        .map(|dt| dt.format("%H%M").to_string())
        .unwrap_or_else(|| "----".to_string())
}
