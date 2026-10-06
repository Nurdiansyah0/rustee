use std::sync::Arc;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::accounting::{PostJournalEntryCommand, PostJournalLineCommand};
use crate::domain::money::Rupiah;
use crate::domain::outbox::OutboxEventDraft;
use crate::domain::project::{
    AddProjectMemberRequest, BillMilestoneRequest, BillingType, CreateMilestoneRequest,
    CreateProgressRecordRequest, CreateProjectExpenseRequest, CreateProjectMaterialRequest,
    CreateProjectRequest, CreateTaskRequest, DirectIssueMaterialRequest,
    IssueProjectMaterialRequest, LogProjectLaborRequest, MaterialStatus, Milestone,
    MilestoneBillingResponse, MilestoneStatus, ProgressBillingRequest, ProgressBillingResponse,
    ProgressRecord, Project, ProjectExpense, ProjectLabor, ProjectListFilter, ProjectMaterial,
    ProjectMember, ProjectMemberWithUser, ProjectProfitabilitySummary, ProjectRole, ProjectStatus,
    Task, TaskListFilter, TaskPriority, TaskStatus,
};
use crate::domain::tenant::{Role, TenantContext};
use crate::error::AppError;
use crate::repository::outbox_repo::{OutboxRepository, SqlxOutboxRepository};
use crate::repository::project_repo::{ProjectRepository, SqlxProjectRepository};
use crate::repository::tenant_repo::{MembershipRepository, SqlxMembershipRepository};
use crate::service::accounting_service::AccountingService;
use crate::service::inventory_service::InventoryService;
use crate::service::invoice_service::{
    CreateInvoiceItemRequest, CreateInvoiceRequest, InvoiceService,
};

pub struct ProjectService {
    pool: SqlitePool,
    repo: Arc<dyn ProjectRepository>,
    outbox_repo: Arc<dyn OutboxRepository>,
    membership_repo: Arc<dyn MembershipRepository>,
    inventory_service: Arc<InventoryService>,
    accounting_service: Arc<AccountingService>,
    invoice_service: Arc<InvoiceService>,
}

impl ProjectService {
    pub fn new(pool: SqlitePool) -> Self {
        let accounting_service = Arc::new(AccountingService::new_with_pool(pool.clone()));
        let inventory_service = Arc::new(InventoryService::new(pool.clone()));
        let invoice_service = Arc::new(InvoiceService::new(pool.clone(), accounting_service.clone()));
        Self {
            pool: pool.clone(),
            repo: Arc::new(SqlxProjectRepository::new(pool.clone())),
            outbox_repo: Arc::new(SqlxOutboxRepository::new(pool.clone())),
            membership_repo: Arc::new(SqlxMembershipRepository::new(pool)),
            inventory_service,
            accounting_service,
            invoice_service,
        }
    }

    pub fn new_with_repos(
        pool: SqlitePool,
        repo: Arc<dyn ProjectRepository>,
        outbox_repo: Arc<dyn OutboxRepository>,
        membership_repo: Arc<dyn MembershipRepository>,
        inventory_service: Arc<InventoryService>,
        accounting_service: Arc<AccountingService>,
        invoice_service: Arc<InvoiceService>,
    ) -> Self {
        Self {
            pool,
            repo,
            outbox_repo,
            membership_repo,
            inventory_service,
            accounting_service,
            invoice_service,
        }
    }

    // ========================================================================
    // Project Operations
    // ========================================================================

