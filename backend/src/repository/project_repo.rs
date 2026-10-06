use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::domain::money::Rupiah;
pub use crate::domain::project::{
    BillingType, Milestone, MilestoneStatus, ProgressRecord, Project, ProjectFilter,
    ProjectMember, ProjectMemberWithUser, ProjectRole, ProjectStatus, Task, TaskFilter,
    TaskPriority, TaskStatus, DEFAULT_PROJECT_PREFIX,
};
pub use crate::domain::project_costing::{
    ExpenseCategory, MaterialStatus, ProjectCostTotals, ProjectExpense, ProjectLabor,
    ProjectMaterial, ProjectProfitabilitySummary,
};
use crate::domain::tenant::TenantContext;
use crate::repository::DbError;

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    // --- Project Operations ---
    async fn create_project(&self, ctx: &TenantContext, project: &Project) -> Result<Project, DbError>;
    async fn create_project_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project: &Project,
    ) -> Result<Project, DbError>;

    async fn find_project_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Project>, DbError>;
    async fn find_project_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Project>, DbError>;

    async fn find_project_by_number(
        &self,
        ctx: &TenantContext,
        project_number: &str,
    ) -> Result<Option<Project>, DbError>;

    async fn list_projects(
        &self,
        ctx: &TenantContext,
        filter: &ProjectFilter,
    ) -> Result<Vec<Project>, DbError>;

    async fn update_project(&self, ctx: &TenantContext, project: &Project) -> Result<(), DbError>;
    async fn update_project_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project: &Project,
    ) -> Result<(), DbError>;

    async fn update_project_status(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        status: ProjectStatus,
        actual_completion_date: Option<NaiveDate>,
    ) -> Result<Project, DbError>;
    async fn update_project_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        status: ProjectStatus,
        actual_completion_date: Option<NaiveDate>,
    ) -> Result<Project, DbError>;

    async fn get_next_project_number(
        &self,
        ctx: &TenantContext,
        prefix: &str,
        year: i32,
    ) -> Result<String, DbError>;
    async fn get_next_project_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        prefix: &str,
        year: i32,
    ) -> Result<String, DbError>;
    async fn generate_project_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError>;

    // --- Project Member Operations ---
    async fn add_member(&self, ctx: &TenantContext, member: &ProjectMember) -> Result<ProjectMember, DbError>;
    async fn add_member_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        member: &ProjectMember,
    ) -> Result<ProjectMember, DbError>;

    async fn find_member(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ProjectMember>, DbError>;

    async fn list_members(&self, ctx: &TenantContext, project_id: Uuid) -> Result<Vec<ProjectMember>, DbError>;
    async fn list_members_with_user(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMemberWithUser>, DbError>;

    async fn update_member(&self, ctx: &TenantContext, member: &ProjectMember) -> Result<(), DbError>;
    async fn remove_member(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), DbError>;

    // --- Milestone Operations ---
    async fn create_milestone(&self, ctx: &TenantContext, milestone: &Milestone) -> Result<Milestone, DbError>;
    async fn create_milestone_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        milestone: &Milestone,
    ) -> Result<Milestone, DbError>;

    async fn find_milestone_by_id(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Milestone>, DbError>;
    async fn find_milestone_by_id_only(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Milestone>, DbError>;
    async fn find_milestone_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Milestone>, DbError>;

    async fn list_milestones(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<Milestone>, DbError>;

    async fn get_next_milestone_sequence(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<i64, DbError>;

    async fn update_milestone(&self, ctx: &TenantContext, milestone: &Milestone) -> Result<(), DbError>;
    async fn update_milestone_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        milestone: &Milestone,
    ) -> Result<(), DbError>;

    async fn update_milestone_status(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: MilestoneStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Milestone, DbError>;
    async fn update_milestone_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: MilestoneStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Milestone, DbError>;

    async fn mark_milestone_billed_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        invoice_id: Uuid,
    ) -> Result<(), DbError>;

    // --- Task Operations ---
    async fn create_task(&self, ctx: &TenantContext, task: &Task) -> Result<Task, DbError>;
    async fn create_task_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        task: &Task,
    ) -> Result<Task, DbError>;

    async fn find_task_by_id(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Task>, DbError>;
    async fn find_task_by_id_only(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Task>, DbError>;
    async fn find_task_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Task>, DbError>;

    async fn list_tasks(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        filter: &TaskFilter,
    ) -> Result<Vec<Task>, DbError>;

    async fn update_task(&self, ctx: &TenantContext, task: &Task) -> Result<(), DbError>;
    async fn update_task_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        task: &Task,
    ) -> Result<(), DbError>;

    async fn update_task_status(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: TaskStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Task, DbError>;
    async fn update_task_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: TaskStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Task, DbError>;

    // --- Progress Record Operations ---
    async fn create_progress_record(
        &self,
        ctx: &TenantContext,
        record: &ProgressRecord,
    ) -> Result<ProgressRecord, DbError>;
    async fn create_progress_record_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        record: &ProgressRecord,
    ) -> Result<ProgressRecord, DbError>;

    async fn find_progress_record_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProgressRecord>, DbError>;

    async fn list_progress_records(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProgressRecord>, DbError>;

    async fn get_latest_progress_record(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Option<ProgressRecord>, DbError>;

    async fn mark_progress_billed_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        invoice_id: Uuid,
    ) -> Result<(), DbError>;

    // --- Labor Operations (Milestone 2) ---
    async fn log_labor(&self, ctx: &TenantContext, labor: &ProjectLabor) -> Result<ProjectLabor, DbError>;
    async fn log_labor_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        labor: &ProjectLabor,
    ) -> Result<ProjectLabor, DbError>;

    async fn find_labor_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectLabor>, DbError>;
    async fn find_labor_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectLabor>, DbError>;

    async fn list_labor_by_project(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectLabor>, DbError>;

    async fn delete_labor(&self, ctx: &TenantContext, id: Uuid) -> Result<bool, DbError>;
    async fn delete_labor_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<bool, DbError>;

    // --- Expense Operations (Milestone 2) ---
    async fn create_expense(&self, ctx: &TenantContext, expense: &ProjectExpense) -> Result<ProjectExpense, DbError>;
    async fn create_expense_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        expense: &ProjectExpense,
    ) -> Result<ProjectExpense, DbError>;

    async fn find_expense_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectExpense>, DbError>;
    async fn find_expense_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectExpense>, DbError>;

    async fn list_expenses_by_project(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExpense>, DbError>;

    async fn delete_expense(&self, ctx: &TenantContext, id: Uuid) -> Result<bool, DbError>;
    async fn delete_expense_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<bool, DbError>;

    // --- Material Operations (Milestone 2 Planned Requisitions) ---
    async fn create_material(&self, ctx: &TenantContext, material: &ProjectMaterial) -> Result<ProjectMaterial, DbError>;
    async fn create_material_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        material: &ProjectMaterial,
    ) -> Result<ProjectMaterial, DbError>;

    async fn find_material_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectMaterial>, DbError>;
    async fn find_material_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectMaterial>, DbError>;

    async fn list_materials_by_project(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMaterial>, DbError>;

    async fn update_material(&self, ctx: &TenantContext, material: &ProjectMaterial) -> Result<ProjectMaterial, DbError>;
    async fn update_material_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        material: &ProjectMaterial,
    ) -> Result<ProjectMaterial, DbError>;

    async fn delete_material(&self, ctx: &TenantContext, id: Uuid) -> Result<bool, DbError>;
    async fn delete_material_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<bool, DbError>;

    // --- Cost Aggregation & Profitability (Milestone 2) ---
    async fn get_project_cost_summary(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<ProjectCostTotals, DbError>;
    async fn get_project_cost_summary_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<ProjectCostTotals, DbError>;
}

pub struct SqlxProjectRepository {
    pool: SqlitePool,
}

impl SqlxProjectRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn parse_date_opt(s: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .or_else(|_| DateTime::parse_from_rfc3339(s).map(|dt| dt.date_naive()))
            .ok()
    }

    fn parse_date(s: &str) -> Result<NaiveDate, DbError> {
        Self::parse_date_opt(s)
            .ok_or_else(|| DbError::Serialization(format!("Invalid date format: {}", s)))
    }

    fn parse_datetime(s: &str) -> Result<DateTime<Utc>, DbError> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid datetime format {}: {}", s, e)))
    }

    fn row_to_project(row: &sqlx::sqlite::SqliteRow) -> Result<Project, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let customer_id_str: Option<String> = row.get("customer_id");
        let status_str: String = row.get("status");
        let billing_type_str: String = row.get("billing_type");
        let budget_raw: i64 = row.get("budget_amount");
        let contract_raw: i64 = row.get("contract_amount");
        let start_date_str: Option<String> = row.get("start_date");
        let end_date_str: Option<String> = row.get("end_date");
        let actual_comp_str: Option<String> = row.get("actual_completion_date");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let customer_id = match customer_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid customer id: {}", e)))?,
            ),
            _ => None,
        };

        let status = ProjectStatus::from_str(&status_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid project status: {}", status_str)))?;
        let billing_type = BillingType::from_str(&billing_type_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid billing type: {}", billing_type_str)))?;

        let start_date = start_date_str.as_deref().and_then(Self::parse_date_opt);
        let end_date = end_date_str.as_deref().and_then(Self::parse_date_opt);
        let actual_completion_date = actual_comp_str.as_deref().and_then(Self::parse_date_opt);

        let created_at = Self::parse_datetime(&created_at_str)?;
        let updated_at = Self::parse_datetime(&updated_at_str)?;

        Ok(Project {
            id,
            tenant_id,
            project_number: row.get("project_number"),
            name: row.get("name"),
            description: row.get("description"),
            customer_id,
            customer_name: row.get("customer_name"),
            status,
            billing_type,
            budget_amount: Rupiah::new(budget_raw),
            contract_amount: Rupiah::new(contract_raw),
            start_date,
            end_date,
            actual_completion_date,
            notes: row.get("notes"),
            created_at,
            updated_at,
        })
    }

    fn row_to_member(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectMember, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let user_id_str: String = row.get("user_id");
        let role_str: String = row.get("role");
        let cost_rate_raw: i64 = row.get("cost_rate");
        let billing_rate_raw: i64 = row.get("billing_rate");
        let created_at_str: String = row.get("created_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid member id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;
        let user_id = Uuid::parse_str(&user_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid user id: {}", e)))?;

        let role = ProjectRole::from_str(&role_str);
        let joined_at = Self::parse_datetime(&created_at_str)?;

        Ok(ProjectMember {
            id,
            tenant_id,
            project_id,
            user_id,
            role,
            cost_rate: Rupiah::new(cost_rate_raw),
            billing_rate: Rupiah::new(billing_rate_raw),
            joined_at,
        })
    }

    fn row_to_milestone(row: &sqlx::sqlite::SqliteRow) -> Result<Milestone, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let status_str: String = row.get("status");
        let billable_raw: i64 = row.get("billable_amount");
        let target_date_str: String = row.get("target_date");
        let comp_at_str: Option<String> = row.get("completed_at");
        let is_billed_int: i64 = row.get("is_billed");
        let invoice_id_str: Option<String> = row.get("invoice_id");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid milestone id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;

        let status = MilestoneStatus::from_str(&status_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid milestone status: {}", status_str)))?;

        let target_date = Self::parse_date(&target_date_str)?;
        let completed_at = match comp_at_str {
            Some(s) if !s.is_empty() => Some(Self::parse_datetime(&s)?),
            _ => None,
        };

        let invoice_id = match invoice_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid invoice id: {}", e)))?,
            ),
            _ => None,
        };

        let created_at = Self::parse_datetime(&created_at_str)?;
        let updated_at = Self::parse_datetime(&updated_at_str)?;

        Ok(Milestone {
            id,
            tenant_id,
            project_id,
            sequence_order: row.get("sequence_order"),
            title: row.get("title"),
            description: row.get("description"),
            target_date,
            completed_at,
            status,
            billable_amount: Rupiah::new(billable_raw),
            is_billed: is_billed_int == 1,
            invoice_id,
            was_already_completed: None,
            created_at,
            updated_at,
        })
    }

    fn row_to_task(row: &sqlx::sqlite::SqliteRow) -> Result<Task, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let milestone_id_str: Option<String> = row.get("milestone_id");
        let assignee_id_str: Option<String> = row.get("assignee_id");
        let status_str: String = row.get("status");
        let priority_str: String = row.get("priority");
        let due_date_str: Option<String> = row.get("due_date");
        let comp_at_str: Option<String> = row.get("completed_at");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid task id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;

        let milestone_id = match milestone_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid milestone id: {}", e)))?,
            ),
            _ => None,
        };

        let assignee_id = match assignee_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid assignee id: {}", e)))?,
            ),
            _ => None,
        };

        let status = TaskStatus::from_str(&status_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid task status: {}", status_str)))?;
        let priority = TaskPriority::from_str(&priority_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid task priority: {}", priority_str)))?;

        let due_date = due_date_str.as_deref().and_then(Self::parse_date_opt);
        let completed_at = match comp_at_str {
            Some(s) if !s.is_empty() => Some(Self::parse_datetime(&s)?),
            _ => None,
        };

        let created_at = Self::parse_datetime(&created_at_str)?;
        let updated_at = Self::parse_datetime(&updated_at_str)?;

        Ok(Task {
            id,
            tenant_id,
            project_id,
            milestone_id,
            assignee_id,
            title: row.get("title"),
            description: row.get("description"),
            status,
            priority,
            estimated_hours: row.get("estimated_hours"),
            actual_hours: row.get("actual_hours"),
            due_date,
            completed_at,
            created_at,
            updated_at,
        })
    }

    fn row_to_progress_record(row: &sqlx::sqlite::SqliteRow) -> Result<ProgressRecord, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let milestone_id_str: Option<String> = row.get("milestone_id");
        let verified_by_str: Option<String> = row.get("verified_by");
        let record_date_str: String = row.get("record_date");
        let is_billed_int: i64 = row.get("is_billed");
        let invoice_id_str: Option<String> = row.get("invoice_id");
        let created_at_str: String = row.get("created_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid progress record id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;

        let milestone_id = match milestone_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid milestone id: {}", e)))?,
            ),
            _ => None,
        };

        let verified_by = match verified_by_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid verifier id: {}", e)))?,
            ),
            _ => None,
        };

        let invoice_id = match invoice_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid invoice id: {}", e)))?,
            ),
            _ => None,
        };

        let record_date = Self::parse_date(&record_date_str)?;
        let created_at = Self::parse_datetime(&created_at_str)?;

        Ok(ProgressRecord {
            id,
            tenant_id,
            project_id,
            milestone_id,
            percentage: row.get("percentage"),
            record_date,
            verified_by,
            notes: row.get("notes"),
            evidence_url: row.get("evidence_url"),
            is_billed: is_billed_int == 1,
            invoice_id,
            created_at,
        })
    }

    fn row_to_labor(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectLabor, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let task_id_str: Option<String> = row.get("task_id");
        let worker_id_str: Option<String> = row.get("worker_id");
        let worker_name: String = row.get("worker_name");
        let work_date_str: String = row.get("work_date");
        let hours_worked: i64 = row.get("hours_worked");
        let hourly_rate_raw: i64 = row.get("hourly_rate");
        let total_cost_raw: i64 = row.get("total_cost");
        let billing_rate_raw: i64 = row.get("billing_rate");
        let is_billable_int: i64 = row.get("is_billable");
        let description: Option<String> = row.get("description");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid labor id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;
        let task_id = match task_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid task id: {}", e)))?,
            ),
            _ => None,
        };
        let worker_id = match worker_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid worker id: {}", e)))?,
            ),
            _ => None,
        };

        let work_date = Self::parse_date(&work_date_str)?;
        let created_at = Self::parse_datetime(&created_at_str)?;
        let updated_at = Self::parse_datetime(&updated_at_str)?;

        Ok(ProjectLabor {
            id,
            tenant_id,
            project_id,
            task_id,
            worker_id,
            worker_name,
            work_date,
            hours_worked,
            hourly_rate: Rupiah::new(hourly_rate_raw),
            total_cost: Rupiah::new(total_cost_raw),
            billing_rate: Rupiah::new(billing_rate_raw),
            is_billable: is_billable_int == 1,
            description,
            created_at,
            updated_at,
        })
    }

    fn row_to_expense(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectExpense, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let task_id_str: Option<String> = row.get("task_id");
        let category_str: String = row.get("category");
        let description: String = row.get("description");
        let amount_raw: i64 = row.get("amount");
        let expense_date_str: String = row.get("expense_date");
        let vendor_name: Option<String> = row.get("vendor_name");
        let receipt_ref: Option<String> = row.get("receipt_ref");
        let is_billable_int: i64 = row.get("is_billable");
        let journal_entry_id_str: Option<String> = row.get("journal_entry_id");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid expense id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;
        let task_id = match task_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid task id: {}", e)))?,
            ),
            _ => None,
        };
        let journal_entry_id = match journal_entry_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid journal entry id: {}", e)))?,
            ),
            _ => None,
        };

        let category = ExpenseCategory::from_str(&category_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid expense category: {}", category_str)))?;

        let expense_date = Self::parse_date(&expense_date_str)?;
        let created_at = Self::parse_datetime(&created_at_str)?;
        let updated_at = Self::parse_datetime(&updated_at_str)?;

        Ok(ProjectExpense {
            id,
            tenant_id,
            project_id,
            task_id,
            category,
            description,
            amount: Rupiah::new(amount_raw),
            expense_date,
            vendor_name,
            receipt_ref,
            is_billable: is_billable_int == 1,
            journal_entry_id,
            created_at,
            updated_at,
        })
    }

    fn row_to_material(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectMaterial, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let project_id_str: String = row.get("project_id");
        let task_id_str: Option<String> = row.get("task_id");
        let product_id_str: String = row.get("product_id");
        let warehouse_id_str: String = row.get("warehouse_id");
        let quantity_planned: i64 = row.get("quantity_planned");
        let quantity_issued: i64 = row.get("quantity_issued");
        let unit_cost_raw: i64 = row.get("unit_cost");
        let total_cost_raw: i64 = row.get("total_cost");
        let status_str: String = row.get("status");
        let is_billable_int: i64 = row.get("is_billable");
        let stock_movement_id_str: Option<String> = row.get("stock_movement_id");
        let journal_entry_id_str: Option<String> = row.get("journal_entry_id");
        let issued_at_str: Option<String> = row.get("issued_at");
        let notes: Option<String> = row.get("notes");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid material id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let project_id = Uuid::parse_str(&project_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid project id: {}", e)))?;
        let task_id = match task_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid task id: {}", e)))?,
            ),
            _ => None,
        };
        let product_id = Uuid::parse_str(&product_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;
        let warehouse_id = Uuid::parse_str(&warehouse_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid warehouse id: {}", e)))?;
        let stock_movement_id = match stock_movement_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid stock movement id: {}", e)))?,
            ),
            _ => None,
        };
        let journal_entry_id = match journal_entry_id_str {
            Some(s) if !s.is_empty() => Some(
                Uuid::parse_str(&s)
                    .map_err(|e| DbError::Serialization(format!("Invalid journal entry id: {}", e)))?,
            ),
            _ => None,
        };

        let status = MaterialStatus::from_str(&status_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid material status: {}", status_str)))?;

        let issued_at = match issued_at_str {
            Some(s) if !s.is_empty() => Some(Self::parse_datetime(&s)?),
            _ => None,
        };
        let created_at = Self::parse_datetime(&created_at_str)?;
        let updated_at = Self::parse_datetime(&updated_at_str)?;

        Ok(ProjectMaterial {
            id,
            tenant_id,
            project_id,
            task_id,
            product_id,
            warehouse_id,
            quantity_planned,
            quantity_issued,
            unit_cost: Rupiah::new(unit_cost_raw),
            total_cost: Rupiah::new(total_cost_raw),
            status,
            is_billable: is_billable_int == 1,
            stock_movement_id,
            journal_entry_id,
            issued_at,
            notes,
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl ProjectRepository for SqlxProjectRepository {
    async fn create_project(&self, ctx: &TenantContext, project: &Project) -> Result<Project, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.create_project_tx(&mut tx, ctx, project).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn create_project_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project: &Project,
    ) -> Result<Project, DbError> {
        sqlx::query(
            r#"
            INSERT INTO projects (
                id, tenant_id, project_number, name, description,
                customer_id, customer_name, status, billing_type,
                budget_amount, contract_amount, start_date, end_date,
                actual_completion_date, notes, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17
            );
            "#,
        )
        .bind(project.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(&project.project_number)
        .bind(&project.name)
        .bind(&project.description)
        .bind(project.customer_id.map(|id| id.to_string()))
        .bind(&project.customer_name)
        .bind(project.status.as_str())
        .bind(project.billing_type.as_str())
        .bind(project.budget_amount.as_i64())
        .bind(project.contract_amount.as_i64())
        .bind(project.start_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(project.end_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(project.actual_completion_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(&project.notes)
        .bind(project.created_at.to_rfc3339())
        .bind(project.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(project.clone())
    }

    async fn find_project_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Project>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM projects WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_project(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_project_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Project>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM projects WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_project(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_project_by_number(
        &self,
        ctx: &TenantContext,
        project_number: &str,
    ) -> Result<Option<Project>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM projects WHERE project_number = ?1 AND tenant_id = ?2;",
        )
        .bind(project_number)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_project(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_projects(
        &self,
        ctx: &TenantContext,
        filter: &ProjectFilter,
    ) -> Result<Vec<Project>, DbError> {
        let mut qb = sqlx::QueryBuilder::new(
            "SELECT * FROM projects WHERE tenant_id = ",
        );
        qb.push_bind(ctx.tenant_id_str());

        if let Some(status) = filter.status {
            qb.push(" AND status = ");
            qb.push_bind(status.as_str());
        }

        if let Some(customer_id) = filter.customer_id {
            qb.push(" AND customer_id = ");
            qb.push_bind(customer_id.to_string());
        }

        if let Some(ref search) = filter.search {
            if !search.trim().is_empty() {
                qb.push(" AND (name LIKE ");
                qb.push_bind(format!("%{}%", search.trim()));
                qb.push(" OR project_number LIKE ");
                qb.push_bind(format!("%{}%", search.trim()));
                qb.push(" OR customer_name LIKE ");
                qb.push_bind(format!("%{}%", search.trim()));
                qb.push(")");
            }
        }

        qb.push(" ORDER BY created_at DESC");

        if let Some(limit) = filter.limit {
            qb.push(" LIMIT ");
            qb.push_bind(limit);
        }

        if let Some(offset) = filter.offset {
            qb.push(" OFFSET ");
            qb.push_bind(offset);
        }

        let rows = qb.build().fetch_all(&self.pool).await.map_err(DbError::from)?;
        rows.iter().map(Self::row_to_project).collect()
    }

    async fn update_project(&self, ctx: &TenantContext, project: &Project) -> Result<(), DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        self.update_project_tx(&mut tx, ctx, project).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(())
    }

    async fn update_project_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project: &Project,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE projects SET
                name = ?1,
                description = ?2,
                customer_id = ?3,
                customer_name = ?4,
                billing_type = ?5,
                budget_amount = ?6,
                contract_amount = ?7,
                start_date = ?8,
                end_date = ?9,
                notes = ?10,
                updated_at = ?11
            WHERE id = ?12 AND tenant_id = ?13;
            "#,
        )
        .bind(&project.name)
        .bind(&project.description)
        .bind(project.customer_id.map(|id| id.to_string()))
        .bind(&project.customer_name)
        .bind(project.billing_type.as_str())
        .bind(project.budget_amount.as_i64())
        .bind(project.contract_amount.as_i64())
        .bind(project.start_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(project.end_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(&project.notes)
        .bind(Utc::now().to_rfc3339())
        .bind(project.id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    async fn update_project_status(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        status: ProjectStatus,
        actual_completion_date: Option<NaiveDate>,
    ) -> Result<Project, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.update_project_status_tx(&mut tx, ctx, id, status, actual_completion_date).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn update_project_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        status: ProjectStatus,
        actual_completion_date: Option<NaiveDate>,
    ) -> Result<Project, DbError> {
        let now_str = Utc::now().to_rfc3339();
        let comp_str = actual_completion_date.map(|d| d.format("%Y-%m-%d").to_string());

        sqlx::query(
            r#"
            UPDATE projects SET
                status = ?1,
                actual_completion_date = ?2,
                updated_at = ?3
            WHERE id = ?4 AND tenant_id = ?5;
            "#,
        )
        .bind(status.as_str())
        .bind(comp_str)
        .bind(now_str)
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        let updated = self.find_project_by_id_tx(tx, ctx, id).await?
            .ok_or_else(|| DbError::NotFound)?;

        Ok(updated)
    }

    async fn get_next_project_number(
        &self,
        ctx: &TenantContext,
        prefix: &str,
        year: i32,
    ) -> Result<String, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let number = self.get_next_project_number_tx(&mut tx, ctx, prefix, year).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(number)
    }

    async fn get_next_project_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        prefix: &str,
        year: i32,
    ) -> Result<String, DbError> {
        let prefix_with_year = format!("{}-{}-", prefix, year);
        let like_pattern = format!("{}%", prefix_with_year);

        let max_val: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT MAX(CAST(SUBSTR(project_number, LENGTH(?2) + 1) AS INTEGER))
            FROM projects
            WHERE tenant_id = ?1 AND project_number LIKE ?3;
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(&prefix_with_year)
        .bind(&like_pattern)
        .fetch_one(&mut **tx)
        .await
        .map_err(DbError::from)?;

        let next_seq = max_val.unwrap_or(0) + 1;
        Ok(format!("{}{:06}", prefix_with_year, next_seq))
    }

    async fn generate_project_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError> {
        let year = Utc::now().year();
        self.get_next_project_number_tx(tx, ctx, DEFAULT_PROJECT_PREFIX, year).await
    }

    // --- Member Operations ---
    async fn add_member(&self, ctx: &TenantContext, member: &ProjectMember) -> Result<ProjectMember, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.add_member_tx(&mut tx, ctx, member).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn add_member_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        member: &ProjectMember,
    ) -> Result<ProjectMember, DbError> {
        let now_str = member.joined_at.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO project_members (
                id, tenant_id, project_id, user_id, role,
                cost_rate, billing_rate, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9
            );
            "#,
        )
        .bind(member.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(member.project_id.to_string())
        .bind(member.user_id.to_string())
        .bind(member.role.as_str())
        .bind(member.cost_rate.as_i64())
        .bind(member.billing_rate.as_i64())
        .bind(&now_str)
        .bind(&now_str)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(member.clone())
    }

    async fn find_member(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ProjectMember>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_members WHERE project_id = ?1 AND user_id = ?2 AND tenant_id = ?3;",
        )
        .bind(project_id.to_string())
        .bind(user_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_member(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_members(&self, ctx: &TenantContext, project_id: Uuid) -> Result<Vec<ProjectMember>, DbError> {
        let rows = sqlx::query(
            "SELECT * FROM project_members WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY created_at ASC;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        rows.iter().map(Self::row_to_member).collect()
    }

    async fn list_members_with_user(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMemberWithUser>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT pm.*, u.display_name AS user_name, u.email AS user_email
            FROM project_members pm
            JOIN users u ON pm.user_id = u.id
            WHERE pm.project_id = ?1 AND pm.tenant_id = ?2
            ORDER BY pm.created_at ASC;
            "#,
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let member = Self::row_to_member(&row)?;
            let user_name: String = row.get("user_name");
            let user_email: String = row.get("user_email");
            results.push(ProjectMemberWithUser {
                member,
                user_name,
                user_email,
            });
        }
        Ok(results)
    }

    async fn update_member(&self, ctx: &TenantContext, member: &ProjectMember) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE project_members SET
                role = ?1,
                cost_rate = ?2,
                billing_rate = ?3,
                updated_at = ?4
            WHERE project_id = ?5 AND user_id = ?6 AND tenant_id = ?7;
            "#,
        )
        .bind(member.role.as_str())
        .bind(member.cost_rate.as_i64())
        .bind(member.billing_rate.as_i64())
        .bind(Utc::now().to_rfc3339())
        .bind(member.project_id.to_string())
        .bind(member.user_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    async fn remove_member(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), DbError> {
        sqlx::query(
            "DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2 AND tenant_id = ?3;",
        )
        .bind(project_id.to_string())
        .bind(user_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    // --- Milestone Operations ---
    async fn create_milestone(&self, ctx: &TenantContext, milestone: &Milestone) -> Result<Milestone, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.create_milestone_tx(&mut tx, ctx, milestone).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn create_milestone_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        milestone: &Milestone,
    ) -> Result<Milestone, DbError> {
        sqlx::query(
            r#"
            INSERT INTO milestones (
                id, tenant_id, project_id, sequence_order, title,
                description, target_date, completed_at, status,
                billable_amount, is_billed, invoice_id, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14
            );
            "#,
        )
        .bind(milestone.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(milestone.project_id.to_string())
        .bind(milestone.sequence_order)
        .bind(&milestone.title)
        .bind(&milestone.description)
        .bind(milestone.target_date.format("%Y-%m-%d").to_string())
        .bind(milestone.completed_at.map(|d| d.to_rfc3339()))
        .bind(milestone.status.as_str())
        .bind(milestone.billable_amount.as_i64())
        .bind(if milestone.is_billed { 1 } else { 0 })
        .bind(milestone.invoice_id.map(|id| id.to_string()))
        .bind(milestone.created_at.to_rfc3339())
        .bind(milestone.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(milestone.clone())
    }

    async fn find_milestone_by_id(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Milestone>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM milestones WHERE id = ?1 AND project_id = ?2 AND tenant_id = ?3;",
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_milestone(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_milestone_by_id_only(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Milestone>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM milestones WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_milestone(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_milestone_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Milestone>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM milestones WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_milestone(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_milestones(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<Milestone>, DbError> {
        let rows = sqlx::query(
            "SELECT * FROM milestones WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY sequence_order ASC, created_at ASC;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        rows.iter().map(Self::row_to_milestone).collect()
    }

    async fn get_next_milestone_sequence(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<i64, DbError> {
        let max_val: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(sequence_order) FROM milestones WHERE project_id = ?1 AND tenant_id = ?2;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(max_val.unwrap_or(0) + 1)
    }

    async fn update_milestone(&self, ctx: &TenantContext, milestone: &Milestone) -> Result<(), DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        self.update_milestone_tx(&mut tx, ctx, milestone).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(())
    }

    async fn update_milestone_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        milestone: &Milestone,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE milestones SET
                sequence_order = ?1,
                title = ?2,
                description = ?3,
                target_date = ?4,
                billable_amount = ?5,
                updated_at = ?6
            WHERE id = ?7 AND tenant_id = ?8;
            "#,
        )
        .bind(milestone.sequence_order)
        .bind(&milestone.title)
        .bind(&milestone.description)
        .bind(milestone.target_date.format("%Y-%m-%d").to_string())
        .bind(milestone.billable_amount.as_i64())
        .bind(Utc::now().to_rfc3339())
        .bind(milestone.id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    async fn update_milestone_status(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: MilestoneStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Milestone, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.update_milestone_status_tx(&mut tx, ctx, project_id, id, status, completed_at).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn update_milestone_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: MilestoneStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Milestone, DbError> {
        let comp_str = completed_at.map(|d| d.to_rfc3339());
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE milestones SET
                status = ?1,
                completed_at = ?2,
                updated_at = ?3
            WHERE id = ?4 AND project_id = ?5 AND tenant_id = ?6;
            "#,
        )
        .bind(status.as_str())
        .bind(comp_str)
        .bind(now_str)
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        let updated = self.find_milestone_by_id_tx(tx, ctx, id).await?
            .ok_or_else(|| DbError::NotFound)?;

        Ok(updated)
    }

    async fn mark_milestone_billed_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        invoice_id: Uuid,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE milestones SET
                is_billed = 1,
                invoice_id = ?1,
                updated_at = ?2
            WHERE id = ?3 AND tenant_id = ?4;
            "#,
        )
        .bind(invoice_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    // --- Task Operations ---
    async fn create_task(&self, ctx: &TenantContext, task: &Task) -> Result<Task, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.create_task_tx(&mut tx, ctx, task).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn create_task_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        task: &Task,
    ) -> Result<Task, DbError> {
        sqlx::query(
            r#"
            INSERT INTO tasks (
                id, tenant_id, project_id, milestone_id, assignee_id,
                title, description, status, priority, estimated_hours,
                actual_hours, due_date, completed_at, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15
            );
            "#,
        )
        .bind(task.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(task.project_id.to_string())
        .bind(task.milestone_id.map(|id| id.to_string()))
        .bind(task.assignee_id.map(|id| id.to_string()))
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.status.as_str())
        .bind(task.priority.as_str())
        .bind(task.estimated_hours)
        .bind(task.actual_hours)
        .bind(task.due_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(task.completed_at.map(|d| d.to_rfc3339()))
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(task.clone())
    }

    async fn find_task_by_id(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Task>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM tasks WHERE id = ?1 AND project_id = ?2 AND tenant_id = ?3;",
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_task(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_task_by_id_only(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Task>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM tasks WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_task(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_task_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<Task>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM tasks WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_task(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_tasks(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        filter: &TaskFilter,
    ) -> Result<Vec<Task>, DbError> {
        let mut qb = sqlx::QueryBuilder::new(
            "SELECT * FROM tasks WHERE project_id = ",
        );
        qb.push_bind(project_id.to_string());
        qb.push(" AND tenant_id = ");
        qb.push_bind(ctx.tenant_id_str());

        if let Some(milestone_id) = filter.milestone_id {
            qb.push(" AND milestone_id = ");
            qb.push_bind(milestone_id.to_string());
        }

        if let Some(status) = filter.status {
            qb.push(" AND status = ");
            qb.push_bind(status.as_str());
        }

        if let Some(assignee_id) = filter.assignee_id {
            qb.push(" AND assignee_id = ");
            qb.push_bind(assignee_id.to_string());
        }

        qb.push(" ORDER BY created_at ASC");

        if let Some(limit) = filter.limit {
            qb.push(" LIMIT ");
            qb.push_bind(limit);
        }

        if let Some(offset) = filter.offset {
            qb.push(" OFFSET ");
            qb.push_bind(offset);
        }

        let rows = qb.build().fetch_all(&self.pool).await.map_err(DbError::from)?;
        rows.iter().map(Self::row_to_task).collect()
    }

    async fn update_task(&self, ctx: &TenantContext, task: &Task) -> Result<(), DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        self.update_task_tx(&mut tx, ctx, task).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(())
    }

    async fn update_task_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        task: &Task,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE tasks SET
                milestone_id = ?1,
                assignee_id = ?2,
                title = ?3,
                description = ?4,
                priority = ?5,
                estimated_hours = ?6,
                actual_hours = ?7,
                due_date = ?8,
                updated_at = ?9
            WHERE id = ?10 AND project_id = ?11 AND tenant_id = ?12;
            "#,
        )
        .bind(task.milestone_id.map(|id| id.to_string()))
        .bind(task.assignee_id.map(|id| id.to_string()))
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.priority.as_str())
        .bind(task.estimated_hours)
        .bind(task.actual_hours)
        .bind(task.due_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(Utc::now().to_rfc3339())
        .bind(task.id.to_string())
        .bind(task.project_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    async fn update_task_status(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: TaskStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Task, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.update_task_status_tx(&mut tx, ctx, project_id, id, status, completed_at).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn update_task_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project_id: Uuid,
        id: Uuid,
        status: TaskStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Task, DbError> {
        let comp_str = completed_at.map(|d| d.to_rfc3339());
        let now_str = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE tasks SET
                status = ?1,
                completed_at = ?2,
                updated_at = ?3
            WHERE id = ?4 AND project_id = ?5 AND tenant_id = ?6;
            "#,
        )
        .bind(status.as_str())
        .bind(comp_str)
        .bind(now_str)
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        let updated = self.find_task_by_id_tx(tx, ctx, id).await?
            .ok_or_else(|| DbError::NotFound)?;

        Ok(updated)
    }

    // --- Progress Record Operations ---
    async fn create_progress_record(
        &self,
        ctx: &TenantContext,
        record: &ProgressRecord,
    ) -> Result<ProgressRecord, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.create_progress_record_tx(&mut tx, ctx, record).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn create_progress_record_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        record: &ProgressRecord,
    ) -> Result<ProgressRecord, DbError> {
        sqlx::query(
            r#"
            INSERT INTO progress_records (
                id, tenant_id, project_id, milestone_id, percentage,
                record_date, verified_by, notes, evidence_url,
                is_billed, invoice_id, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13
            );
            "#,
        )
        .bind(record.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(record.project_id.to_string())
        .bind(record.milestone_id.map(|id| id.to_string()))
        .bind(record.percentage)
        .bind(record.record_date.format("%Y-%m-%d").to_string())
        .bind(record.verified_by.map(|id| id.to_string()))
        .bind(&record.notes)
        .bind(&record.evidence_url)
        .bind(if record.is_billed { 1 } else { 0 })
        .bind(record.invoice_id.map(|id| id.to_string()))
        .bind(record.created_at.to_rfc3339())
        .bind(record.created_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(record.clone())
    }

    async fn find_progress_record_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProgressRecord>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM progress_records WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_progress_record(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_progress_records(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProgressRecord>, DbError> {
        let rows = sqlx::query(
            "SELECT * FROM progress_records WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY record_date DESC, created_at DESC;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        rows.iter().map(Self::row_to_progress_record).collect()
    }

    async fn get_latest_progress_record(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Option<ProgressRecord>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM progress_records WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY record_date DESC, created_at DESC LIMIT 1;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_progress_record(&r)?)),
            None => Ok(None),
        }
    }

    async fn mark_progress_billed_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        invoice_id: Uuid,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE progress_records SET
                is_billed = 1,
                invoice_id = ?1,
                updated_at = ?2
            WHERE id = ?3 AND tenant_id = ?4;
            "#,
        )
        .bind(invoice_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    // --- Labor Operations (Milestone 2) ---

    async fn log_labor(&self, ctx: &TenantContext, labor: &ProjectLabor) -> Result<ProjectLabor, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.log_labor_tx(&mut tx, ctx, labor).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn log_labor_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        labor: &ProjectLabor,
    ) -> Result<ProjectLabor, DbError> {
        sqlx::query(
            r#"
            INSERT INTO project_labor (
                id, tenant_id, project_id, task_id, worker_id, worker_name,
                work_date, hours_worked, hourly_rate, total_cost, billing_rate,
                is_billable, description, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15
            );
            "#,
        )
        .bind(labor.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(labor.project_id.to_string())
        .bind(labor.task_id.map(|id| id.to_string()))
        .bind(labor.worker_id.map(|id| id.to_string()))
        .bind(&labor.worker_name)
        .bind(labor.work_date.to_string())
        .bind(labor.hours_worked)
        .bind(labor.hourly_rate.as_i64())
        .bind(labor.total_cost.as_i64())
        .bind(labor.billing_rate.as_i64())
        .bind(if labor.is_billable { 1 } else { 0 })
        .bind(&labor.description)
        .bind(labor.created_at.to_rfc3339())
        .bind(labor.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(labor.clone())
    }

    async fn find_labor_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectLabor>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_labor WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_labor(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_labor_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectLabor>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_labor WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_labor(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_labor_by_project(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectLabor>, DbError> {
        let rows = sqlx::query(
            "SELECT * FROM project_labor WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY work_date DESC, created_at DESC;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        rows.iter().map(Self::row_to_labor).collect()
    }

    async fn delete_labor(&self, ctx: &TenantContext, id: Uuid) -> Result<bool, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.delete_labor_tx(&mut tx, ctx, id).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn delete_labor_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<bool, DbError> {
        let res = sqlx::query(
            "DELETE FROM project_labor WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    // --- Expense Operations (Milestone 2) ---

    async fn create_expense(&self, ctx: &TenantContext, expense: &ProjectExpense) -> Result<ProjectExpense, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.create_expense_tx(&mut tx, ctx, expense).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn create_expense_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        expense: &ProjectExpense,
    ) -> Result<ProjectExpense, DbError> {
        sqlx::query(
            r#"
            INSERT INTO project_expenses (
                id, tenant_id, project_id, task_id, category, description,
                amount, expense_date, vendor_name, receipt_ref, is_billable,
                journal_entry_id, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14
            );
            "#,
        )
        .bind(expense.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(expense.project_id.to_string())
        .bind(expense.task_id.map(|id| id.to_string()))
        .bind(expense.category.as_str())
        .bind(&expense.description)
        .bind(expense.amount.as_i64())
        .bind(expense.expense_date.to_string())
        .bind(&expense.vendor_name)
        .bind(&expense.receipt_ref)
        .bind(if expense.is_billable { 1 } else { 0 })
        .bind(expense.journal_entry_id.map(|id| id.to_string()))
        .bind(expense.created_at.to_rfc3339())
        .bind(expense.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(expense.clone())
    }

    async fn find_expense_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectExpense>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_expenses WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_expense(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_expense_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectExpense>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_expenses WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_expense(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_expenses_by_project(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectExpense>, DbError> {
        let rows = sqlx::query(
            "SELECT * FROM project_expenses WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY expense_date DESC, created_at DESC;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        rows.iter().map(Self::row_to_expense).collect()
    }

    async fn delete_expense(&self, ctx: &TenantContext, id: Uuid) -> Result<bool, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.delete_expense_tx(&mut tx, ctx, id).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn delete_expense_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<bool, DbError> {
        let res = sqlx::query(
            "DELETE FROM project_expenses WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    // --- Material Operations (Milestone 2 Planned Requisitions) ---

    async fn create_material(&self, ctx: &TenantContext, material: &ProjectMaterial) -> Result<ProjectMaterial, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.create_material_tx(&mut tx, ctx, material).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn create_material_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        material: &ProjectMaterial,
    ) -> Result<ProjectMaterial, DbError> {
        sqlx::query(
            r#"
            INSERT INTO project_materials (
                id, tenant_id, project_id, task_id, product_id, warehouse_id,
                quantity_planned, quantity_issued, unit_cost, total_cost,
                status, is_billable, stock_movement_id, journal_entry_id,
                issued_at, notes, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18
            );
            "#,
        )
        .bind(material.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(material.project_id.to_string())
        .bind(material.task_id.map(|id| id.to_string()))
        .bind(material.product_id.to_string())
        .bind(material.warehouse_id.to_string())
        .bind(material.quantity_planned)
        .bind(material.quantity_issued)
        .bind(material.unit_cost.as_i64())
        .bind(material.total_cost.as_i64())
        .bind(material.status.as_str())
        .bind(if material.is_billable { 1 } else { 0 })
        .bind(material.stock_movement_id.map(|id| id.to_string()))
        .bind(material.journal_entry_id.map(|id| id.to_string()))
        .bind(material.issued_at.map(|d| d.to_rfc3339()))
        .bind(&material.notes)
        .bind(material.created_at.to_rfc3339())
        .bind(material.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(material.clone())
    }

    async fn find_material_by_id(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectMaterial>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_materials WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_material(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_material_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<ProjectMaterial>, DbError> {
        let row = sqlx::query(
            "SELECT * FROM project_materials WHERE id = ?1 AND tenant_id = ?2;",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        match row {
            Some(r) => Ok(Some(Self::row_to_material(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_materials_by_project(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<Vec<ProjectMaterial>, DbError> {
        let rows = sqlx::query(
            "SELECT * FROM project_materials WHERE project_id = ?1 AND tenant_id = ?2 ORDER BY created_at ASC;",
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        rows.iter().map(Self::row_to_material).collect()
    }

    async fn update_material(&self, ctx: &TenantContext, material: &ProjectMaterial) -> Result<ProjectMaterial, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.update_material_tx(&mut tx, ctx, material).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn update_material_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        material: &ProjectMaterial,
    ) -> Result<ProjectMaterial, DbError> {
        sqlx::query(
            r#"
            UPDATE project_materials
            SET task_id = ?1,
                product_id = ?2,
                warehouse_id = ?3,
                quantity_planned = ?4,
                quantity_issued = ?5,
                unit_cost = ?6,
                total_cost = ?7,
                status = ?8,
                is_billable = ?9,
                stock_movement_id = ?10,
                journal_entry_id = ?11,
                issued_at = ?12,
                notes = ?13,
                updated_at = ?14
            WHERE id = ?15 AND tenant_id = ?16;
            "#,
        )
        .bind(material.task_id.map(|id| id.to_string()))
        .bind(material.product_id.to_string())
        .bind(material.warehouse_id.to_string())
        .bind(material.quantity_planned)
        .bind(material.quantity_issued)
        .bind(material.unit_cost.as_i64())
        .bind(material.total_cost.as_i64())
        .bind(material.status.as_str())
        .bind(if material.is_billable { 1 } else { 0 })
        .bind(material.stock_movement_id.map(|id| id.to_string()))
        .bind(material.journal_entry_id.map(|id| id.to_string()))
        .bind(material.issued_at.map(|d| d.to_rfc3339()))
        .bind(&material.notes)
        .bind(material.updated_at.to_rfc3339())
        .bind(material.id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(material.clone())
    }

    async fn delete_material(&self, ctx: &TenantContext, id: Uuid) -> Result<bool, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.delete_material_tx(&mut tx, ctx, id).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn delete_material_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<bool, DbError> {
        let res = sqlx::query(
            "DELETE FROM project_materials WHERE id = ?1 AND tenant_id = ?2 AND status = 'PLANNED';",
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    // --- Cost Aggregation & Profitability (Milestone 2) ---

    async fn get_project_cost_summary(
        &self,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<ProjectCostTotals, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from)?;
        let res = self.get_project_cost_summary_tx(&mut tx, ctx, project_id).await?;
        tx.commit().await.map_err(DbError::from)?;
        Ok(res)
    }

    async fn get_project_cost_summary_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        project_id: Uuid,
    ) -> Result<ProjectCostTotals, DbError> {
        let row = sqlx::query(
            r#"
            SELECT
                p.id AS project_id,
                p.budget_amount,
                p.contract_amount,
                COALESCE((
                    SELECT SUM(pm.total_cost)
                    FROM project_materials pm
                    WHERE pm.tenant_id = p.tenant_id AND pm.project_id = p.id AND pm.status = 'ISSUED'
                ), 0) AS total_material_cost,
                COALESCE((
                    SELECT SUM(pl.total_cost)
                    FROM project_labor pl
                    WHERE pl.tenant_id = p.tenant_id AND pl.project_id = p.id
                ), 0) AS total_labor_cost,
                COALESCE((
                    SELECT SUM(pe.amount)
                    FROM project_expenses pe
                    WHERE pe.tenant_id = p.tenant_id AND pe.project_id = p.id
                ), 0) AS total_expense_cost,
                COALESCE((
                    SELECT SUM(m.billable_amount)
                    FROM milestones m
                    WHERE m.tenant_id = p.tenant_id AND m.project_id = p.id AND m.is_billed = 1
                ), 0) AS milestone_billed_revenue,
                COALESCE((
                    SELECT SUM(i.total_amount)
                    FROM invoices i
                    WHERE i.tenant_id = p.tenant_id AND i.id IN (
                        SELECT invoice_id FROM milestones WHERE tenant_id = p.tenant_id AND project_id = p.id AND invoice_id IS NOT NULL
                        UNION
                        SELECT invoice_id FROM progress_records WHERE tenant_id = p.tenant_id AND project_id = p.id AND invoice_id IS NOT NULL
                    )
                ), 0) AS invoice_billed_revenue
            FROM projects p
            WHERE p.id = ?1 AND p.tenant_id = ?2;
            "#,
        )
        .bind(project_id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from)?;

        let row = match row {
            Some(r) => r,
            None => return Err(DbError::NotFound),
        };

        let budget_amount = Rupiah::new(row.get::<i64, _>("budget_amount"));
        let contract_amount = Rupiah::new(row.get::<i64, _>("contract_amount"));
        let total_material_cost = Rupiah::new(row.get::<i64, _>("total_material_cost"));
        let total_labor_cost = Rupiah::new(row.get::<i64, _>("total_labor_cost"));
        let total_expense_cost = Rupiah::new(row.get::<i64, _>("total_expense_cost"));

        let milestone_billed_revenue: i64 = row.get("milestone_billed_revenue");
        let invoice_billed_revenue: i64 = row.get("invoice_billed_revenue");
        let billed_revenue_raw = std::cmp::max(milestone_billed_revenue, invoice_billed_revenue);
        let total_billed_revenue = Rupiah::new(billed_revenue_raw);

        let total_actual_cost = total_material_cost
            .saturating_add(total_labor_cost)
            .saturating_add(total_expense_cost);

        let net_profit_amount = total_billed_revenue.saturating_sub(total_actual_cost);

        let margin_percentage_basis_points = ProjectProfitabilitySummary::calculate_margin_bps(
            net_profit_amount,
            total_billed_revenue,
        );

        Ok(ProjectCostTotals {
            project_id,
            budget_amount,
            contract_amount,
            total_material_cost,
            total_labor_cost,
            total_expense_cost,
            total_actual_cost,
            total_billed_revenue,
            net_profit_amount,
            margin_percentage_basis_points,
        })
    }
}
