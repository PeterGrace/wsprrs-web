# Fix: Popup race condition during map pan/zoom animation

**Date:** 2026-03-12T17:00:00Z

## Problem

Clicking a spot table row triggered `wsprMap.highlight()`, which called
`map.setView()` and then immediately called `layer.openPopup()`. Leaflet's
`Popup._adjustPan` internally reads `map._mapPane` to reposition the popup,
but that reference is `null` while the pan/zoom animation is still in
progress. This caused:

```
TypeError: Cannot read properties of null (reading 'layerPointToContainerPoint')
    at e._adjustPan (Popup.js:277)
```

The symptom was non-deterministic: sometimes the popup appeared (if the view
was already close enough that no animation ran), sometimes nothing happened,
sometimes the map moved but no popup opened.

## Fix (`public/map.js`)

- Introduced a module-level `pendingHighlight` variable to track the
  registered `moveend` listener.
- In `highlight()`, any previously registered listener is cancelled via
  `map.off("moveend", pendingHighlight)` before registering a new one,
  preventing stale callbacks when the user clicks rows rapidly.
- The `openMatchingPopup` inner function is registered with `map.once("moveend", ...)`
  **before** `map.setView()` is called. This guarantees the popup opens only
  after the animation has fully completed and the pane container is valid.

## Files changed

- `public/map.js` — `highlight()` rewritten to defer popup open until `moveend`
