-- backend/migrations/0012_v4_1_project_management.sql
-- Milestone 1: Schema & Persistence Foundation for Phase 3 Project & Contractor Operations
-- Creates 8 domain tables:
--   1. projects
--   2. project_members
--   3. milestones
--   4. tasks
--   5. progress_records
--   6. project_materials
--   7. project_labor
--   8. project_expenses
-- Expands domain table count from 37 to 45.

-- ============================================================================
-- 1. Projects Table (Master Project Entity & Budget Tracking)
-- ============================================================================
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_number TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    customer_id TEXT,
    customer_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'DRAFT' CHECK (status IN ('DRAFT', 'ACTIVE', 'ON_HOLD', 'COMPLETED', 'CANCELLED')),
    billing_type TEXT NOT NULL DEFAULT 'MILESTONE' CHECK (billing_type IN ('MILESTONE', 'PERCENTAGE_OF_COMPLETION', 'TIME_AND_MATERIALS', 'HYBRID')),
    budget_amount INTEGER NOT NULL DEFAULT 0 CHECK (budget_amount >= 0),
    contract_amount INTEGER NOT NULL DEFAULT 0 CHECK (contract_amount >= 0),
    start_date TEXT,
    end_date TEXT,
    actual_completion_date TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_projects_tenant_number UNIQUE (tenant_id, project_number)
);

