use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};

use crate::components::{FilterPanel, LiveBadge, SpotTable, StatsBar, WorldMap};
use crate::components::live_badge::LiveState;
use crate::models::SpotFilter;
use crate::server_fns::{get_map_spots, get_public_config, get_spots, get_stats};

// ---------------------------------------------------------------------------
// HTML shell
// ---------------------------------------------------------------------------

/// Returns the full HTML document used by both SSR and WASM hydration.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                // Leaflet 1.9.4
                <link
                    rel="stylesheet"
                    href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css"
                />
                <script
                    src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"
                ></script>
                <script src="/map.js"></script>
                // Google Fonts
                <link rel="preconnect" href="https://fonts.googleapis.com"/>
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin=""/>
                <link
                    href="https://fonts.googleapis.com/css2?family=Exo+2:wght@400;600;700&family=Inter:wght@400;500&family=JetBrains+Mono:wght@400;500&display=swap"
                    rel="stylesheet"
                />
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

// ---------------------------------------------------------------------------
// Root App component
// ---------------------------------------------------------------------------

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/wsprrs-web.css"/>
        <Title text="WSPR Visualizer"/>
        <Router>
            <Routes fallback=|| "Page not found.".into_view()>
                <Route path=StaticSegment("") view=HomePage/>
            </Routes>
        </Router>
    }
}

// ---------------------------------------------------------------------------
// Home page
// ---------------------------------------------------------------------------

