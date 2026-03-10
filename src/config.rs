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

    /// ClickHouse table name for WSPR spots.
    pub clickhouse_table: String,

    /// Optional ClickHouse username.
    pub clickhouse_user: Option<String>,

    /// Optional ClickHouse password (loaded from `.env`, never logged).
    pub clickhouse_password: Option<String>,

    /// Receiver QTH Maidenhead grid square used for great-circle lines.
    /// When `None`, great-circle lines are disabled.
    pub my_grid: Option<String>,

    /// How many hours of data to show on initial page load.
    pub time_window_hours: u32,
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
            clickhouse_user: std::env::var("WSPR_CLICKHOUSE_USER").ok(),
            clickhouse_password: std::env::var("WSPR_CLICKHOUSE_PASSWORD").ok(),
            my_grid: std::env::var("WSPR_MY_GRID").ok(),
            time_window_hours: std::env::var("WSPR_TIME_WINDOW_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
        })
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
