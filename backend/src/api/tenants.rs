use axum::{
    extract::{Path, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::middleware::tenant_extractor::parse_user_uuid;
use crate::api::AppState;
use crate::domain::tenant::{Role, TenantContext};
use crate::error::AppError;
use crate::service::tenant_service::{
    CreateTenantRequest as ServiceCreateTenantRequest,
    InviteMemberRequest as ServiceInviteMemberRequest,
    UpdateBusinessProfileRequest as ServiceUpdateProfileRequest,
};

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: Option<String>,
    pub business_type: Option<String>,
    pub timezone: Option<String>,
    pub currency: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwitchTenantResponse {
    pub active_tenant_id: String,
    pub name: String,
    pub slug: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TenantCapabilitiesResponse {
    pub tenant_id: String,
    pub business_type: String,
    pub role: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TenantSummaryResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub role: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TenantDetailResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub role: String,
    pub member_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BusinessProfileResponse {
    pub tenant_id: String,
    pub business_name: String,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub timezone: String,
    pub currency: String,
    pub locale: String,
    pub invoice_prefix: String,
    pub business_type: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBusinessProfileRequest {
    pub business_name: Option<String>,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub timezone: Option<String>,
    pub currency: Option<String>,
    pub locale: Option<String>,
    pub invoice_prefix: Option<String>,
    pub business_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberResponse {
    pub membership_id: String,
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub joined_at: String,
}

#[derive(Debug, Deserialize)]
pub struct InviteMemberRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

// POST /api/v1/tenants
pub async fn create_tenant(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateTenantRequest>,
) -> Result<impl IntoResponse, AppError> {
    let service_req = ServiceCreateTenantRequest {
        name: payload.name,
        slug: payload.slug,
        business_type: payload.business_type,
        timezone: payload.timezone,
        currency: payload.currency,
        legal_name: None,
        tax_id: None,
    };

    let detail = state
        .tenant_service
        .create_tenant_workspace(&user.user_id, service_req)
        .await?;

    let res = TenantSummaryResponse {
        id: detail.tenant.id,
        name: detail.tenant.name,
        slug: detail.tenant.slug,
        status: detail.tenant.status,
        role: detail.caller_role.as_str().to_string(),
        is_default: detail.tenant.is_personal,
        created_at: detail.tenant.created_at,
        updated_at: detail.tenant.updated_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::CREATED, headers, Json(res)))
}

// GET /api/v1/tenants
pub async fn list_tenants(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let workspaces = state
        .tenant_service
        .list_user_workspaces(&user.user_id)
        .await?;

    let list: Vec<TenantSummaryResponse> = workspaces
        .into_iter()
        .map(|w| TenantSummaryResponse {
            id: w.id,
            name: w.name,
            slug: w.slug,
            status: w.status,
            role: w.role.as_str().to_string(),
            is_default: w.is_personal,
            created_at: w.created_at,
            updated_at: w.updated_at,
        })
        .collect();

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(list)))
}

// GET /api/v1/tenants/{id}
pub async fn get_tenant(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let tenant = state
        .tenant_repo
        .get_by_id(&ctx.tenant_id_str())
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Workspace not found".into(), "NOT_FOUND"))?;

    let members = state.tenant_service.list_members(&ctx).await?;

    let detail = TenantDetailResponse {
        id: tenant.id,
        name: tenant.name,
        slug: tenant.slug,
        status: tenant.status,
        role: ctx.role.as_str().to_string(),
        member_count: members.len() as i64,
        created_at: tenant.created_at,
        updated_at: tenant.updated_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(detail)))
}

// GET /api/v1/tenants/{id}/profile
pub async fn get_tenant_profile(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let profile = state.tenant_service.get_business_profile(&ctx).await?;

    let res = BusinessProfileResponse {
        tenant_id: profile.tenant_id,
        business_name: profile.business_name,
        legal_name: profile.legal_name,
        tax_id: profile.tax_id,
        email: profile.email,
        phone: profile.phone,
        address: profile.address,
        timezone: profile.timezone,
        currency: profile.currency,
        locale: profile.locale,
        invoice_prefix: profile.invoice_prefix,
        business_type: profile.business_type,
        updated_at: profile.updated_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(res)))
}

// PUT /api/v1/tenants/{id}/profile
pub async fn update_tenant_profile(
    ctx: TenantContext,
    State(state): State<AppState>,
    Json(payload): Json<UpdateBusinessProfileRequest>,
) -> Result<impl IntoResponse, AppError> {
    let service_req = ServiceUpdateProfileRequest {
        business_name: payload.business_name,
        legal_name: payload.legal_name,
        tax_id: payload.tax_id,
        email: payload.email,
        phone: payload.phone,
        address: payload.address,
        timezone: payload.timezone,
        currency: payload.currency,
        invoice_prefix: payload.invoice_prefix,
        business_type: payload.business_type,
    };

    let profile = state
        .tenant_service
        .update_business_profile(&ctx, service_req)
        .await?;

    let res = BusinessProfileResponse {
        tenant_id: profile.tenant_id,
        business_name: profile.business_name,
        legal_name: profile.legal_name,
        tax_id: profile.tax_id,
        email: profile.email,
        phone: profile.phone,
        address: profile.address,
        timezone: profile.timezone,
        currency: profile.currency,
        locale: profile.locale,
        invoice_prefix: profile.invoice_prefix,
        business_type: profile.business_type,
        updated_at: profile.updated_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(res)))
}

