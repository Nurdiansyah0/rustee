use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::money::Rupiah;

// ============================================================================
// Enums
// ============================================================================

/// Lifecycle status for a project material requisition line item (PRD §10, §31)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MaterialStatus {
    Planned,
    Issued,
    Returned,
    Cancelled,
}

impl MaterialStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "PLANNED",
            Self::Issued => "ISSUED",
            Self::Returned => "RETURNED",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PLANNED" => Some(Self::Planned),
            "ISSUED" => Some(Self::Issued),
            "RETURNED" => Some(Self::Returned),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn can_transition_to(&self, target: MaterialStatus) -> bool {
        matches!(
            (self, target),
            (Self::Planned, Self::Issued)
                | (Self::Planned, Self::Cancelled)
                | (Self::Issued, Self::Returned)
        )
    }

    pub fn is_issued(&self) -> bool {
        matches!(self, Self::Issued)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Returned | Self::Cancelled)
    }
}

/// Category of direct project expense (PRD §10, §31)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpenseCategory {
    Permits,
    EquipmentRental,
    Subcontractor,
    Travel,
    Other,
}

impl ExpenseCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Permits => "PERMITS",
            Self::EquipmentRental => "EQUIPMENT_RENTAL",
            Self::Subcontractor => "SUBCONTRACTOR",
            Self::Travel => "TRAVEL",
            Self::Other => "OTHER",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PERMITS" => Some(Self::Permits),
            "EQUIPMENT_RENTAL" | "EQUIPMENT" => Some(Self::EquipmentRental),
            "SUBCONTRACTOR" => Some(Self::Subcontractor),
            "TRAVEL" => Some(Self::Travel),
            "OTHER" => Some(Self::Other),
            _ => None,
        }
    }
}

// ============================================================================
// Domain Entities
// ============================================================================

