use crate::repository::DbError;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct IdempotencyRecord {
    pub id: String,
    pub user_id: String,
    pub idempotency_key: String,
    pub request_hash: String,
    pub status: String,
    pub response_code: Option<i32>,
    pub response_body: Option<String>,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IdempotencyLockResult {
    Acquired,
    Cached {
        response_code: i32,
        response_body: String,
    },
    InProgress,
    MismatchedPayload,
}

#[async_trait]
pub trait IdempotencyRepository: Send + Sync {
    async fn acquire_lock(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_hash: &str,
        ttl_seconds: i64,
    ) -> Result<IdempotencyLockResult, DbError>;

    async fn get_cached_response(
        &self,
        user_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<IdempotencyRecord>, DbError>;

    async fn save_response(
        &self,
        user_id: &str,
        idempotency_key: &str,
        response_code: i32,
        response_body: &str,
    ) -> Result<(), DbError>;

    async fn save_response_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        user_id: &str,
        idempotency_key: &str,
        response_code: i32,
        response_body: &str,
    ) -> Result<(), DbError>;

    async fn release_lock_on_failure(
        &self,
        user_id: &str,
        idempotency_key: &str,
    ) -> Result<(), DbError>;
}

pub struct SqlxIdempotencyRepository {
    pool: SqlitePool,
}

impl SqlxIdempotencyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdempotencyRepository for SqlxIdempotencyRepository {
    async fn acquire_lock(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_hash: &str,
        ttl_seconds: i64,
    ) -> Result<IdempotencyLockResult, DbError> {
        let now = Utc::now();
        let expires_at = now + Duration::seconds(ttl_seconds);
        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let insert_res = sqlx::query(
            r#"
            INSERT INTO idempotency_keys (id, user_id, idempotency_key, request_hash, status, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, 'in_progress', ?5, ?6)
            "#
        )
        .bind(&id)
        .bind(user_id)
        .bind(idempotency_key)
        .bind(request_hash)
        .bind(&now_str)
        .bind(&expires_str)
        .execute(&self.pool)
        .await;

        match insert_res {
            Ok(_) => Ok(IdempotencyLockResult::Acquired),
            Err(sqlx::Error::Database(ref db_err))
                if db_err.code().unwrap_or_default() == "2067"
                    || db_err.message().contains("UNIQUE constraint failed") =>
            {
                let existing = sqlx::query_as::<_, IdempotencyRecord>(
                    r#"
                    SELECT id, user_id, idempotency_key, request_hash, status, response_code, response_body, created_at, expires_at
                    FROM idempotency_keys
                    WHERE user_id = ?1 AND idempotency_key = ?2
                    "#
                )
                .bind(user_id)
                .bind(idempotency_key)
                .fetch_optional(&self.pool)
                .await
                .map_err(DbError::from_sqlx)?;

                match existing {
                    Some(rec) => {
                        if rec.request_hash != request_hash {
                            return Ok(IdempotencyLockResult::MismatchedPayload);
                        }

                        if rec.status == "completed" {
                            if let (Some(code), Some(body)) = (rec.response_code, rec.response_body)
                            {
                                return Ok(IdempotencyLockResult::Cached {
                                    response_code: code,
                                    response_body: body,
                                });
                            }
                        }

                        if rec.status == "in_progress" {
                            let is_expired = chrono::DateTime::parse_from_rfc3339(&rec.expires_at)
                                .map(|dt| dt < now)
                                .unwrap_or(false);

                            if is_expired {
                                let reset = sqlx::query(
                                    r#"
                                    UPDATE idempotency_keys
                                    SET request_hash = ?1, status = 'in_progress', created_at = ?2, expires_at = ?3, response_code = NULL, response_body = NULL
                                    WHERE user_id = ?4 AND idempotency_key = ?5 AND status = 'in_progress'
                                    "#
                                )
                                .bind(request_hash)
                                .bind(&now_str)
                                .bind(&expires_str)
                                .bind(user_id)
                                .bind(idempotency_key)
                                .execute(&self.pool)
                                .await
                                .map_err(DbError::from_sqlx)?;

                                if reset.rows_affected() > 0 {
                                    return Ok(IdempotencyLockResult::Acquired);
                                }
                            }
                            return Ok(IdempotencyLockResult::InProgress);
                        }

                        // Failed status -> allow retry
                        let retry_res = sqlx::query(
                            r#"
                            UPDATE idempotency_keys
                            SET request_hash = ?1, status = 'in_progress', created_at = ?2, expires_at = ?3, response_code = NULL, response_body = NULL
                            WHERE user_id = ?4 AND idempotency_key = ?5
                            "#
                        )
                        .bind(request_hash)
                        .bind(&now_str)
                        .bind(&expires_str)
                        .bind(user_id)
                        .bind(idempotency_key)
                        .execute(&self.pool)
                        .await
                        .map_err(DbError::from_sqlx)?;

                        if retry_res.rows_affected() > 0 {
                            Ok(IdempotencyLockResult::Acquired)
                        } else {
                            Ok(IdempotencyLockResult::InProgress)
                        }
                    }
                    None => Err(DbError::NotFound),
                }
            }
            Err(e) => Err(DbError::from_sqlx(e)),
        }
    }

    async fn get_cached_response(
        &self,
        user_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<IdempotencyRecord>, DbError> {
        let record = sqlx::query_as::<_, IdempotencyRecord>(
            r#"
            SELECT id, user_id, idempotency_key, request_hash, status, response_code, response_body, created_at, expires_at
            FROM idempotency_keys
            WHERE user_id = ?1 AND idempotency_key = ?2
            "#
        )
        .bind(user_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(record)
    }

    async fn save_response(
        &self,
        user_id: &str,
        idempotency_key: &str,
        response_code: i32,
        response_body: &str,
    ) -> Result<(), DbError> {
        let result = sqlx::query(
            r#"
            UPDATE idempotency_keys
            SET status = 'completed', response_code = ?1, response_body = ?2
            WHERE user_id = ?3 AND idempotency_key = ?4 AND status = 'in_progress'
            "#,
        )
        .bind(response_code)
        .bind(response_body)
        .bind(user_id)
        .bind(idempotency_key)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn save_response_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        user_id: &str,
        idempotency_key: &str,
        response_code: i32,
        response_body: &str,
    ) -> Result<(), DbError> {
        let result = sqlx::query(
            r#"
            UPDATE idempotency_keys
            SET status = 'completed', response_code = ?1, response_body = ?2
            WHERE user_id = ?3 AND idempotency_key = ?4 AND status = 'in_progress'
            "#,
        )
        .bind(response_code)
        .bind(response_body)
        .bind(user_id)
        .bind(idempotency_key)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn release_lock_on_failure(
        &self,
        user_id: &str,
        idempotency_key: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            DELETE FROM idempotency_keys
            WHERE user_id = ?1 AND idempotency_key = ?2 AND status = 'in_progress'
            "#,
        )
        .bind(user_id)
        .bind(idempotency_key)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }
}
