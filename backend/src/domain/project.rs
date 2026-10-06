use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::money::Rupiah;
pub use crate::domain::project_costing::*;

/// Default server-side project numbering prefix
pub const DEFAULT_PROJECT_PREFIX: &str = "PRJ";

// ============================================================================
// Enums
// ============================================================================

/// Project lifecycle state machine (PRD §12, §31)
/// Workflow: DRAFT -> ACTIVE -> ON_HOLD -> COMPLETED / CANCELLED
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectStatus {
    Draft,
    Active,
    OnHold,
    Completed,
    Cancelled,
}

impl ProjectStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Active => "ACTIVE",
            Self::OnHold => "ON_HOLD",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "DRAFT" => Some(Self::Draft),
            "ACTIVE" => Some(Self::Active),
            "ON_HOLD" => Some(Self::OnHold),
            "COMPLETED" => Some(Self::Completed),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Enforces legal state machine transitions (§12)
    pub fn can_transition_to(&self, target: ProjectStatus) -> bool {
        matches!(
            (self, target),
            (Self::Draft, Self::Active)
                | (Self::Draft, Self::Cancelled)
                | (Self::Active, Self::OnHold)
                | (Self::Active, Self::Completed)
                | (Self::Active, Self::Cancelled)
                | (Self::OnHold, Self::Active)
                | (Self::OnHold, Self::Cancelled)
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    pub fn can_modify(&self) -> bool {
        matches!(self, Self::Draft | Self::Active)
    }
}

/// Project progress billing method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BillingType {
    Milestone,
    PercentageOfCompletion,
    TimeAndMaterials,
    Hybrid,
}

impl BillingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Milestone => "MILESTONE",
            Self::PercentageOfCompletion => "PERCENTAGE_OF_COMPLETION",
            Self::TimeAndMaterials => "TIME_AND_MATERIALS",
            Self::Hybrid => "HYBRID",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "MILESTONE" => Some(Self::Milestone),
            "PERCENTAGE_OF_COMPLETION" => Some(Self::PercentageOfCompletion),
            "TIME_AND_MATERIALS" => Some(Self::TimeAndMaterials),
            "HYBRID" => Some(Self::Hybrid),
            _ => None,
        }
    }
}

/// Milestone lifecycle state machine
/// Workflow: PENDING -> IN_PROGRESS -> COMPLETED / CANCELLED
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MilestoneStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl MilestoneStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::InProgress => "IN_PROGRESS",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PENDING" => Some(Self::Pending),
            "IN_PROGRESS" => Some(Self::InProgress),
            "COMPLETED" => Some(Self::Completed),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, target: MilestoneStatus) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::InProgress)
                | (Self::Pending, Self::Cancelled)
                | (Self::InProgress, Self::Completed)
                | (Self::InProgress, Self::Cancelled)
        )
    }

    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// Task lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Todo => "TODO",
            Self::InProgress => "IN_PROGRESS",
            Self::Blocked => "BLOCKED",
            Self::Done => "DONE",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "TODO" => Some(Self::Todo),
            "IN_PROGRESS" => Some(Self::InProgress),
            "BLOCKED" => Some(Self::Blocked),
            "DONE" => Some(Self::Done),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, target: TaskStatus) -> bool {
        matches!(
            (self, target),
            (Self::Todo, Self::InProgress)
                | (Self::Todo, Self::Blocked)
                | (Self::Todo, Self::Cancelled)
                | (Self::InProgress, Self::Blocked)
                | (Self::InProgress, Self::Done)
                | (Self::InProgress, Self::Cancelled)
                | (Self::Blocked, Self::InProgress)
                | (Self::Blocked, Self::Cancelled)
                | (Self::Done, Self::InProgress)
        )
    }

    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

/// Task urgency / priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Urgent,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Urgent => "URGENT",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "LOW" => Some(Self::Low),
            "MEDIUM" => Some(Self::Medium),
            "HIGH" => Some(Self::High),
            "URGENT" => Some(Self::Urgent),
            _ => None,
        }
    }
}

/// Contractor & Project team roles
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectRole {
    #[serde(rename = "PROJECT_MANAGER", alias = "project_manager", alias = "ProjectManager")]
    ProjectManager,
    #[serde(rename = "SITE_ENGINEER", alias = "site_engineer", alias = "SiteEngineer")]
    SiteEngineer,
    #[serde(rename = "FOREMAN", alias = "foreman", alias = "Foreman")]
    Foreman,
    #[serde(rename = "WORKER", alias = "worker", alias = "Worker")]
    Worker,
    #[serde(rename = "SUBCONTRACTOR", alias = "subcontractor", alias = "Subcontractor")]
    Subcontractor,
    #[serde(rename = "CONSULTANT", alias = "consultant", alias = "Consultant")]
    Consultant,
    #[serde(untagged)]
    Custom(String),
}

impl ProjectRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ProjectManager => "PROJECT_MANAGER",
            Self::SiteEngineer => "SITE_ENGINEER",
            Self::Foreman => "FOREMAN",
            Self::Worker => "WORKER",
            Self::Subcontractor => "SUBCONTRACTOR",
            Self::Consultant => "CONSULTANT",
            Self::Custom(name) => name.as_str(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "PROJECT_MANAGER" => Self::ProjectManager,
            "SITE_ENGINEER" => Self::SiteEngineer,
            "FOREMAN" => Self::Foreman,
            "WORKER" => Self::Worker,
            "SUBCONTRACTOR" => Self::Subcontractor,
            "CONSULTANT" => Self::Consultant,
            other => Self::Custom(other.to_string()),
        }
    }
}

