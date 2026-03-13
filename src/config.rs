/// Server-side configuration loaded from environment variables / `.env` file.
///
/// Use `Config::from_env()` once at startup, then wrap in `Arc<Config>` and
/// share via Axum state / Leptos context.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone)]
pub struct Config {
    /// Full HTTP URL of the ClickHouse server (e.g. `http://10.174.3.247:8123`).
    pub clickhouse_url: String,

    /// ClickHouse database name.
    pub clickhouse_db: String,

    /// ClickHouse table name for personal WSPR spots.
    pub clickhouse_table: String,

    /// ClickHouse database name for the global WSPR spots table.
    ///
    /// Populated from `WSPR_GLOBAL_DB`.  When `None`, the global table is
    /// queried inside `clickhouse_db` (same database as personal spots).
    pub global_db: Option<String>,

    /// ClickHouse table name for global WSPR spots.
    ///
    /// Populated from `WSPR_GLOBAL_TABLE` (default: `"global_spots"`).
    pub global_table: String,

    /// Optional ClickHouse username.
    pub clickhouse_user: Option<String>,

    /// Optional ClickHouse password (loaded from `.env`, never logged).
    pub clickhouse_password: Option<String>,

    /// Receiver QTH Maidenhead grid square used for great-circle lines.
    /// When `None`, great-circle lines are disabled.
    pub my_grid: Option<String>,

    /// How many hours of data to show on initial page load.
    pub time_window_hours: u32,

    /// Default and maximum row limit applied to local spot and map-spot queries.
    ///
    /// Populated from `WSPR_SPOT_LIMIT` (default: `5000`).  Acts as both the
    /// fallback when the caller supplies no limit and the hard cap on any
    /// caller-supplied limit.
    pub spot_limit: u32,

    /// Default and maximum row limit applied to global spot and map-spot queries.
    ///
    /// Populated from `WSPR_GLOBAL_SPOT_LIMIT` (default: `10000`).  Kept
    /// separate from `spot_limit` because the global table is orders of
    /// magnitude larger and benefits from a higher default cap.
    pub global_spot_limit: u32,

    /// Callsigns excluded from all query results, normalised to uppercase.
    ///
    /// Populated from `WSPR_IGNORE_CALLSIGNS` as a comma-separated list,
    /// e.g. `W3POG,N0CALL`.  Empty list disables the filter.
    pub ignore_callsigns: Vec<String>,

    /// Leaflet zoom level applied when the user clicks a table row.
    ///
    /// Populated from `WSPR_DETAIL_ZOOM` (default: `10`).  The map will
    /// `setView` to this zoom level so the selected station's grid is shown at
    /// a useful street-level detail.
    pub detail_zoom: u8,
}

#[cfg(feature = "ssr")]
impl Config {
    /// Load configuration from environment variables.
    ///
    /// Reads `.env` first (via `dotenvy`) then falls back to process env.
    ///
    /// # Errors
    ///
    /// Returns an error if required variables are missing.
    pub fn from_env() -> anyhow::Result<Self> {
        // Load .env if present; ignore error if file doesn't exist.
        let _ = dotenvy::dotenv();

        Ok(Self {
            clickhouse_url: std::env::var("WSPR_CLICKHOUSE_URL")
                .unwrap_or_else(|_| "http://10.174.3.247:8123".to_string()),
            clickhouse_db: std::env::var("WSPR_CLICKHOUSE_DB")
                .unwrap_or_else(|_| "wsprrs".to_string()),
            clickhouse_table: std::env::var("WSPR_CLICKHOUSE_TABLE")
                .unwrap_or_else(|_| "wspr_spots".to_string()),
            global_db: std::env::var("WSPR_GLOBAL_DB").ok(),
            global_table: std::env::var("WSPR_GLOBAL_TABLE")
                .unwrap_or_else(|_| "global_spots".to_string()),
            clickhouse_user: std::env::var("WSPR_CLICKHOUSE_USER").ok(),
            clickhouse_password: std::env::var("WSPR_CLICKHOUSE_PASSWORD").ok(),
            my_grid: std::env::var("WSPR_MY_GRID").ok(),
            time_window_hours: std::env::var("WSPR_TIME_WINDOW_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
            spot_limit: std::env::var("WSPR_SPOT_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5_000),
            global_spot_limit: std::env::var("WSPR_GLOBAL_SPOT_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000),
            ignore_callsigns: std::env::var("WSPR_IGNORE_CALLSIGNS")
                .ok()
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_uppercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            detail_zoom: std::env::var("WSPR_DETAIL_ZOOM")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        })
    }

    /// Return the fully-qualified global table identifier.
    ///
    /// When `WSPR_GLOBAL_DB` is set this returns `"db.table"`, otherwise just
    /// `"table"`.  Pass the result wherever a table name is needed in global
    /// queries so ClickHouse can resolve it even when the client is pointed at
    /// a different database.
    pub fn global_table_qualified(&self) -> String {
        match &self.global_db {
            Some(db) => format!("{}.{}", db, self.global_table),
            None => self.global_table.clone(),
        }
    }

    /// Build a `clickhouse::Client` from this configuration.
    pub fn clickhouse_client(&self) -> clickhouse::Client {
        let mut client = clickhouse::Client::default()
            .with_url(&self.clickhouse_url)
            .with_database(&self.clickhouse_db);

        if let Some(user) = &self.clickhouse_user {
            client = client.with_user(user);
        }
        if let Some(pass) = &self.clickhouse_password {
            client = client.with_password(pass);
        }

        client
    }
}
