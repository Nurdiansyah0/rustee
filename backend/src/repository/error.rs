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
        match err {
            sqlx::Error::PoolTimedOut => {
                return DbError::UniqueViolation {
                    constraint: "Database lock contention during concurrent operation, please retry".to_string(),
                };
            }
            sqlx::Error::Database(ref db_err) => {
                let code = db_err.code().unwrap_or_default();
                let msg = db_err.message();

                // SQLite 2067: SQLITE_CONSTRAINT_UNIQUE, 1555: SQLITE_CONSTRAINT_PRIMARYKEY
                // Code 19: Primary SQLITE_CONSTRAINT code
                // Code 23505: PostgreSQL unique_violation
                if code == "2067"
                    || code == "1555"
                    || code == "23505"
                    || (code == "19" && (msg.contains("UNIQUE") || msg.contains("PRIMARY KEY")))
                    || msg.contains("UNIQUE constraint failed")
                    || msg.contains("PRIMARY KEY constraint failed")
                {
                    return DbError::UniqueViolation {
                        constraint: msg.to_string(),
                    };
                }

                // SQLite 787: SQLITE_CONSTRAINT_FOREIGNKEY, 23503: PostgreSQL foreign_key_violation
                if code == "787" || code == "23503" || msg.contains("FOREIGN KEY constraint failed") {
                    return DbError::ForeignKeyViolation(msg.to_string());
                }

                // SQLite 275: SQLITE_CONSTRAINT_CHECK, 23514: PostgreSQL check_violation
                if code == "275" || code == "23514" || msg.contains("CHECK constraint failed") {
                    return DbError::Validation(msg.to_string());
                }

                // SQLite Trigger Abort (SQLITE_CONSTRAINT_TRIGGER 1811 or code 19 with trigger messages)
                if code == "1811"
                    || (code == "19" && (msg.contains("cannot") || msg.contains("immutable") || msg.contains("invalid status")))
                    || msg.contains("cannot have financial terms modified")
                    || msg.contains("Issued invoices cannot be deleted")
                    || msg.contains("Non-draft invoices cannot be deleted")
                    || msg.contains("Line items cannot")
                    || msg.contains("Invoice snapshots are strictly immutable")
                    || msg.contains("Confirmed payments cannot")
                    || msg.contains("Payment allocations are immutable")
                    || msg.contains("Payment allocations cannot be deleted")
                {
                    return DbError::Validation(msg.to_string());
                }

                // SQLite Lock Contention / Busy (code 5 SQLITE_BUSY, code 517 SQLITE_BUSY_SNAPSHOT, code 6 SQLITE_LOCKED)
                if code == "5" || code == "517" || code == "6" || msg.contains("database is locked") || msg.contains("busy snapshot") {
                    return DbError::UniqueViolation {
                        constraint: "Database lock contention during concurrent operation, please retry".to_string(),
                    };
                }
            }
            _ => {}
        }
        DbError::Sqlx(err)
    }
}
