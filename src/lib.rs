#![recursion_limit = "512"]

pub mod app;
pub mod components;
pub mod error;
pub mod models;
pub mod server_fns;

#[cfg(feature = "hydrate")]
pub mod sse;

// The config and db modules contain SSR-only code; gating them prevents the
// WASM compiler from trying to link ClickHouse's native HTTP client into the
// WASM bundle.
#[cfg(feature = "ssr")]
pub mod cache;
#[cfg(feature = "ssr")]
pub mod config;
#[cfg(feature = "ssr")]
pub mod db;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::App;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
