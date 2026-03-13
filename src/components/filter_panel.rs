/// Filter panel component: source toggle, callsign search, reporter search,
/// grid search, band selector, SNR slider, relative time-window selector,
/// and a live-mode toggle.
///
/// All controls write into a shared `RwSignal<SpotFilter>`.  Components that
/// display data (`WorldMap`, `SpotTable`, `StatsBar`) read the same signal so
/// they update automatically whenever the filter changes.
use leptos::prelude::*;

use crate::models::{BandInfo, SpotFilter, SpotSource};
use crate::server_fns::get_reporter_suggestions;

// Used as the fallback before the server config resolves.
const DEFAULT_WINDOW_SECS_FALLBACK: i64 = 3_600;

/// Predefined relative time windows shown in the time selector.
///
/// Each tuple is `(label, seconds_back)`.
const TIME_WINDOWS: &[(&str, i64)] = &[
    ("1 hour", 3_600),
    ("2 hours", 7_200),
    ("4 hours", 14_400),
    ("8 hours", 28_800),
    ("12 hours", 43_200),
    ("24 hours", 86_400),
    ("48 hours", 172_800),
    ("7 days", 604_800),
];

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
    /// Server-configured default time window in seconds (derived from
    /// `WSPR_TIME_WINDOW_HOURS`).  `None` while the config is still loading;
    /// applied once on first resolution and used by the Reset button.
    default_window_secs: Signal<Option<i64>>,
) -> impl IntoView {
    // -----------------------------------------------------------------------
    // Local state
    // -----------------------------------------------------------------------

    // Tracks which relative window (seconds) is currently selected so the
    // `<select>` can reflect the correct option after a Reset.
    let window_secs: RwSignal<i64> = RwSignal::new(DEFAULT_WINDOW_SECS_FALLBACK);

    // Guard so we only apply the server default once (on first resolution).
    let config_applied = RwSignal::new(false);

    // Separate signal for the reporter input's current text, used to drive
    // the autocomplete datalist without coupling to filter.reporter directly.
    // (filter.reporter may carry a leading "!" which we strip for suggestions.)
    let reporter_prefix: RwSignal<String> = RwSignal::new(String::new());

    // When the server config resolves for the first time, snap the dropdown
    // and the filter's `since_unix` to the server-configured window.
    Effect::new(move |_| {
        if let Some(secs) = default_window_secs.get() {
            if !config_applied.get_untracked() {
                config_applied.set(true);
                window_secs.set(secs);
                let now = chrono::Utc::now().timestamp();
                filter.update(|f| {
                    f.since_unix = Some(now - secs);
                    f.until_unix = None;
                });
            }
        }
    });

    // -----------------------------------------------------------------------
    // Derived readable signals for individual filter fields
    // -----------------------------------------------------------------------

    let source_is_global = move || filter.read().source == SpotSource::Global;
    let callsign = move || filter.read().callsign.clone().unwrap_or_default();
    let reporter_val = move || filter.read().reporter.clone().unwrap_or_default();
    let grid_val = move || filter.read().grid.clone().unwrap_or_default();
    let snr_min = move || filter.read().snr_min.unwrap_or(-30);
    let grid_only = move || filter.read().grid_only.unwrap_or(false);

    // -----------------------------------------------------------------------
    // Reporter autocomplete resource
    //
    // Fetches suggestions on every keystroke (debouncing handled by the
    // browser's datalist rendering rather than an explicit timer).  Returns
    // an empty list when the prefix is empty or contains only a "!" prefix.
    // -----------------------------------------------------------------------
    let reporter_suggestions_resource = LocalResource::new(move || {
        let prefix = reporter_prefix.get();
        async move {
            // Strip leading "!" before querying so negation prefixes don't
            // prevent suggestions from appearing.
            let clean = prefix.strip_prefix('!').unwrap_or(&prefix).to_string();
            if clean.is_empty() {
                return Ok::<Vec<String>, leptos::server_fn::error::ServerFnError>(vec![]);
            }
            get_reporter_suggestions(clean).await
        }
    });

    let reporter_suggestions = Signal::derive(move || {
        reporter_suggestions_resource
            .get()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    });

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------
    view! {
        <aside id="filter-panel">
            <h2 class="panel-title">"Filters"</h2>

            // --- Source toggle ---
            <div class="filter-group filter-group--source">
                <label>"Source"</label>
                <div class="source-toggle">
                    <button
                        class=move || {
                            if !source_is_global() { "btn btn--source btn--source-active" }
                            else { "btn btn--source" }
                        }
                        on:click=move |_| {
                            filter.update(|f| {
                                f.source = SpotSource::Local;
                                // Clear reporter when switching away from global mode.
                                f.reporter = None;
                            });
                            reporter_prefix.set(String::new());
                        }
                    >
                        "Local Receive"
                    </button>
                    <button
                        class=move || {
                            if source_is_global() { "btn btn--source btn--source-active" }
                            else { "btn btn--source" }
                        }
                        on:click=move |_| {
                            filter.update(|f| {
                                f.source = SpotSource::Global;
                            });
                        }
                    >
                        "Global"
                    </button>
                </div>
            </div>

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

            // --- Reporter (global mode only) ---
            <Show when=source_is_global>
                <div class="filter-group">
                    <label for="filter-reporter">"Reporter"</label>
                    <datalist id="reporter-suggestions">
                        <For
                            each=move || reporter_suggestions.get()
                            key=|s| s.clone()
                            children=move |s| view! { <option value=s /> }
                        />
                    </datalist>
                    <input
                        id="filter-reporter"
                        type="text"
                        list="reporter-suggestions"
                        placeholder="e.g. W3POG or !W3POG"
                        maxlength="21"
                        value=reporter_val
                        on:input=move |ev| {
                            let val = event_target_value(&ev);
                            // Strip "!" for suggestion prefix but keep the full
                            // value in the filter so negation works.
                            let prefix = val
                                .strip_prefix('!')
                                .unwrap_or(&val)
                                .to_string();
                            reporter_prefix.set(prefix);
                            filter.update(|f| {
                                f.reporter = if val.is_empty() { None } else { Some(val) };
                            });
                        }
                    />
                </div>
            </Show>

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
                        // Reset to the server-configured window, falling back
                        // to 1 hour if config has not yet resolved.
                        let secs = default_window_secs
                            .get_untracked()
                            .unwrap_or(DEFAULT_WINDOW_SECS_FALLBACK);
                        window_secs.set(secs);
                        let now = chrono::Utc::now().timestamp();
                        reporter_prefix.set(String::new());
                        filter.set(SpotFilter {
                            since_unix: Some(now - secs),
                            ..SpotFilter::default()
                        });
                    }
                >
                    "Reset"
                </button>
            </div>
        </aside>
    }
}