// GET /api/v1/tenants/{id}/members
pub async fn list_tenant_members(
    ctx: TenantContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let members = state.tenant_service.list_members(&ctx).await?;

    let res: Vec<MemberResponse> = members
        .into_iter()
        .map(|m| MemberResponse {
            membership_id: m.membership_id,
            user_id: m.user_id,
            email: m.email,
            display_name: m.display_name,
            role: m.role.as_str().to_string(),
            status: m.status,
            joined_at: m.joined_at,
        })
        .collect();

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(res)))
}

// POST /api/v1/tenants/{id}/members
pub async fn invite_tenant_member(
    ctx: TenantContext,
    State(state): State<AppState>,
    Json(payload): Json<InviteMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    let service_req = ServiceInviteMemberRequest {
        email: payload.email,
        role: payload.role,
    };

    let member = state
        .tenant_service
        .invite_member(&ctx, service_req)
        .await?;

    let res = MemberResponse {
        membership_id: member.membership_id,
        user_id: member.user_id,
        email: member.email,
        display_name: member.display_name,
        role: member.role.as_str().to_string(),
        status: member.status,
        joined_at: member.joined_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::CREATED, headers, Json(res)))
}

// PUT /api/v1/tenants/{id}/members/{user_id}
pub async fn update_tenant_member_role(
    ctx: TenantContext,
    State(state): State<AppState>,
    Path((_id, target_user_id)): Path<(String, String)>,
    Json(payload): Json<UpdateMemberRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let role_str = payload.role.trim().to_lowercase();
    if role_str == "owner" {
        return Err(AppError::BadRequest(
            "Cannot assign owner role via member update".into(),
            "INVALID_ROLE",
        ));
    }

    let role = Role::from_str(&role_str).ok_or_else(|| {
        AppError::BadRequest(
            format!(
                "Invalid role '{}'. Allowed roles: administrator, manager, staff, accountant",
                payload.role
            ),
            "INVALID_ROLE",
        )
    })?;

    state
        .tenant_service
        .update_member_role(&ctx, &target_user_id, role)
        .await?;

    let members = state.tenant_service.list_members(&ctx).await?;
    let updated_member = members
        .into_iter()
        .find(|m| m.user_id == target_user_id)
        .ok_or_else(|| {
            AppError::NotFound("Member not found in this workspace".into(), "NOT_FOUND")
        })?;

    let res = MemberResponse {
        membership_id: updated_member.membership_id,
        user_id: updated_member.user_id,
        email: updated_member.email,
        display_name: updated_member.display_name,
        role: updated_member.role.as_str().to_string(),
        status: updated_member.status,
        joined_at: updated_member.joined_at,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(res)))
}

