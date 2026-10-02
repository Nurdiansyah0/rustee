-- backend/migrations/0006_v4_1_core_foundation.sql
-- Milestone 1: Multi-Tenant Architecture & Identity Boundaries (R1)
-- Introduces tenants, business_profiles, and memberships tables,
-- and automatically backfills all existing users with a default personal workspace.

-- 1. Tenants Table
CREATE TABLE tenants (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'SUSPENDED', 'ARCHIVED')),
    is_personal INTEGER NOT NULL DEFAULT 0 CHECK (is_personal IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_tenants_slug UNIQUE (slug)
);

CREATE UNIQUE INDEX idx_tenants_slug ON tenants(slug);
CREATE INDEX idx_tenants_status ON tenants(status);

-- 2. Business Profiles Table (Indonesian Localization Defaults)
CREATE TABLE business_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    business_name TEXT NOT NULL,
    legal_name TEXT,
    tax_id TEXT,                                      -- NPWP / NIK
    address TEXT,
    phone TEXT,
    email TEXT,
    timezone TEXT NOT NULL DEFAULT 'Asia/Jakarta',    -- WIB default (Asia/Jakarta, Asia/Makassar, Asia/Jayapura)
    currency TEXT NOT NULL DEFAULT 'IDR',            -- Indonesian Rupiah
    locale TEXT NOT NULL DEFAULT 'id-ID',            -- Indonesian locale default
    invoice_prefix TEXT NOT NULL DEFAULT 'INV',
    business_type TEXT NOT NULL DEFAULT 'general',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_business_profiles_tenant UNIQUE (tenant_id)
);

CREATE UNIQUE INDEX idx_business_profiles_tenant ON business_profiles(tenant_id);

-- 3. Memberships Table (Multi-User RBAC & Workspace Membership)
CREATE TABLE memberships (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'owner',
    status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'INVITED', 'SUSPENDED')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_membership_tenant_user UNIQUE (tenant_id, user_id)
);

CREATE INDEX idx_memberships_user ON memberships(user_id);
CREATE INDEX idx_memberships_tenant ON memberships(tenant_id);
CREATE INDEX idx_memberships_user_status ON memberships(user_id, status);

-- 4. Automatic Backfill: Provision Default Personal Workspaces for Existing Users
-- Generates correlated RFC 4122 v4 UUIDs for all existing users lacking memberships
CREATE TEMP TABLE IF NOT EXISTS _migration_user_workspaces AS
SELECT 
    u.id AS user_id,
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS tenant_id,
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS profile_id,
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS membership_id,
    'personal-' || lower(hex(randomblob(4))) AS slug,
    u.display_name,
    u.created_at,
    u.updated_at
FROM users u
WHERE NOT EXISTS (
    SELECT 1 FROM memberships m WHERE m.user_id = u.id
);

INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at)
SELECT tenant_id, display_name || '''s Workspace', slug, 'ACTIVE', 1, created_at, updated_at
FROM _migration_user_workspaces;

INSERT INTO business_profiles (id, tenant_id, business_name, legal_name, timezone, currency, locale, invoice_prefix, business_type, created_at, updated_at)
SELECT profile_id, tenant_id, display_name, display_name, 'Asia/Jakarta', 'IDR', 'id-ID', 'INV', 'personal', created_at, updated_at
FROM _migration_user_workspaces;

INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at)
SELECT membership_id, tenant_id, user_id, 'owner', 'ACTIVE', created_at, updated_at
FROM _migration_user_workspaces;

DROP TABLE IF EXISTS _migration_user_workspaces;
