# Project: Invinite Business OS v4.1 Phase 3 Project & Contractor Operations

## Architecture
Phase 3 extends the Invinite Business OS v4.1 multi-tenant architecture with project lifecycle management, milestone tracking, task assignments, direct job costing (material, labor, expense), warehouse inventory allocation with atomic negative balance prevention, hybrid milestone/percentage-of-completion progress billing integrated with commercial invoicing, double-entry GL journal automation, transactional outbox eventing, and universal Vue 3 PWA frontend alignment per PRD (§10, §31, §60, §61, §62).

### Core Principles
1. **Strict Multi-Tenancy & Project Security (R1)**:
   All project domain models (`Project`, `ProjectMember`, `Milestone`, `Task`, `ProgressRecord`, `ProjectMaterial`, `ProjectLabor`, `ProjectExpense`) enforce strict `TenantContext` isolation. Cross-tenant lookups strictly return HTTP 404 Not Found (anti-enumeration boundary), while tenant-internal unauthorized role actions return HTTP 403 Forbidden.
2. **Project Lifecycle & Sequential Numbering (R1)**:
   Project lifecycle state machine (`DRAFT` -> `ACTIVE` -> `ON_HOLD` -> `COMPLETED` / `CANCELLED`) with server-side sequential gapless numbering (`PRJ-YYYY-XXXXXX`). Only `ACTIVE` projects permit material issuing and progress billing; inactive projects reject mutations with HTTP 422 Unprocessable Entity.
3. **Pure Integer Rupiah (`i64`) Costing Arithmetic (R2)**:
   All budgets, contract amounts, labor rates, expense amounts, material unit costs, and profitability metrics operate strictly on integer Rupiah (`Rupiah(i64)`) using `round_half_up_i128`. Floating-point arithmetic is strictly prohibited.
4. **Direct Job Costing & Inventory Integration (R3)**:
   Direct material requisitions allocate warehouse stock to project tasks. Material issues atomically deduct stock via `InventoryService` under `BEGIN IMMEDIATE` write locks, strictly preventing negative stock balances (insufficient stock returns HTTP 422 `INSUFFICIENT_STOCK`). Immediately posts balanced double-entry GL journals (`Debit 5000 Beban Pokok / Direct Project Cost = Credit 1300 Persediaan Barang Dagang`) and records immutable movement audit logs.
5. **Hybrid Progress Billing & Commercial Invoicing (R4)**:
   Supports both Fixed Milestone Billing (upon milestone completion) and Percentage of Completion (PoC) Billing (verified progress percentage <= 100%). Generates immutable commercial invoices via `InvoiceService` with sequential `INV-YYYY-XXXXXX`, tax snapshotting (`invoice_snapshots`), and automatic double-entry GL journals (`Debit 1200 Piutang Usaha = Credit 4000 Pendapatan Usaha + Credit 2100 Utang Pajak`). Concurrent or duplicate billing on the same milestone returns HTTP 409 Conflict. Links directly to receivables tracking and payment allocation.
6. **Transactional Outbox Eventing (R5)**:
   Emits `ProjectCreated`, `MilestoneCompleted`, `ProjectMaterialIssued`, and `ProgressBilled` within the same atomic SQLite transaction as domain mutations. Delivered with at-least-once guarantees and UUID deduplication.
7. **Universal PWA Alignment & Invariant Protection (R5)**:
   Universal Vue 3 PWA navigation dynamically surfaces "Proyek & Kontraktor" when enabled in capability matrix (`contractor: ['projects', ...]`). In `App.vue`, the personal workspace eviction watcher strictly preserves the literal array check `['inventory', 'purchasing', 'invoices', 'accounting'].includes(currentTab.value)` to protect test 2.8b. Mobile bottom nav Slot 3 (rapid POS keypad) is strictly preserved.
8. **Test Synchronization & Zero Regressions**:
   Database migration `0012_v4_1_project_management.sql` expands domain tables from 37 to 45 (backend) and 33 to 41 (E2E reference server). All test assertions are synchronized in lockstep. 100% passing tests across `cargo test`, `cargo clippy`, `npm run build`, and `e2e_tests/runner.sh all`.

---