// DELETE /api/v1/tenants/{id}/members/{user_id}
pub async fn remove_tenant_member(
    ctx: TenantContext,
    State(state): State<AppState>,
    Path((_id, target_user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    state
        .tenant_service
        .remove_member(&ctx, &target_user_id)
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::NO_CONTENT, headers))
}

// POST /api/v1/tenants/{id}/switch
pub async fn switch_tenant(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _tenant_uuid = uuid::Uuid::parse_str(&id).map_err(|_| {
        AppError::NotFound("Workspace not found or access denied".to_string(), "NOT_FOUND")
    })?;

    let workspaces = state
        .tenant_service
        .list_user_workspaces(&user.user_id)
        .await?;

    let found = workspaces
        .into_iter()
        .find(|w| w.id == id && w.status.to_uppercase() == "ACTIVE");

    match found {
        Some(w) => {
            let res = SwitchTenantResponse {
                active_tenant_id: w.id,
                name: w.name,
                slug: w.slug,
                role: w.role.as_str().to_string(),
                status: "switched".to_string(),
            };
            let mut headers = HeaderMap::new();
            headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
            Ok((StatusCode::OK, headers, Json(res)))
        }
        None => Err(AppError::NotFound(
            "Workspace not found or access denied".to_string(),
            "NOT_FOUND",
        )),
    }
}

// GET /api/v1/tenants/{id}/capabilities
pub async fn get_tenant_capabilities(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let tenant_uuid = uuid::Uuid::parse_str(&id).map_err(|_| {
        AppError::NotFound("Workspace not found or access denied".to_string(), "NOT_FOUND")
    })?;

    let workspaces = state
        .tenant_service
        .list_user_workspaces(&user.user_id)
        .await?;

    let found = workspaces
        .into_iter()
        .find(|w| w.id == id && w.status.to_uppercase() == "ACTIVE");

    let member = match found {
        Some(m) => m,
        None => {
            return Err(AppError::NotFound(
                "Workspace not found or access denied".to_string(),
                "NOT_FOUND",
            ))
        }
    };

    let ctx = TenantContext::new(tenant_uuid, parse_user_uuid(&user.user_id), member.role.clone());
    let profile = state.tenant_service.get_business_profile(&ctx).await.ok();

    let b_type = profile
        .map(|p| p.business_type)
        .unwrap_or_else(|| {
            if member.is_personal {
                "personal".to_string()
            } else {
                "general".to_string()
            }
        });

    let caps: Vec<String> = match b_type.as_str() {
        "personal" => vec!["accounts", "transactions", "budgets", "analytics"],
        "retail" => vec!["pos", "inventory", "invoicing", "accounting", "receivables", "reports"],
        "fnb" => vec!["pos", "tables", "kitchen", "inventory", "accounting", "reports"],
        "rental" => vec!["inventory", "bookings", "invoicing", "receivables", "accounting"],
        "contractor" => vec!["projects", "milestones", "invoicing", "receivables", "accounting"],
        _ => vec!["invoicing", "accounting", "receivables", "reports"],
    }
    .into_iter()
    .map(String::from)
    .collect();

    let res = TenantCapabilitiesResponse {
        tenant_id: id,
        business_type: b_type,
        role: member.role.as_str().to_string(),
        capabilities: caps,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(res)))
}

pub fn tenants_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tenants).post(create_tenant))
        .route("/{id}", get(get_tenant))
        .route("/{id}/switch", post(switch_tenant))
        .route("/{id}/capabilities", get(get_tenant_capabilities))
        .route(
            "/{id}/profile",
            get(get_tenant_profile).put(update_tenant_profile),
        )
        .route(
            "/{id}/members",
            get(list_tenant_members).post(invite_tenant_member),
        )
        .route(
            "/{id}/members/{user_id}",
            put(update_tenant_member_role).delete(remove_tenant_member),
        )
}