CREATE INDEX IF NOT EXISTS idx_projects_tenant ON projects(tenant_id);
CREATE INDEX IF NOT EXISTS idx_projects_tenant_number ON projects(tenant_id, project_number);
CREATE INDEX IF NOT EXISTS idx_projects_status ON projects(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_projects_customer ON projects(tenant_id, customer_id);
CREATE INDEX IF NOT EXISTS idx_projects_created ON projects(tenant_id, created_at);

-- ============================================================================
-- 2. Project Members Table (Team Members & Labor Rate Allocation)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_members (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'CONTRIBUTOR',
    cost_rate INTEGER NOT NULL DEFAULT 0 CHECK (cost_rate >= 0),
    billing_rate INTEGER NOT NULL DEFAULT 0 CHECK (billing_rate >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_project_members_tenant_project_user UNIQUE (tenant_id, project_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_project_members_tenant ON project_members(tenant_id);
CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_user ON project_members(user_id);
CREATE INDEX IF NOT EXISTS idx_project_members_lookup ON project_members(tenant_id, project_id, user_id);

-- ============================================================================
-- 3. Milestones Table (Milestone State Machine & Billing Targets)
-- ============================================================================
CREATE TABLE IF NOT EXISTS milestones (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    sequence_order INTEGER NOT NULL DEFAULT 1 CHECK (sequence_order > 0),
    title TEXT NOT NULL,
    description TEXT,
    target_date TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'IN_PROGRESS', 'COMPLETED', 'CANCELLED')),
    billable_amount INTEGER NOT NULL DEFAULT 0 CHECK (billable_amount >= 0),
    is_billed INTEGER NOT NULL DEFAULT 0 CHECK (is_billed IN (0, 1)),
    invoice_id TEXT REFERENCES invoices(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_milestones_tenant ON milestones(tenant_id);
CREATE INDEX IF NOT EXISTS idx_milestones_project ON milestones(tenant_id, project_id);
CREATE INDEX IF NOT EXISTS idx_milestones_status ON milestones(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_milestones_invoice ON milestones(tenant_id, invoice_id);
CREATE INDEX IF NOT EXISTS idx_milestones_sequence ON milestones(project_id, sequence_order);

-- ============================================================================
-- 4. Tasks Table (Granular Work Packages)
-- ============================================================================
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    milestone_id TEXT REFERENCES milestones(id) ON DELETE SET NULL,
    assignee_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'TODO' CHECK (status IN ('TODO', 'IN_PROGRESS', 'BLOCKED', 'DONE', 'CANCELLED')),
    priority TEXT NOT NULL DEFAULT 'MEDIUM' CHECK (priority IN ('LOW', 'MEDIUM', 'HIGH', 'URGENT')),
    estimated_hours INTEGER NOT NULL DEFAULT 0 CHECK (estimated_hours >= 0),
    actual_hours INTEGER NOT NULL DEFAULT 0 CHECK (actual_hours >= 0),
    due_date TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_tenant ON tasks(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(tenant_id, project_id);
CREATE INDEX IF NOT EXISTS idx_tasks_milestone ON tasks(milestone_id);
CREATE INDEX IF NOT EXISTS idx_tasks_assignee ON tasks(assignee_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(tenant_id, priority);

-- ============================================================================
-- 5. Progress Records Table (Verified Physical Progress & PoC Audits)
-- ============================================================================
CREATE TABLE IF NOT EXISTS progress_records (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    milestone_id TEXT REFERENCES milestones(id) ON DELETE SET NULL,
    percentage INTEGER NOT NULL CHECK (percentage >= 0 AND percentage <= 100),
    record_date TEXT NOT NULL,
    verified_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    notes TEXT,
    evidence_url TEXT,
    is_billed INTEGER NOT NULL DEFAULT 0 CHECK (is_billed IN (0, 1)),
    invoice_id TEXT REFERENCES invoices(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_progress_records_tenant ON progress_records(tenant_id);
CREATE INDEX IF NOT EXISTS idx_progress_records_project ON progress_records(tenant_id, project_id);
CREATE INDEX IF NOT EXISTS idx_progress_records_milestone ON progress_records(milestone_id);
CREATE INDEX IF NOT EXISTS idx_progress_records_date ON progress_records(tenant_id, record_date);
CREATE INDEX IF NOT EXISTS idx_progress_records_invoice ON progress_records(tenant_id, invoice_id);

-- ============================================================================
-- 6. Project Materials Table (Job Costing Material Requisitions & Allocation)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_materials (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    warehouse_id TEXT NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    quantity_planned INTEGER NOT NULL DEFAULT 0 CHECK (quantity_planned >= 0),
    quantity_issued INTEGER NOT NULL DEFAULT 0 CHECK (quantity_issued >= 0),
    unit_cost INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
    total_cost INTEGER NOT NULL DEFAULT 0 CHECK (total_cost >= 0),
    status TEXT NOT NULL DEFAULT 'PLANNED' CHECK (status IN ('PLANNED', 'ISSUED', 'RETURNED', 'CANCELLED')),
    is_billable INTEGER NOT NULL DEFAULT 1 CHECK (is_billable IN (0, 1)),
    stock_movement_id TEXT REFERENCES stock_movements(id) ON DELETE SET NULL,
    journal_entry_id TEXT REFERENCES journal_entries(id) ON DELETE SET NULL,
    issued_at TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_materials_tenant ON project_materials(tenant_id);
CREATE INDEX IF NOT EXISTS idx_project_materials_project ON project_materials(tenant_id, project_id);
CREATE INDEX IF NOT EXISTS idx_project_materials_task ON project_materials(task_id);
CREATE INDEX IF NOT EXISTS idx_project_materials_product ON project_materials(tenant_id, product_id);
CREATE INDEX IF NOT EXISTS idx_project_materials_warehouse ON project_materials(tenant_id, warehouse_id);
CREATE INDEX IF NOT EXISTS idx_project_materials_status ON project_materials(tenant_id, status);

-- ============================================================================
-- 7. Project Labor Table (Direct Labor Cost Logging)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_labor (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    worker_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    worker_name TEXT NOT NULL,
    work_date TEXT NOT NULL,
    hours_worked INTEGER NOT NULL CHECK (hours_worked > 0),
    hourly_rate INTEGER NOT NULL DEFAULT 0 CHECK (hourly_rate >= 0),
    total_cost INTEGER NOT NULL DEFAULT 0 CHECK (total_cost >= 0),
    billing_rate INTEGER NOT NULL DEFAULT 0 CHECK (billing_rate >= 0),
    is_billable INTEGER NOT NULL DEFAULT 1 CHECK (is_billable IN (0, 1)),
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_labor_tenant ON project_labor(tenant_id);
CREATE INDEX IF NOT EXISTS idx_project_labor_project ON project_labor(tenant_id, project_id);
CREATE INDEX IF NOT EXISTS idx_project_labor_task ON project_labor(task_id);
CREATE INDEX IF NOT EXISTS idx_project_labor_worker ON project_labor(tenant_id, worker_id);
CREATE INDEX IF NOT EXISTS idx_project_labor_date ON project_labor(tenant_id, work_date);

-- ============================================================================
-- 8. Project Expenses Table (Third-Party Direct Project Expenses)
-- ============================================================================
CREATE TABLE IF NOT EXISTS project_expenses (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    category TEXT NOT NULL,
    description TEXT NOT NULL,
    amount INTEGER NOT NULL CHECK (amount > 0),
    expense_date TEXT NOT NULL,
    vendor_name TEXT,
    receipt_ref TEXT,
    is_billable INTEGER NOT NULL DEFAULT 1 CHECK (is_billable IN (0, 1)),
    journal_entry_id TEXT REFERENCES journal_entries(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_expenses_tenant ON project_expenses(tenant_id);
CREATE INDEX IF NOT EXISTS idx_project_expenses_project ON project_expenses(tenant_id, project_id);
CREATE INDEX IF NOT EXISTS idx_project_expenses_task ON project_expenses(task_id);
CREATE INDEX IF NOT EXISTS idx_project_expenses_category ON project_expenses(tenant_id, category);
CREATE INDEX IF NOT EXISTS idx_project_expenses_date ON project_expenses(tenant_id, expense_date);
