/// Filter panel component: callsign search, grid search, band selector,
/// SNR slider, relative time-window selector, and a live-mode toggle.
///
/// All controls write into a shared `RwSignal<SpotFilter>`.  Components that
/// display data (`WorldMap`, `SpotTable`, `StatsBar`) read the same signal so
/// they update automatically whenever the filter changes.
use leptos::prelude::*;

use crate::models::{BandInfo, SpotFilter};

/// Predefined relative time windows shown in the time selector.
///
/// Each tuple is `(label, seconds_back)`.
const TIME_WINDOWS: &[(&str, i64)] = &[
    ("1 hour",   3_600),
    ("2 hours",  7_200),
    ("4 hours",  14_400),
    ("8 hours",  28_800),
    ("12 hours", 43_200),
    ("24 hours", 86_400),
    ("48 hours", 172_800),
    ("7 days",   604_800),
];

/// Default time window in seconds (1 hour).
const DEFAULT_WINDOW_SECS: i64 = 3_600;

#[component]
pub fn FilterPanel(
    /// Shared filter state — reads and writes for every control.
    filter: RwSignal<SpotFilter>,
    /// Band list returned from the server (name, dial_hz, color, count).
    bands: Signal<Vec<BandInfo>>,
    /// Callback invoked when the user clicks the Refresh button.
    on_refresh: Callback<()>,
    /// Callback invoked when the user toggles the live-stream on/off.
    on_live_toggle: Callback<bool>,
    /// Current live-stream state.
    live: Signal<bool>,
    /// Whether the Maidenhead grid overlay is enabled on the map.
    grid_overlay: RwSignal<bool>,
) -> impl IntoView {
    // -----------------------------------------------------------------------
    // Local state
    // -----------------------------------------------------------------------

    // Tracks which relative window (seconds) is currently selected so the
    // `<select>` can reflect the correct option after a Reset.
    let window_secs: RwSignal<i64> = RwSignal::new(DEFAULT_WINDOW_SECS);

    // -----------------------------------------------------------------------
    // Derived readable signals for individual filter fields
    // -----------------------------------------------------------------------

    let callsign = move || filter.read().callsign.clone().unwrap_or_default();
    let grid_val = move || filter.read().grid.clone().unwrap_or_default();
    let snr_min = move || filter.read().snr_min.unwrap_or(-30);
    let grid_only = move || filter.read().grid_only.unwrap_or(false);

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------
    view! {
        <aside id="filter-panel">
            <h2 class="panel-title">"Filters"</h2>

            // --- Callsign ---
            <div class="filter-group">
                <label for="filter-callsign">"Callsign"</label>
                <input
                    id="filter-callsign"
                    type="text"
                    placeholder="e.g. K1ABC or !K1ABC"
                    maxlength="21"
                    value=callsign
                    on:input=move |ev| {
                        let val = event_target_value(&ev);
                        filter.update(|f| {
                            f.callsign = if val.is_empty() { None } else { Some(val) };
                        });
                    }
                />
            </div>

            // --- Grid ---
            <div class="filter-group">
                <label for="filter-grid">"Grid square"</label>
                <input
                    id="filter-grid"
                    type="text"
                    placeholder="e.g. FN20 or !FN20"
                    maxlength="7"
                    value=grid_val
                    on:input=move |ev| {
                        let val = event_target_value(&ev).to_uppercase();
                        filter.update(|f| {
                            f.grid = if val.is_empty() { None } else { Some(val) };
                        });
                    }
                />
            </div>

            // --- Band selector ---
            <div class="filter-group">
                <label for="filter-band">"Band"</label>
                <select
                    id="filter-band"
                    on:change=move |ev| {
                        let val = event_target_value(&ev);
                        filter.update(|f| {
                            f.band_hz = val.parse::<u64>().ok();
                        });
                    }
                >
                    <option value="">"All bands"</option>
                    <For
                        each=move || bands.get()
                        key=|b| b.name.clone()
                        children=move |band| {
                            let selected = move || {
                                filter.read().band_hz == Some(band.dial_hz)
                            };
                            view! {
                                <option
                                    value=band.dial_hz.to_string()
                                    selected=selected
                                >
                                    {band.name.clone()}
                                </option>
                            }
                        }
                    />
                </select>
            </div>

            // --- SNR minimum ---
            <div class="filter-group">
                <label for="filter-snr">
                    "Min SNR: "
                    <span class="filter-value">{move || format!("{} dB", snr_min())}</span>
                </label>
                <input
                    id="filter-snr"
                    type="range"
                    min="-30"
                    max="10"
                    step="1"
                    value=move || snr_min().to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                            filter.update(|f| {
                                f.snr_min = if v == -30 { None } else { Some(v) };
                            });
                        }
                    }
                />
            </div>

            // --- Relative time window ---
            <div class="filter-group">
                <label for="filter-window">"Time window"</label>
                <select
                    id="filter-window"
                    on:change=move |ev| {
                        // Parse the selected number of seconds.
                        if let Ok(secs) = event_target_value(&ev).parse::<i64>() {
                            window_secs.set(secs);
                            let now = chrono::Utc::now().timestamp();
                            filter.update(|f| {
                                f.since_unix = Some(now - secs);
                                f.until_unix = None;
                            });
                        }
                    }
                >
                    {TIME_WINDOWS
                        .iter()
                        .map(|(label, secs)| {
                            let secs = *secs;
                            let label = *label;
                            view! {
                                <option
                                    value=secs.to_string()
                                    selected=move || window_secs.get() == secs
                                >
                                    {label}
                                </option>
                            }
                        })
                        .collect_view()
                    }
                </select>
            </div>

            // --- Grid-only toggle ---
            <div class="filter-group filter-group--check">
                <label>
                    <input
                        type="checkbox"
                        checked=grid_only
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            filter.update(|f| f.grid_only = Some(checked));
                        }
                    />
                    " Grid spots only"
                </label>
            </div>

            // --- Map display section ---
            <h2 class="panel-title" style="margin-top:0.5rem">"Map"</h2>

            // --- Maidenhead grid overlay ---
            <div class="filter-group filter-group--check">
                <label>
                    <input
                        type="checkbox"
                        checked=move || grid_overlay.get()
                        on:change=move |ev| {
                            grid_overlay.set(event_target_checked(&ev));
                        }
                    />
                    " Maidenhead grid"
                </label>
            </div>

            // --- Actions ---
            <div class="filter-actions">
                <button
                    class="btn btn--primary"
                    on:click=move |_| on_refresh.run(())
                >
                    "Refresh"
                </button>

                <button
                    class=move || if live.get() { "btn btn--live btn--live-on" } else { "btn btn--live" }
                    on:click=move |_| on_live_toggle.run(!live.get())
                >
                    {move || if live.get() { "Live: ON" } else { "Live: OFF" }}
                </button>

                <button
                    class="btn btn--secondary"
                    on:click=move |_| {
                        window_secs.set(DEFAULT_WINDOW_SECS);
                        filter.set(SpotFilter::default());
                    }
                >
                    "Reset"
                </button>
            </div>
        </aside>
    }
}
