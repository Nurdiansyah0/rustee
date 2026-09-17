-- backend/migrations/0003_v3_1_0_schema_upgrade.sql
-- Invinite Personal Finance Master Specification v3.1.0 Schema Upgrade
-- Implements custom vocabulary, transaction ingestion signals, and support tables.

-- 1. Upgrade Categories for User-Owned Vocabulary (§4, §5)
ALTER TABLE categories ADD COLUMN display_name TEXT;
ALTER TABLE categories ADD COLUMN normalized_name TEXT;
ALTER TABLE categories ADD COLUMN metadata TEXT;

-- Backfill existing categories: display_name = name, normalized_name = LOWER(name)
UPDATE categories 
SET display_name = name, 
    normalized_name = LOWER(name) 
WHERE display_name IS NULL;

CREATE INDEX idx_categories_user_norm ON categories(user_id, normalized_name);

-- 2. Upgrade Transactions for Ingestion Signals (§24)
ALTER TABLE transactions ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE transactions ADD COLUMN external_reference TEXT;
ALTER TABLE transactions ADD COLUMN merchant TEXT;
ALTER TABLE transactions ADD COLUMN confidence TEXT NOT NULL DEFAULT 'HIGH';
ALTER TABLE transactions ADD COLUMN ingestion_id TEXT;

CREATE INDEX idx_transactions_external_ref ON transactions(external_reference);
CREATE INDEX idx_transactions_ingestion ON transactions(ingestion_id);

-- 3. Ingestion Events Table (§16, §20)
CREATE TABLE ingestion_events (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (source IN ('notification', 'sms', 'gmail', 'manual')),
    raw_payload_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'auto_created', 'requires_confirmation', 'rejected', 'duplicate')),
    confidence TEXT NOT NULL CHECK (confidence IN ('HIGH', 'MEDIUM', 'LOW')),
    parsed_candidate TEXT,
    created_at TEXT NOT NULL,
    processed_at TEXT
);

CREATE INDEX idx_ingestion_user_status ON ingestion_events(user_id, status);
CREATE INDEX idx_ingestion_user_hash ON ingestion_events(user_id, raw_payload_hash);

-- 4. Subscription Events Table (§16)
CREATE TABLE subscription_events (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    provider TEXT NOT NULL,
    event_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_subscription_events_user ON subscription_events(user_id);
CREATE INDEX idx_subscription_events_provider_event ON subscription_events(provider, event_id);

-- 5. Device Installations Table (§16, §25, §28)
CREATE TABLE device_installations (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('android_webview', 'pwa_web', 'ios_safari')),
    app_version TEXT,
    push_token TEXT,
    last_active_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    CONSTRAINT uq_user_device UNIQUE (user_id, device_id)
);

CREATE INDEX idx_device_installations_user ON device_installations(user_id);

-- 6. Sync Cursors Table (§16, §25)
CREATE TABLE sync_cursors (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_cursor INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