## Feature Inventory

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | TenantContext Isolation for Project Domain | Mandate explicit TenantContext across all project repository queries (`WHERE tenant_id = ?`) | M1 | PRD §5; ORIGINAL_REQUEST §R1 |
| 2 | Anti-Enumeration Entity Resolution | Cross-tenant lookups strictly return HTTP 404; unauthorized tenant roles return HTTP 403 | M1 | PRD §5; ORIGINAL_REQUEST §Acceptance Criteria |
| 3 | Project RBAC Enforcement | Enforce role permissions (Owner, Admin, Manager, Staff, Accountant) over project actions | M1 | PRD §6; ORIGINAL_REQUEST §Acceptance Criteria |
| 4 | Sequential Gapless Project Numbering | Server-authoritative gapless project code generation (`PRJ-YYYY-XXXXXX`) | M1 | PRD §14, §31; ORIGINAL_REQUEST §R1 |
| 5 | Project Master Entity & Budget Tracking | Master project model with budget tracking, contract value, customer linkage, and dates | M1 | PRD §10, §31; ORIGINAL_REQUEST §R1 |
| 6 | Project State Machine Engine | Lifecycle states: `DRAFT` -> `ACTIVE` -> `ON_HOLD` -> `COMPLETED` / `CANCELLED` | M1 | PRD §12, §31; ORIGINAL_REQUEST §R1 |
| 7 | Project Member Assignment & Role Rates | Assign users/contractors to projects with hourly cost & billing rates | M1 | PRD §10, §31; ORIGINAL_REQUEST §R1 |
| 8 | Milestone State Machine & Target Tracking | Milestone tracking (`PENDING` -> `IN_PROGRESS` -> `COMPLETED` / `CANCELLED`) with billable amounts | M1 | PRD §10, §31; ORIGINAL_REQUEST §R1 |
| 9 | Task Assignment & Milestone Alignment | Granular work items linked to milestones with priority, estimated & actual hours | M1 | PRD §10, §31; ORIGINAL_REQUEST §R1 |
| 10 | Progress Record Audit & Completion Tracking | Verified physical progress records (0-100%) with inspector and attachment audit | M1 | PRD §10, §31; ORIGINAL_REQUEST §R1 |
| 11 | Pure Integer Rupiah (`i64`) Arithmetic Engine | All costing, budgets, labor rates, and profitability calculations operate in integer Rupiah | M2 | PRD §11, §72; ORIGINAL_REQUEST §R2 |
| 12 | Material Requisition & Allocation | Allocate warehouse products to project tasks (`PLANNED` -> `ISSUED`) | M3 | PRD §10, §27, §31; ORIGINAL_REQUEST §R2 |
| 13 | Atomic Stock Deduction & Negative Balance Guard | Issue stock via InventoryService under `BEGIN IMMEDIATE`, insufficient stock returns HTTP 422 | M3 | PRD §27, §72; ORIGINAL_REQUEST §R3 |
| 14 | Direct Job Costing GL Double-Entry Posting | Auto-post balanced journal: `Debit 5000 (Beban Pokok) = Credit 1300 (Persediaan)` | M3 | PRD §11, §27, §62; ORIGINAL_REQUEST §R3 |
| 15 | Labor Cost & Rate Calculation Engine | Log labor hours, computing total cost (`hours * hourly_rate`) and billable values | M2 | PRD §10, §31; ORIGINAL_REQUEST §R2 |
| 16 | Project Expense Tracking Engine | Log third-party expenses (permits, rentals, subcontractors) with billable flag | M2 | PRD §10, §31; ORIGINAL_REQUEST §R2 |
| 17 | Real-Time Project Profitability Engine | Budget vs Actual Costs (Material + Labor + Expense) vs Billed Revenue vs Net Margin | M2 | PRD §38, §63; ORIGINAL_REQUEST §R2 |
| 18 | Fixed Milestone Progress Billing Mode | Invoice fixed agreed amount upon milestone completion; requires status == COMPLETED | M4 | PRD §12, §31; ORIGINAL_REQUEST §R4 |
| 19 | Percentage of Completion (PoC) Billing Mode | Invoice percentage of contract value verified by progress record (monotonic <= 100%) | M4 | PRD §12, §31; ORIGINAL_REQUEST §R4 |
| 20 | Commercial Invoice Engine Integration | Issue immutable commercial invoices via InvoiceService with `INV-YYYY-XXXXXX` | M4 | PRD §13, §14, §15; ORIGINAL_REQUEST §R4 |
| 21 | Customer Tax Snapshotting & Calculation | Snapshot Indonesian VAT (PPN 11%, 12%, UMKM 0.5%) via integer `round_half_up_i128` | M4 | PRD §15, §16; ORIGINAL_REQUEST §R4 |
| 22 | Commercial Invoicing GL Double-Entry Posting | Auto-post balanced journal: `Debit 1200 (AR) = Credit 4000 (Revenue) + Credit 2100 (Tax)` | M4 | PRD §11, §62; ORIGINAL_REQUEST §R4 |
| 23 | Duplicate & Concurrent Billing Prevention | Duplicate billing on same milestone/stage returns HTTP 409 Conflict | M4 | PRD §72; ORIGINAL_REQUEST §R4 |
| 24 | Receivables Linkage & Payment Allocation | Auto-create open `Receivable` record linked to invoice for atomic payment allocation | M4 | PRD §17, §18; ORIGINAL_REQUEST §Acceptance Criteria |
| 25 | ProjectCreated Transactional Outbox Event | Commit `ProjectCreated` event in same SQLite transaction as project creation | M5 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 26 | MilestoneCompleted Transactional Outbox Event | Commit `MilestoneCompleted` event in same SQLite transaction as milestone completion | M4 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 27 | ProjectMaterialIssued Transactional Outbox Event | Commit `ProjectMaterialIssued` in same SQLite transaction as stock deduction & GL journal | M3 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 28 | ProgressBilled Transactional Outbox Event | Commit `ProgressBilled` event in same SQLite transaction as invoice issuance | M4 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 29 | Outbox At-Least-Once Delivery & Deduplication | Outbox processor polls and delivers project events with UUID deduplication | M5 | PRD §20, §55; ORIGINAL_REQUEST §Acceptance Criteria |
| 30 | Universal PWA Navigation & Capability Matrix | Surface "Proyek & Kontraktor" when capability matrix includes `projects` | M5 | PRD §35, §37; ORIGINAL_REQUEST §R5 |
| 31 | Pinia Project Workspace Store | Reactive Pinia store for projects, milestones, tasks, and project costing | M5 | PRD §37; ORIGINAL_REQUEST §R5 |
| 32 | Universal Vue 3 `ProjectsView.vue` | Universal PWA view for managing projects, tracking milestones, logging costs, and billing | M5 | PRD §37; ORIGINAL_REQUEST §R5 |
| 33 | Schema Persistence & Migration Synchronization | Migration `0012_v4_1_project_management.sql` (8 tables), backend table count sync (37->45) | M1 | PRD §10, §60; ORIGINAL_REQUEST §Acceptance Criteria |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| Test | E2E Testing Track | Design and maintain requirement-driven opaque-box test suite (Tiers 1-4) covering all 33 inventoried features; publishes `TEST_READY.md` | none | IN_PROGRESS |
| M1 | Project & Milestone Domain Architecture & Migrations | Features 1-10, 33: Migration `0012_v4_1_project_management.sql` (8 tables), domain models, sequential code `PRJ-YYYY-XXXXXX`, lifecycle state machines, `TenantContext` isolation (404/403), Axum routes & controllers, table count sync (37->45) | none | DONE |
| M2 | Material, Labor & Expense Cost Tracking Engine | Features 11, 15, 16, 17: Integer Rupiah `i64` math, labor hours/rates, vendor expenses, project profitability summary (Budget vs Actual Cost vs Revenue vs Margin) | M1 | DONE |
| M3 | Direct Job Costing Material Allocation & Inventory Integration | Features 12, 13, 14, 27: Requisitions, warehouse stock deduction via `InventoryService` under `BEGIN IMMEDIATE`, negative stock prevention (HTTP 422), balanced GL journals (Debit 5000 = Credit 1300), outbox event `ProjectMaterialIssued` | M1, M2 | DONE |
| M4 | Hybrid Progress Billing & Commercial Invoicing Integration | Features 18, 19, 20, 21, 22, 23, 24, 26, 28: Fixed Milestone & PoC billing, invoice generation `INV-YYYY-XXXXXX` via `InvoiceService`, tax snapshotting, GL journals (Debit 1200 = Credit 4000 + 2100), duplicate conflict guard (HTTP 409), outbox events `MilestoneCompleted` & `ProgressBilled` | M1, M2, M3 | DONE |
| M5 | Universal Vue 3 PWA Alignment & Outbox Event Processing | Features 25, 29, 30, 31, 32: `ProjectsView.vue`, `projects.js` Pinia store, `App.vue` navigation (preserving exact substring), outbox event `ProjectCreated` & processor at-least-once delivery, `npm run build` clean | M1, M2, M3, M4 | DONE |
| M6 | Final System Integration & Adversarial Hardening | Phase 1: 100% pass of E2E test suite (Tiers 1-4), update `e2e_tests/harness/server.py` (33->41 tables), cargo test, clippy clean, build clean. Phase 2: Adversarial coverage hardening (Tier 5) with Challengers & Forensic Auditor | Test, M1, M2, M3, M4, M5 | IN_PROGRESS |

