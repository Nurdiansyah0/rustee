use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct Category {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub category_type: String, // 'income' | 'expense'
    pub icon: Option<String>,
    pub color: Option<String>,
    pub is_system: bool,
    pub display_name: Option<String>,
    pub normalized_name: Option<String>,
    pub metadata: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewCategory {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub category_type: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub is_system: bool,
    pub display_name: Option<String>,
    pub normalized_name: Option<String>,
    pub metadata: Option<String>,
}

impl NewCategory {
    pub fn simple(
        id: impl Into<String>,
        user_id: Option<String>,
        name: impl Into<String>,
        category_type: impl Into<String>,
        icon: Option<String>,
        color: Option<String>,
        is_system: bool,
    ) -> Self {
        let n = name.into();
        Self {
            id: id.into(),
            user_id,
            display_name: Some(n.clone()),
            normalized_name: Some(n.to_lowercase()),
            name: n,
            category_type: category_type.into(),
            icon,
            color,
            is_system,
            metadata: None,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct UpdateCategory {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub normalized_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub metadata: Option<String>,
}

#[async_trait]
pub trait CategoryRepository: Send + Sync {
    async fn create(&self, category: &NewCategory) -> Result<Category, DbError>;
    async fn find_by_id(&self, user_id: &str, id: &str) -> Result<Option<Category>, DbError>;
    async fn list_by_user(&self, user_id: &str) -> Result<Vec<Category>, DbError>;
    async fn update(
        &self,
        user_id: &str,
        id: &str,
        update: &UpdateCategory,
    ) -> Result<Category, DbError>;
    async fn soft_delete(&self, user_id: &str, id: &str) -> Result<(), DbError>;
}

pub struct SqlxCategoryRepository {
    pool: SqlitePool,
}

impl SqlxCategoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CategoryRepository for SqlxCategoryRepository {
    async fn create(&self, category: &NewCategory) -> Result<Category, DbError> {
        let now = Utc::now().to_rfc3339();
        let display_name = category
            .display_name
            .clone()
            .unwrap_or_else(|| category.name.clone());
        let normalized_name = category
            .normalized_name
            .clone()
            .unwrap_or_else(|| category.name.to_lowercase());
        let metadata = category.metadata.clone();

        sqlx::query(
            r#"
            INSERT INTO categories (id, user_id, name, category_type, icon, color, is_system, display_name, normalized_name, metadata, deleted_at, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, ?12)
            "#
        )
        .bind(&category.id)
        .bind(&category.user_id)
        .bind(&category.name)
        .bind(&category.category_type)
        .bind(&category.icon)
        .bind(&category.color)
        .bind(if category.is_system { 1 } else { 0 })
        .bind(&display_name)
        .bind(&normalized_name)
        .bind(&metadata)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Category {
            id: category.id.clone(),
            user_id: category.user_id.clone(),
            name: category.name.clone(),
            category_type: category.category_type.clone(),
            icon: category.icon.clone(),
            color: category.color.clone(),
            is_system: category.is_system,
            display_name: Some(display_name),
            normalized_name: Some(normalized_name),
            metadata,
            deleted_at: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_id(&self, user_id: &str, id: &str) -> Result<Option<Category>, DbError> {
        let category = sqlx::query_as::<_, Category>(
            r#"
            SELECT id, user_id, name, category_type, icon, color, is_system, display_name, normalized_name, metadata, deleted_at, created_at, updated_at
            FROM categories
            WHERE id = ?1 AND (user_id = ?2 OR is_system = 1) AND deleted_at IS NULL
            "#
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(category)
    }

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<Category>, DbError> {
        let categories = sqlx::query_as::<_, Category>(
            r#"
            SELECT id, user_id, name, category_type, icon, color, is_system, display_name, normalized_name, metadata, deleted_at, created_at, updated_at
            FROM categories
            WHERE (user_id = ?1 OR is_system = 1) AND deleted_at IS NULL
            ORDER BY is_system DESC, name ASC
            "#
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(categories)
    }

    async fn update(
        &self,
        user_id: &str,
        id: &str,
        update: &UpdateCategory,
    ) -> Result<Category, DbError> {
        let existing = self
            .find_by_id(user_id, id)
            .await?
            .ok_or(DbError::NotFound)?;
        if existing.is_system {
            return Err(DbError::CannotDeleteSystemEntity);
        }
        let now = Utc::now().to_rfc3339();
        let name = update.name.clone().unwrap_or(existing.name);
        let display_name = update
            .display_name
            .clone()
            .or_else(|| existing.display_name.clone());
        let normalized_name = update
            .normalized_name
            .clone()
            .or_else(|| Some(name.to_lowercase()));
        let icon = update.icon.clone().or_else(|| existing.icon.clone());
        let color = update.color.clone().or_else(|| existing.color.clone());
        let metadata = update
            .metadata
            .clone()
            .or_else(|| existing.metadata.clone());

        sqlx::query(
            r#"
            UPDATE categories
            SET name = ?1, display_name = ?2, normalized_name = ?3, icon = ?4, color = ?5, metadata = ?6, updated_at = ?7
            WHERE id = ?8 AND user_id = ?9 AND is_system = 0 AND deleted_at IS NULL
            "#,
        )
        .bind(&name)
        .bind(&display_name)
        .bind(&normalized_name)
        .bind(&icon)
        .bind(&color)
        .bind(&metadata)
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Category {
            id: existing.id,
            user_id: existing.user_id,
            name,
            category_type: existing.category_type,
            icon,
            color,
            is_system: false,
            display_name,
            normalized_name,
            metadata,
            deleted_at: None,
            created_at: existing.created_at,
            updated_at: now,
        })
    }

    async fn soft_delete(&self, user_id: &str, id: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE categories
            SET deleted_at = ?1, updated_at = ?2
            WHERE id = ?3 AND user_id = ?4 AND is_system = 0 AND deleted_at IS NULL
            "#,
        )
        .bind(&now)
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            // Check if it's a system category or non-existent
            let is_sys =
                sqlx::query_scalar::<_, i64>("SELECT is_system FROM categories WHERE id = ?1")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(DbError::from_sqlx)?;

            match is_sys {
                Some(1) => Err(DbError::CannotDeleteSystemEntity),
                _ => Err(DbError::NotFound),
            }
        } else {
            Ok(())
        }
    }
}
