/**
 * WSPR Visualizer — Leaflet map bridge
 *
 * Exposes a single namespace: window.wsprMap with three methods:
 *   init(configJson, spotsJson)  — create the map and draw initial markers
 *   update(spotsJson)            — replace all markers with a new dataset
 *   highlight(grid)              — bring a specific grid's markers to the front
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
 * the grid is shown as its own circle marker at its true lat/lon.
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

  /** Home QTH coordinates (null when not configured). */
  let homeLatLon = null;

  /** The home QTH marker, kept so it is only added once. */
  let homeMarker = null;

  /**
   * The full, unfiltered spot array most recently passed to drawSpots().
   * Retained so that zoom events can trigger a redraw without a new data
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

    // Angular distance between the two points.
    const d =
      2 *
      Math.asin(
        Math.sqrt(
          Math.pow(Math.sin((φ2 - φ1) / 2), 2) +
            Math.cos(φ1) * Math.cos(φ2) * Math.pow(Math.sin((λ2 - λ1) / 2), 2)
        )
      );

    // Points are coincident or nearly so — return a trivial two-point line.
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
  // Maidenhead helpers
  // -------------------------------------------------------------------------

  /**
   * Compute the geographic centre [lat, lon] of a 4-character Maidenhead
   * grid square.
   *
   * A 4-char square spans exactly 2° longitude × 1° latitude.  The centre is
   * at the midpoint of that cell.
   *
   * @param {string} grid - 4-character grid locator, e.g. "FN20"
   * @returns {[number, number]} [latitude, longitude] of the cell centre
   */
  function gridCenter(grid) {
    const g = grid.toUpperCase();
    // Field letters encode 20° lon / 10° lat increments starting at -180/-90.
    const lonField = (g.charCodeAt(0) - 65) * 20 - 180;
    const latField = (g.charCodeAt(1) - 65) * 10 - 90;
    // Square digits subdivide each field into 10 cells (2° lon / 1° lat each).
    const lon = lonField + parseInt(g[2], 10) * 2 + 1.0;
    const lat = latField + parseInt(g[3], 10) * 1 + 0.5;
    return [lat, lon];
  }

  /**
   * Return the pixel width of a 4-char Maidenhead grid square at the current
   * map zoom level, using Leaflet's Mercator projection helpers.
   *
   * A 4-char square is exactly 2° wide in longitude; we project the western
   * and eastern edges at the given latitude and return the pixel difference.
   *
   * @param {number} lat - Latitude of the grid centre (degrees)
   * @param {number} lon - Longitude of the grid centre (degrees)
   * @returns {number} Width in screen pixels at current zoom
   */
  function gridPixelWidth(lat, lon) {
    const zoom = map.getZoom();
    const west = map.project(L.latLng(lat, lon - 1.0), zoom);
    const east = map.project(L.latLng(lat, lon + 1.0), zoom);
    return east.x - west.x;
  }

  // -------------------------------------------------------------------------
  // Spot grouping helpers
  // -------------------------------------------------------------------------

  /**
   * Group a flat array of spots by their 4-character grid square.
   *
   * @param {Object[]} spots
   * @returns {Map<string, Object[]>} grid -> spots
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
   * Reduce a group of spots (all within one grid) to one representative per
   * callsign, keeping the spot with the highest SNR for each station.
   *
   * @param {Object[]} spots
   * @returns {Object[]}
   */
  function bestPerCallsign(spots) {
    /** @type {Map<string, Object>} */
    const best = new Map();
    for (const spot of spots) {
      const prev = best.get(spot.callsign);
      if (!prev || spot.snr_db > prev.snr_db) {
        best.set(spot.callsign, spot);
      }
    }
    return Array.from(best.values());
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
   * Build the HTML string for a single spot's Leaflet popup.
   * @param {Object} spot - MapSpot from the server
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
   * in the grid and their best SNR.
   *
   * @param {string} grid - 4-character grid locator
   * @param {Object[]} spots - All spots in this grid
   * @returns {string}
   */
  function buildClusterPopup(grid, spots) {
    const perCallsign = bestPerCallsign(spots);
    // Sort by descending SNR so the strongest station is at the top.
    perCallsign.sort(function (a, b) { return b.snr_db - a.snr_db; });

    const count = perCallsign.length;
    const rows = perCallsign.map(function (s) {
      return (
        '<div class="popup-row">' +
        '<span class="callsign">' + s.callsign + "</span>" +
        '<span>' + s.snr_db + " dB</span>" +
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
   * Compute evenly-spaced positions around a fixed-pixel-radius ring centred
   * on the grid centre, converting pixel offsets back to lat/lon via Leaflet's
   * projection helpers.
   *
   * WSPR spots derive their latitude and longitude from the 4-character grid
   * locator centre, so every station within the same grid reports the identical
   * coordinates.  Without spiderfying they would be stacked on a single pixel
   * and only the topmost marker would be interactive.
   *
   * @param {number} count     - Number of positions to generate
   * @param {number} centerLat - Grid centre latitude (degrees)
   * @param {number} centerLon - Grid centre longitude (degrees)
   * @returns {[number, number][]} Array of [lat, lon] pairs
   */
  function spiderfyPositions(count, centerLat, centerLon) {
    if (count === 1) return [[centerLat, centerLon]];

    const RADIUS_PX = 18;
    const zoom = map.getZoom();
    const centerPx = map.project(L.latLng(centerLat, centerLon), zoom);

    const positions = [];
    for (let i = 0; i < count; i++) {
      // Start at the top (-PI/2) and space evenly clockwise.
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

  /**
   * Create a Leaflet circle marker for a single spot at an explicit position.
   *
   * The position is passed separately from `spot.lat`/`spot.lon` because WSPR
   * spots within the same 4-char grid share identical coordinates; callers
   * supply a spiderfied offset position instead.
   *
   * @param {Object} spot
   * @param {number} lat - Rendered latitude (may differ from spot.lat)
   * @param {number} lon - Rendered longitude (may differ from spot.lon)
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
      // Store metadata for highlight() lookup.
      _grid: spot.grid,
      _isCluster: false,
      _bandColor: color,
    });
    marker.bindPopup(buildPopup(spot), { maxWidth: 240 });
    return marker;
  }

  /**
   * Create a cluster DivIcon marker that displays the number of unique
   * stations in a grid square.
   *
   * The marker is placed at the geographic centre of the grid square (not the
   * centroid of spot coordinates) so that its position is stable as new spots
   * arrive.
   *
   * @param {string} grid   - 4-character grid locator
   * @param {Object[]} spots - All spots in this grid
   * @param {number} lat    - Grid centre latitude
   * @param {number} lon    - Grid centre longitude
   * @returns {L.Marker}
   */
  function makeClusterMarker(grid, spots, lat, lon) {
    const perCallsign = bestPerCallsign(spots);
    const count = perCallsign.length;

    // Use the band color of the strongest-SNR spot as the badge accent.
    const dominant = spots.reduce(function (best, s) {
      return s.snr_db > best.snr_db ? s : best;
    }, spots[0]);
    const color = dominant.band_color || "#00d4aa";

    const html =
      '<div class="grid-cluster" style="--cluster-color:' + color + '">' +
        count +
      "</div>";

    const icon = L.divIcon({
      className: "",
      html: html,
      iconSize: [28, 28],
      iconAnchor: [14, 14],
      popupAnchor: [0, -14],
    });

    const marker = L.marker([lat, lon], {
      icon: icon,
      // Store for highlight() and zoom-redraw housekeeping.
      _grid: grid,
      _isCluster: true,
    });

    marker.bindPopup(buildClusterPopup(grid, spots), { maxWidth: 260 });
    return marker;
  }

  // -------------------------------------------------------------------------
  // Great-circle line helper
  // -------------------------------------------------------------------------

  /**
   * Add a great-circle polyline from the home QTH to a destination point.
   *
   * @param {number} destLat
   * @param {number} destLon
   * @param {string} color - CSS colour string
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
   * For each 4-character grid square the function computes the grid's pixel
   * footprint at the current zoom level.  When that footprint is narrower than
   * EXPAND_THRESHOLD_PX the grid is represented by a single cluster badge;
   * otherwise every unique station within the grid gets its own circle marker.
   *
   * @param {Object[]} spots - Array of MapSpot objects (full dataset, not pre-deduplicated)
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

      const expand = gridPixelWidth(lat, lon) >= EXPAND_THRESHOLD_PX;

      if (expand) {
        // Show one circle marker per unique callsign within this grid.
        // Because WSPR positions are derived from the 4-char grid centre,
        // all stations share identical coordinates — spiderfy them into a
        // ring so each marker is individually visible and clickable.
        const perCallsign = bestPerCallsign(gridSpots);
        const positions = spiderfyPositions(perCallsign.length, lat, lon);
        perCallsign.forEach(function (spot, i) {
          markerLayer.addLayer(makeMarker(spot, positions[i][0], positions[i][1]));
          addHomeLine(positions[i][0], positions[i][1], spot.band_color);
        });
      } else {
        // Collapse the entire grid into a single cluster badge.
        markerLayer.addLayer(makeClusterMarker(grid, gridSpots, lat, lon));
        // Draw one line to the grid centre representing the whole cluster.
        const dominant = gridSpots.reduce(function (best, s) {
          return s.snr_db > best.snr_db ? s : best;
        }, gridSpots[0]);
        addHomeLine(lat, lon, dominant.band_color);
      }
    });
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
     * the config and refreshes the markers instead.
     *
     * @param {string} configJson - Serialised PublicConfig
     * @param {string} spotsJson  - Serialised Vec<MapSpot>
     */
    init: function (configJson, spotsJson) {
      let config = {};
      let spots = [];
      try { config = JSON.parse(configJson); } catch (_) {}
      try { spots = JSON.parse(spotsJson); } catch (_) {}

      // Store home QTH for great-circle rendering.
      if (config.my_lat != null && config.my_lon != null) {
        homeLatLon = [config.my_lat, config.my_lon];
      }

      if (!map) {
        // First call: create the Leaflet map instance.
        map = L.map("map", {
          center: [20, 0],
          zoom: 2,
          zoomControl: true,
          attributionControl: true,
        });

        L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
          maxZoom: 18,
          attribution:
            '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
        }).addTo(map);

        lineLayer   = L.layerGroup().addTo(map);
        markerLayer = L.layerGroup().addTo(map);

        // Redraw on zoom so cluster/expand thresholds are re-evaluated.
        map.on("zoomend", function () {
          if (currentSpots.length > 0) drawSpots(currentSpots);
        });
      }

      // Place the home QTH marker the first time coordinates are available.
      // Outside the `!map` block because config may arrive after map init.
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
     * Called when the filter changes or the live stream delivers new spots.
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
     * Bring all markers for a specific grid square to the visual foreground
     * and open their popups.  If the grid is currently clustered the cluster
     * popup is opened at the grid centre; if it is expanded each individual
     * station marker's popup is opened.
     *
     * @param {string} grid - Maidenhead grid square, e.g. "FN20"
     */
    highlight: function (grid) {
      if (!map || !markerLayer) return;
      const upper = grid.toUpperCase().slice(0, 4);
      markerLayer.eachLayer(function (layer) {
        if (!layer.options || !layer.options._grid) return;
        const layerGrid = layer.options._grid.toUpperCase().slice(0, 4);
        if (layerGrid !== upper) return;

        layer.openPopup();

        // Pan to the correct coordinates regardless of marker type.
        const latlng = layer.getLatLng
          ? layer.getLatLng()
          : L.latLng.apply(null, gridCenter(upper));
        map.panTo(latlng);
      });
    },
  };
})();
