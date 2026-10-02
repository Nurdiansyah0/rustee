-- backend/migrations/0009_v4_1_transactional_outbox.sql
-- Milestone 4: Transactional Outbox Pattern & Sidecar Boundary (R4, Features 22-25)
-- Provides durable transactional outbox mechanism committed atomically with domain mutations.

-- 1. Outbox Events Table
CREATE TABLE IF NOT EXISTS outbox_events (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,                                       -- e.g. 'InvoiceIssued', 'InvoiceVoided', 'PaymentConfirmed', 'JournalPosted'
    aggregate_type TEXT NOT NULL,                                   -- e.g. 'Invoice', 'Payment', 'Journal'
    aggregate_id TEXT NOT NULL,                                     -- Target aggregate entity ID
    payload_json TEXT NOT NULL,                                     -- Structured JSON event payload
    status TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PROCESSING', 'PUBLISHED', 'FAILED', 'DEAD_LETTER')),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_retries INTEGER NOT NULL DEFAULT 5 CHECK (max_retries >= 1),
    next_retry_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_error TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    published_at TEXT
);

-- 2. Performance & Dispatch Indexes
-- Fast polling index for claiming pending/retryable events due for dispatch
CREATE INDEX IF NOT EXISTS idx_outbox_events_status_retry 
    ON outbox_events(status, next_retry_at);

-- Tenant chronological event stream index
CREATE INDEX IF NOT EXISTS idx_outbox_events_tenant_created 
    ON outbox_events(tenant_id, created_at);

-- Consumer deduplication & entity audit trail index
CREATE INDEX IF NOT EXISTS idx_outbox_events_aggregate 
    ON outbox_events(tenant_id, aggregate_type, aggregate_id);

-- Tenant status filtering index
CREATE INDEX IF NOT EXISTS idx_outbox_events_tenant_status 
    ON outbox_events(tenant_id, status);

-- 3. Immutability & Lifecycle Triggers

-- Trigger 3.1: Prevent modification of immutable identity, aggregate, and payload attributes
CREATE TRIGGER IF NOT EXISTS trg_outbox_events_prevent_immutable_update
BEFORE UPDATE ON outbox_events
FOR EACH ROW
WHEN (
    OLD.id IS NOT NEW.id OR
    OLD.tenant_id != NEW.tenant_id OR
    OLD.event_type != NEW.event_type OR
    OLD.aggregate_type != NEW.aggregate_type OR
    OLD.aggregate_id != NEW.aggregate_id OR
    OLD.payload_json != NEW.payload_json OR
    OLD.created_at != NEW.created_at
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox event identity and payload fields are strictly immutable');
END;

-- Trigger 3.2: Prevent status regression once an event has been published
CREATE TRIGGER IF NOT EXISTS trg_outbox_events_prevent_published_update
BEFORE UPDATE ON outbox_events
FOR EACH ROW
WHEN OLD.status = 'PUBLISHED' AND NEW.status != 'PUBLISHED'
BEGIN
    SELECT RAISE(ABORT, 'Published outbox events cannot transition back to another status');
END;

-- Trigger 3.3: Prevent deletion of active or published outbox events
CREATE TRIGGER IF NOT EXISTS trg_outbox_events_prevent_delete
BEFORE DELETE ON outbox_events
FOR EACH ROW
WHEN OLD.status IN ('PENDING', 'PROCESSING', 'PUBLISHED')
BEGIN
    SELECT RAISE(ABORT, 'Active or published outbox events are immutable and cannot be deleted');
END;
