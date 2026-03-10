/// Application-level error type used by Axum handlers and server functions.
///
/// All fallible operations return `Result<T, AppError>`. The `IntoResponse`
/// implementation converts errors into appropriate HTTP responses so handlers
/// can use the `?` operator freely.
#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// ClickHouse query or connection failure.
    #[error("database error: {0}")]
    Database(#[from] clickhouse::error::Error),

    /// JSON serialization failure.
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Generic internal error with context string.
    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(feature = "ssr")]
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

#[cfg(feature = "ssr")]
impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        let status = match &self {
            AppError::Database(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
    }
}
