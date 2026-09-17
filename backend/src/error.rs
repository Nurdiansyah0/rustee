use crate::repository::error::DbError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String, &'static str),

    #[error("Unauthorized: {0}")]
    Unauthorized(String, &'static str),

    #[error("Forbidden: {0}")]
    Forbidden(String, &'static str),

    #[error("Not found: {0}")]
    NotFound(String, &'static str),

    #[error("Conflict: {0}")]
    Conflict(String, &'static str),

    #[error("Too many requests: {0}")]
    TooManyRequests(String, &'static str),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal server error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rfc7807Error {
    pub r#type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub code: String,
}

impl From<DbError> for AppError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => AppError::NotFound("Resource not found".to_string(), "NOT_FOUND"),
            DbError::UniqueViolation { constraint } => {
                AppError::Conflict(constraint, "UNIQUE_VIOLATION")
            }
            DbError::ForeignKeyViolation(msg) => AppError::BadRequest(msg, "FOREIGN_KEY_VIOLATION"),
            DbError::CannotDeleteSystemEntity => AppError::Forbidden(
                "Cannot modify or delete system default entity".to_string(),
                "CANNOT_DELETE_SYSTEM_ENTITY",
            ),
            DbError::IdempotencyPayloadMismatch => AppError::Conflict(
                "Idempotency key payload mismatch".to_string(),
                "IDEMPOTENCY_MISMATCH",
            ),
            DbError::IdempotencyInProgress => AppError::Conflict(
                "Operation with this idempotency key is already in progress".to_string(),
                "IDEMPOTENCY_IN_PROGRESS",
            ),
            DbError::Validation(msg) => AppError::BadRequest(msg, "VALIDATION_FAILED"),
            DbError::Sqlx(e) => AppError::Database(e),
            DbError::Serialization(e) => AppError::Internal(e),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, title, type_slug, detail, code) = match self {
            AppError::BadRequest(msg, code) => (
                StatusCode::BAD_REQUEST,
                "Bad Request",
                "bad-request",
                msg,
                code,
            ),
            AppError::Unauthorized(msg, code) => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                "unauthorized",
                msg,
                code,
            ),
            AppError::Forbidden(msg, code) => {
                let type_slug = if code == "FEATURE_LOCKED" {
                    "feature-locked"
                } else {
                    "forbidden"
                };
                (StatusCode::FORBIDDEN, "Forbidden", type_slug, msg, code)
            }
            AppError::NotFound(msg, code) => {
                (StatusCode::NOT_FOUND, "Not Found", "not-found", msg, code)
            }
            AppError::Conflict(msg, code) => {
                (StatusCode::CONFLICT, "Conflict", "conflict", msg, code)
            }
            AppError::TooManyRequests(msg, code) => (
                StatusCode::TOO_MANY_REQUESTS,
                "Too Many Requests",
                "too-many-requests",
                msg,
                code,
            ),
            AppError::Database(_err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                "internal-error",
                "A database error occurred".to_string(),
                "DATABASE_ERROR",
            ),
            AppError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                "internal-error",
                err,
                "INTERNAL_ERROR",
            ),
        };

        let body = Rfc7807Error {
            r#type: format!("https://api.nurdiansyahlabs.com/errors/{}", type_slug),
            title: title.to_string(),
            status: status.as_u16(),
            detail,
            code: code.to_string(),
        };

        (status, Json(body)).into_response()
    }
}
