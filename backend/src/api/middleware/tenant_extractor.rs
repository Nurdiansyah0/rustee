use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::AppState;
use crate::domain::tenant::{Role, TenantContext};
use crate::error::AppError;

pub const TENANT_HEADER: &str = "x-tenant-id";

pub fn parse_user_uuid(user_id_str: &str) -> Uuid {
    Uuid::parse_str(user_id_str).unwrap_or_else(|_| {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(user_id_str.as_bytes());
        let hash = hasher.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hash[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    })
}

impl FromRequestParts<AppState> for TenantContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Authenticate user
        let auth_user = AuthenticatedUser::from_request_parts(parts, state).await?;
        let actor_uuid = parse_user_uuid(&auth_user.user_id);
        let pool = &state.pool;

        // 2. Resolve target tenant_id from X-Tenant-ID header, path, or fallback to personal workspace
        let (target_tenant_uuid, is_path_resolved): (Uuid, bool) = if let Some(header_val) =
            parts.headers.get(TENANT_HEADER)
        {
            let s = header_val.to_str().map_err(|_| {
                AppError::BadRequest(
                    "Invalid X-Tenant-ID header encoding".to_string(),
                    "INVALID_TENANT_HEADER",
                )
            })?;
            let uuid = Uuid::parse_str(s.trim()).map_err(|_| {
                AppError::BadRequest(
                    "Invalid UUID in X-Tenant-ID header".to_string(),
                    "INVALID_TENANT_ID",
                )
            })?;
            (uuid, false)
        } else {
            // Check URI path: /api/v1/tenants/{id}/... or nested /{id}/...
            let path = parts.uri.path();
            let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let non_tenant_roots = [
                "accounting",
                "accounts",
                "categories",
                "transactions",
                "users",
                "sync",
                "ws",
                "auth",
                "health",
                "payment",
                "api",
                "analytics",
                "webhooks",
                "invoices",
                "receivables",
                "payments",
            ];

            let path_tenant_id_or_slug =
                if let Some(idx) = segments.iter().position(|&s| s == "tenants") {
                    if segments.len() > idx + 1 {
                        Some(segments[idx + 1])
                    } else {
                        None
                    }
                } else if !segments.is_empty() && !non_tenant_roots.contains(&segments[0]) {
                    Some(segments[0])
                } else {
                    None
                };

            if let Some(id_or_slug) = path_tenant_id_or_slug {
                let resolved_uuid = if let Ok(uuid) = Uuid::parse_str(id_or_slug) {
                    Some(uuid)
                } else {
                    // Try looking up tenant by slug (case-insensitive)
                    let tenant_id_str: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM tenants WHERE LOWER(slug) = LOWER(?1) AND status != 'ARCHIVED'"
                    )
                    .bind(id_or_slug)
                    .fetch_optional(pool)
                    .await
                    .map_err(AppError::from)?;

                    tenant_id_str.and_then(|tid| Uuid::parse_str(&tid).ok())
                };

                match resolved_uuid {
                    Some(uuid) => (uuid, true),
                    None => {
                        // Anti-enumeration: invalid UUID or non-existent slug returns 404 NOT_FOUND
                        return Err(AppError::NotFound(
                            "Workspace not found".to_string(),
                            "NOT_FOUND",
                        ));
                    }
                }
            } else {
                // Fallback for legacy personal endpoints & backward compatibility
                let default_tenant_id_str: Option<String> = sqlx::query_scalar(
                    r#"
                    SELECT m.tenant_id 
                    FROM memberships m
                    JOIN tenants t ON t.id = m.tenant_id
                    WHERE m.user_id = ?1 AND m.status = 'ACTIVE' AND t.is_personal = 1
                    ORDER BY t.created_at ASC
                    LIMIT 1
                    "#,
                )
                .bind(&auth_user.user_id)
                .fetch_optional(pool)
                .await
                .map_err(AppError::from)?;

                // If no personal workspace found, fall back to any active workspace
                let tenant_str = match default_tenant_id_str {
                    Some(id) => id,
                    None => {
                        let any_tenant_id_str: Option<String> = sqlx::query_scalar(
                            r#"
                            SELECT m.tenant_id 
                            FROM memberships m
                            JOIN tenants t ON t.id = m.tenant_id
                            WHERE m.user_id = ?1 AND m.status = 'ACTIVE' AND t.status != 'ARCHIVED'
                            ORDER BY t.created_at ASC
                            LIMIT 1
                            "#,
                        )
                        .bind(&auth_user.user_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(AppError::from)?;

                        any_tenant_id_str.ok_or_else(|| {
                            AppError::NotFound(
                                "No active workspace found for user".to_string(),
                                "WORKSPACE_NOT_FOUND",
                            )
                        })?
                    }
                };

                let uuid = Uuid::parse_str(&tenant_str).map_err(|_| {
                    AppError::Internal("Database stored malformed tenant UUID".to_string())
                })?;
                (uuid, false)
            }
        };

        // 3. Verify membership & access boundary
        let membership_row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT role, status
            FROM memberships
            WHERE tenant_id = ?1 AND user_id = ?2
            "#,
        )
        .bind(target_tenant_uuid.to_string())
        .bind(&auth_user.user_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::from)?;

        let (role_str, status_str) = match membership_row {
            Some(row) => row,
            None => {
                // Cross-tenant data isolation: return 404 to prevent entity existence enumeration
                return Err(AppError::NotFound(
                    "Workspace not found".to_string(),
                    "NOT_FOUND",
                ));
            }
        };

        if status_str.to_uppercase() != "ACTIVE" {
            if is_path_resolved {
                return Err(AppError::NotFound(
                    "Workspace not found".to_string(),
                    "NOT_FOUND",
                ));
            } else {
                return Err(AppError::Forbidden(
                    "Membership in this workspace is not active".to_string(),
                    "MEMBERSHIP_INACTIVE",
                ));
            }
        }

        let role = Role::from_str_or_custom(&role_str);

        Ok(TenantContext::new(target_tenant_uuid, actor_uuid, role))
    }
}
