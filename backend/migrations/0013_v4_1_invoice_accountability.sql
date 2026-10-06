-- backend/migrations/0013_v4_1_invoice_accountability.sql
-- Invoicing Accountability and Audit Tracking
-- Adds created_by, issued_by, and authorized_by to invoices table

ALTER TABLE invoices ADD COLUMN created_by TEXT REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE invoices ADD COLUMN issued_by TEXT REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE invoices ADD COLUMN authorized_by TEXT;

CREATE INDEX IF NOT EXISTS idx_invoices_tenant_created_by ON invoices(tenant_id, created_by);
CREATE INDEX IF NOT EXISTS idx_invoices_tenant_issued_by ON invoices(tenant_id, issued_by);
