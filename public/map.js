/**
 * WSPR Visualizer — Leaflet map bridge
 *
 * Exposes a single namespace: window.wsprMap with methods:
 *   init(configJson, spotsJson)  — create the map and draw initial markers
 *   update(spotsJson)            — replace all markers with a new dataset
 *   highlight(grid)             — bring a specific grid's markers to the front
 *   setGridOverlay(enabled)     — show or hide the Maidenhead grid overlay
 *
 * Called from Rust/WASM via js_sys::Reflect (see components/map.rs).
 *
 * Clustering behaviour
 * --------------------
 * Spots are grouped by 4-character Maidenhead grid square.  At each zoom level
 * the pixel footprint of every grid square is computed using Leaflet's own
 * projection helpers.  When the footprint is narrower than EXPAND_THRESHOLD_PX
 * the grid is rendered as a single cluster marker (a DivIcon with a station
 * count badge).  Once the user zooms in past the threshold every station within
 * the grid is shown as its own circle marker, spiderfied into a ring so that
 * co-located markers (all WSPR spots in the same 4-char grid share identical
 * coordinates) remain individually clickable.
 *
 * Maidenhead grid overlay
 * -----------------------
 * Three levels are rendered depending on zoom:
 *   zoom ≤ 4  : Field boundaries (20° × 10°)  with 2-char labels
 *   zoom 3-8  : Square boundaries (2° × 1°)    with 4-char labels at zoom ≥ 7
 *   zoom ≥ 9  : Subsquare boundaries (5′ × 2.5′) with 6-char labels at zoom ≥ 13
 *
 * Lines are drawn to span only the visible viewport so the element count stays
 * O(viewport_rows + viewport_cols) regardless of zoom.  Labels are limited to
 * cells whose centre falls inside the viewport.
 */
