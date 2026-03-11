/// World map component backed by Leaflet.js.
///
/// On the server this renders an empty `<div id="map">` placeholder; after
/// hydration the client-side Effect calls into `window.wsprMap.init()` and
/// `window.wsprMap.update()` (defined in `public/map.js`) to initialise the
/// Leaflet map and draw spot markers.
///
/// # Props
///
/// * `spots_json`    — reactive JSON string of `Vec<MapSpot>` for the current
///   filter, serialised server-side and passed as a signal.
/// * `config_json`   — reactive JSON string of `PublicConfig` (QTH coords,
///   band palette).
/// * `selected_grid` — reactive `Option<String>` — when `Some`, the JS layer
///   highlights the marker(s) for that grid.
/// * `grid_overlay`  — when `true`, the Maidenhead grid overlay is shown.
use leptos::prelude::*;

#[component]
pub fn WorldMap(
    /// Serialised `Vec<MapSpot>` JSON string, updated whenever the filter changes.
    spots_json: Signal<String>,
    /// Serialised `PublicConfig` JSON string (home QTH, band palette).
    config_json: Signal<String>,
    /// When `Some(grid)`, ask Leaflet to highlight that grid's marker(s).
    #[prop(optional)]
    selected_grid: Option<Signal<Option<String>>>,
    /// Whether the Maidenhead grid overlay should be drawn on the map.
    #[prop(optional, default = Signal::derive(|| false))]
    grid_overlay: Signal<bool>,
) -> impl IntoView {
    // -----------------------------------------------------------------------
    // Client-side effects: drive the Leaflet map from Rust signals
    // -----------------------------------------------------------------------

    // Initialise the map once the component mounts (config_json available).
    // We depend on `config_json` so this re-runs if config changes (unlikely
    // in practice, but safe).
    let config_for_init = config_json;
    let spots_for_init = spots_json;
    Effect::new(move |_| {
        let _cfg = config_for_init.get();
        let _spts = spots_for_init.get();
        #[cfg(feature = "hydrate")]
        call_js_init_map(&_cfg, &_spts);
    });

    // Update markers whenever spots change after the initial render.
    Effect::new(move |_| {
        let _spts = spots_json.get();
        #[cfg(feature = "hydrate")]
        call_js_update_map(&_spts);
    });

    // Toggle the Maidenhead grid overlay whenever the signal changes.
    Effect::new(move |_| {
        let _enabled = grid_overlay.get();
        #[cfg(feature = "hydrate")]
        call_js_set_grid_overlay(_enabled);
    });

    // Highlight a specific grid square when the user clicks a table row.
    if let Some(grid_signal) = selected_grid {
        Effect::new(move |_| {
            if let Some(_grid) = grid_signal.get() {
                #[cfg(feature = "hydrate")]
                call_js_highlight_grid(&_grid);
            }
        });
    }

    view! {
        <div id="map-container">
            <div id="map"></div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// JS bridge — compiled only for the WASM/hydrate target
// ---------------------------------------------------------------------------

/// Retrieve `window.wsprMap` as a JS object, logging an error if absent.
///
/// Returns `None` when `window` is unavailable or `wsprMap` is not yet defined
/// (e.g. if `map.js` failed to load).
#[cfg(feature = "hydrate")]
fn js_wspr_map() -> Option<wasm_bindgen::JsValue> {
    use js_sys::Reflect;
    use wasm_bindgen::JsValue;

    let window = web_sys::window()?;
    let val = Reflect::get(&window, &JsValue::from_str("wsprMap")).ok()?;
    if val.is_undefined() || val.is_null() {
        web_sys::console::error_1(
            &JsValue::from_str("wsprMap: window.wsprMap is undefined — map.js may not have loaded"),
        );
        return None;
    }
    Some(val)
}

/// Call `window.wsprMap.init(configJson, spotsJson)` to create the Leaflet map.
#[cfg(feature = "hydrate")]
fn call_js_init_map(config_json: &str, spots_json: &str) {
    use js_sys::{Function, Reflect};
    use wasm_bindgen::JsValue;

    let wspr_map = match js_wspr_map() {
        Some(v) => v,
        None => return,
    };

    let init_fn = match Reflect::get(&wspr_map, &JsValue::from_str("init"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    {
        Some(f) => f,
        None => {
            web_sys::console::error_1(
                &JsValue::from_str("wsprMap: window.wsprMap.init is not a function"),
            );
            return;
        }
    };

    if let Err(e) = init_fn.call2(
        &wspr_map,
        &JsValue::from_str(config_json),
        &JsValue::from_str(spots_json),
    ) {
        web_sys::console::error_2(&JsValue::from_str("wsprMap.init() threw:"), &e);
    }
}

/// Call `window.wsprMap.update(spotsJson)` to replace all map markers.
#[cfg(feature = "hydrate")]
fn call_js_update_map(spots_json: &str) {
    use js_sys::{Function, Reflect};
    use wasm_bindgen::JsValue;

    let wspr_map = match js_wspr_map() {
        Some(v) => v,
        None => return,
    };

    let update_fn = match Reflect::get(&wspr_map, &JsValue::from_str("update"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    {
        Some(f) => f,
        None => {
            web_sys::console::error_1(
                &JsValue::from_str("wsprMap: window.wsprMap.update is not a function"),
            );
            return;
        }
    };

    if let Err(e) = update_fn.call1(&wspr_map, &JsValue::from_str(spots_json)) {
        web_sys::console::error_2(&JsValue::from_str("wsprMap.update() threw:"), &e);
    }
}

/// Call `window.wsprMap.setGridOverlay(enabled)` to show or hide the
/// Maidenhead grid overlay.
///
/// This mirrors the Leaflet layer-control toggle so both the sidebar checkbox
/// and the in-map layer picker stay in sync.
#[cfg(feature = "hydrate")]
fn call_js_set_grid_overlay(enabled: bool) {
    use js_sys::{Function, Reflect};
    use wasm_bindgen::JsValue;

    let wspr_map = match js_wspr_map() {
        Some(v) => v,
        None => return,
    };

    let set_fn = match Reflect::get(&wspr_map, &JsValue::from_str("setGridOverlay"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    {
        Some(f) => f,
        None => {
            web_sys::console::error_1(
                &JsValue::from_str("wsprMap: window.wsprMap.setGridOverlay is not a function"),
            );
            return;
        }
    };

    if let Err(e) = set_fn.call1(&wspr_map, &JsValue::from_bool(enabled)) {
        web_sys::console::error_2(&JsValue::from_str("wsprMap.setGridOverlay() threw:"), &e);
    }
}

/// Call `window.wsprMap.highlight(grid)` to emphasise a particular grid square.
#[cfg(feature = "hydrate")]
fn call_js_highlight_grid(grid: &str) {
    use js_sys::{Function, Reflect};
    use wasm_bindgen::JsValue;

    let wspr_map = match js_wspr_map() {
        Some(v) => v,
        None => return,
    };

    let hl_fn = match Reflect::get(&wspr_map, &JsValue::from_str("highlight"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    {
        Some(f) => f,
        None => {
            web_sys::console::error_1(
                &JsValue::from_str("wsprMap: window.wsprMap.highlight is not a function"),
            );
            return;
        }
    };

    if let Err(e) = hl_fn.call1(&wspr_map, &JsValue::from_str(grid)) {
        web_sys::console::error_2(&JsValue::from_str("wsprMap.highlight() threw:"), &e);
    }
}

// Allow the unused import lint to be quiet in SSR builds where wasm_bindgen
// types are not available.
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;