/// Project material requisition / allocation line item
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMaterial {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity_planned: i64,
    pub quantity_issued: i64,
    pub unit_cost: Rupiah,
    pub total_cost: Rupiah,
    pub status: MaterialStatus,
    pub is_billable: bool,
    pub stock_movement_id: Option<Uuid>,
    pub journal_entry_id: Option<Uuid>,
    pub issued_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectMaterial {
    pub fn is_issued(&self) -> bool {
        self.status == MaterialStatus::Issued
    }

    /// Pure integer calculation of material line cost: quantity * unit_cost
    pub fn calculate_total_cost(quantity: i64, unit_cost: Rupiah) -> Rupiah {
        if quantity <= 0 || unit_cost.is_negative() {
            return Rupiah::ZERO;
        }
        let total_128 = (quantity as i128) * (unit_cost.as_i64() as i128);
        Rupiah::new(total_128 as i64)
    }
}

/// Direct project labor log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLabor {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub worker_name: String,
    pub work_date: NaiveDate,
    pub hours_worked: i64,
    pub hourly_rate: Rupiah,
    pub total_cost: Rupiah,
    pub billing_rate: Rupiah,
    pub is_billable: bool,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectLabor {
    /// Pure integer calculation of labor cost: hours * hourly_rate
    pub fn calculate_total_cost(hours: i64, rate: Rupiah) -> Rupiah {
        if hours <= 0 || rate.is_negative() {
            return Rupiah::ZERO;
        }
        let total_128 = (hours as i128) * (rate.as_i64() as i128);
        Rupiah::new(total_128 as i64)
    }
}

/// Direct third-party project expense
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectExpense {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub category: ExpenseCategory,
    pub description: String,
    pub amount: Rupiah,
    pub expense_date: NaiveDate,
    pub vendor_name: Option<String>,
    pub receipt_ref: Option<String>,
    pub is_billable: bool,
    pub journal_entry_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Single-roundtrip repository aggregation container
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCostTotals {
    pub project_id: Uuid,
    pub budget_amount: Rupiah,
    pub contract_amount: Rupiah,
    pub total_material_cost: Rupiah,
    pub total_labor_cost: Rupiah,
    pub total_expense_cost: Rupiah,
    pub total_actual_cost: Rupiah,
    pub total_billed_revenue: Rupiah,
    pub net_profit_amount: Rupiah,
    pub margin_percentage_basis_points: i64,
}

impl ProjectCostTotals {
    pub fn to_profitability_summary(&self) -> ProjectProfitabilitySummary {
        ProjectProfitabilitySummary {
            project_id: self.project_id,
            budget_amount: self.budget_amount,
            contract_amount: self.contract_amount,
            total_material_cost: self.total_material_cost,
            total_labor_cost: self.total_labor_cost,
            total_expense_cost: self.total_expense_cost,
            total_actual_cost: self.total_actual_cost,
            total_billed_revenue: self.total_billed_revenue,
            net_profit_amount: self.net_profit_amount,
            margin_percentage_basis_points: self.margin_percentage_basis_points,
        }
    }
}

/// Official Project Profitability Summary DTO (PROJECT.md line 127)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectProfitabilitySummary {
    pub project_id: Uuid,
    pub budget_amount: Rupiah,
    pub contract_amount: Rupiah,
    pub total_material_cost: Rupiah,
    pub total_labor_cost: Rupiah,
    pub total_expense_cost: Rupiah,
    pub total_actual_cost: Rupiah,
    pub total_billed_revenue: Rupiah,
    pub net_profit_amount: Rupiah,
    pub margin_percentage_basis_points: i64, // e.g. 2500 = 25.00%
}

impl ProjectProfitabilitySummary {
    /// Pure integer calculation of margin in basis points:
    /// margin_bps = (net_profit * 10,000) / total_billed_revenue
    /// Zero if total_billed_revenue is 0 to avoid division by zero.
    /// Operates on i128 to prevent integer overflow.
    pub fn calculate_margin_bps(net_profit: Rupiah, total_billed_revenue: Rupiah) -> i64 {
        let revenue = total_billed_revenue.as_i64();
        if revenue == 0 {
            return 0;
        }

        let profit_128 = net_profit.as_i64() as i128;
        let revenue_128 = revenue as i128;
        let numerator = profit_128 * 10_000;

        (numerator / revenue_128) as i64
    }

    pub fn calculate(
        project_id: Uuid,
        budget_amount: Rupiah,
        contract_amount: Rupiah,
        total_material_cost: Rupiah,
        total_labor_cost: Rupiah,
        total_expense_cost: Rupiah,
        total_billed_revenue: Rupiah,
    ) -> Self {
        let total_actual_cost = total_material_cost
            .saturating_add(total_labor_cost)
            .saturating_add(total_expense_cost);

        let net_profit_amount = total_billed_revenue.saturating_sub(total_actual_cost);
        let margin_percentage_basis_points =
            Self::calculate_margin_bps(net_profit_amount, total_billed_revenue);

        Self {
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
        }
    }
}

// ============================================================================
// Request / Response DTOs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogProjectLaborRequest {
    pub task_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub worker_name: Option<String>,
    pub work_date: NaiveDate,
    pub hours_worked: i64,
    pub hourly_rate: Option<Rupiah>,
    pub billing_rate: Option<Rupiah>,
    pub is_billable: Option<bool>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLaborListResponse {
    pub labor: Vec<ProjectLabor>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectExpenseRequest {
    pub task_id: Option<Uuid>,
    pub category: ExpenseCategory,
    pub description: String,
    pub amount: Rupiah,
    pub expense_date: NaiveDate,
    pub vendor_name: Option<String>,
    pub receipt_ref: Option<String>,
    pub is_billable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectExpensesListResponse {
    pub expenses: Vec<ProjectExpense>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectMaterialRequest {
    pub task_id: Option<Uuid>,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity_planned: i64,
    pub is_billable: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMaterialsListResponse {
    pub materials: Vec<ProjectMaterial>,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IssueProjectMaterialRequest {
    pub quantity: Option<i64>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectIssueMaterialRequest {
    pub task_id: Option<Uuid>,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity: i64,
    pub is_billable: Option<bool>,
    pub notes: Option<String>,
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_status_lifecycle() {
        assert!(MaterialStatus::Planned.can_transition_to(MaterialStatus::Issued));
        assert!(MaterialStatus::Planned.can_transition_to(MaterialStatus::Cancelled));
        assert!(MaterialStatus::Issued.can_transition_to(MaterialStatus::Returned));
        assert!(!MaterialStatus::Issued.can_transition_to(MaterialStatus::Planned));
        assert!(!MaterialStatus::Cancelled.can_transition_to(MaterialStatus::Issued));

        assert_eq!(MaterialStatus::from_str("planned"), Some(MaterialStatus::Planned));
        assert_eq!(MaterialStatus::from_str("ISSUED"), Some(MaterialStatus::Issued));
        assert_eq!(MaterialStatus::from_str("invalid"), None);
    }

    #[test]
    fn test_expense_category_parsing() {
        assert_eq!(ExpenseCategory::from_str("permits"), Some(ExpenseCategory::Permits));
        assert_eq!(ExpenseCategory::from_str("EQUIPMENT_RENTAL"), Some(ExpenseCategory::EquipmentRental));
        assert_eq!(ExpenseCategory::from_str("equipment"), Some(ExpenseCategory::EquipmentRental));
        assert_eq!(ExpenseCategory::from_str("SUBCONTRACTOR"), Some(ExpenseCategory::Subcontractor));
        assert_eq!(ExpenseCategory::from_str("travel"), Some(ExpenseCategory::Travel));
        assert_eq!(ExpenseCategory::from_str("other"), Some(ExpenseCategory::Other));
        assert_eq!(ExpenseCategory::from_str("unknown"), None);
    }

    #[test]
    fn test_pure_integer_cost_calculations() {
        // Labor
        let cost = ProjectLabor::calculate_total_cost(8, Rupiah::new(150_000));
        assert_eq!(cost, Rupiah::new(1_200_000));

        // Zero hours
        assert_eq!(ProjectLabor::calculate_total_cost(0, Rupiah::new(150_000)), Rupiah::ZERO);

        // Material
        let mat_cost = ProjectMaterial::calculate_total_cost(25, Rupiah::new(50_000));
        assert_eq!(mat_cost, Rupiah::new(1_250_000));
    }

    #[test]
    fn test_profitability_margin_basis_points() {
        let project_id = Uuid::new_v4();
        // Revenue Rp 100,000,000, Costs: Mat 20M, Labor 30M, Exp 10M -> Actual 60M
        // Net profit: 40M (40.00% = 4000 bps)
        let summary = ProjectProfitabilitySummary::calculate(
            project_id,
            Rupiah::new(80_000_000),
            Rupiah::new(100_000_000),
            Rupiah::new(20_000_000),
            Rupiah::new(30_000_000),
            Rupiah::new(10_000_000),
            Rupiah::new(100_000_000),
        );

        assert_eq!(summary.total_actual_cost, Rupiah::new(60_000_000));
        assert_eq!(summary.net_profit_amount, Rupiah::new(40_000_000));
        assert_eq!(summary.margin_percentage_basis_points, 4000);

        // Zero revenue: bps should be 0, no division by zero panic
        let zero_rev_summary = ProjectProfitabilitySummary::calculate(
            project_id,
            Rupiah::new(80_000_000),
            Rupiah::new(100_000_000),
            Rupiah::new(10_000_000),
            Rupiah::new(10_000_000),
            Rupiah::new(5_000_000),
            Rupiah::ZERO,
        );
        assert_eq!(zero_rev_summary.net_profit_amount, Rupiah::new(-25_000_000));
        assert_eq!(zero_rev_summary.margin_percentage_basis_points, 0);

        // Negative profit (Loss): Revenue 50M, Actual 75M -> Net profit -25M (-50.00% = -5000 bps)
        let loss_summary = ProjectProfitabilitySummary::calculate(
            project_id,
            Rupiah::new(80_000_000),
            Rupiah::new(100_000_000),
            Rupiah::new(25_000_000),
            Rupiah::new(25_000_000),
            Rupiah::new(25_000_000),
            Rupiah::new(50_000_000),
        );
        assert_eq!(loss_summary.net_profit_amount, Rupiah::new(-25_000_000));
        assert_eq!(loss_summary.margin_percentage_basis_points, -5000);
    }
}