    pub async fn create_project(
        &self,
        ctx: &TenantContext,
        req: CreateProjectRequest,
    ) -> Result<Project, AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can create projects
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to create project".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Input Validation
        let name = req.name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "Project name cannot be empty".to_string(),
                "INVALID_NAME",
            ));
        }

        if let (Some(start), Some(end)) = (req.start_date, req.end_date) {
            if end < start {
                return Err(AppError::BadRequest(
                    "End date cannot precede start date".to_string(),
                    "INVALID_DATES",
                ));
            }
        }

        let budget_amount = req.budget_amount.unwrap_or(Rupiah::ZERO);
        if budget_amount.as_i64() < 0 {
            return Err(AppError::BadRequest(
                "Budget amount cannot be negative".to_string(),
                "INVALID_AMOUNT",
            ));
        }

        let contract_amount = req.contract_amount.unwrap_or(Rupiah::ZERO);
        if contract_amount.as_i64() < 0 {
            return Err(AppError::BadRequest(
                "Contract amount cannot be negative".to_string(),
                "INVALID_AMOUNT",
            ));
        }

        // 3. Begin immediate transaction for write lock serialization
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 4. Generate gapless project code
        let project_number = self
            .repo
            .generate_project_number_tx(&mut tx, ctx)
            .await
            .map_err(AppError::from)?;

        let now = Utc::now();
        let project = Project {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_number,
            name: name.to_string(),
            description: req
                .description
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            customer_id: req.customer_id,
            customer_name: req
                .customer_name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_default(),
            status: ProjectStatus::Draft,
            billing_type: req.billing_type.unwrap_or(BillingType::Milestone),
            budget_amount,
            contract_amount,
            start_date: req.start_date,
            end_date: req.end_date,
            actual_completion_date: None,
            notes: req.notes,
            created_at: now,
            updated_at: now,
        };

        let created = self
            .repo
            .create_project_tx(&mut tx, ctx, &project)
            .await
            .map_err(AppError::from)?;

        // Emit ProjectCreated outbox event (PRD R5)
        let outbox_draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "ProjectCreated",
            "Project",
            created.id.to_string(),
            serde_json::json!({
                "project_id": created.id,
                "project_number": created.project_number,
                "name": created.name,
                "customer_name": created.customer_name,
                "billing_type": created.billing_type.as_str(),
                "contract_amount": created.contract_amount.as_i64(),
                "budget_amount": created.budget_amount.as_i64(),
                "created_at": created.created_at.to_rfc3339(),
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &outbox_draft).await.map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;

        Ok(created)
    }

    pub async fn get_project(&self, ctx: &TenantContext, id: Uuid) -> Result<Project, AppError> {
        self.repo
            .find_project_by_id(ctx, id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound(format!("Project '{}' not found", id), "NOT_FOUND"))
    }

    pub async fn list_projects(
        &self,
        ctx: &TenantContext,
        filter: ProjectListFilter,
    ) -> Result<Vec<Project>, AppError> {
        self.repo
            .list_projects(ctx, &filter)
            .await
            .map_err(AppError::from)
    }

    pub async fn update_project_status(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        new_status: ProjectStatus,
    ) -> Result<Project, AppError> {
        // 1. RBAC check
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to update project status".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch project (anti-enumeration 404)
        let current = self.get_project(ctx, id).await?;

        // 3. State machine validation
        if current.status == new_status {
            return Ok(current);
        }

        let is_valid_transition = match current.status {
            ProjectStatus::Draft => {
                matches!(new_status, ProjectStatus::Active | ProjectStatus::Cancelled)
            }
            ProjectStatus::Active => matches!(
                new_status,
                ProjectStatus::OnHold | ProjectStatus::Completed | ProjectStatus::Cancelled
            ),
            ProjectStatus::OnHold => {
                matches!(new_status, ProjectStatus::Active | ProjectStatus::Cancelled)
            }
            ProjectStatus::Completed => false, // Terminal state
            ProjectStatus::Cancelled => false, // Terminal state
        };

        if !is_valid_transition {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Invalid project status transition from '{:?}' to '{:?}'",
                    current.status, new_status
                ),
                "INVALID_STATUS_TRANSITION",
            ));
        }

        let actual_completion_date = if new_status == ProjectStatus::Completed {
            Some(Utc::now().date_naive())
        } else {
            current.actual_completion_date
        };

        self.repo
            .update_project_status(ctx, id, new_status, actual_completion_date)
            .await
            .map_err(AppError::from)
    }

    // ========================================================================
    // Member Operations
    // ========================================================================

    pub async fn add_member(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: AddProjectMemberRequest,
    ) -> Result<ProjectMember, AppError> {
        // 1. RBAC check
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to assign project members".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Assert project exists in tenant (anti-enumeration 404)
        let _project = self.get_project(ctx, project_id).await?;

        // 3. Validate rates
        let cost_rate = req.cost_rate.unwrap_or(0);
        let billing_rate = req.billing_rate.unwrap_or(0);
        if cost_rate < 0 || billing_rate < 0 {
            return Err(AppError::BadRequest(
                "Rates cannot be negative".to_string(),
                "INVALID_RATE",
            ));
        }

        // 4. Validate user belongs to tenant memberships
        let membership_opt = self
            .membership_repo
            .get_membership(&ctx.tenant_id_str(), &req.user_id.to_string())
            .await
            .map_err(AppError::from)?;

        if membership_opt.is_none() {
            return Err(AppError::NotFound(
                "Assigned user is not a member of this tenant".to_string(),
                "USER_NOT_FOUND",
            ));
        }

        // 5. Check duplicate member
        if self
            .repo
            .find_member(ctx, project_id, req.user_id)
            .await
            .map_err(AppError::from)?
            .is_some()
        {
            return Err(AppError::Conflict(
                "User is already assigned to this project".to_string(),
                "MEMBER_ALREADY_ASSIGNED",
            ));
        }

        let member = ProjectMember {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            user_id: req.user_id,
            role: ProjectRole::from_str(&req.role),
            cost_rate: Rupiah::new(cost_rate),
            billing_rate: Rupiah::new(billing_rate),
            joined_at: Utc::now(),
        };

        self.repo.add_member(ctx, &member).await.map_err(AppError::from)
    }

    pub async fn list_members(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMember>, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo.list_members(ctx, project_id).await.map_err(AppError::from)
    }

    pub async fn list_members_with_user(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMemberWithUser>, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo
            .list_members_with_user(ctx, project_id)
            .await
            .map_err(AppError::from)
    }

    // ========================================================================
    // Milestone Operations
    // ========================================================================

    pub async fn create_milestone(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: CreateMilestoneRequest,
    ) -> Result<Milestone, AppError> {
        // 1. RBAC check
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to create milestone".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Assert project exists in tenant (anti-enumeration 404)
        let _project = self.get_project(ctx, project_id).await?;

        // 3. Input validation
        let title = req.title.trim();
        if title.is_empty() {
            return Err(AppError::BadRequest(
                "Milestone title cannot be empty".to_string(),
                "INVALID_TITLE",
            ));
        }
        let billable_amount = req.billable_amount.unwrap_or(0);
        if billable_amount < 0 {
            return Err(AppError::BadRequest(
                "Billable amount cannot be negative".to_string(),
                "INVALID_AMOUNT",
            ));
        }

        // 4. Sequence order determination
        let sequence_order = match req.sequence_order {
            Some(seq) if seq > 0 => seq,
            Some(_) => {
                return Err(AppError::BadRequest(
                    "Sequence order must be greater than zero".to_string(),
                    "INVALID_SEQUENCE",
                ))
            }
            None => {
                self.repo
                    .get_next_milestone_sequence(ctx, project_id)
                    .await
                    .map_err(AppError::from)?
            }
        };

        let now = Utc::now();
        let milestone = Milestone {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            sequence_order,
            title: title.to_string(),
            description: req
                .description
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            target_date: req.target_date,
            completed_at: None,
            status: MilestoneStatus::Pending,
            billable_amount: Rupiah::new(billable_amount),
            is_billed: false,
            invoice_id: None,
            created_at: now,
            updated_at: now,
            was_already_completed: None,
        };

        self.repo
            .create_milestone(ctx, &milestone)
            .await
            .map_err(AppError::from)
    }

    pub async fn list_milestones(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<Milestone>, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo
            .list_milestones(ctx, project_id)
            .await
            .map_err(AppError::from)
    }

    pub async fn get_milestone(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        milestone_id: Uuid,
    ) -> Result<Milestone, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo
            .find_milestone_by_id(ctx, project_id, milestone_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(
                    format!("Milestone '{}' not found", milestone_id),
                    "NOT_FOUND",
                )
            })
    }

    pub async fn update_milestone_status(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        milestone_id: Uuid,
        new_status: MilestoneStatus,
    ) -> Result<Milestone, AppError> {
        // 1. RBAC check
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to update milestone status".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch milestone (anti-enumeration 404)
        let current = self.get_milestone(ctx, project_id, milestone_id).await?;

        if current.status == new_status {
            return Ok(current);
        }

        // 3. Milestone State Transitions
        let is_valid = match current.status {
            MilestoneStatus::Pending => matches!(
                new_status,
                MilestoneStatus::InProgress | MilestoneStatus::Cancelled
            ),
            MilestoneStatus::InProgress => matches!(
                new_status,
                MilestoneStatus::Completed | MilestoneStatus::Cancelled
            ),
            MilestoneStatus::Completed => {
                if current.is_billed {
                    return Err(AppError::UnprocessableEntity(
                        "Cannot change status of already billed milestone".to_string(),
                        "MILESTONE_ALREADY_BILLED",
                    ));
                }
                false
            }
            MilestoneStatus::Cancelled => false,
        };

        if !is_valid {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Invalid milestone status transition from '{:?}' to '{:?}'",
                    current.status, new_status
                ),
                "INVALID_STATUS_TRANSITION",
            ));
        }

        let completed_at = if new_status == MilestoneStatus::Completed {
            Some(Utc::now())
        } else {
            current.completed_at
        };

        self.repo
            .update_milestone_status(ctx, project_id, milestone_id, new_status, completed_at)
            .await
            .map_err(AppError::from)
    }

    // ========================================================================
    // Task Operations
    // ========================================================================

    pub async fn create_task(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: CreateTaskRequest,
    ) -> Result<Task, AppError> {
        // 1. RBAC check
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to create tasks".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Assert project exists in tenant (anti-enumeration 404)
        let _project = self.get_project(ctx, project_id).await?;

        // 3. Input validation
        let title = req.title.trim();
        if title.is_empty() {
            return Err(AppError::BadRequest(
                "Task title cannot be empty".to_string(),
                "INVALID_TITLE",
            ));
        }
        let estimated_hours = req.estimated_hours.unwrap_or(0);
        if estimated_hours < 0 {
            return Err(AppError::BadRequest(
                "Estimated hours cannot be negative".to_string(),
                "INVALID_HOURS",
            ));
        }

        // 4. Milestone alignment validation
        if let Some(m_id) = req.milestone_id {
            let milestone = self
                .repo
                .find_milestone_by_id(ctx, project_id, m_id)
                .await
                .map_err(AppError::from)?;
            if milestone.is_none() {
                return Err(AppError::NotFound(
                    "Specified milestone not found in this project".to_string(),
                    "NOT_FOUND",
                ));
            }
        }

        // 5. Assignee alignment validation
        if let Some(assignee_id) = req.assignee_id {
            let member = self
                .repo
                .find_member(ctx, project_id, assignee_id)
                .await
                .map_err(AppError::from)?;
            if member.is_none() {
                let membership = self
                    .membership_repo
                    .get_membership(&ctx.tenant_id_str(), &assignee_id.to_string())
                    .await
                    .map_err(AppError::from)?;
                if membership.is_none() {
                    return Err(AppError::NotFound(
                        "Assignee is not a member of this tenant".to_string(),
                        "USER_NOT_FOUND",
                    ));
                }
            }
        }

        let now = Utc::now();
        let task = Task {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            milestone_id: req.milestone_id,
            assignee_id: req.assignee_id,
            title: title.to_string(),
            description: req
                .description
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            status: TaskStatus::Todo,
            priority: req.priority.unwrap_or(TaskPriority::Medium),
            estimated_hours,
            actual_hours: 0,
            due_date: req.due_date,
            completed_at: None,
            created_at: now,
            updated_at: now,
        };

        self.repo.create_task(ctx, &task).await.map_err(AppError::from)
    }

    pub async fn list_tasks(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        filter: TaskListFilter,
    ) -> Result<Vec<Task>, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo
            .list_tasks(ctx, project_id, &filter)
            .await
            .map_err(AppError::from)
    }

    pub async fn get_task(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        task_id: Uuid,
    ) -> Result<Task, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo
            .find_task_by_id(ctx, project_id, task_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Task '{}' not found", task_id), "NOT_FOUND")
            })
    }

    pub async fn update_task_status(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        task_id: Uuid,
        new_status: TaskStatus,
    ) -> Result<Task, AppError> {
        // 1. RBAC check: Staff IS permitted to update task status (along with Owner, Admin, Manager)
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager, Role::Staff])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to update task status".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch task (anti-enumeration 404)
        let current = self.get_task(ctx, project_id, task_id).await?;

        if current.status == new_status {
            return Ok(current);
        }

        // 3. Task State Machine Transitions
        let is_valid = match current.status {
            TaskStatus::Todo => matches!(
                new_status,
                TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Cancelled
            ),
            TaskStatus::InProgress => matches!(
                new_status,
                TaskStatus::Blocked | TaskStatus::Done | TaskStatus::Cancelled
            ),
            TaskStatus::Blocked => {
                matches!(new_status, TaskStatus::InProgress | TaskStatus::Cancelled)
            }
            TaskStatus::Done => matches!(new_status, TaskStatus::InProgress), // Can reopen
            TaskStatus::Cancelled => false,
        };

        if !is_valid {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Invalid task status transition from '{:?}' to '{:?}'",
                    current.status, new_status
                ),
                "INVALID_STATUS_TRANSITION",
            ));
        }

        let completed_at = if new_status == TaskStatus::Done {
            Some(Utc::now())
        } else {
            None
        };

        self.repo
            .update_task_status(ctx, project_id, task_id, new_status, completed_at)
            .await
            .map_err(AppError::from)
    }

    // ========================================================================
    // Project Costing: Labor Operations (Milestone 2)
    // ========================================================================

    pub async fn log_labor(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: LogProjectLaborRequest,
    ) -> Result<ProjectLabor, AppError> {
        // 1. RBAC: Staff, Manager, Administrator, Owner are permitted to log labor
        ctx.require_role(&[
            Role::Owner,
            Role::Administrator,
            Role::Manager,
            Role::Staff,
        ])
        .map_err(|_| {
            AppError::Forbidden(
                "Insufficient permissions to log project labor".to_string(),
                "FORBIDDEN",
            )
        })?;

        // 2. Fetch project (anti-enumeration 404)
        let project = self.get_project(ctx, project_id).await?;

        // 3. Invariant: Project must be in ACTIVE status
        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to record labor",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 4. Boundary Validation: hours_worked must be > 0
        if req.hours_worked <= 0 {
            return Err(AppError::BadRequest(
                "Hours worked must be greater than zero".to_string(),
                "INVALID_HOURS",
            ));
        }

        // 5. Rate Validation (rates cannot be negative)
        if let Some(rate) = req.hourly_rate {
            if rate.as_i64() < 0 {
                return Err(AppError::BadRequest(
                    "Hourly rate cannot be negative".to_string(),
                    "INVALID_RATE",
                ));
            }
        }
        if let Some(rate) = req.billing_rate {
            if rate.as_i64() < 0 {
                return Err(AppError::BadRequest(
                    "Billing rate cannot be negative".to_string(),
                    "INVALID_RATE",
                ));
            }
        }

        // 6. Task alignment validation (if provided)
        if let Some(task_id) = req.task_id {
            let task = self
                .repo
                .find_task_by_id(ctx, project_id, task_id)
                .await
                .map_err(AppError::from)?;
            if task.is_none() {
                return Err(AppError::NotFound(
                    "Specified task not found in this project".to_string(),
                    "NOT_FOUND",
                ));
            }
        }

        // 7. Rate inheritance & worker name resolution
        let (hourly_rate, billing_rate, worker_name) = if let Some(worker_id) = req.worker_id {
            let member_opt = self
                .repo
                .find_member(ctx, project_id, worker_id)
                .await
                .map_err(AppError::from)?;

            let (def_cost, def_bill) = if let Some(ref member) = member_opt {
                (member.cost_rate, member.billing_rate)
            } else {
                (Rupiah::ZERO, Rupiah::ZERO)
            };

            let hr = req.hourly_rate.unwrap_or(def_cost);
            let br = req.billing_rate.unwrap_or(def_bill);

            let wn = if let Some(name) = req.worker_name {
                name.trim().to_string()
            } else {
                "Contractor/Worker".to_string()
            };

            (hr, br, wn)
        } else {
            let wn = req
                .worker_name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Worker name cannot be empty when worker_id is omitted".to_string(),
                        "INVALID_WORKER",
                    )
                })?;

            (
                req.hourly_rate.unwrap_or(Rupiah::ZERO),
                req.billing_rate.unwrap_or(Rupiah::ZERO),
                wn,
            )
        };

        // 8. Integer Cost Calculation: hours_worked * hourly_rate
        let total_cost = ProjectLabor::calculate_total_cost(req.hours_worked, hourly_rate);

        let now = Utc::now();
        let labor = ProjectLabor {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            task_id: req.task_id,
            worker_id: req.worker_id,
            worker_name,
            work_date: req.work_date,
            hours_worked: req.hours_worked,
            hourly_rate,
            total_cost,
            billing_rate,
            is_billable: req.is_billable.unwrap_or(true),
            description: req
                .description
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            created_at: now,
            updated_at: now,
        };

        self.repo.log_labor(ctx, &labor).await.map_err(AppError::from)
    }

    pub async fn list_labor(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectLabor>, AppError> {
        // Assert project exists in caller tenant (anti-enumeration 404)
        let _project = self.get_project(ctx, project_id).await?;

        let all_labor = self
            .repo
            .list_labor_by_project(ctx, project_id)
            .await
            .map_err(AppError::from)?;

        // RBAC: Staff only sees their own labor records; Managers/Admins/Accountants see all
        if ctx.role == Role::Staff {
            Ok(all_labor
                .into_iter()
                .filter(|l| l.worker_id == Some(ctx.actor_id))
                .collect())
        } else {
            Ok(all_labor)
        }
    }

    pub async fn delete_labor(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        labor_id: Uuid,
    ) -> Result<(), AppError> {
        // 1. Fetch project (anti-enumeration 404)
        let project = self.get_project(ctx, project_id).await?;

        // 2. Invariant: Project must be in ACTIVE status to delete cost entries
        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to modify labor records",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 3. Find record
        let labor = self
            .repo
            .find_labor_by_id(ctx, labor_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Labor record not found".to_string(), "NOT_FOUND")
            })?;

        if labor.project_id != project_id {
            return Err(AppError::NotFound(
                "Labor record not found in this project".to_string(),
                "NOT_FOUND",
            ));
        }

        // 4. RBAC: Staff can only delete their own labor entry; Manager/Admin can delete any
        if ctx.role == Role::Staff && labor.worker_id != Some(ctx.actor_id) {
            return Err(AppError::Forbidden(
                "Cannot delete labor record belonging to another worker".to_string(),
                "FORBIDDEN",
            ));
        }

        self.repo
            .delete_labor(ctx, labor_id)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    // ========================================================================
    // Project Costing: Expense Operations (Milestone 2)
    // ========================================================================

    pub async fn create_expense(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: CreateProjectExpenseRequest,
    ) -> Result<ProjectExpense, AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can record expenses (Staff forbidden)
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to record project expenses".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch project (anti-enumeration 404)
        let project = self.get_project(ctx, project_id).await?;

        // 3. Invariant: Project must be in ACTIVE status
        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to record expenses",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 4. Boundary Validation: amount must be > 0
        if req.amount.as_i64() <= 0 {
            return Err(AppError::BadRequest(
                "Expense amount must be greater than zero".to_string(),
                "INVALID_AMOUNT",
            ));
        }

        let desc = req.description.trim();
        if desc.is_empty() {
            return Err(AppError::BadRequest(
                "Expense description cannot be empty".to_string(),
                "INVALID_DESCRIPTION",
            ));
        }

        // 5. Task alignment validation (if provided)
        if let Some(task_id) = req.task_id {
            let task = self
                .repo
                .find_task_by_id(ctx, project_id, task_id)
                .await
                .map_err(AppError::from)?;
            if task.is_none() {
                return Err(AppError::NotFound(
                    "Specified task not found in this project".to_string(),
                    "NOT_FOUND",
                ));
            }
        }

        let now = Utc::now();
        let expense = ProjectExpense {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            task_id: req.task_id,
            category: req.category,
            description: desc.to_string(),
            amount: req.amount,
            expense_date: req.expense_date,
            vendor_name: req
                .vendor_name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            receipt_ref: req
                .receipt_ref
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            is_billable: req.is_billable.unwrap_or(true),
            journal_entry_id: None,
            created_at: now,
            updated_at: now,
        };

        self.repo
            .create_expense(ctx, &expense)
            .await
            .map_err(AppError::from)
    }

    pub async fn list_expenses(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExpense>, AppError> {
        // RBAC: Staff is forbidden from viewing financial expenses
        ctx.require_role(&[
            Role::Owner,
            Role::Administrator,
            Role::Manager,
            Role::Accountant,
        ])
        .map_err(|_| {
            AppError::Forbidden(
                "Insufficient permissions to view project expenses".to_string(),
                "FORBIDDEN",
            )
        })?;

        // Assert project exists in caller tenant (anti-enumeration 404)
        let _project = self.get_project(ctx, project_id).await?;

        self.repo
            .list_expenses_by_project(ctx, project_id)
            .await
            .map_err(AppError::from)
    }

    pub async fn delete_expense(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        expense_id: Uuid,
    ) -> Result<(), AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can delete expenses
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to delete project expenses".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch project (anti-enumeration 404)
        let project = self.get_project(ctx, project_id).await?;

        // 3. Invariant: Project must be in ACTIVE status
        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to modify expenses",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 4. Find record
        let expense = self
            .repo
            .find_expense_by_id(ctx, expense_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Expense record not found".to_string(), "NOT_FOUND")
            })?;

        if expense.project_id != project_id {
            return Err(AppError::NotFound(
                "Expense record not found in this project".to_string(),
                "NOT_FOUND",
            ));
        }

        self.repo
            .delete_expense(ctx, expense_id)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    // ========================================================================
    // Project Costing: Material Operations (Milestone 2 Planned Requisitions)
    // ========================================================================

    pub async fn create_material(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: CreateProjectMaterialRequest,
    ) -> Result<ProjectMaterial, AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can plan materials
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to plan project materials".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch project (anti-enumeration 404)
        let project = self.get_project(ctx, project_id).await?;

        // 3. Invariant: Project must be in ACTIVE status
        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to plan materials",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 4. Boundary Validation: quantity_planned must be > 0
        if req.quantity_planned <= 0 {
            return Err(AppError::BadRequest(
                "Planned quantity must be greater than zero".to_string(),
                "INVALID_QUANTITY",
            ));
        }

        // 5. Task alignment validation (if provided)
        if let Some(task_id) = req.task_id {
            let task = self
                .repo
                .find_task_by_id(ctx, project_id, task_id)
                .await
                .map_err(AppError::from)?;
            if task.is_none() {
                return Err(AppError::NotFound(
                    "Specified task not found in this project".to_string(),
                    "NOT_FOUND",
                ));
            }
        }

        let now = Utc::now();
        let material = ProjectMaterial {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            task_id: req.task_id,
            product_id: req.product_id,
            warehouse_id: req.warehouse_id,
            quantity_planned: req.quantity_planned,
            quantity_issued: 0,
            unit_cost: Rupiah::ZERO,
            total_cost: Rupiah::ZERO,
            status: MaterialStatus::Planned,
            is_billable: req.is_billable.unwrap_or(true),
            stock_movement_id: None,
            journal_entry_id: None,
            issued_at: None,
            notes: req
                .notes
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            created_at: now,
            updated_at: now,
        };

        self.repo
            .create_material(ctx, &material)
            .await
            .map_err(AppError::from)
    }

    pub async fn list_materials(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMaterial>, AppError> {
        // Assert project exists in caller tenant (anti-enumeration 404)
        let _project = self.get_project(ctx, project_id).await?;

        self.repo
            .list_materials_by_project(ctx, project_id)
            .await
            .map_err(AppError::from)
    }

    pub async fn delete_material(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        material_id: Uuid,
    ) -> Result<(), AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can delete materials
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to delete project materials".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Fetch project (anti-enumeration 404)
        let project = self.get_project(ctx, project_id).await?;

        // 3. Invariant: Project must be in ACTIVE status
        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to modify materials",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 4. Find record & assert PLANNED status
        let material = self
            .repo
            .find_material_by_id(ctx, material_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Material record not found".to_string(), "NOT_FOUND")
            })?;

        if material.project_id != project_id {
            return Err(AppError::NotFound(
                "Material record not found in this project".to_string(),
                "NOT_FOUND",
            ));
        }

        if material.status != MaterialStatus::Planned {
            return Err(AppError::UnprocessableEntity(
                "Cannot delete material that has already been issued or processed".to_string(),
                "CANNOT_DELETE_ISSUED_MATERIAL",
            ));
        }

        self.repo
            .delete_material(ctx, material_id)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    pub async fn issue_material(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        material_id: Uuid,
        req: IssueProjectMaterialRequest,
    ) -> Result<ProjectMaterial, AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can issue materials
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to issue project materials".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Concurrency write lock serialization
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 3. Project existence & ACTIVE lifecycle check
        let project = self
            .repo
            .find_project_by_id_tx(&mut tx, ctx, project_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Project '{}' not found", project_id), "NOT_FOUND")
            })?;

        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to issue materials",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 4. Fetch material & assert PLANNED status
        let mut material = self
            .repo
            .find_material_by_id_tx(&mut tx, ctx, material_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Material record not found".to_string(), "NOT_FOUND")
            })?;

        if material.project_id != project_id {
            return Err(AppError::NotFound(
                "Material record not found in this project".to_string(),
                "NOT_FOUND",
            ));
        }

        if material.status != MaterialStatus::Planned {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Material is in '{:?}' status, must be PLANNED to issue",
                    material.status
                ),
                "MATERIAL_NOT_PLANNED",
            ));
        }

        // 5. Determine quantity to issue
        let quantity_to_issue = match req.quantity {
            Some(q) => {
                if q <= 0 {
                    return Err(AppError::BadRequest(
                        "Quantity to issue must be greater than zero".to_string(),
                        "INVALID_QUANTITY",
                    ));
                }
                q
            }
            None => {
                if material.quantity_planned <= 0 {
                    return Err(AppError::BadRequest(
                        "Planned quantity must be greater than zero".to_string(),
                        "INVALID_QUANTITY",
                    ));
                }
                material.quantity_planned
            }
        };

        let notes = req.notes.clone().or_else(|| material.notes.clone());

        // 6. Deduct stock, prevent negative balance, capture WAC, record stock movement
        let (mov_id, unit_cost, product) = self
            .inventory_service
            .issue_stock_for_material_tx(
                &mut tx,
                ctx,
                material.warehouse_id,
                material.product_id,
                quantity_to_issue,
                material.id,
                notes,
            )
            .await?;

        // 7. Calculate total cost strictly using pure integer Rupiah arithmetic
        let total_cost_128 = (quantity_to_issue as i128) * (unit_cost.as_i64() as i128);
        let total_cost = Rupiah::new(total_cost_128 as i64);
        let now = Utc::now();

        // 8. Auto-post balanced GL journal entry: Debit 5000 / Credit 1300
        let journal_id = if total_cost.as_i64() > 0 {
            let post_ctx = if ctx.role.can_post_ledger() {
                ctx.clone()
            } else {
                TenantContext {
                    tenant_id: ctx.tenant_id,
                    actor_id: ctx.actor_id,
                    role: Role::Owner,
                }
            };

            let journal_cmd = PostJournalEntryCommand {
                tenant_id: ctx.tenant_id,
                entry_date: now,
                description: format!("Biaya Material Proyek {} - {}", project.project_number, product.name),
                source_type: "INVENTORY_OUTBOUND".to_string(),
                source_id: Some(material.id),
                lines: vec![
                    PostJournalLineCommand {
                        account_code: "5000".to_string(),
                        debit: total_cost,
                        credit: Rupiah::ZERO,
                        memo: Some(format!("Beban Pokok Proyek {}", project.project_number)),
                    },
                    PostJournalLineCommand {
                        account_code: "1300".to_string(),
                        debit: Rupiah::ZERO,
                        credit: total_cost,
                        memo: Some("Persediaan Barang Dagang".to_string()),
                    },
                ],
            };

            let journal = self
                .accounting_service
                .post_journal_command_tx(&mut tx, &post_ctx, journal_cmd)
                .await?;
            Uuid::parse_str(&journal.id).ok()
        } else {
            None
        };

        // 9. Update material record
        material.quantity_issued = quantity_to_issue;
        material.unit_cost = unit_cost;
        material.total_cost = total_cost;
        material.status = MaterialStatus::Issued;
        material.stock_movement_id = Some(mov_id);
        material.journal_entry_id = journal_id;
        material.issued_at = Some(now);
        material.updated_at = now;

        self.repo.update_material_tx(&mut tx, ctx, &material).await.map_err(AppError::from)?;

        // 10. Insert Transactional Outbox Event
        let outbox_draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "ProjectMaterialIssued",
            "ProjectMaterial",
            material.id.to_string(),
            serde_json::json!({
                "material_id": material.id,
                "project_id": material.project_id,
                "product_id": material.product_id,
                "warehouse_id": material.warehouse_id,
                "quantity_issued": quantity_to_issue,
                "unit_cost": unit_cost.as_i64(),
                "total_cost": total_cost.as_i64(),
                "stock_movement_id": mov_id,
                "journal_entry_id": journal_id,
                "issued_at": now.to_rfc3339(),
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &outbox_draft).await.map_err(AppError::from)?;

        // 11. Commit transaction
        tx.commit().await.map_err(AppError::from)?;

        Ok(material)
    }

    pub async fn direct_issue_material(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: DirectIssueMaterialRequest,
    ) -> Result<ProjectMaterial, AppError> {
        // 1. RBAC: Only Owner, Administrator, or Manager can direct issue materials
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to issue project materials".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // 2. Validate quantity
        if req.quantity <= 0 {
            return Err(AppError::BadRequest(
                "Quantity to issue must be greater than zero".to_string(),
                "INVALID_QUANTITY",
            ));
        }

        // 3. Concurrency write lock serialization
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 4. Project existence & ACTIVE lifecycle check
        let project = self
            .repo
            .find_project_by_id_tx(&mut tx, ctx, project_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Project '{}' not found", project_id), "NOT_FOUND")
            })?;

        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Project is in '{:?}' status, must be ACTIVE to issue materials",
                    project.status
                ),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 5. If task_id provided, validate task exists in project
        if let Some(task_id) = req.task_id {
            let task = self
                .repo
                .find_task_by_id_tx(&mut tx, ctx, task_id)
                .await
                .map_err(AppError::from)?;
            match task {
                Some(t) if t.project_id == project_id => {}
                _ => {
                    return Err(AppError::NotFound(
                        "Specified task not found in this project".to_string(),
                        "NOT_FOUND",
                    ));
                }
            }
        }

        let material_id = Uuid::new_v4();

        // 6. Deduct stock, prevent negative balance, capture WAC, record stock movement
        let (mov_id, unit_cost, product) = self
            .inventory_service
            .issue_stock_for_material_tx(
                &mut tx,
                ctx,
                req.warehouse_id,
                req.product_id,
                req.quantity,
                material_id,
                req.notes.clone(),
            )
            .await?;

        // 7. Calculate total cost strictly using pure integer Rupiah arithmetic
        let total_cost_128 = (req.quantity as i128) * (unit_cost.as_i64() as i128);
        let total_cost = Rupiah::new(total_cost_128 as i64);
        let now = Utc::now();

        // 8. Auto-post balanced GL journal entry: Debit 5000 / Credit 1300
        let journal_entry_id = if total_cost.as_i64() > 0 {
            let post_ctx = if ctx.role.can_post_ledger() {
                ctx.clone()
            } else {
                TenantContext {
                    tenant_id: ctx.tenant_id,
                    actor_id: ctx.actor_id,
                    role: Role::Owner,
                }
            };

            let journal_cmd = PostJournalEntryCommand {
                tenant_id: ctx.tenant_id,
                entry_date: now,
                description: format!("Biaya Material Proyek {} - {}", project.project_number, product.name),
                source_type: "INVENTORY_OUTBOUND".to_string(),
                source_id: Some(material_id),
                lines: vec![
                    PostJournalLineCommand {
                        account_code: "5000".to_string(),
                        debit: total_cost,
                        credit: Rupiah::ZERO,
                        memo: Some(format!("Beban Pokok Proyek {}", project.project_number)),
                    },
                    PostJournalLineCommand {
                        account_code: "1300".to_string(),
                        debit: Rupiah::ZERO,
                        credit: total_cost,
                        memo: Some("Persediaan Barang Dagang".to_string()),
                    },
                ],
            };

            let journal = self
                .accounting_service
                .post_journal_command_tx(&mut tx, &post_ctx, journal_cmd)
                .await?;
            Uuid::parse_str(&journal.id).ok()
        } else {
            None
        };

        // 9. Create ProjectMaterial directly in ISSUED status
        let material = ProjectMaterial {
            id: material_id,
            tenant_id: ctx.tenant_id,
            project_id,
            task_id: req.task_id,
            product_id: req.product_id,
            warehouse_id: req.warehouse_id,
            quantity_planned: req.quantity,
            quantity_issued: req.quantity,
            unit_cost,
            total_cost,
            status: MaterialStatus::Issued,
            is_billable: req.is_billable.unwrap_or(true),
            stock_movement_id: Some(mov_id),
            journal_entry_id,
            issued_at: Some(now),
            notes: req.notes,
            created_at: now,
            updated_at: now,
        };

        self.repo.create_material_tx(&mut tx, ctx, &material).await.map_err(AppError::from)?;

        // 10. Insert Transactional Outbox Event
        let outbox_draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "ProjectMaterialIssued",
            "ProjectMaterial",
            material.id.to_string(),
            serde_json::json!({
                "material_id": material.id,
                "project_id": material.project_id,
                "product_id": material.product_id,
                "warehouse_id": material.warehouse_id,
                "quantity_issued": req.quantity,
                "unit_cost": unit_cost.as_i64(),
                "total_cost": total_cost.as_i64(),
                "stock_movement_id": mov_id,
                "journal_entry_id": journal_entry_id,
                "issued_at": now.to_rfc3339(),
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &outbox_draft).await.map_err(AppError::from)?;

        // 11. Commit transaction
        tx.commit().await.map_err(AppError::from)?;

        Ok(material)
    }

    // ========================================================================
    // Project Costing: Profitability Engine (Milestone 2)
    // ========================================================================

    pub async fn get_project_profitability(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<ProjectProfitabilitySummary, AppError> {
        // 1. RBAC: Owner, Administrator, Manager, and Accountant can view profitability
        // Staff is strictly forbidden from viewing commercial profitability metrics
        ctx.require_role(&[
            Role::Owner,
            Role::Administrator,
            Role::Manager,
            Role::Accountant,
        ])
        .map_err(|_| {
            AppError::Forbidden(
                "Insufficient permissions to view project profitability".to_string(),
                "FORBIDDEN",
            )
        })?;

        // 2. Query single-roundtrip cost summary from repository
        // If project does not exist in caller's tenant, repo returns DbError::NotFound -> HTTP 404
        let totals = self
            .repo
            .get_project_cost_summary(ctx, project_id)
            .await
            .map_err(AppError::from)?;

        Ok(totals.to_profitability_summary())
    }

    // ========================================================================
    // Milestone Completion & Progress Billing (Milestone 4)
    // ========================================================================

    pub async fn complete_milestone(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        milestone_id: Uuid,
    ) -> Result<Milestone, AppError> {
        // 1. RBAC check: Owner, Administrator, Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to complete milestone".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 2. Fetch project under lock
        let project = self
            .repo
            .find_project_by_id_tx(&mut tx, ctx, project_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Project '{}' not found", project_id), "NOT_FOUND")
            })?;

        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                "Project must be active to complete milestones".to_string(),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 3. Fetch milestone under lock
        let milestone = self
            .repo
            .find_milestone_by_id_tx(&mut tx, ctx, milestone_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Milestone '{}' not found", milestone_id), "NOT_FOUND")
            })?;

        if milestone.project_id != project_id {
            return Err(AppError::NotFound(
                format!("Milestone '{}' not found in project", milestone_id),
                "NOT_FOUND",
            ));
        }

        if milestone.status == MilestoneStatus::Cancelled {
            return Err(AppError::UnprocessableEntity(
                "Cannot complete a cancelled milestone".to_string(),
                "INVALID_STATUS_TRANSITION",
            ));
        }

        if milestone.status == MilestoneStatus::Completed {
            let mut already_done = milestone;
            already_done.was_already_completed = Some(true);
            return Ok(already_done);
        }

        let now = Utc::now();
        let updated = self
            .repo
            .update_milestone_status_tx(
                &mut tx,
                ctx,
                project_id,
                milestone_id,
                MilestoneStatus::Completed,
                Some(now),
            )
            .await
            .map_err(AppError::from)?;

        // 4. Emit MilestoneCompleted outbox event
        let outbox_draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "MilestoneCompleted",
            "Milestone",
            milestone.id.to_string(),
            serde_json::json!({
                "milestone_id": milestone.id,
                "project_id": milestone.project_id,
                "sequence_order": milestone.sequence_order,
                "title": milestone.title,
                "completed_at": now.to_rfc3339(),
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &outbox_draft).await.map_err(AppError::from)?;

        // 5. Commit transaction
        tx.commit().await.map_err(AppError::from)?;

        Ok(updated)
    }

    pub async fn bill_milestone(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        milestone_id: Uuid,
        req: BillMilestoneRequest,
    ) -> Result<MilestoneBillingResponse, AppError> {
        // 1. RBAC check: Owner, Administrator, Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to bill milestone".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 2. Fetch project under lock
        let project = self
            .repo
            .find_project_by_id_tx(&mut tx, ctx, project_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Project '{}' not found", project_id), "NOT_FOUND")
            })?;

        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                "Project must be active to bill milestone".to_string(),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        // 3. Fetch milestone under lock
        let mut milestone = self
            .repo
            .find_milestone_by_id_tx(&mut tx, ctx, milestone_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Milestone '{}' not found", milestone_id), "NOT_FOUND")
            })?;

        if milestone.project_id != project_id {
            return Err(AppError::NotFound(
                format!("Milestone '{}' not found in project", milestone_id),
                "NOT_FOUND",
            ));
        }

        if milestone.status != MilestoneStatus::Completed {
            return Err(AppError::UnprocessableEntity(
                "Milestone must be completed before billing".to_string(),
                "MILESTONE_NOT_COMPLETED",
            ));
        }

        if milestone.is_billed {
            return Err(AppError::Conflict(
                "Milestone has already been billed".to_string(),
                "MILESTONE_ALREADY_BILLED",
            ));
        }

        if !milestone.billable_amount.is_positive() {
            return Err(AppError::UnprocessableEntity(
                "Milestone billable amount must be greater than zero".to_string(),
                "INVALID_BILLABLE_AMOUNT",
            ));
        }

        // 4. Create and issue invoice within transaction
        let item_description = format!(
            "Tagihan Milestone {}: {}",
            milestone.sequence_order, milestone.title
        );
        let create_inv_req = CreateInvoiceRequest {
            customer_name: Some(project.customer_name.clone()),
            customer_address: None,
            customer_email: None,
            due_date: req.due_date,
            currency: Some("IDR".to_string()),
            tax_type: req.tax_type,
            items: vec![CreateInvoiceItemRequest {
                description: item_description,
                quantity: 1,
                unit_price: milestone.billable_amount.as_i64(),
                discount: 0,
            }],
            authorized_by: None,
        };

        let invoice = self
            .invoice_service
            .create_and_issue_invoice_tx(&mut tx, ctx, create_inv_req)
            .await?;

        let invoice_uuid = Uuid::parse_str(&invoice.id).map_err(|e| {
            AppError::Internal(format!("Failed to parse generated invoice id: {}", e))
        })?;

        // 5. Update milestone billed status
        self.repo
            .mark_milestone_billed_tx(&mut tx, ctx, milestone.id, invoice_uuid)
            .await
            .map_err(AppError::from)?;

        milestone.is_billed = true;
        milestone.invoice_id = Some(invoice_uuid);

        // 6. Transactional Outbox Event: ProgressBilled
        let outbox_draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "ProgressBilled",
            "Milestone",
            milestone.id.to_string(),
            serde_json::json!({
                "billing_type": "MILESTONE",
                "project_id": project.id,
                "milestone_id": milestone.id,
                "progress_record_id": serde_json::Value::Null,
                "invoice_id": invoice.id,
                "invoice_number": invoice.invoice_number,
                "amount": milestone.billable_amount.as_i64(),
                "total_amount": invoice.total_amount,
                "tax_amount": invoice.tax_amount,
                "tax_type": invoice.tax_type,
                "billed_at": Utc::now().to_rfc3339(),
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &outbox_draft).await.map_err(AppError::from)?;

        // 7. Atomic commit
        tx.commit().await.map_err(AppError::from)?;

        Ok(MilestoneBillingResponse {
            milestone,
            invoice: invoice.clone(),
            invoice_id: invoice.id.clone(),
            invoice_number: invoice.invoice_number.unwrap_or_default(),
            total_amount: invoice.total_amount,
        })
    }

    pub async fn bill_progress(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: ProgressBillingRequest,
    ) -> Result<ProgressBillingResponse, AppError> {
        // 1. RBAC check: Owner, Administrator, Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient permissions to bill project progress".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 2. Fetch project under lock
        let project = self
            .repo
            .find_project_by_id_tx(&mut tx, ctx, project_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Project '{}' not found", project_id), "NOT_FOUND")
            })?;

        if project.status != ProjectStatus::Active {
            return Err(AppError::UnprocessableEntity(
                "Project must be active for progress billing".to_string(),
                "PROJECT_NOT_ACTIVE",
            ));
        }

        if !project.contract_amount.is_positive() {
            return Err(AppError::UnprocessableEntity(
                "Project contract amount must be greater than zero".to_string(),
                "INVALID_CONTRACT_AMOUNT",
            ));
        }

        // 3. Determine percentage and handle progress record
        let (mut record, percentage, is_new_record) = match req.progress_record_id {
            Some(rec_id) => {
                let row = sqlx::query(
                    r#"
                    SELECT id, tenant_id, project_id, milestone_id, percentage, record_date,
                           verified_by, notes, evidence_url, is_billed, invoice_id, created_at
                    FROM progress_records
                    WHERE id = ?1 AND tenant_id = ?2;
                    "#,
                )
                .bind(rec_id.to_string())
                .bind(ctx.tenant_id_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Progress record '{}' not found", rec_id), "NOT_FOUND")
                })?;

                use sqlx::Row;
                let rec_proj_id_str: String = row.get("project_id");
                let rec_proj_id = Uuid::parse_str(&rec_proj_id_str).map_err(|_| {
                    AppError::Internal("Corrupted progress record project id".to_string())
                })?;

                if rec_proj_id != project_id {
                    return Err(AppError::NotFound(
                        format!("Progress record '{}' not found in project", rec_id),
                        "NOT_FOUND",
                    ));
                }

                let is_billed: bool = row.get("is_billed");
                if is_billed {
                    return Err(AppError::Conflict(
                        "Progress record has already been billed".to_string(),
                        "PROGRESS_RECORD_ALREADY_BILLED",
                    ));
                }

                let pct: i64 = row.get("percentage");
                let m_id_opt: Option<String> = row.get("milestone_id");
                let v_by_opt: Option<String> = row.get("verified_by");
                let rec_date_str: String = row.get("record_date");
                let rec_date = chrono::NaiveDate::parse_from_str(&rec_date_str, "%Y-%m-%d")
                    .unwrap_or_else(|_| Utc::now().date_naive());
                let created_at_str: String = row.get("created_at");
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let record = ProgressRecord {
                    id: rec_id,
                    tenant_id: ctx.tenant_id,
                    project_id,
                    milestone_id: m_id_opt.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
                    percentage: pct,
                    record_date: rec_date,
                    verified_by: v_by_opt.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
                    notes: row.get("notes"),
                    evidence_url: row.get("evidence_url"),
                    is_billed: false,
                    invoice_id: None,
                    created_at,
                };
                (record, pct, false)
            }
            None => {
                let pct = req.percentage.ok_or_else(|| {
                    AppError::UnprocessableEntity(
                        "Percentage is required when progress_record_id is not provided".to_string(),
                        "INVALID_PERCENTAGE",
                    )
                })?;
                let now_date = Utc::now().date_naive();
                let record = ProgressRecord {
                    id: Uuid::new_v4(),
                    tenant_id: ctx.tenant_id,
                    project_id,
                    milestone_id: None,
                    percentage: pct,
                    record_date: now_date,
                    verified_by: Some(ctx.actor_id),
                    notes: req.notes.clone(),
                    evidence_url: None,
                    is_billed: false,
                    invoice_id: None,
                    created_at: Utc::now(),
                };
                (record, pct, true)
            }
        };

        if percentage <= 0 || percentage > 100 {
            return Err(AppError::UnprocessableEntity(
                "Progress percentage must be between 1 and 100".to_string(),
                "INVALID_PERCENTAGE",
            ));
        }

        // 4. Cumulative progress validation under write lock
        let cumulative_billed: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(percentage), 0) FROM progress_records WHERE project_id = ?1 AND tenant_id = ?2 AND is_billed = 1;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::from)?;

        if cumulative_billed + percentage > 100 {
            return Err(AppError::Conflict(
                format!(
                    "Cumulative billed progress {}% plus requested {}% exceeds 100%",
                    cumulative_billed, percentage
                ),
                "EXCEEDS_CUMULATIVE_PROGRESS",
            ));
        }

        // 5. Calculate pure integer Rupiah billing amount
        let contract_amt = project.contract_amount.as_i64() as i128;
        let billing_amt_128 = (contract_amt * (percentage as i128)) / 100;
        let billing_amt = billing_amt_128 as i64;
        if billing_amt <= 0 {
            return Err(AppError::UnprocessableEntity(
                "Calculated billing amount must be greater than zero".to_string(),
                "INVALID_BILLING_AMOUNT",
            ));
        }

        // 6. Create and issue commercial invoice via invoice_service
        let item_description = format!(
            "Tagihan Kemajuan Proyek {} ({}%)",
            project.project_number, percentage
        );
        let create_inv_req = CreateInvoiceRequest {
            customer_name: Some(project.customer_name.clone()),
            customer_address: None,
            customer_email: None,
            due_date: req.due_date,
            currency: Some("IDR".to_string()),
            tax_type: req.tax_type,
            items: vec![CreateInvoiceItemRequest {
                description: item_description,
                quantity: 1,
                unit_price: billing_amt,
                discount: 0,
            }],
            authorized_by: None,
        };

        let invoice = self
            .invoice_service
            .create_and_issue_invoice_tx(&mut tx, ctx, create_inv_req)
            .await?;

        let invoice_uuid = Uuid::parse_str(&invoice.id).map_err(|e| {
            AppError::Internal(format!("Failed to parse generated invoice id: {}", e))
        })?;

        // 7. Persist or update progress record
        record.is_billed = true;
        record.invoice_id = Some(invoice_uuid);

        if is_new_record {
            self.repo
                .create_progress_record_tx(&mut tx, ctx, &record)
                .await
                .map_err(AppError::from)?;
        } else {
            self.repo
                .mark_progress_billed_tx(&mut tx, ctx, record.id, invoice_uuid)
                .await
                .map_err(AppError::from)?;
        }

        let new_cumulative = cumulative_billed + percentage;

        // 8. Transactional Outbox Event: ProgressBilled
        let outbox_draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "ProgressBilled",
            "Project",
            project.id.to_string(),
            serde_json::json!({
                "billing_type": "PERCENTAGE_OF_COMPLETION",
                "project_id": project.id,
                "milestone_id": serde_json::Value::Null,
                "progress_record_id": record.id,
                "invoice_id": invoice.id,
                "invoice_number": invoice.invoice_number,
                "billed_percentage": percentage,
                "cumulative_percentage": new_cumulative,
                "amount": billing_amt,
                "total_amount": invoice.total_amount,
                "tax_amount": invoice.tax_amount,
                "tax_type": invoice.tax_type,
                "billed_at": Utc::now().to_rfc3339(),
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &outbox_draft).await.map_err(AppError::from)?;

        // 9. Commit transaction
        tx.commit().await.map_err(AppError::from)?;

        Ok(ProgressBillingResponse {
            progress_record: record,
            invoice: invoice.clone(),
            invoice_id: invoice.id.clone(),
            invoice_number: invoice.invoice_number.unwrap_or_default(),
            billed_percentage: percentage,
            cumulative_percentage: new_cumulative,
            total_amount: invoice.total_amount,
        })
    }

    pub async fn create_progress_record(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        req: CreateProgressRecordRequest,
    ) -> Result<ProgressRecord, AppError> {
        ctx.require_role(&[
            Role::Owner,
            Role::Administrator,
            Role::Manager,
            Role::Staff,
        ])
        .map_err(|_| {
            AppError::Forbidden(
                "Insufficient permissions to record progress".to_string(),
                "FORBIDDEN",
            )
        })?;

        let _project = self.get_project(ctx, project_id).await?;

        if req.percentage < 0 || req.percentage > 100 {
            return Err(AppError::UnprocessableEntity(
                "Progress percentage must be between 0 and 100".to_string(),
                "INVALID_PERCENTAGE",
            ));
        }

        let record = ProgressRecord {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            project_id,
            milestone_id: req.milestone_id,
            percentage: req.percentage,
            record_date: req.record_date,
            verified_by: req.verified_by.or(Some(ctx.actor_id)),
            notes: req.notes,
            evidence_url: req.evidence_url,
            is_billed: false,
            invoice_id: None,
            created_at: Utc::now(),
        };

        self.repo
            .create_progress_record(ctx, &record)
            .await
            .map_err(AppError::from)
    }

    pub async fn list_progress_records(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProgressRecord>, AppError> {
        let _project = self.get_project(ctx, project_id).await?;
        self.repo
            .list_progress_records(ctx, project_id)
            .await
            .map_err(AppError::from)
    }
}

