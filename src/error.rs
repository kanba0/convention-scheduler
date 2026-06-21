//! The single error type every handler returns on failure. Implementing axum's
//! `IntoResponse` turns any `Err` into a JSON response with the right status.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Every way a request can fail, mapped to an HTTP status in `into_response`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested row doesn't exist. -> 404
    #[error("not found")]
    NotFound,

    /// A value broke a DB rule treated as user error (e.g. a CHECK). -> 422
    #[error("validation failed: {0}")]
    Validation(String),

    /// A uniqueness rule was violated. -> 409
    #[error("conflict: {0}")]
    Conflict(String),

    /// Anything else from the database — a real bug or outage, not the
    /// client's fault. Logged in full, reported as a generic message. -> 500
    #[error(transparent)]
    Database(sqlx::Error),
}

/// Lets `?` turn a raw `sqlx::Error` into an `AppError`, mapping violated
/// constraints to 4xx and everything else to 500.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &err {
            if db.is_unique_violation() {
                return AppError::Conflict(db.constraint().unwrap_or("unique").to_string());
            }
            if db.is_check_violation() {
                return AppError::Validation(db.constraint().unwrap_or("check").to_string());
            }
            if db.is_foreign_key_violation() {
                return AppError::Validation(db.constraint().unwrap_or("foreign key").to_string());
            }
        }
        AppError::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            AppError::Validation(what) => (StatusCode::UNPROCESSABLE_ENTITY, what),
            AppError::Conflict(what) => (StatusCode::CONFLICT, what),
            AppError::Database(err) => {
                tracing::error!("database error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
