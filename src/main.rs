/// SSR entry point: Axum web server with Leptos SSR, server functions, and
/// a live-stream SSE endpoint.
///
/// # Architecture
///
/// ```text
/// ┌─────────────────────────────────────────────────────────────┐
/// │ Background task: spot_poll_task                             │
/// │  - polls ClickHouse every 120 s                             │
/// │  - broadcasts new spot JSON via tokio::sync::broadcast      │
/// └────────────────────────┬────────────────────────────────────┘
///                          │ broadcast::Sender<Arc<String>>
///          ┌───────────────┴──────────────────────────────────┐
///          │           SSE handler (per client)               │
///          │  - subscribes on connect                          │
///          │  - streams events to browser EventSource          │
///          └──────────────────────────────────────────────────┘
/// ```
///
/// This design means **one** ClickHouse query per 120-second window regardless
/// of how many clients are connected, compared with N queries under the old
/// per-client polling approach.
///
/// State injection:
/// - `clickhouse::Client` and `Arc<Config>` — Axum `Extension` layers, read
///   by Leptos server functions via `expect_context::<T>()`.
/// - `Arc<QueryCache>` — TTL cache for aggregate queries, same pattern.
/// - `Arc<broadcast::Sender<Arc<String>>>` — broadcaster for SSE fan-out.
/// - `LeptosOptions` — standard Axum router state for `leptos_axum`.
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use std::sync::Arc;

    use axum::{extract::Extension, routing::get, Router};
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use tokio::sync::broadcast;
    use tower_http::{compression::CompressionLayer, trace::TraceLayer};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    use wsprrs_web::app::{shell, App};
    use wsprrs_web::cache::QueryCache;
    use wsprrs_web::config::Config;

    // -----------------------------------------------------------------------
    // Observability
    // -----------------------------------------------------------------------
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // -----------------------------------------------------------------------
    // Configuration and ClickHouse client
    // -----------------------------------------------------------------------
    let config = Config::from_env().expect("failed to load configuration");
    let db = config.clickhouse_client();
    let config = Arc::new(config);

    // -----------------------------------------------------------------------
    // Shared SSE broadcaster
    //
    // Capacity of 16 means a slow client can lag up to 16 messages (≈32 min)
    // before its receiver is dropped with `RecvError::Lagged`.  In practice
    // clients that fall behind this far have stale data anyway.
    // -----------------------------------------------------------------------
    let (spot_tx, _) = broadcast::channel::<Arc<String>>(16);
    let spot_tx = Arc::new(spot_tx);

    // Spawn the single background poller.  It owns its own DB client clone and
    // runs independently of any HTTP request lifecycle.
    tokio::spawn(spot_poll_task(
        db.clone(),
        config.clickhouse_table.clone(),
        config.ignore_callsigns.clone(),
        config.spot_limit,
        Arc::clone(&spot_tx),
    ));

    // -----------------------------------------------------------------------
    // Shared query cache
    // -----------------------------------------------------------------------
    let cache = Arc::new(QueryCache::new());

    // -----------------------------------------------------------------------
    // Leptos setup
    // -----------------------------------------------------------------------
    let conf = get_configuration(None).expect("failed to read Leptos configuration");
    let leptos_options = conf.leptos_options.clone();
    let routes = generate_route_list(App);
    let addr = leptos_options.site_addr;

    // -----------------------------------------------------------------------
    // Router
    // -----------------------------------------------------------------------
    let app = Router::new()
        .route("/api/stream", get(sse_handler))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let db = db.clone();
                let config = config.clone();
                let cache = cache.clone();
                move || {
                    provide_context(db.clone());
                    provide_context(config.clone());
                    provide_context(cache.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(Extension(spot_tx))
        .layer(Extension(db))
        .layer(Extension(config))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(leptos_options);

    // -----------------------------------------------------------------------
    // Start server
    // -----------------------------------------------------------------------
    log!("Listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind TCP listener");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("server error");
}

/// Background task: poll ClickHouse every 120 seconds and broadcast new spots
/// to all connected SSE clients via the shared sender.
///
/// Only one instance of this task runs for the entire server lifetime, so the
/// number of ClickHouse queries is always 1 per 120-second window regardless
/// of the number of connected clients.
#[cfg(feature = "ssr")]
async fn spot_poll_task(
    db: clickhouse::Client,
    table: String,
    ignore_callsigns: Vec<String>,
    spot_limit: u32,
    tx: std::sync::Arc<tokio::sync::broadcast::Sender<std::sync::Arc<String>>>,
) {
    use wsprrs_web::db::queries;

    let mut last_unix = chrono::Utc::now().timestamp() - 120;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    // Skip missed ticks rather than firing a burst of catch-up queries if the
    // server was paused (e.g. during a GC or system suspend).
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        match queries::query_new_spots(&db, &table, last_unix, &ignore_callsigns, spot_limit).await {
            Ok(spots) if !spots.is_empty() => {
                last_unix = spots
                    .iter()
                    .map(|s| s.window_start_unix)
                    .max()
                    .unwrap_or(last_unix);
                match serde_json::to_string(&spots) {
                    Ok(json) => {
                        // send() only errors when there are zero receivers; that
                        // is fine — it means no clients are currently connected.
                        let _ = tx.send(std::sync::Arc::new(json));
                    }
                    Err(e) => tracing::error!("SSE serialisation error: {e}"),
                }
            }
            Ok(_) => {} // no new spots this window
            Err(e) => tracing::error!("SSE poll error: {e}"),
        }
    }
}

/// SSE handler: subscribe to the shared broadcaster and stream events to one
/// client.  The handler itself does **no** ClickHouse I/O.
#[cfg(feature = "ssr")]
async fn sse_handler(
    axum::extract::Extension(tx): axum::extract::Extension<
        std::sync::Arc<tokio::sync::broadcast::Sender<std::sync::Arc<String>>>,
    >,
) -> axum::response::sse::Sse<
    impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use async_stream::stream;
    use axum::response::sse::{Event, KeepAlive, Sse};
    use tokio::sync::broadcast::error::RecvError;

    let mut rx = tx.subscribe();

    let s = stream! {
        // Emit the build version immediately on connect.  The WASM client
        // compares this to its own compiled-in version and reloads the page
        // if they differ, ensuring users always run the current frontend after
        // a backend redeployment.
        const BUILD_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GIT_SHA"));
        yield Ok(Event::default().event("version").data(BUILD_VERSION));

        loop {
            match rx.recv().await {
                Ok(json) => {
                    yield Ok(Event::default().event("spots").data(json.as_str()));
                }
                Err(RecvError::Lagged(n)) => {
                    // Client was too slow to consume events; skip the missed
                    // messages and log a warning.  The browser will re-fetch
                    // on the next event anyway.
                    tracing::warn!("SSE client lagged by {n} message(s); skipping");
                }
                Err(RecvError::Closed) => break,
            }
        }
    };

    Sse::new(s).keep_alive(KeepAlive::default())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // No client-side main; hydration entry point is `lib.rs::hydrate()`.
}
