use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct UserPreferences {
    pub user_id: String,
    pub display_name: Option<String>,
    pub income_title: Option<String>,
    pub expense_title: Option<String>,
    pub financial_goals: Option<String>,
    pub onboarding_completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct UpdateUserPreferences {
    pub display_name: Option<String>,
    pub income_title: Option<String>,
    pub expense_title: Option<String>,
    pub financial_goals: Option<Vec<String>>,
    pub onboarding_completed: Option<bool>,
}

#[async_trait]
pub trait UserPreferencesRepository: Send + Sync {
    async fn get_by_user_id(&self, user_id: &str) -> Result<Option<UserPreferences>, DbError>;
    async fn upsert(
        &self,
        user_id: &str,
        update: &UpdateUserPreferences,
    ) -> Result<UserPreferences, DbError>;
}

pub struct SqlxUserPreferencesRepository {
    pool: SqlitePool,
}

impl SqlxUserPreferencesRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserPreferencesRepository for SqlxUserPreferencesRepository {
    async fn get_by_user_id(&self, user_id: &str) -> Result<Option<UserPreferences>, DbError> {
        let prefs = sqlx::query_as::<_, UserPreferences>(
            r#"
            SELECT user_id, display_name, income_title, expense_title, financial_goals, onboarding_completed, created_at, updated_at
            FROM user_preferences
            WHERE user_id = ?1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(prefs)
    }

    async fn upsert(
        &self,
        user_id: &str,
        update: &UpdateUserPreferences,
    ) -> Result<UserPreferences, DbError> {
        let now = Utc::now().to_rfc3339();
        let existing = self.get_by_user_id(user_id).await?;

        let display_name = update
            .display_name
            .clone()
            .or_else(|| existing.as_ref().and_then(|e| e.display_name.clone()));
        let income_title = update
            .income_title
            .clone()
            .or_else(|| existing.as_ref().and_then(|e| e.income_title.clone()));
        let expense_title = update
            .expense_title
            .clone()
            .or_else(|| existing.as_ref().and_then(|e| e.expense_title.clone()));
        let financial_goals = update
            .financial_goals
            .as_ref()
            .map(|g| serde_json::to_string(g).unwrap_or_default())
            .or_else(|| existing.as_ref().and_then(|e| e.financial_goals.clone()));
        let onboarding_completed = update.onboarding_completed.unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|e| e.onboarding_completed)
                .unwrap_or(false)
        });

        let created_at = existing
            .as_ref()
            .map(|e| e.created_at.clone())
            .unwrap_or_else(|| now.clone());

        sqlx::query(
            r#"
            INSERT INTO user_preferences (user_id, display_name, income_title, expense_title, financial_goals, onboarding_completed, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(user_id) DO UPDATE SET
                display_name = excluded.display_name,
                income_title = excluded.income_title,
                expense_title = excluded.expense_title,
                financial_goals = excluded.financial_goals,
                onboarding_completed = excluded.onboarding_completed,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(user_id)
        .bind(&display_name)
        .bind(&income_title)
        .bind(&expense_title)
        .bind(&financial_goals)
        .bind(if onboarding_completed { 1 } else { 0 })
        .bind(&created_at)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        // If display_name is given, synchronize users.display_name
        if let Some(ref dname) = display_name {
            let _ =
                sqlx::query("UPDATE users SET display_name = ?1, updated_at = ?2 WHERE id = ?3")
                    .bind(dname)
                    .bind(&now)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await;
        }

        Ok(UserPreferences {
            user_id: user_id.to_string(),
            display_name,
            income_title,
            expense_title,
            financial_goals,
            onboarding_completed,
            created_at,
            updated_at: now,
        })
    }
}