---

## Interface Contracts

### M1 ↔ Repositories & Domain
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_number: String, // PRJ-YYYY-XXXXXX
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<Uuid>,
    pub customer_name: String,
    pub status: ProjectStatus, // Draft, Active, OnHold, Completed, Cancelled
    pub billing_type: BillingType, // Milestone, PercentageOfCompletion, TimeAndMaterials, Hybrid
    pub budget_amount: Rupiah, // integer i64
    pub contract_amount: Rupiah, // integer i64
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub actual_completion_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub sequence_order: i64,
    pub title: String,
    pub description: Option<String>,
    pub target_date: NaiveDate,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: MilestoneStatus, // Pending, InProgress, Completed, Cancelled
    pub billable_amount: Rupiah,
    pub is_billed: bool,
    pub invoice_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### M2 ↔ Costing Engine & Profitability
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

### M3 ↔ Inventory Deduction & GL Posting
```rust
// Issue Material Command:
// 1. BEGIN IMMEDIATE write lock
// 2. Assert stock_item.quantity_on_hand - requested >= 0 (else HTTP 422 INSUFFICIENT_STOCK)
// 3. Decrement stock_item.quantity_on_hand
// 4. Capture current WAC: unit_cost = stock_item.average_cost
// 5. Insert stock_movement (OUTBOUND, reference_type = "PROJECT_MATERIAL")
// 6. Post GL Journal via AccountingService:
//    Debit 5000 (Beban Pokok / Direct Project Cost) = quantity * unit_cost
//    Credit 1300 (Persediaan Barang Dagang)         = quantity * unit_cost
// 7. Insert OutboxEvent: ProjectMaterialIssued
```