(function () {
  "use strict";

  console.log("[wsprMap] map.js loaded");

  /** @type {L.Map | null} */
  let map = null;

  /** @type {L.LayerGroup} */
  let markerLayer = null;

  /** @type {L.LayerGroup} */
  let lineLayer = null;

  /** @type {L.LayerGroup} */
  let gridLayer = null;

  /** Whether the Maidenhead grid overlay is currently shown. */
  let gridOverlayEnabled = false;

  /** Home QTH coordinates (null when not configured). */
  let homeLatLon = null;

  /** The home QTH marker, kept so it is only added once. */
  let homeMarker = null;

  /**
   * The full, unfiltered spot array most recently passed to drawSpots().
   * Retained so that zoom/move events can trigger a redraw without a new data
   * fetch from Rust/WASM.
   * @type {Object[]}
   */
  let currentSpots = [];

  /**
   * Pixel width threshold for a grid square below which spots are collapsed
   * into a single cluster marker.  80 px gives comfortable inter-marker
   * spacing when individual stations are visible.
   * @type {number}
   */
  const EXPAND_THRESHOLD_PX = 80;

  // Grid step sizes in degrees.
  const FIELD_LON_STEP  = 20;
  const FIELD_LAT_STEP  = 10;
  const SQUARE_LON_STEP = 2;
  const SQUARE_LAT_STEP = 1;
  const SUBSQ_LON_STEP  = 5  / 60;   // 5 arc-minutes  ≈ 0.08333°
  const SUBSQ_LAT_STEP  = 2.5 / 60;  // 2.5 arc-minutes ≈ 0.04167°

  // -------------------------------------------------------------------------
  // Great-circle helpers
  // -------------------------------------------------------------------------

  /**
   * Compute a series of intermediate WGS-84 points along the great-circle arc
   * between two coordinates.
   *
   * Uses the intermediate-point formula from Ed Williams' Aviation Formulary.
   *
   * @param {number} lat1 - Origin latitude (degrees)
   * @param {number} lon1 - Origin longitude (degrees)
   * @param {number} lat2 - Destination latitude (degrees)
   * @param {number} lon2 - Destination longitude (degrees)
   * @param {number} [n=60] - Number of intermediate points
   * @returns {[number, number][]} Array of [lat, lon] pairs
   */
  function greatCirclePoints(lat1, lon1, lat2, lon2, n) {
    n = n || 60;
    const toRad = (d) => (d * Math.PI) / 180;
    const toDeg = (r) => (r * 180) / Math.PI;

    const φ1 = toRad(lat1), λ1 = toRad(lon1);
    const φ2 = toRad(lat2), λ2 = toRad(lon2);

    const d =
      2 *
      Math.asin(
        Math.sqrt(
          Math.pow(Math.sin((φ2 - φ1) / 2), 2) +
            Math.cos(φ1) * Math.cos(φ2) * Math.pow(Math.sin((λ2 - λ1) / 2), 2)
        )
      );

    if (d < 1e-6) return [[lat1, lon1], [lat2, lon2]];

    const points = [];
    for (let i = 0; i <= n; i++) {
      const f = i / n;
      const A = Math.sin((1 - f) * d) / Math.sin(d);
      const B = Math.sin(f * d) / Math.sin(d);
      const x = A * Math.cos(φ1) * Math.cos(λ1) + B * Math.cos(φ2) * Math.cos(λ2);
      const y = A * Math.cos(φ1) * Math.sin(λ1) + B * Math.cos(φ2) * Math.sin(λ2);
      const z = A * Math.sin(φ1) + B * Math.sin(φ2);
      const φ = Math.atan2(z, Math.sqrt(x * x + y * y));
      const λ = Math.atan2(y, x);
      points.push([toDeg(φ), toDeg(λ)]);
    }
    return points;
  }

  // -------------------------------------------------------------------------
  // Maidenhead grid square helpers
  // -------------------------------------------------------------------------

  /**
   * Snap a value down to the nearest multiple of step using integer arithmetic
   * to avoid floating-point drift.
   *
   * @param {number} v
   * @param {number} step
   * @returns {number}
   */
  function snapDown(v, step) {
    return Math.floor(v / step) * step;
  }

  /**
   * Compute the geographic centre [lat, lon] of a 4-character Maidenhead
   * grid square.
   *
   * @param {string} grid - 4-character grid locator, e.g. "FN20"
   * @returns {[number, number]} [latitude, longitude] of the cell centre
   */
  function gridCenter(grid) {
    const g = grid.toUpperCase();
    const lonField = (g.charCodeAt(0) - 65) * 20 - 180;
    const latField = (g.charCodeAt(1) - 65) * 10 - 90;
    const lon = lonField + parseInt(g[2], 10) * 2 + 1.0;
    const lat = latField + parseInt(g[3], 10) * 1 + 0.5;
    return [lat, lon];
  }

  /**
   * Return the pixel width of a 4-char grid square at the current zoom level.
   *
   * @param {number} lat - Latitude of the grid centre (degrees)
   * @param {number} lon - Longitude of the grid centre (degrees)
   * @returns {number} Width in screen pixels
   */
  function gridPixelWidth(lat, lon) {
    const zoom = map.getZoom();
    const west = map.project(L.latLng(lat, lon - 1.0), zoom);
    const east = map.project(L.latLng(lat, lon + 1.0), zoom);
    return east.x - west.x;
  }

  /**
   * Convert the south-west corner of a cell to its 2-character Maidenhead
   * field label (e.g. "FN").
   *
   * @param {number} lat - South edge latitude (degrees)
   * @param {number} lon - West edge longitude (degrees)
   * @returns {string}
   */
  function cellToField(lat, lon) {
    const lonNorm = lon + 180;
    const latNorm = lat + 90;
    return (
      String.fromCharCode(65 + Math.floor(lonNorm / 20)) +
      String.fromCharCode(65 + Math.floor(latNorm / 10))
    );
  }

  /**
   * Convert the south-west corner of a cell to its 4-character Maidenhead
   * square label (e.g. "FN20").
   *
   * @param {number} lat - South edge latitude (degrees)
   * @param {number} lon - West edge longitude (degrees)
   * @returns {string}
   */
  function cellToSquare(lat, lon) {
    const lonNorm = lon + 180;
    const latNorm = lat + 90;
    return (
      String.fromCharCode(65 + Math.floor(lonNorm / 20)) +
      String.fromCharCode(65 + Math.floor(latNorm / 10)) +
      Math.floor((lonNorm % 20) / 2).toString() +
      Math.floor(latNorm % 10).toString()
    );
  }

  /**
   * Convert the south-west corner of a cell to its 6-character Maidenhead
   * subsquare label (e.g. "FN20aa").
   *
   * Within each 4-char square (2° × 1°) there are 24 × 24 subsquares, each
   * spanning 5 arc-minutes of longitude and 2.5 arc-minutes of latitude.
   * The subsquare characters are lowercase a–x.
   *
   * @param {number} lat - South edge latitude (degrees)
   * @param {number} lon - West edge longitude (degrees)
   * @returns {string}
   */
  function cellToSubsquare(lat, lon) {
    const lonNorm = lon + 180;
    const latNorm = lat + 90;
    // Subsquare index within the 4-char square:
    //   24 cells per 2° longitude  → index = floor((lonNorm % 2) * 12)
    //   24 cells per 1° latitude   → index = floor((latNorm % 1) * 24)
    const subLon = Math.min(23, Math.floor((lonNorm % 2) * 12));
    const subLat = Math.min(23, Math.floor((latNorm % 1) * 24));
    return (
      String.fromCharCode(65 + Math.floor(lonNorm / 20)) +
      String.fromCharCode(65 + Math.floor(latNorm / 10)) +
      Math.floor((lonNorm % 20) / 2).toString() +
      Math.floor(latNorm % 10).toString() +
      String.fromCharCode(97 + subLon) +
      String.fromCharCode(97 + subLat)
    );
  }

  // -------------------------------------------------------------------------
  // Maidenhead grid overlay drawing
  // -------------------------------------------------------------------------

  /**
   * Draw a single level of the Maidenhead grid within the current viewport.
   *
   * Grid lines are drawn as polylines spanning the full visible extent so that
   * element count is O(rows + cols) rather than O(rows × cols).  Labels are
   * placed at each cell centre that falls inside the viewport.
   *
   * @param {number}   latStep   - Cell height in degrees
   * @param {number}   lonStep   - Cell width in degrees
   * @param {Object}   lineStyle - Leaflet polyline options
   * @param {Function} labelFn   - (cellSouthLat, cellWestLon) → string label
   * @param {boolean}  showLabels - Whether to add text labels
   */
  function drawGridLevel(latStep, lonStep, lineStyle, labelFn, showLabels) {
    const bounds = map.getBounds();
    const minLat = Math.max(-90,  bounds.getSouth());
    const maxLat = Math.min(90,   bounds.getNorth());
    const minLon = Math.max(-180, bounds.getWest());
    const maxLon = Math.min(180,  bounds.getEast());

    // --- Horizontal lines (constant latitude) --------------------------------
    const latOrigin = snapDown(minLat, latStep);
    const latCount  = Math.ceil((maxLat - latOrigin) / latStep) + 1;
    for (let i = 0; i <= latCount; i++) {
      const lat = latOrigin + i * latStep;
      if (lat < -90 || lat > 90) continue;
      L.polyline([[lat, minLon], [lat, maxLon]], lineStyle).addTo(gridLayer);
    }

    // --- Vertical lines (constant longitude) ---------------------------------
    const lonOrigin = snapDown(minLon, lonStep);
    const lonCount  = Math.ceil((maxLon - lonOrigin) / lonStep) + 1;
    for (let j = 0; j <= lonCount; j++) {
      const lon = lonOrigin + j * lonStep;
      if (lon < -180 || lon > 180) continue;
      L.polyline([[minLat, lon], [maxLat, lon]], lineStyle).addTo(gridLayer);
    }

    if (!showLabels) return;

    // --- Labels at each visible cell centre ----------------------------------
    for (let i = 0; i < latCount; i++) {
      const cellSouth = latOrigin + i * latStep;
      const centerLat = cellSouth + latStep / 2;
      if (centerLat <= minLat || centerLat >= maxLat) continue;

      for (let j = 0; j < lonCount; j++) {
        const cellWest = lonOrigin + j * lonStep;
        const centerLon = cellWest + lonStep / 2;
        if (centerLon <= minLon || centerLon >= maxLon) continue;

        const label = labelFn(cellSouth, cellWest);
        const icon = L.divIcon({
          className: "",
          html: '<span class="grid-label">' + label + "</span>",
          // iconAnchor null lets CSS centering via transform handle placement.
          iconSize: null,
          iconAnchor: null,
        });
        L.marker([centerLat, centerLon], { icon: icon, interactive: false })
          .addTo(gridLayer);
      }
    }
  }

  /**
   * Clear and redraw the Maidenhead grid overlay for the current viewport and
   * zoom level.
   *
   * Level selection:
   *   zoom ≤ 4  → field boundaries (20° × 10°) + field labels
   *   zoom 3–8  → square boundaries (2° × 1°)  + square labels at zoom ≥ 7
   *   zoom ≥ 9  → subsquare boundaries (5′ × 2.5′) + subsquare labels at zoom ≥ 13
   *
   * Field boundaries are always drawn as context even when a finer level is
   * active, but with reduced opacity so they don't compete with square lines.
   */
  function drawGrid() {
    if (!map || !gridLayer || !gridOverlayEnabled) return;
    gridLayer.clearLayers();

    const zoom = map.getZoom();

    // Field boundaries (always shown when the overlay is on).
    const fieldOpacity = zoom <= 4 ? 0.55 : 0.35;
    drawGridLevel(
      FIELD_LAT_STEP,
      FIELD_LON_STEP,
      { color: "#00d4aa", weight: zoom <= 4 ? 1.5 : 1, opacity: fieldOpacity,
        interactive: false, smoothFactor: 1 },
      cellToField,
      zoom <= 4  // field labels only when cells are large enough to read
    );

    // Square boundaries (zoom ≥ 3).
    if (zoom >= 3) {
      drawGridLevel(
        SQUARE_LAT_STEP,
        SQUARE_LON_STEP,
        { color: "#8b949e", weight: 0.75, opacity: 0.4,
          interactive: false, smoothFactor: 1 },
        cellToSquare,
        zoom >= 7  // labels only when pixels-per-cell is large enough to read
      );
    }

    // Subsquare boundaries (zoom ≥ 9).
    if (zoom >= 9) {
      drawGridLevel(
        SUBSQ_LAT_STEP,
        SUBSQ_LON_STEP,
        { color: "#484f58", weight: 0.5, opacity: 0.35,
          interactive: false, smoothFactor: 1 },
        cellToSubsquare,
        zoom >= 13  // labels only at very high zoom
      );
    }
  }

  // -------------------------------------------------------------------------
  // Spot grouping and clustering helpers
  // -------------------------------------------------------------------------

  /**
   * Group a flat array of spots by their 4-character grid square.
   *
   * @param {Object[]} spots
   * @returns {Map<string, Object[]>}
   */
  function groupByGrid(spots) {
    /** @type {Map<string, Object[]>} */
    const groups = new Map();
    for (const spot of spots) {
      const key = spot.grid.slice(0, 4).toUpperCase();
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(spot);
    }
    return groups;
  }

  /**
   * Reduce a group of spots to one representative per callsign, keeping the
   * spot with the highest SNR for each unique station.
   *
   * @param {Object[]} spots
   * @returns {Object[]}
   */
  function bestPerCallsign(spots) {
    /** @type {Map<string, Object>} */
    const best = new Map();
    for (const spot of spots) {
      const prev = best.get(spot.callsign);
      if (!prev || spot.snr_db > prev.snr_db) best.set(spot.callsign, spot);
    }
    return Array.from(best.values());
  }

  /**
   * Compute evenly-spaced positions arranged in a ring around a centre point,
   * with offsets expressed in screen pixels and converted back to lat/lon via
   * Leaflet's projection helpers.
   *
   * WSPR spots derive their lat/lon from the 4-char grid centre, so every
   * station in the same grid has identical coordinates.  Without spiderfying
   * they would stack on a single pixel and only the topmost marker would be
   * interactive.
   *
   * @param {number} count     - Number of positions to generate
   * @param {number} centerLat - Grid centre latitude
   * @param {number} centerLon - Grid centre longitude
   * @returns {[number, number][]} Array of [lat, lon] pairs
   */
  function spiderfyPositions(count, centerLat, centerLon) {
    if (count === 1) return [[centerLat, centerLon]];

    const RADIUS_PX = 18;
    const zoom = map.getZoom();
    const centerPx = map.project(L.latLng(centerLat, centerLon), zoom);

    const positions = [];
    for (let i = 0; i < count; i++) {
      // Start at top (−π/2) and space evenly clockwise.
      const angle = (2 * Math.PI * i) / count - Math.PI / 2;
      const px = L.point(
        centerPx.x + RADIUS_PX * Math.cos(angle),
        centerPx.y + RADIUS_PX * Math.sin(angle)
      );
      const latlng = map.unproject(px, zoom);
      positions.push([latlng.lat, latlng.lng]);
    }
    return positions;
  }

  // -------------------------------------------------------------------------
  // Marker helpers
  // -------------------------------------------------------------------------

  /**
   * Format a Unix epoch timestamp as "YYYY-MM-DD HH:MM UTC".
   * @param {number} unix
   * @returns {string}
   */
  function fmtTime(unix) {
    const d = new Date(unix * 1000);
    return d.toISOString().replace("T", " ").slice(0, 16) + " UTC";
  }

  /**
   * Format a frequency in Hz as "NN.NNNNNN MHz".
   * @param {number} hz
   * @returns {string}
   */
  function fmtFreq(hz) {
    return (hz / 1e6).toFixed(6) + " MHz";
  }

  /**
   * Build the HTML popup for a single spot.
   * @param {Object} spot
   * @returns {string}
   */
  function buildPopup(spot) {
    const distRow = spot.distance_km != null
      ? '<div class="popup-row"><span>Dist</span><span>' +
        Math.round(spot.distance_km) + " km</span></div>"
      : "";
    return (
      '<div class="popup-callsign">' + spot.callsign + "</div>" +
      '<div class="popup-grid">' + spot.grid + "</div>" +
      '<div class="popup-row"><span>Band</span><span>' + (spot.band_name || "?") + "</span></div>" +
      '<div class="popup-row"><span>Freq</span><span>' + fmtFreq(spot.freq_hz) + "</span></div>" +
      distRow +
      '<div class="popup-row"><span>SNR</span><span>' + spot.snr_db + " dB</span></div>" +
      '<div class="popup-row"><span>Pwr</span><span>' + spot.power_dbm + " dBm</span></div>" +
      '<div class="popup-row"><span>Time</span><span>' + fmtTime(spot.window_start_unix) + "</span></div>"
    );
  }

  /**
   * Build the HTML popup for a cluster marker, listing all unique callsigns
   * sorted by descending SNR.
   *
   * @param {string} grid   - 4-char grid label shown as the popup title
   * @param {Object[]} spots - All spots in this grid
   * @returns {string}
   */
  function buildClusterPopup(grid, spots) {
    const perCallsign = bestPerCallsign(spots);
    perCallsign.sort(function (a, b) { return b.snr_db - a.snr_db; });

    const count = perCallsign.length;
    const rows = perCallsign.map(function (s) {
      return (
        '<div class="popup-row">' +
        '<span class="callsign">' + s.callsign + "</span>" +
        "<span>" + s.snr_db + " dB</span>" +
        "</div>"
      );
    }).join("");

    return (
      '<div class="popup-callsign">' + grid + "</div>" +
      '<div class="popup-grid">' +
        count + " station" + (count !== 1 ? "s" : "") +
      "</div>" +
      rows
    );
  }

  /**
   * Create a Leaflet circle marker for a single spot at an explicit position.
   *
   * The rendered position is passed separately from spot.lat/spot.lon because
   * WSPR spots within the same 4-char grid share identical coordinates and
   * callers supply a spiderfied offset position instead.
   *
   * @param {Object} spot
   * @param {number} lat - Rendered latitude
   * @param {number} lon - Rendered longitude
   * @returns {L.CircleMarker}
   */
  function makeMarker(spot, lat, lon) {
    const color = spot.band_color || "#808080";
    const marker = L.circleMarker([lat, lon], {
      radius: 5,
      color: color,
      fillColor: color,
      fillOpacity: 0.75,
      weight: 1.5,
      opacity: 0.9,
      _grid: spot.grid,
      _isCluster: false,
      _bandColor: color,
    });
    marker.bindPopup(buildPopup(spot), { maxWidth: 240 });
    return marker;
  }

  /**
   * Create a cluster DivIcon marker displaying the station count for a grid.
   *
   * The marker is placed at the geographic centre of the grid square, not the
   * centroid of spot coordinates, so its position is stable as new spots
   * arrive.
   *
   * @param {string}   grid  - 4-character grid locator
   * @param {Object[]} spots - All spots in this grid
   * @param {number}   lat   - Grid centre latitude
   * @param {number}   lon   - Grid centre longitude
   * @returns {L.Marker}
   */
  function makeClusterMarker(grid, spots, lat, lon) {
    const count = bestPerCallsign(spots).length;
    const dominant = spots.reduce(function (best, s) {
      return s.snr_db > best.snr_db ? s : best;
    }, spots[0]);
    const color = dominant.band_color || "#00d4aa";

    const icon = L.divIcon({
      className: "",
      html: '<div class="grid-cluster" style="--cluster-color:' + color + '">' + count + "</div>",
      iconSize: [28, 28],
      iconAnchor: [14, 14],
      popupAnchor: [0, -14],
    });

    const marker = L.marker([lat, lon], { icon: icon, _grid: grid, _isCluster: true });
    marker.bindPopup(buildClusterPopup(grid, spots), { maxWidth: 260 });
    return marker;
  }

  // -------------------------------------------------------------------------
  // Great-circle line helper
  // -------------------------------------------------------------------------

  /**
   * Add a great-circle polyline from the home QTH to a destination.
   *
   * @param {number} destLat
   * @param {number} destLon
   * @param {string} color
   */
  function addHomeLine(destLat, destLon, color) {
    if (!homeLatLon) return;
    const pts = greatCirclePoints(homeLatLon[0], homeLatLon[1], destLat, destLon, 60);
    L.polyline(pts, {
      color: color || "#808080",
      weight: 1,
      opacity: 0.35,
      dashArray: "4 4",
    }).addTo(lineLayer);
  }

  // -------------------------------------------------------------------------
  // Core drawing — zoom-aware cluster / expand logic
  // -------------------------------------------------------------------------

  /**
   * Clear and redraw all spot markers.
   *
   * For each 4-char grid square the pixel footprint at the current zoom is
   * computed.  When it is narrower than EXPAND_THRESHOLD_PX the grid shows as
   * a cluster badge; otherwise each unique callsign gets its own circle marker,
   * spiderfied into a ring so they remain individually accessible.
   *
   * @param {Object[]} spots
   */
  function drawSpots(spots) {
    if (!map) return;

    markerLayer.clearLayers();
    lineLayer.clearLayers();

    const groups = groupByGrid(spots);

    groups.forEach(function (gridSpots, grid) {
      const center = gridCenter(grid);
      const lat = center[0];
      const lon = center[1];

      if (gridPixelWidth(lat, lon) >= EXPAND_THRESHOLD_PX) {
        // Expand: one circle marker per unique callsign, spiderfied.
        // All WSPR spots in the same 4-char grid share identical coordinates
        // (derived from the grid centre), so spiderfying is always needed.
        const perCallsign = bestPerCallsign(gridSpots);
        const positions = spiderfyPositions(perCallsign.length, lat, lon);
        perCallsign.forEach(function (spot, i) {
          markerLayer.addLayer(makeMarker(spot, positions[i][0], positions[i][1]));
          addHomeLine(positions[i][0], positions[i][1], spot.band_color);
        });
      } else {
        // Cluster: single badge at grid centre.
        markerLayer.addLayer(makeClusterMarker(grid, gridSpots, lat, lon));
        const dominant = gridSpots.reduce(function (best, s) {
          return s.snr_db > best.snr_db ? s : best;
        }, gridSpots[0]);
        addHomeLine(lat, lon, dominant.band_color);
      }
    });
  }

  // -------------------------------------------------------------------------
  // Combined redraw callback (spots + grid) on zoom / pan
  // -------------------------------------------------------------------------

  function onViewChange() {
    if (currentSpots.length > 0) drawSpots(currentSpots);
    if (gridOverlayEnabled) drawGrid();
  }

  // -------------------------------------------------------------------------
  // Public API: window.wsprMap
  // -------------------------------------------------------------------------
  window.wsprMap = {
    /**
     * Initialise the Leaflet map on the #map element and draw the first batch
     * of spots.
     *
     * Safe to call multiple times: if the map is already initialised, updates
     * the config and refreshes the markers.
     *
     * @param {string} configJson - Serialised PublicConfig
     * @param {string} spotsJson  - Serialised Vec<MapSpot>
     */
    init: function (configJson, spotsJson) {
      let config = {};
      let spots  = [];
      try { config = JSON.parse(configJson); } catch (_) {}
      try { spots  = JSON.parse(spotsJson);  } catch (_) {}

      if (config.my_lat != null && config.my_lon != null) {
        homeLatLon = [config.my_lat, config.my_lon];
      }

      if (!map) {
        map = L.map("map", {
          center: [20, 0],
          zoom: 2,
          zoomControl: true,
          attributionControl: true,
        });

        const tileLayer = L.tileLayer(
          "https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png",
          {
            maxZoom: 18,
            attribution:
              '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
          }
        ).addTo(map);

        lineLayer   = L.layerGroup().addTo(map);
        markerLayer = L.layerGroup().addTo(map);

        // Maidenhead grid overlay — wired into Leaflet's built-in layer control
        // so users can toggle it from the layers icon in the map corner.
        gridLayer = L.layerGroup();

        L.control.layers(
          null,
          { "Maidenhead Grid": gridLayer },
          { position: "topright", collapsed: true }
        ).addTo(map);

        map.on("overlayadd", function (e) {
          if (e.layer === gridLayer) {
            gridOverlayEnabled = true;
            drawGrid();
          }
        });
        map.on("overlayremove", function (e) {
          if (e.layer === gridLayer) {
            gridOverlayEnabled = false;
            gridLayer.clearLayers();
          }
        });

        // Single handler for both spots and grid on view change.
        map.on("zoomend moveend", onViewChange);
      }

      if (homeLatLon && !homeMarker) {
        const grid = config.my_grid || "";
        homeMarker = L.circleMarker(homeLatLon, {
          radius: 8,
          color: "#ffffff",
          fillColor: "#00d4aa",
          fillOpacity: 1,
          weight: 2,
        })
          .bindPopup(
            '<div class="popup-callsign">Home QTH</div>' +
              '<div class="popup-grid">' + grid + "</div>"
          )
          .addTo(map);
      }

      currentSpots = spots;
      drawSpots(spots);
    },

    /**
     * Replace all spot markers with a new set.
     *
     * @param {string} spotsJson - Serialised Vec<MapSpot>
     */
    update: function (spotsJson) {
      if (!map) return;
      let spots = [];
      try { spots = JSON.parse(spotsJson); } catch (_) {}
      currentSpots = spots;
      drawSpots(spots);
    },

    /**
     * Bring all markers for a specific grid square to the foreground and open
     * their popups.  Works for both cluster and individual markers.
     *
     * @param {string} grid - Maidenhead grid square, e.g. "FN20"
     */
    highlight: function (grid) {
      if (!map || !markerLayer) return;
      const upper = grid.toUpperCase().slice(0, 4);
      markerLayer.eachLayer(function (layer) {
        if (!layer.options || !layer.options._grid) return;
        if (layer.options._grid.toUpperCase().slice(0, 4) !== upper) return;
        layer.openPopup();
        const latlng = layer.getLatLng
          ? layer.getLatLng()
          : L.latLng.apply(null, gridCenter(upper));
        map.panTo(latlng);
      });
    },

    /**
     * Show or hide the Maidenhead grid overlay programmatically.
     *
     * This is called from Rust/WASM when the sidebar checkbox changes.
     * It mirrors the same state that the Leaflet layer control manages when
     * the user clicks the overlay toggle directly on the map.
     *
     * @param {boolean} enabled
     */
    setGridOverlay: function (enabled) {
      if (!map || !gridLayer) return;
      if (enabled && !gridOverlayEnabled) {
        map.addLayer(gridLayer);
        // 'overlayadd' fires, which sets gridOverlayEnabled and calls drawGrid().
      } else if (!enabled && gridOverlayEnabled) {
        map.removeLayer(gridLayer);
        // 'overlayremove' fires, which clears gridOverlayEnabled and layers.
      }
    },
  };
})();
