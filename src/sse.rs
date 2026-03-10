/// Client-side SSE (Server-Sent Events) helper.
///
/// Compiled only for the `hydrate` (WASM) target.  The [`SseHandle`] returned
/// by [`start_sse`] can be used to close the `EventSource` when the user
/// turns live mode off.
#[cfg(feature = "hydrate")]
pub struct SseHandle {
    source: web_sys::EventSource,
}

#[cfg(feature = "hydrate")]
impl SseHandle {
    /// Close the underlying `EventSource` connection.
    pub fn close(self) {
        self.source.close();
    }
}

/// Open an `EventSource` connected to `url` and wire up callbacks.
///
/// # Arguments
///
/// * `url`        — SSE endpoint, e.g. `"/api/stream"`
/// * `on_open`    — called once when the connection is successfully established
/// * `on_spots`   — called with the raw JSON string for each `event: spots` message
/// * `on_error`   — called when the browser reports a connection failure
///
/// Closures are leaked (`.forget()`) so they live for the lifetime of the
/// `EventSource`.  Call [`SseHandle::close`] to drop the connection and stop
/// receiving events; the leaked closures are small and bounded in number.
#[cfg(feature = "hydrate")]
pub fn start_sse(
    url: &str,
    on_open: impl Fn() + 'static,
    on_spots: impl Fn(String) + 'static,
    on_error: impl Fn() + 'static,
) -> SseHandle {
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen::JsCast;

    let source =
        web_sys::EventSource::new(url).expect("EventSource construction should not fail");

    // `open` fires once the HTTP connection is established and the server has
    // sent the initial response headers.  This is the right moment to flip the
    // badge from "Connecting…" to "Live".
    let open_cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        on_open();
    });
    source
        .add_event_listener_with_callback("open", open_cb.as_ref().unchecked_ref())
        .expect("add_event_listener (open) should not fail");
    open_cb.forget();

    // Named `event: spots` messages carry a JSON array of MapSpot objects.
    let spots_cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
        move |ev: web_sys::MessageEvent| {
            if let Some(data) = ev.data().as_string() {
                on_spots(data);
            }
        },
    );
    source
        .add_event_listener_with_callback("spots", spots_cb.as_ref().unchecked_ref())
        .expect("add_event_listener (spots) should not fail");
    spots_cb.forget();

    // `error` fires on connection failure (network error, server closed, etc.).
    // Note: this is the browser-native connection error, distinct from any
    // `event: error` messages the server might send (which would also arrive
    // here as a named event listener — we intentionally conflate them since
    // both mean "something went wrong").
    let error_cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        on_error();
    });
    source
        .add_event_listener_with_callback("error", error_cb.as_ref().unchecked_ref())
        .expect("add_event_listener (error) should not fail");
    error_cb.forget();

    SseHandle { source }
}
