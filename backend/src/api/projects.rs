//! Project Management API Endpoints (Milestone 1, Features 1-10)
//!
//! Provides endpoints for projects, project members, milestones, and tasks
//! with strict TenantContext isolation, anti-enumeration security (HTTP 404),
//! and role-based access control (HTTP 403 for Staff on management mutations).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::api::AppState;
use crate::domain::project::{
    AddProjectMemberRequest, BillMilestoneRequest, CompleteMilestoneRequest,
    CreateMilestoneRequest, CreateProgressRecordRequest, CreateProjectExpenseRequest,
    CreateProjectMaterialRequest, CreateProjectRequest, CreateTaskRequest,
    DirectIssueMaterialRequest, IssueProjectMaterialRequest, LogProjectLaborRequest,
    MilestonesListResponse, ProgressBillingRequest, ProgressRecordsListResponse,
    ProjectExpensesListResponse, ProjectLaborListResponse, ProjectMaterialsListResponse,
    ProjectMembersListResponse, ProjectsListResponse, TaskListFilter, TasksListResponse,
    UpdateMilestoneStatusRequest, UpdateProjectStatusRequest, UpdateTaskStatusRequest,
};
use crate::domain::project::ProjectListFilter;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;

pub fn projects_router() -> Router<AppState> {
    Router::new()
        // Milestone 1 routes
        .route("/", get(list_projects_handler).post(create_project_handler))
        .route("/{id}", get(get_project_handler))
        .route("/{id}/status", patch(update_project_status_handler))
        .route("/{id}/members", get(list_members_handler).post(add_member_handler))
        .route("/{id}/milestones", get(list_milestones_handler).post(create_milestone_handler))
        .route("/{id}/milestones/{m_id}/status", patch(update_milestone_status_handler))
        .route("/{id}/tasks", get(list_tasks_handler).post(create_task_handler))
        .route("/{id}/tasks/{t_id}/status", patch(update_task_status_handler))
        // Milestone 2 costing routes
        .route("/{id}/labor", get(list_labor_handler).post(log_labor_handler))
        .route("/{id}/labor/{labor_id}", delete(delete_labor_handler))
        .route("/{id}/expenses", get(list_expenses_handler).post(create_expense_handler))
        .route("/{id}/expenses/{expense_id}", delete(delete_expense_handler))
        .route("/{id}/materials", get(list_materials_handler).post(create_material_handler))
        .route("/{id}/materials/issue", post(direct_issue_material_handler))
        .route("/{id}/materials/{material_id}", delete(delete_material_handler))
        .route("/{id}/materials/{material_id}/issue", post(issue_material_handler))
        .route("/{id}/profitability", get(get_project_profitability_handler))
        // Milestone 4 billing and progress routes
        .route("/{id}/milestones/{m_id}/complete", post(complete_milestone_handler))
        .route("/{id}/milestones/{m_id}/bill", post(bill_milestone_handler))
        .route("/{id}/billing/progress", post(bill_progress_handler))
        .route("/{id}/progress", get(list_progress_records_handler).post(create_progress_record_handler))
}

// ============================================================================
// Project Handlers
// ============================================================================

async fn create_project_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, AppError> {
    let service = state.project_service();
    let project = service.create_project(&ctx, payload).await?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn list_projects_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Query(filter): Query<ProjectListFilter>,
) -> Result<impl IntoResponse, AppError> {
    let service = state.project_service();
    let projects = service.list_projects(&ctx, filter).await?;
    let count = projects.len();
    Ok((StatusCode::OK, Json(ProjectsListResponse { projects, count })))
}

async fn get_project_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let project = service.get_project(&ctx, id).await?;
    Ok((StatusCode::OK, Json(project)))
}

async fn update_project_status_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<UpdateProjectStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let project = service.update_project_status(&ctx, id, payload.status).await?;
    Ok((StatusCode::OK, Json(project)))
}

// ============================================================================
// Member Handlers
// ============================================================================

async fn add_member_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<AddProjectMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let member = service.add_member(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(member)))
}

async fn list_members_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let members = service.list_members(&ctx, project_id).await?;
    let count = members.len();
    Ok((StatusCode::OK, Json(ProjectMembersListResponse { members, count })))
}

// ============================================================================
// Milestone Handlers
// ============================================================================

async fn create_milestone_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<CreateMilestoneRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let milestone = service.create_milestone(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(milestone)))
}

async fn list_milestones_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let milestones = service.list_milestones(&ctx, project_id).await?;
    let count = milestones.len();
    Ok((StatusCode::OK, Json(MilestonesListResponse { milestones, count })))
}