#[component]
fn HomePage() -> impl IntoView {
    // Shared filter state written by FilterPanel on every input event.
    let filter = RwSignal::new(SpotFilter::default());
    // Debounced copy of `filter`: updated 1 second after the last change.
    // All server resources read this signal so rapid keystrokes / slider drags
    // do not fire a ClickHouse query on every event.
    let debounced_filter = RwSignal::new(SpotFilter::default());
    // Grid selected by clicking a spot table row — drives map highlight.
    let selected_grid: RwSignal<Option<String>> = RwSignal::new(None);
    // Live-stream badge state.
    let live_state = RwSignal::new(LiveState::Connecting);
    // Whether the Maidenhead grid overlay is drawn on the map.
    let grid_overlay: RwSignal<bool> = RwSignal::new(false);

    // -----------------------------------------------------------------------
    // Server resources
    //
    // We use LocalResource instead of Resource so that data is fetched
    // client-side after hydration only.  This is correct for a live dashboard
    // where displayed data is always current; it also avoids Leptos warnings
    // about reading resources outside <Suspense/> and eliminates hydration
    // mismatches caused by stale SSR-embedded data.
    // -----------------------------------------------------------------------
    let config_resource = LocalResource::new(get_public_config);

    let map_spots_resource =
        LocalResource::new(move || get_map_spots(debounced_filter.get()));

    let spots_resource =
        LocalResource::new(move || get_spots(debounced_filter.get()));

    let stats_resource = LocalResource::new(move || {
        let since = debounced_filter.with(|f| f.since_unix);
        let until = debounced_filter.with(|f| f.until_unix);
        let now = chrono::Utc::now().timestamp();
        get_stats(since.unwrap_or(now - 3600), until.unwrap_or(now))
    });

    // -----------------------------------------------------------------------
    // Derived signals from resources
    // -----------------------------------------------------------------------

    // JSON strings fed into the Leaflet JS bridge.
    // LocalResource::get() returns Option<Result<T, ServerFnError>>;
    // .and_then(|r| r.ok()) flattens to Option<T>.
    let spots_json = Signal::derive(move || {
        map_spots_resource
            .get()
            .and_then(|r| r.ok())
            .map(|s| serde_json::to_string(&s).unwrap_or_default())
            .unwrap_or_default()
    });

    let config_json = Signal::derive(move || {
        config_resource
            .get()
            .and_then(|r| r.ok())
            .map(|c| serde_json::to_string(&c).unwrap_or_default())
            .unwrap_or_default()
    });

    let stats_signal = Signal::derive(move || {
        stats_resource.get().and_then(|r| r.ok())
    });

    let bands_signal = Signal::derive(move || {
        config_resource
            .get()
            .and_then(|r| r.ok())
            .map(|c| c.bands)
            .unwrap_or_default()
    });

    // Spots list for the table (defaults to empty while loading).
    let table_spots = Signal::derive(move || {
        spots_resource.get().and_then(|r| r.ok()).unwrap_or_default()
    });

    let selected_grid_signal =
        Signal::derive(move || selected_grid.get());

    let live_signal = Signal::derive(move || live_state.get());
    let live_bool = Signal::derive(move || {
        matches!(live_state.get(), LiveState::Connected | LiveState::Connecting)
    });

    // -----------------------------------------------------------------------
    // Callbacks
    // -----------------------------------------------------------------------
    let on_refresh = Callback::new(move |_: ()| {
        map_spots_resource.refetch();
        spots_resource.refetch();
        stats_resource.refetch();
    });

    let on_live_toggle = Callback::new(move |enabled: bool| {
        if enabled {
            live_state.set(LiveState::Connecting);
            // SSE wiring happens in the hydrate Effect below.
        } else {
            live_state.set(LiveState::Off);
        }
    });

    // Debounce: propagate `filter` → `debounced_filter` after 1 second of
    // inactivity.  Any pending timer is cancelled when a new input event
    // arrives, so only the final settled value fires a server request.
    // This block is client-only because `web_sys::window()` does not exist on
    // the server and `LocalResource`s never fetch during SSR anyway.
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        let debounce_handle: RwSignal<Option<i32>> = RwSignal::new(None);

        Effect::new(move |_| {
            let new_filter = filter.get();
            let window = web_sys::window().expect("window must exist in WASM");

            // Cancel the previous pending timer, if any.
            if let Some(handle) = debounce_handle.get_untracked() {
                window.clear_timeout_with_handle(handle);
            }

            // Schedule `debounced_filter` update for 1 000 ms from now.
            let cb = wasm_bindgen::closure::Closure::once(move || {
                debounced_filter.set(new_filter);
            });
            let handle = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    1_000,
                )
                .expect("setTimeout must not fail");
            // `forget` keeps the closure alive until the timer fires.
            cb.forget();
            debounce_handle.set(Some(handle));
        });
    }

    // Client-side SSE connection (no-op on SSR).
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;

        let sse_handle: RwSignal<Option<crate::sse::SseHandle>> = RwSignal::new(None);
        // Tracks how many consecutive reconnect attempts have been made.
        // Reset to 0 on a successful connection.
        let reconnect_attempt: RwSignal<u32> = RwSignal::new(0);

        // Maximum consecutive reconnect attempts before giving up entirely.
        const MAX_RECONNECT_ATTEMPTS: u32 = 10;

        // Helper: close any live SSE handle stored in the signal.
        let close_handle = move || {
            sse_handle.update(|h| {
                if let Some(handle) = h.take() {
                    handle.close();
                }
            });
        };

        Effect::new(move |_| {
            match live_state.get() {
                LiveState::Connecting => {
                    // Close any stale handle before opening a fresh connection.
                    close_handle();

                    let handle = crate::sse::start_sse(
                        "/api/stream",
                        // on_open: HTTP handshake complete → flip badge and reset backoff
                        move || {
                            reconnect_attempt.set(0);
                            live_state.set(LiveState::Connected);
                        },
                        // on_version: compare server build to this client's compiled-in
                        // version; reload the page if they differ so users always run
                        // the current frontend after a backend redeployment.
                        move |server_version| {
                            const CLIENT_VERSION: &str =
                                concat!(env!("CARGO_PKG_VERSION"), "+", env!("GIT_SHA"));
                            if server_version != CLIENT_VERSION {
                                leptos::logging::log!(
                                    "Backend version {server_version} != client version \
                                     {CLIENT_VERSION}; reloading"
                                );
                                let window = web_sys::window()
                                    .expect("should always have a Window in WASM");
                                let _ = window.location().reload();
                            }
                        },
                        // on_spots: new data arrived → refresh all resources
                        move |_json| {
                            map_spots_resource.refetch();
                            spots_resource.refetch();
                            stats_resource.refetch();
                        },
                        // on_error: connection lost → schedule reconnect with
                        // exponential backoff, or give up after MAX_RECONNECT_ATTEMPTS.
                        move || {
                            // Read without tracking to avoid re-triggering Effects.
                            let attempt = reconnect_attempt.get_untracked();

                            if attempt >= MAX_RECONNECT_ATTEMPTS {
                                live_state.set(LiveState::Error);
                                return;
                            }

                            // Delay doubles each attempt: 1 s, 2 s, 4 s, … capped at 30 s.
                            let delay_ms = (1_000u32 << attempt.min(4)).min(30_000) as i32;
                            reconnect_attempt.update(|a| *a += 1);
                            // Show "Reconnecting..." badge (1-based attempt number).
                            live_state.set(LiveState::Reconnecting(attempt + 1));

                            let window =
                                web_sys::window().expect("should always have a Window in WASM");
                            // Closure::once is used because the timer fires exactly once.
                            // We guard on the current state so that a pending timer is
                            // harmlessly discarded if the user switches live mode off.
                            let cb = wasm_bindgen::closure::Closure::once(move || {
                                if matches!(
                                    live_state.get_untracked(),
                                    LiveState::Reconnecting(_)
                                ) {
                                    live_state.set(LiveState::Connecting);
                                }
                            });
                            window
                                .set_timeout_with_callback_and_timeout_and_arguments_0(
                                    cb.as_ref().unchecked_ref(),
                                    delay_ms,
                                )
                                .expect("setTimeout should not fail");
                            // Leak the closure so it remains valid when the timer fires.
                            cb.forget();
                        },
                    );
                    sse_handle.set(Some(handle));
                }
                // Connection errored; close the stale handle and wait for the
                // timer (set in the on_error callback above) to trigger Connecting.
                LiveState::Reconnecting(_) => {
                    close_handle();
                }
                LiveState::Off | LiveState::Error => {
                    reconnect_attempt.set(0);
                    close_handle();
                }
                LiveState::Connected => {}
            }
        });
    }

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------
    view! {
        <div id="app-layout">
            <header id="app-header">
                <div class="header-left">
                    <h1 class="app-title">"WSPR Visualizer"</h1>
                    <LiveBadge state=live_signal/>
                    <span class="app-version">
                        {concat!("v", env!("CARGO_PKG_VERSION"), "+", env!("GIT_SHA"))}
                    </span>
                </div>
                <div class="header-right">
                    <StatsBar stats=stats_signal/>
                </div>
            </header>

            <div id="main-content">
                <FilterPanel
                    filter=filter
                    bands=bands_signal
                    on_refresh=on_refresh
                    on_live_toggle=on_live_toggle
                    live=live_bool
                    grid_overlay=grid_overlay
                />

                <div id="content-area">
                    <WorldMap
                        spots_json=spots_json
                        config_json=config_json
                        selected_grid=selected_grid_signal
                        grid_overlay=Signal::derive(move || grid_overlay.get())
                    />

                    <SpotTable
                        spots=table_spots
                        on_row_select=Callback::new(move |g| selected_grid.set(g))
                    />
                </div>
            </div>
        </div>
    }
}
