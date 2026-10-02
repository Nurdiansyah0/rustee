-- backend/migrations/0008_v4_1_invoicing_receivables.sql
-- Milestone 3: Commercial Invoicing, Receivables, & Payment Allocation (R3)
-- Features 16 (Lifecycle), 17 (Numbering), 18 (Snapshots), 19 (Receivables), 20 (Payments), 21 (Idempotency)

-- 1. Invoices Table
CREATE TABLE IF NOT EXISTS invoices (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_number TEXT,                                -- Server-assigned upon issue (NULL while DRAFT)
    status TEXT NOT NULL DEFAULT 'DRAFT' CHECK (status IN ('DRAFT', 'ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED')),
    customer_id TEXT,
    customer_name TEXT NOT NULL,
    customer_address TEXT,
    customer_email TEXT,
    issue_date TEXT,                                    -- ISO 8601 string assigned on issue
    due_date TEXT NOT NULL,                             -- ISO 8601 string
    currency TEXT NOT NULL DEFAULT 'IDR',
    subtotal INTEGER NOT NULL DEFAULT 0 CHECK (subtotal >= 0),
    discount INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
    tax_type TEXT NOT NULL DEFAULT 'PPN_11_EXCL',
    tax_amount INTEGER NOT NULL DEFAULT 0 CHECK (tax_amount >= 0),
    total_amount INTEGER NOT NULL DEFAULT 0 CHECK (total_amount >= 0),
    balance_due INTEGER NOT NULL DEFAULT 0 CHECK (balance_due >= 0),
    snapshot_json TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invoices_tenant ON invoices(tenant_id);
CREATE INDEX IF NOT EXISTS idx_invoices_tenant_status ON invoices(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_invoices_tenant_due_date ON invoices(tenant_id, due_date);
CREATE UNIQUE INDEX IF NOT EXISTS idx_invoices_tenant_number ON invoices(tenant_id, invoice_number) WHERE invoice_number IS NOT NULL;

-- 2. Invoice Items Table
CREATE TABLE IF NOT EXISTS invoice_items (
    id TEXT PRIMARY KEY NOT NULL,
    invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price INTEGER NOT NULL CHECK (unit_price >= 0),
    discount INTEGER NOT NULL DEFAULT 0 CHECK (discount >= 0),
    tax_amount INTEGER NOT NULL DEFAULT 0 CHECK (tax_amount >= 0),
    line_total INTEGER NOT NULL CHECK (line_total >= 0),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invoice_items_invoice ON invoice_items(invoice_id);
CREATE INDEX IF NOT EXISTS idx_invoice_items_tenant ON invoice_items(tenant_id);

-- 3. Invoice Snapshots Table (Frozen Historical Document Preservation)
CREATE TABLE IF NOT EXISTS invoice_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    snapshot_json TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    CONSTRAINT uq_invoice_snapshots_invoice UNIQUE (invoice_id)
);

CREATE INDEX IF NOT EXISTS idx_invoice_snapshots_invoice ON invoice_snapshots(invoice_id);
CREATE INDEX IF NOT EXISTS idx_invoice_snapshots_tenant ON invoice_snapshots(tenant_id);

-- 4. Receivables Relational Model
CREATE TABLE IF NOT EXISTS receivables (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_id TEXT NOT NULL UNIQUE REFERENCES invoices(id) ON DELETE CASCADE,
    total_amount INTEGER NOT NULL CHECK (total_amount >= 0),
    allocated_amount INTEGER NOT NULL DEFAULT 0 CHECK (allocated_amount >= 0),
    outstanding_amount INTEGER NOT NULL CHECK (outstanding_amount >= 0),
    due_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN', 'PARTIALLY_PAID', 'PAID', 'VOIDED', 'OVERDUE', 'WRITTEN_OFF')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_receivables_tenant ON receivables(tenant_id);
CREATE INDEX IF NOT EXISTS idx_receivables_tenant_status ON receivables(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_receivables_tenant_due ON receivables(tenant_id, due_date);
CREATE INDEX IF NOT EXISTS idx_receivables_invoice ON receivables(invoice_id);

-- 5. Payments Table
CREATE TABLE IF NOT EXISTS payments (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    receivable_id TEXT NOT NULL REFERENCES receivables(id) ON DELETE CASCADE,
    payment_number TEXT NOT NULL,                       -- PAY-YYYY-XXXXXX
    payment_date TEXT NOT NULL,
    amount INTEGER NOT NULL CHECK (amount > 0),
    payment_method TEXT NOT NULL DEFAULT 'BANK_TRANSFER' CHECK (payment_method IN ('CASH', 'BANK_TRANSFER', 'QRIS', 'PAYMENT_GATEWAY', 'MANUAL')),
    reference TEXT,
    status TEXT NOT NULL DEFAULT 'CONFIRMED' CHECK (status IN ('PENDING', 'CONFIRMED', 'FAILED')),
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_payments_tenant_number UNIQUE (tenant_id, payment_number)
);

CREATE INDEX IF NOT EXISTS idx_payments_tenant ON payments(tenant_id);
CREATE INDEX IF NOT EXISTS idx_payments_tenant_date ON payments(tenant_id, payment_date);
CREATE INDEX IF NOT EXISTS idx_payments_number ON payments(tenant_id, payment_number);
CREATE INDEX IF NOT EXISTS idx_payments_invoice ON payments(invoice_id);
CREATE INDEX IF NOT EXISTS idx_payments_receivable ON payments(receivable_id);

-- 6. Payment Allocations Table
CREATE TABLE IF NOT EXISTS payment_allocations (
    id TEXT PRIMARY KEY NOT NULL,
    payment_id TEXT NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    amount INTEGER NOT NULL CHECK (amount > 0),
    allocated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_payment_allocations_payment ON payment_allocations(payment_id);
CREATE INDEX IF NOT EXISTS idx_payment_allocations_invoice ON payment_allocations(invoice_id);
CREATE INDEX IF NOT EXISTS idx_payment_allocations_tenant ON payment_allocations(tenant_id);

-- 7. Upgrade Idempotency Keys Table for Multi-Tenancy & Path Tracking
ALTER TABLE idempotency_keys ADD COLUMN tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE idempotency_keys ADD COLUMN request_path TEXT;
ALTER TABLE idempotency_keys ADD COLUMN response_status INTEGER;

CREATE INDEX IF NOT EXISTS idx_idempotency_tenant_key ON idempotency_keys(tenant_id, idempotency_key);

-- 8. Immutability Triggers

-- Trigger 8.1: Prevent mutation of financial terms on issued, paid, or voided invoices
CREATE TRIGGER IF NOT EXISTS trg_prevent_issued_invoice_financial_mutation
BEFORE UPDATE ON invoices
FOR EACH ROW
WHEN OLD.status IN ('ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED') AND (
    OLD.tenant_id != NEW.tenant_id OR
    OLD.invoice_number IS NOT NEW.invoice_number OR
    OLD.issue_date IS NOT NEW.issue_date OR
    OLD.currency != NEW.currency OR
    OLD.customer_name != NEW.customer_name OR
    OLD.customer_address IS NOT NEW.customer_address OR
    OLD.customer_email IS NOT NEW.customer_email OR
    OLD.subtotal != NEW.subtotal OR
    OLD.discount != NEW.discount OR
    OLD.tax_type != NEW.tax_type OR
    OLD.tax_amount != NEW.tax_amount OR
    OLD.total_amount != NEW.total_amount OR
    NEW.status = 'DRAFT' OR
    (OLD.status = 'PARTIALLY_PAID' AND NEW.status NOT IN ('PARTIALLY_PAID', 'PAID')) OR
    (OLD.status = 'PAID' AND NEW.status != 'PAID') OR
    (OLD.status = 'VOIDED' AND NEW.status != 'VOIDED')
)
BEGIN
    SELECT RAISE(ABORT, 'Issued, paid, or voided invoices cannot have financial terms modified or invalid status transitions');
END;

-- Trigger 8.2: Prevent deletion of issued or settled invoices
CREATE TRIGGER IF NOT EXISTS trg_prevent_issued_invoice_delete
BEFORE DELETE ON invoices
FOR EACH ROW
WHEN OLD.status IN ('ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED')
BEGIN
    SELECT RAISE(ABORT, 'Non-draft invoices cannot be deleted; issued invoices must be voided via reversal workflow');
END;

-- Trigger 8.2b: Prevent adding line items to non-draft invoices
CREATE TRIGGER IF NOT EXISTS trg_invoice_items_prevent_insert
BEFORE INSERT ON invoice_items
FOR EACH ROW
WHEN (SELECT status FROM invoices WHERE id = NEW.invoice_id) IN ('ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED')
BEGIN
    SELECT RAISE(ABORT, 'Line items cannot be added to issued or finalized invoices');
END;

-- Trigger 8.3: Prevent line item updates on non-draft invoices
CREATE TRIGGER IF NOT EXISTS trg_invoice_items_prevent_update
BEFORE UPDATE ON invoice_items
FOR EACH ROW
WHEN (SELECT status FROM invoices WHERE id = OLD.invoice_id) IN ('ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED')
   OR (SELECT status FROM invoices WHERE id = NEW.invoice_id) IN ('ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED')
BEGIN
    SELECT RAISE(ABORT, 'Line items of issued or finalized invoices cannot be modified');
END;

-- Trigger 8.4: Prevent line item deletion on non-draft invoices
CREATE TRIGGER IF NOT EXISTS trg_invoice_items_prevent_delete
BEFORE DELETE ON invoice_items
FOR EACH ROW
WHEN (SELECT status FROM invoices WHERE id = OLD.invoice_id) IN ('ISSUED', 'PARTIALLY_PAID', 'PAID', 'VOIDED')
BEGIN
    SELECT RAISE(ABORT, 'Line items of issued or finalized invoices cannot be deleted');
END;

-- Trigger 8.5: Invoice snapshots are strictly append-only (No UPDATE)
CREATE TRIGGER IF NOT EXISTS trg_protect_snapshot_update
BEFORE UPDATE ON invoice_snapshots
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Invoice snapshots are strictly immutable and cannot be updated');
END;

-- Trigger 8.6: Invoice snapshots cannot be deleted directly
CREATE TRIGGER IF NOT EXISTS trg_protect_snapshot_delete
BEFORE DELETE ON invoice_snapshots
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Invoice snapshots are strictly immutable and cannot be deleted');
END;

-- Trigger 8.7: Confirmed payments cannot be deleted
CREATE TRIGGER IF NOT EXISTS trg_prevent_confirmed_payment_delete
BEFORE DELETE ON payments
FOR EACH ROW
WHEN OLD.status = 'CONFIRMED'
BEGIN
    SELECT RAISE(ABORT, 'Confirmed payments cannot be deleted; corrections require reversal journal');
END;

-- Trigger 8.8: Confirmed payments cannot alter monetary amount, date, status, or relations
CREATE TRIGGER IF NOT EXISTS trg_prevent_confirmed_payment_update
BEFORE UPDATE ON payments
FOR EACH ROW
WHEN OLD.status = 'CONFIRMED' AND (
    OLD.amount != NEW.amount OR
    OLD.payment_date != NEW.payment_date OR
    OLD.tenant_id != NEW.tenant_id OR
    OLD.payment_number != NEW.payment_number OR
    OLD.status != NEW.status OR
    OLD.invoice_id != NEW.invoice_id OR
    OLD.receivable_id != NEW.receivable_id
)
BEGIN
    SELECT RAISE(ABORT, 'Confirmed payments cannot have monetary, status, or structural attributes modified');
END;

-- Trigger 8.9: Payment allocations cannot be updated or deleted
CREATE TRIGGER IF NOT EXISTS trg_prevent_payment_allocation_update
BEFORE UPDATE ON payment_allocations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Payment allocations are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_prevent_payment_allocation_delete
BEFORE DELETE ON payment_allocations
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Payment allocations cannot be deleted directly');
END;
