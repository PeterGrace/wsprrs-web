use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};

use crate::components::{FilterPanel, LiveBadge, SpotTable, StatsBar, WorldMap};
use crate::components::live_badge::LiveState;
use crate::models::SpotFilter;
use crate::server_fns::{
    get_band_counts, get_map_spots, get_public_config, get_spots, get_stats,
};

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
    // Shared filter state written by FilterPanel, read by all data fetching.
    let filter = RwSignal::new(SpotFilter::default());
    // Grid selected by clicking a spot table row — drives map highlight.
    let selected_grid: RwSignal<Option<String>> = RwSignal::new(None);
    // Live-stream badge state.
    let live_state = RwSignal::new(LiveState::Off);

    // -----------------------------------------------------------------------
    // Server resources
    //
    // We use LocalResource instead of Resource so that data is fetched
    // client-side after hydration only.  This is correct for a live dashboard
    // where displayed data is always current; it also avoids Leptos warnings
    // about reading resources outside <Suspense/> and eliminates hydration
    // mismatches caused by stale SSR-embedded data.
    // -----------------------------------------------------------------------
    let config_resource = LocalResource::new(|| get_public_config());

    let map_spots_resource =
        LocalResource::new(move || get_map_spots(filter.get()));

    let spots_resource =
        LocalResource::new(move || get_spots(filter.get()));

    let stats_resource = LocalResource::new(move || {
        let since = filter.with(|f| f.since_unix);
        let until = filter.with(|f| f.until_unix);
        let now = chrono::Utc::now().timestamp();
        get_stats(since.unwrap_or(now - 3600), until.unwrap_or(now))
    });

    let bands_resource = LocalResource::new(move || {
        let since = filter.with(|f| f.since_unix);
        let now = chrono::Utc::now().timestamp();
        get_band_counts(since.unwrap_or(now - 3600))
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
        bands_resource.get().and_then(|r| r.ok()).unwrap_or_default()
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
        bands_resource.refetch();
    });

    let on_live_toggle = Callback::new(move |enabled: bool| {
        if enabled {
            live_state.set(LiveState::Connecting);
            // SSE wiring happens in the hydrate Effect below.
        } else {
            live_state.set(LiveState::Off);
        }
    });

    // Client-side SSE connection (no-op on SSR).
    #[cfg(feature = "hydrate")]
    {
        let sse_handle: RwSignal<Option<crate::sse::SseHandle>> = RwSignal::new(None);

        Effect::new(move |_| {
            match live_state.get() {
                LiveState::Connecting => {
                    let handle = crate::sse::start_sse(
                        "/api/stream",
                        // on_open: HTTP handshake complete → flip badge immediately
                        move || live_state.set(LiveState::Connected),
                        // on_spots: new data arrived → refresh all resources
                        move |_json| {
                            map_spots_resource.refetch();
                            spots_resource.refetch();
                            stats_resource.refetch();
                            bands_resource.refetch();
                        },
                        // on_error: connection lost → show error state
                        move || live_state.set(LiveState::Error),
                    );
                    sse_handle.set(Some(handle));
                }
                LiveState::Off | LiveState::Error => {
                    sse_handle.update(|h| {
                        if let Some(handle) = h.take() {
                            handle.close();
                        }
                    });
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
                />

                <div id="content-area">
                    <WorldMap
                        spots_json=spots_json
                        config_json=config_json
                        selected_grid=selected_grid_signal
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
