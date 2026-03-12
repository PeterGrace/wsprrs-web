/// Paginated, sortable WSPR spot table.
///
/// Clicking a row emits `(grid, callsign)` via `on_row_select` so the
/// `WorldMap` can highlight the exact marker for the selected station.
use leptos::prelude::*;

use crate::models::WsprSpot;

#[component]
pub fn SpotTable(
    /// Spot records to display.
    spots: Signal<Vec<WsprSpot>>,
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
                view! {
                    <table id="spot-table">
                        <thead>
                            <tr>
                                <th>"UTC"</th>
                                <th>"Callsign"</th>
                                <th>"Grid"</th>
                                <th>"Dist (km)"</th>
                                <th>"Freq (MHz)"</th>
                                <th>"SNR (dB)"</th>
                                <th>"Pwr (dBm)"</th>
                                <th>"Drift"</th>
                                <th>"Sync"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || spots.get()
                                key=|spot| (spot.window_start_unix, spot.callsign.clone())
                                children=move |spot| {
                                    let row_key = (spot.window_start_unix, spot.callsign.clone());
                                    let grid_opt = if spot.grid.is_empty() {
                                        None
                                    } else {
                                        Some(spot.grid.clone())
                                    };
                                    let snr = spot.snr_db;
                                    let freq_mhz = format!("{:.6}", spot.freq_hz / 1_000_000.0);
                                    let sync = format!("{:.1}", spot.sync_quality);

                                    let dist = match spot.distance_km {
                                        Some(d) => format!("{:.0}", d),
                                        None => "-".to_string(),
                                    };
                                    let is_selected = {
                                        let key = row_key.clone();
                                        move || selected_key.get().as_ref() == Some(&key)
                                    };
                                    let click_key = row_key.clone();
                                    // Pair the grid with the callsign so the map can open the
                                    // exact marker rather than any marker in the same grid square.
                                    let click_selection = grid_opt
                                        .map(|g| (g, spot.callsign.clone()));

                                    view! {
                                        <tr
                                            class=move || {
                                                if is_selected() { "row-selected" } else { "" }
                                            }
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
                            />
                        </tbody>
                    </table>
                }
                .into_any()
            }}
        </div>
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