async fn update_milestone_status_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, m_id_str)): Path<(String, String)>,
    Json(payload): Json<UpdateMilestoneStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let milestone_id = Uuid::parse_str(&m_id_str)
        .map_err(|_| AppError::NotFound(format!("Milestone '{}' not found", m_id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let milestone = service
        .update_milestone_status(&ctx, project_id, milestone_id, payload.status)
        .await?;
    Ok((StatusCode::OK, Json(milestone)))
}

// ============================================================================
// Task Handlers
// ============================================================================

async fn create_task_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<CreateTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let task = service.create_task(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn list_tasks_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Query(filter): Query<TaskListFilter>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let tasks = service.list_tasks(&ctx, project_id, filter).await?;
    let count = tasks.len();
    Ok((StatusCode::OK, Json(TasksListResponse { tasks, count })))
}

async fn update_task_status_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, t_id_str)): Path<(String, String)>,
    Json(payload): Json<UpdateTaskStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let task_id = Uuid::parse_str(&t_id_str)
        .map_err(|_| AppError::NotFound(format!("Task '{}' not found", t_id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let task = service
        .update_task_status(&ctx, project_id, task_id, payload.status)
        .await?;
    Ok((StatusCode::OK, Json(task)))
}

// ============================================================================
// Labor Handlers (Milestone 2)
// ============================================================================

async fn log_labor_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<LogProjectLaborRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let labor = service.log_labor(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(labor)))
}

async fn list_labor_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let labor = service.list_labor(&ctx, project_id).await?;
    let count = labor.len();
    Ok((StatusCode::OK, Json(ProjectLaborListResponse { labor, count })))
}

async fn delete_labor_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, labor_id_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let labor_id = Uuid::parse_str(&labor_id_str)
        .map_err(|_| AppError::NotFound(format!("Labor '{}' not found", labor_id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    service.delete_labor(&ctx, project_id, labor_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Expense Handlers (Milestone 2)
// ============================================================================

async fn create_expense_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<CreateProjectExpenseRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let expense = service.create_expense(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(expense)))
}

async fn list_expenses_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let expenses = service.list_expenses(&ctx, project_id).await?;
    let count = expenses.len();
    Ok((StatusCode::OK, Json(ProjectExpensesListResponse { expenses, count })))
}

async fn delete_expense_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, expense_id_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let expense_id = Uuid::parse_str(&expense_id_str)
        .map_err(|_| AppError::NotFound(format!("Expense '{}' not found", expense_id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    service.delete_expense(&ctx, project_id, expense_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Material Handlers (Milestone 2 Planned Requisitions)
// ============================================================================

async fn create_material_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<CreateProjectMaterialRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let material = service.create_material(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(material)))
}

async fn list_materials_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let materials = service.list_materials(&ctx, project_id).await?;
    let count = materials.len();
    Ok((StatusCode::OK, Json(ProjectMaterialsListResponse { materials, count })))
}

async fn delete_material_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, material_id_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let material_id = Uuid::parse_str(&material_id_str)
        .map_err(|_| AppError::NotFound(format!("Material '{}' not found", material_id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    service.delete_material(&ctx, project_id, material_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Profitability Handler (Milestone 2)
// ============================================================================

async fn get_project_profitability_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let summary = service.get_project_profitability(&ctx, project_id).await?;
    Ok((StatusCode::OK, Json(summary)))
}

// ============================================================================
// Material Issuance Handlers (Milestone 3 Direct Job Costing)
// ============================================================================

async fn issue_material_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, material_id_str)): Path<(String, String)>,
    body: Option<Json<IssueProjectMaterialRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let material_id = Uuid::parse_str(&material_id_str)
        .map_err(|_| AppError::NotFound(format!("Material '{}' not found", material_id_str), "NOT_FOUND"))?;
    let payload = body.map(|b| b.0).unwrap_or_default();
    let service = state.project_service();
    let material = service.issue_material(&ctx, project_id, material_id, payload).await?;
    Ok((StatusCode::OK, Json(material)))
}

async fn direct_issue_material_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<DirectIssueMaterialRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let material = service.direct_issue_material(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(material)))
}

// ============================================================================
// Billing & Progress Handlers (Milestone 4)
// ============================================================================

async fn complete_milestone_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, m_id_str)): Path<(String, String)>,
    _body: Option<Json<CompleteMilestoneRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let milestone_id = Uuid::parse_str(&m_id_str)
        .map_err(|_| AppError::NotFound(format!("Milestone '{}' not found", m_id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let milestone = service.complete_milestone(&ctx, project_id, milestone_id).await?;
    Ok((StatusCode::OK, Json(milestone)))
}

async fn bill_milestone_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path((id_str, m_id_str)): Path<(String, String)>,
    body: Option<Json<BillMilestoneRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let milestone_id = Uuid::parse_str(&m_id_str)
        .map_err(|_| AppError::NotFound(format!("Milestone '{}' not found", m_id_str), "NOT_FOUND"))?;
    let payload = body.map(|b| b.0).unwrap_or_default();
    let service = state.project_service();
    let response = service.bill_milestone(&ctx, project_id, milestone_id, payload).await?;
    Ok((StatusCode::OK, Json(response)))
}

async fn bill_progress_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<ProgressBillingRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let response = service.bill_progress(&ctx, project_id, payload).await?;
    Ok((StatusCode::OK, Json(response)))
}

async fn create_progress_record_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    Json(payload): Json<CreateProgressRecordRequest>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let record = service.create_progress_record(&ctx, project_id, payload).await?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn list_progress_records_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Project '{}' not found", id_str), "NOT_FOUND"))?;
    let service = state.project_service();
    let records = service.list_progress_records(&ctx, project_id).await?;
    let count = records.len();
    Ok((StatusCode::OK, Json(ProgressRecordsListResponse { records, count })))
}

