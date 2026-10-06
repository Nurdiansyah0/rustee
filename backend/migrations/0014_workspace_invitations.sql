-- 0014: Workspace Invitations
-- Enables Owner/Admin to invite staff via a one-time secure link.
-- Invited user registers with minimal profile (name + password only) and
-- is automatically added to the workspace with the pre-assigned role,
-- bypassing business onboarding entirely.

CREATE TABLE IF NOT EXISTS workspace_invitations (
    id          TEXT PRIMARY KEY,                -- inv_{uuid}
    tenant_id   TEXT NOT NULL,
    invited_by  TEXT NOT NULL,                   -- user_id of inviter (Owner/Admin)
    email       TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'staff',   -- administrator|manager|staff|accountant
    token       TEXT NOT NULL UNIQUE,            -- secure random hex token (32 bytes)
    status      TEXT NOT NULL DEFAULT 'pending', -- pending|accepted|expired|revoked
    expires_at  TEXT NOT NULL,                   -- RFC-3339, default 7 days
    accepted_at TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,

    FOREIGN KEY (tenant_id)  REFERENCES tenants(id)  ON DELETE CASCADE,
    FOREIGN KEY (invited_by) REFERENCES users(id)    ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_invitations_token     ON workspace_invitations(token);
CREATE INDEX IF NOT EXISTS idx_invitations_email     ON workspace_invitations(LOWER(email));
CREATE INDEX IF NOT EXISTS idx_invitations_tenant_id ON workspace_invitations(tenant_id);

-- Track account_type on users so we know if they are a business owner or
-- a pure staff account that joined only via invitation.
ALTER TABLE users ADD COLUMN account_type TEXT NOT NULL DEFAULT 'owner';
-- 'owner' = went through business onboarding, can create workspaces
-- 'staff' = joined only via invitation, cannot create business workspaces