### M4 ↔ Progress Billing & Invoicing
```rust
// Fixed Milestone Billing:
// Assert milestone.status == MilestoneStatus::Completed (else HTTP 422)
// Assert !milestone.is_billed (else HTTP 409 MILESTONE_ALREADY_BILLED)
// Issue invoice via InvoiceService::issue_invoice:
// - Sequential INV-YYYY-XXXXXX
// - Snapshot tax (PPN 11%, etc.)
// - Post GL Journal:
//   Debit 1200 (Piutang Usaha) = total_amount
//   Credit 4000 (Pendapatan Usaha) = net_revenue
//   Credit 2100 (Utang Pajak) = tax_amount
// - Create open Receivable
// - Set milestone.is_billed = 1, milestone.invoice_id = Some(invoice.id)
// - Insert OutboxEvent: ProgressBilled
```

---

## Code Layout

```text
backend/
├── migrations/
│   ├── 0001_initial_schema.sql through 0011_v4_1_inventory_management.sql (Preserved)
│   └── 0012_v4_1_project_management.sql      # [M1] 8 project domain tables, indexes, constraints
├── src/
│   ├── domain/
│   │   ├── project.rs                          # [M1] Project, Member, Milestone, Task, ProgressRecord
│   │   └── project_costing.rs                  # [M2] ProjectMaterial, ProjectLabor, ProjectExpense
│   ├── repository/
│   │   └── project_repo.rs                     # [M1, M2, M3, M4] SqlxProjectRepository with TenantContext
│   ├── service/
│   │   └── project_service.rs                  # [M1, M2, M3, M4] Projects, Costing, Stock issue, Progress billing
│   ├── api/
│   │   ├── router.rs                           # [M1] Mount /api/v1/projects routes
│   │   └── projects.rs                         # [M1, M2, M3, M4] Axum HTTP controller handlers
└── tests/
    ├── m1_persistence_tests.rs                 # [M1] Synchronized table count (45)
    ├── v4_m4_challenger_outbox_tests.rs        # [M1] Synchronized table count (45)
    └── v4_projects_management_tests.rs         # [M1-M4] Full integration test suite for R1-R5

frontend/
├── src/
│   ├── stores/
│   │   ├── workspace.js                        # [M5] Contractor capability matrix check
│   │   └── projects.js                         # [M5] Reactive Pinia project operational store
│   ├── services/
│   │   ├── api.js                              # [M5] Project REST API endpoints
│   │   └── mockData.js                         # [M5] Project mock datasets & offline support
│   ├── components/layout/
│   │   ├── DesktopSidebar.vue                  # [M5] Dynamic nav entry for "Proyek & Kontraktor"
│   │   └── MobileBottomNav.vue                 # [M5] Preserved Slot 3 POS button, adaptive Slot 4
│   ├── views/
│   │   └── ProjectsView.vue                    # [M5] Universal Vue 3 Projects & Job Costing view
│   └── App.vue                                 # [M5] Tab routing with exact literal string preservation

e2e_tests/
├── harness/
│   ├── client.py                               # [Test] Extended client with project API endpoints
│   └── server.py                               # [Test] Reference oracle with project support (41 tables)
├── tier1/test_tier1.py                         # [Test] F43-F52 coverage & table count sync (41)
├── tier2/test_tier2.py                         # [Test] B43-B52 corner cases & table count sync (41)
├── tier3/test_tier3.py                         # [Test] Cross-feature combination tests
├── tier4/test_tier4.py                         # [Test] Real-world contractor workflow scenarios
└── runner.sh                                   # Master TAP v13 test runner
```
