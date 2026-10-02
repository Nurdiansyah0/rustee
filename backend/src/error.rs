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

    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String, &'static str),

    #[error("Method not allowed: {0}")]
    MethodNotAllowed(String, &'static str),

    #[error("Database error: {0}")]
    Database(sqlx::Error),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest(..) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized(..) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(..) => StatusCode::FORBIDDEN,
            AppError::NotFound(..) => StatusCode::NOT_FOUND,
            AppError::Conflict(..) => StatusCode::CONFLICT,
            AppError::UnprocessableEntity(..) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::MethodNotAllowed(..) => StatusCode::METHOD_NOT_ALLOWED,
            AppError::TooManyRequests(..) => StatusCode::TOO_MANY_REQUESTS,
            AppError::Database(..) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(..) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
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
                if constraint.contains("tenants.slug") || constraint.contains("slug") {
                    AppError::Conflict(
                        "A workspace with this slug already exists".to_string(),
                        "SLUG_ALREADY_EXISTS",
                    )
                } else if constraint.contains("chart_of_accounts") || constraint.contains("coa") {
                    AppError::Conflict(
                        "An account with this code already exists".to_string(),
                        "ACCOUNT_ALREADY_EXISTS",
                    )
                } else if constraint.contains("invoice_snapshots") || constraint.contains("receivables") {
                    AppError::Conflict(
                        "Invoice is already issued or finalized".to_string(),
                        "ALREADY_ISSUED",
                    )
                } else if constraint.contains("invoices.invoice_number") || constraint.contains("idx_invoices_tenant_number") {
                    AppError::Conflict(
                        "Invoice sequence collision during concurrent issuing, please retry".to_string(),
                        "CONFLICT",
                    )
                } else if constraint.contains("journal_entries") || constraint.contains("uq_journal_entries_number") {
                    AppError::Conflict(
                        "Journal sequence collision during concurrent posting, please retry".to_string(),
                        "CONFLICT",
                    )
                } else if constraint.contains("payments") || constraint.contains("uq_payments_tenant_number") {
                    AppError::Conflict(
                        "Payment sequence collision during concurrent payment, please retry".to_string(),
                        "CONFLICT",
                    )
                } else if constraint.contains("lock contention") || constraint.contains("locked") || constraint.contains("busy") {
                    AppError::Conflict(
                        "Database lock contention during concurrent operation, please retry".to_string(),
                        "LOCK_CONTENTION",
                    )
                } else {
                    AppError::Conflict(constraint, "UNIQUE_VIOLATION")
                }
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
            DbError::Validation(msg) => {
                if msg.contains("chk_journal_line_nonzero") || (msg.contains("debit") && msg.contains("credit")) {
                    AppError::BadRequest(
                        "A journal line cannot have both debit and credit".to_string(),
                        "INVALID_JOURNAL_LINES",
                    )
                } else if msg.contains("cannot have financial terms modified")
                    || msg.contains("cannot be modified")
                    || msg.contains("strictly immutable")
                    || msg.contains("invalid status")
                    || msg.contains("cannot be deleted")
                    || msg.contains("Line items cannot")
                {
                    AppError::Conflict(msg, "INVOICE_LOCKED")
                } else {
                    AppError::BadRequest(msg, "VALIDATION_FAILED")
                }
            }
            DbError::Sqlx(e) => AppError::Database(e),
            DbError::Serialization(e) => AppError::Internal(e),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::from(DbError::from_sqlx(err))
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
            AppError::UnprocessableEntity(msg, code) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Unprocessable Entity",
                "unprocessable-entity",
                msg,
                code,
            ),
            AppError::MethodNotAllowed(msg, code) => (
                StatusCode::METHOD_NOT_ALLOWED,
                "Method Not Allowed",
                "method-not-allowed",
                msg,
                code,
            ),
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
