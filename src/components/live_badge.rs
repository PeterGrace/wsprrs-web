/// Live-stream status badge component.
///
/// Shows a pulsing dot to indicate whether the SSE connection to `/api/stream`
/// is active.  The badge is purely visual; the actual `EventSource` lifecycle
/// is managed in `app.rs`.
use leptos::prelude::*;

/// SSE connection state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiveState {
    /// No live stream requested by the user.
    Off,
    /// EventSource opened, waiting for first event.
    Connecting,
    /// Receiving events.
    Connected,
    /// Connection lost or browser error.
    Error,
}

#[component]
pub fn LiveBadge(state: Signal<LiveState>) -> impl IntoView {
    view! {
        <div id="live-badge" class=move || format!("live-badge live-badge--{}", state.get().css_class())>
            <span class="live-dot"></span>
            <span class="live-label">{move || state.get().label()}</span>
        </div>
    }
}

impl LiveState {
    fn css_class(self) -> &'static str {
        match self {
            LiveState::Off => "off",
            LiveState::Connecting => "connecting",
            LiveState::Connected => "connected",
            LiveState::Error => "error",
        }
    }

    fn label(self) -> &'static str {
        match self {
            LiveState::Off => "Live off",
            LiveState::Connecting => "Connecting...",
            LiveState::Connected => "Live",
            LiveState::Error => "Disconnected",
        }
    }
}
