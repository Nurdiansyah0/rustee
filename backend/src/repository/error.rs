use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Entity not found or access denied")]
    NotFound,

    #[error("Unique constraint violation: {constraint}")]
    UniqueViolation { constraint: String },

    #[error("Foreign key constraint violation: {0}")]
    ForeignKeyViolation(String),

    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Idempotent request payload mismatch")]
    IdempotencyPayloadMismatch,

    #[error("Idempotent operation already in progress")]
    IdempotencyInProgress,

    #[error("Cannot modify or delete system default entity")]
    CannotDeleteSystemEntity,

    #[error("Validation error: {0}")]
    Validation(String),
}

impl DbError {
    pub fn from_sqlx(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db_err) = err {
            let code = db_err.code().unwrap_or_default();
            let msg = db_err.message();

            // SQLite 2067: SQLITE_CONSTRAINT_UNIQUE, 1555: SQLITE_CONSTRAINT_PRIMARYKEY
            if code == "2067" || code == "1555" || msg.contains("UNIQUE constraint failed") {
                return DbError::UniqueViolation {
                    constraint: msg.to_string(),
                };
            }

            // SQLite 787: SQLITE_CONSTRAINT_FOREIGNKEY
            if code == "787" || msg.contains("FOREIGN KEY constraint failed") {
                return DbError::ForeignKeyViolation(msg.to_string());
            }
        }
        DbError::Sqlx(err)
    }
}
