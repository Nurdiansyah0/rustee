use axum::{
    extract::{Path, State},
    http::{
        header::{CACHE_CONTROL, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::auth::make_auth_cookie;
use crate::api::AppState;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::service::invitation_service::{
    AcceptInvitationRequest as ServiceAcceptRequest,
    CreateInvitationRequest as ServiceCreateRequest,
    InvitationService,
};

pub const CACHE_CONTROL_PRIVATE: &str = "private, no-store, must-revalidate";
pub const CACHE_CONTROL_PUBLIC: &str = "public, max-age=0, must-revalidate";

// --- Request / Response DTOs ---

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
    pub display_name: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct InvitationResponse {
    pub id: String,
    pub tenant_id: String,
    pub email: String,
    pub role: String,
    pub token: String,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    /// Convenience: the full invitation link for the frontend to display / copy
    pub invite_url: String,
}

#[derive(Debug, Serialize)]
pub struct InvitationPreviewResponse {
    pub workspace_name: String,
    pub workspace_slug: String,
    pub invited_email: String,
    pub role: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct AcceptInvitationResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub token: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub role: String,
    pub account_type: String,
    pub permissions: Vec<String>,
}

// --- Helpers ---

fn invitation_service(state: &AppState) -> InvitationService {
    InvitationService::new_with_pool(
        state.pool.clone(),
        state.auth_state.auth_service.crypto_service(),
        state.auth_state.auth_service.jwt_engine_arc(),
    )
}

fn build_invite_url(token: &str) -> String {
    // In production this would come from config (APP_URL env var).
    // For now we use a relative path that the frontend SPA will handle.
    format!("/join/{}", token)
}

// --- Handlers ---

// POST /api/v1/tenants/{id}/invitations
pub async fn create_invitation(
    ctx: TenantContext,
    State(state): State<AppState>,
    Json(payload): Json<CreateInvitationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let svc = invitation_service(&state);
    let req = ServiceCreateRequest {
        email: payload.email,
        role: payload.role,
    };

    let inv = svc.create_invitation(&ctx, req).await?;
    let invite_url = build_invite_url(&inv.token);

    let res = InvitationResponse {
        id: inv.id,
        tenant_id: inv.tenant_id,
        email: inv.email,
        role: inv.role,
        token: inv.token,
        status: inv.status,
        expires_at: inv.expires_at,
        created_at: inv.created_at,
        invite_url,
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_PRIVATE),
    );
    Ok((StatusCode::CREATED, headers, Json(res)))
}

// GET /api/v1/tenants/{id}/invitations
pub async fn list_invitations(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let svc = invitation_service(&state);
    let invitations = svc.list_invitations(&ctx).await?;

    let list: Vec<InvitationResponse> = invitations
        .into_iter()
        .map(|inv| {
            let invite_url = build_invite_url(&inv.token);
            InvitationResponse {
                id: inv.id,
                tenant_id: inv.tenant_id,
                email: inv.email,
                role: inv.role,
                token: inv.token,
                status: inv.status,
                expires_at: inv.expires_at,
                created_at: inv.created_at,
                invite_url,
            }
        })
        .collect();

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_PRIVATE),
    );
    Ok((StatusCode::OK, headers, Json(list)))
}

// DELETE /api/v1/tenants/{id}/invitations/{invitation_id}
pub async fn revoke_invitation(
    ctx: TenantContext,
    Path((_id, invitation_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let svc = invitation_service(&state);
    svc.revoke_invitation(&ctx, &invitation_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// GET /api/v1/invitations/{token}  — PUBLIC, no auth required
pub async fn preview_invitation(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let svc = invitation_service(&state);
    let preview = svc.preview_invitation(&token).await?;

    let res = InvitationPreviewResponse {
        workspace_name: preview.workspace_name,
        workspace_slug: preview.workspace_slug,
        invited_email: preview.invited_email,
        role: preview.role,
        expires_at: preview.expires_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_PUBLIC),
    );
    Ok((StatusCode::OK, headers, Json(res)))
}

// POST /api/v1/invitations/{token}/accept  — PUBLIC, no auth required
pub async fn accept_invitation(
    Path(token): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<AcceptInvitationRequest>,
) -> Result<impl IntoResponse, AppError> {
    if payload.token != token {
        return Err(AppError::BadRequest(
            "Token mismatch".to_string(),
            "TOKEN_MISMATCH",
        ));
    }

    let svc = invitation_service(&state);
    let req = ServiceAcceptRequest {
        token,
        display_name: payload.display_name,
        username: payload.username,
        phone: payload.phone,
        password: payload.password,
    };

    let result = svc.accept_invitation(req).await?;
    let cookie_val = make_auth_cookie(&result.token, 900, state.auth_state.secure_cookie);

    let res = AcceptInvitationResponse {
        user_id: result.user_id,
        email: result.email,
        display_name: result.display_name,
        username: result.username,
        phone: result.phone,
        token: result.token,
        workspace_id: result.workspace_id,
        workspace_name: result.workspace_name,
        role: result.role,
        account_type: result.account_type,
        permissions: result.permissions,
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie_val)
            .map_err(|e| AppError::Internal(format!("Invalid cookie header: {}", e)))?,
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_PRIVATE),
    );
    Ok((StatusCode::CREATED, headers, Json(res)))
}

// --- Router ---

pub fn invitations_router() -> Router<AppState> {
    Router::new()
        // Public: no auth required
        .route("/invitations/:token", get(preview_invitation))
        .route("/invitations/:token/accept", post(accept_invitation))
}

pub fn tenant_invitations_router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(list_invitations).post(create_invitation),
        )
        .route("/:invitation_id", delete(revoke_invitation))
}