// ============================================================================
// Domain Entities
// ============================================================================

/// Master Project domain entity (R1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_number: String, // Server-generated PRJ-YYYY-XXXXXX
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<Uuid>,
    pub customer_name: String,
    pub status: ProjectStatus,
    pub billing_type: BillingType,
    pub budget_amount: Rupiah,   // integer Rupiah i64
    pub contract_amount: Rupiah, // integer Rupiah i64
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub actual_completion_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn is_active(&self) -> bool {
        self.status == ProjectStatus::Active
    }

    pub fn can_issue_materials(&self) -> bool {
        self.status == ProjectStatus::Active
    }

    pub fn can_bill_progress(&self) -> bool {
        self.status == ProjectStatus::Active
    }

    pub fn can_modify(&self) -> bool {
        self.status.can_modify()
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }
}

/// Project team member assignment with cost & billing rates (R1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMember {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub role: ProjectRole,
    pub cost_rate: Rupiah,    // Hourly cost rate in Rupiah i64
    pub billing_rate: Rupiah, // Hourly billing rate in Rupiah i64
    pub joined_at: DateTime<Utc>,
}

/// Project member with joined user details for presentation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMemberWithUser {
    pub member: ProjectMember,
    pub user_name: String,
    pub user_email: String,
}

/// Project contractual milestone (R1, R4)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub sequence_order: i64,
    pub title: String,
    pub description: Option<String>,
    pub target_date: NaiveDate,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: MilestoneStatus,
    pub billable_amount: Rupiah, // Fixed contractual billing amount
    pub is_billed: bool,
    pub invoice_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_already_completed: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Milestone {
    pub fn is_completed(&self) -> bool {
        self.status == MilestoneStatus::Completed
    }

    pub fn can_bill(&self) -> bool {
        self.status == MilestoneStatus::Completed && !self.is_billed && self.billable_amount.is_positive()
    }
}

/// Granular project task (R1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub milestone_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub estimated_hours: i64,
    pub actual_hours: i64,
    pub due_date: Option<NaiveDate>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Verified physical site progress audit record (R1, R4)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub milestone_id: Option<Uuid>,
    pub percentage: i64, // 0..=100
    pub record_date: NaiveDate,
    pub verified_by: Option<Uuid>,
    pub notes: Option<String>,
    pub evidence_url: Option<String>,
    pub is_billed: bool,
    pub invoice_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Request / Response DTOs and Query Filters
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectFilter {
    pub status: Option<ProjectStatus>,
    pub customer_id: Option<Uuid>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub type ProjectListFilter = ProjectFilter;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskFilter {
    pub milestone_id: Option<Uuid>,
    pub status: Option<TaskStatus>,
    pub assignee_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub type TaskListFilter = TaskFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<Uuid>,
    pub customer_name: Option<String>,
    pub billing_type: Option<BillingType>,
    pub budget_amount: Option<Rupiah>,
    pub contract_amount: Option<Rupiah>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectStatusRequest {
    pub status: ProjectStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddProjectMemberRequest {
    pub user_id: Uuid,
    pub role: String,
    pub cost_rate: Option<i64>,
    pub billing_rate: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMilestoneRequest {
    pub sequence_order: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub target_date: NaiveDate,
    pub billable_amount: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMilestoneStatusRequest {
    pub status: MilestoneStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub milestone_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<TaskPriority>,
    pub estimated_hours: Option<i64>,
    pub due_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskStatusRequest {
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProgressRecordRequest {
    pub milestone_id: Option<Uuid>,
    pub percentage: i64,
    pub record_date: NaiveDate,
    pub verified_by: Option<Uuid>,
    pub notes: Option<String>,
    pub evidence_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectsListResponse {
    pub projects: Vec<Project>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMembersListResponse {
    pub members: Vec<ProjectMember>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestonesListResponse {
    pub milestones: Vec<Milestone>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksListResponse {
    pub tasks: Vec<Task>,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CompleteMilestoneRequest {}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BillMilestoneRequest {
    pub tax_type: Option<String>,
    pub due_date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneBillingResponse {
    pub milestone: Milestone,
    pub invoice: crate::service::invoice_service::InvoiceResponse,
    pub invoice_id: String,
    pub invoice_number: String,
    pub total_amount: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProgressBillingRequest {
    pub percentage: Option<i64>,
    pub progress_record_id: Option<Uuid>,
    pub tax_type: Option<String>,
    pub due_date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressBillingResponse {
    pub progress_record: ProgressRecord,
    pub invoice: crate::service::invoice_service::InvoiceResponse,
    pub invoice_id: String,
    pub invoice_number: String,
    pub billed_percentage: i64,
    pub cumulative_percentage: i64,
    pub total_amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressRecordsListResponse {
    pub records: Vec<ProgressRecord>,
    pub count: usize,
}

