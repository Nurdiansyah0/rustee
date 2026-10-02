-- backend/migrations/0007_v4_1_accounting_core.sql
-- Milestone 2: Double-Entry Accounting Core & Financial State Machine (R2)

-- 1. Chart of Accounts Table
CREATE TABLE chart_of_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    account_type TEXT NOT NULL CHECK (account_type IN ('asset', 'liability', 'income', 'expense', 'equity')),
    is_system INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_coa_tenant_code UNIQUE (tenant_id, code)
);

CREATE INDEX idx_coa_tenant ON chart_of_accounts(tenant_id);
CREATE INDEX idx_coa_tenant_code ON chart_of_accounts(tenant_id, code);
CREATE INDEX idx_coa_tenant_type ON chart_of_accounts(tenant_id, account_type);

-- 2. Journal Entries Table
CREATE TABLE journal_entries (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    entry_number TEXT NOT NULL,
    entry_date TEXT NOT NULL,
    description TEXT NOT NULL,
    source_type TEXT NOT NULL DEFAULT 'MANUAL' CHECK (source_type IN ('MANUAL', 'INVOICE', 'PAYMENT', 'REVERSAL', 'SYSTEM')),
    source_id TEXT,
    status TEXT NOT NULL DEFAULT 'POSTED' CHECK (status IN ('DRAFT', 'POSTED', 'ARCHIVED')),
    is_reversed INTEGER NOT NULL DEFAULT 0 CHECK (is_reversed IN (0, 1)),
    reversal_entry_id TEXT REFERENCES journal_entries(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_journal_tenant_entry_number UNIQUE (tenant_id, entry_number)
);

CREATE INDEX idx_journals_tenant ON journal_entries(tenant_id);
CREATE INDEX idx_journals_tenant_date ON journal_entries(tenant_id, entry_date);
CREATE INDEX idx_journals_tenant_entry_number ON journal_entries(tenant_id, entry_number);
CREATE INDEX idx_journals_source ON journal_entries(source_type, source_id);

-- 3. Journal Lines Table
CREATE TABLE journal_lines (
    id TEXT PRIMARY KEY NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    account_code TEXT NOT NULL,
    debit INTEGER NOT NULL DEFAULT 0 CHECK (debit >= 0),
    credit INTEGER NOT NULL DEFAULT 0 CHECK (credit >= 0),
    memo TEXT,
    created_at TEXT NOT NULL,
    CONSTRAINT chk_journal_line_nonzero CHECK ((debit > 0 AND credit = 0) OR (credit > 0 AND debit = 0) OR (debit = 0 AND credit = 0))
);

CREATE INDEX idx_journal_lines_journal ON journal_lines(journal_id);
CREATE INDEX idx_journal_lines_tenant_account ON journal_lines(tenant_id, account_code);

-- 4. Tax Rules Table
CREATE TABLE tax_rules (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    tax_type TEXT NOT NULL,
    name TEXT NOT NULL,
    rate_basis_points INTEGER NOT NULL DEFAULT 0,
    is_inclusive INTEGER NOT NULL DEFAULT 0 CHECK (is_inclusive IN (0, 1)),
    is_system INTEGER NOT NULL DEFAULT 1 CHECK (is_system IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_tax_rules_tenant_type UNIQUE (tenant_id, tax_type)
);

CREATE INDEX idx_tax_rules_tenant ON tax_rules(tenant_id);

-- 5. Immutability Triggers for Posted Journals and Lines
CREATE TRIGGER IF NOT EXISTS trg_prevent_posted_journal_delete
BEFORE DELETE ON journal_entries
FOR EACH ROW
WHEN OLD.status = 'POSTED'
BEGIN
    SELECT RAISE(ABORT, 'Posted journals are immutable and cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_prevent_posted_journal_update
BEFORE UPDATE ON journal_entries
FOR EACH ROW
WHEN OLD.status = 'POSTED' AND (
    OLD.entry_number != NEW.entry_number OR
    OLD.entry_date != NEW.entry_date OR
    OLD.tenant_id != NEW.tenant_id OR
    OLD.source_type != NEW.source_type OR
    OLD.source_id IS NOT NEW.source_id OR
    OLD.description != NEW.description OR
    OLD.status != NEW.status OR
    OLD.id != NEW.id OR
    OLD.created_at != NEW.created_at OR
    (OLD.is_reversed = 1 AND NEW.is_reversed != 1) OR
    (OLD.reversal_entry_id IS NOT NULL AND NEW.reversal_entry_id IS NOT OLD.reversal_entry_id)
)
BEGIN
    SELECT RAISE(ABORT, 'Posted journals are immutable and cannot be modified');
END;

CREATE TRIGGER IF NOT EXISTS trg_journal_lines_prevent_update
BEFORE UPDATE ON journal_lines
FOR EACH ROW
WHEN (SELECT status FROM journal_entries WHERE id = OLD.journal_id) = 'POSTED'
   OR (SELECT status FROM journal_entries WHERE id = NEW.journal_id) = 'POSTED'
BEGIN
    SELECT RAISE(ABORT, 'Journal lines of posted journals cannot be modified');
END;

CREATE TRIGGER IF NOT EXISTS trg_journal_lines_prevent_delete
BEFORE DELETE ON journal_lines
FOR EACH ROW
WHEN (SELECT status FROM journal_entries WHERE id = OLD.journal_id) = 'POSTED'
BEGIN
    SELECT RAISE(ABORT, 'Journal lines of posted journals cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_protect_system_accounts
BEFORE DELETE ON chart_of_accounts
FOR EACH ROW
WHEN OLD.is_system = 1
BEGIN
    SELECT RAISE(ABORT, 'System accounts are protected and cannot be deleted');
END;

-- 6. Automatic Backfill: Seed Canonical 8 Chart of Accounts for All Existing Tenants
INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '1000', 'Kas', 'asset', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '1100', 'Bank', 'asset', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '1200', 'Piutang Usaha', 'asset', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '2000', 'Utang Usaha', 'liability', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '2100', 'Utang Pajak (PPN/PPh)', 'liability', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '4000', 'Pendapatan Usaha', 'income', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '5000', 'Beban Pokok Penjualan', 'expense', 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '6000', 'Beban Operasional', 'expense', 1, t.created_at, t.created_at
FROM tenants t;

-- 7. Automatic Backfill: Seed Default Tax Rules for All Existing Tenants
INSERT INTO tax_rules (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, 'PPN_11_EXCL', 'PPN 11% Eksklusif', 1100, 0, 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO tax_rules (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, 'PPN_11_INCL', 'PPN 11% Inklusif', 1100, 1, 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO tax_rules (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, 'PPN_12_EXCL', 'PPN 12% Eksklusif', 1200, 0, 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO tax_rules (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, 'PPN_12_INCL', 'PPN 12% Inklusif', 1200, 1, 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO tax_rules (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, 'UMKM_05', 'PPh Final UMKM 0.5%', 50, 0, 1, t.created_at, t.created_at
FROM tenants t;

INSERT INTO tax_rules (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, 'EXEMPT', 'Bebas Pajak (0%)', 0, 0, 1, t.created_at, t.created_at
FROM tenants t;
