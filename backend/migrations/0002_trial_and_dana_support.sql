-- backend/migrations/0002_trial_and_dana_support.sql
-- Milestone 1: 7-Day Premium Free Trial & Direct DANA Open API Schema Support
-- Adds trial tracking fields to users and expands CHECK constraints on subscriptions and webhook_events.

-- 1. Add Trial Tracking Columns to Users Table
ALTER TABLE users ADD COLUMN has_used_trial INTEGER NOT NULL DEFAULT 0 CHECK (has_used_trial IN (0, 1));
ALTER TABLE users ADD COLUMN trial_started_at TEXT;
ALTER TABLE users ADD COLUMN trial_ends_at TEXT;

-- 2. Expand Subscriptions CHECK Constraints
-- provider includes: 'midtrans', 'xendit', 'dana', 'trial'
-- status includes: 'active', 'grace', 'cancelled', 'expired', 'trialing'
CREATE TABLE subscriptions_new (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('midtrans', 'xendit', 'dana', 'trial')),
    provider_subscription_id TEXT,
    plan_id TEXT NOT NULL DEFAULT 'premium_monthly',
    status TEXT NOT NULL CHECK (status IN ('active', 'grace', 'cancelled', 'expired', 'trialing')),
    amount INTEGER NOT NULL DEFAULT 5000 CHECK (amount >= 0),
    current_period_start TEXT NOT NULL,
    current_period_end TEXT NOT NULL,
    cancel_at_period_end INTEGER NOT NULL DEFAULT 0 CHECK (cancel_at_period_end IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO subscriptions_new (
    id, user_id, provider, provider_subscription_id, plan_id, status, amount,
    current_period_start, current_period_end, cancel_at_period_end, created_at, updated_at
)
SELECT 
    id, user_id, provider, provider_subscription_id, plan_id, status, amount,
    current_period_start, current_period_end, cancel_at_period_end, created_at, updated_at
FROM subscriptions;

DROP TABLE subscriptions;
ALTER TABLE subscriptions_new RENAME TO subscriptions;

CREATE INDEX idx_subscriptions_user ON subscriptions(user_id);
CREATE INDEX idx_subscriptions_provider_id ON subscriptions(provider, provider_subscription_id);

-- 3. Expand Webhook Events CHECK Constraints
-- provider includes: 'midtrans', 'xendit', 'dana'
CREATE TABLE webhook_events_new (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('midtrans', 'xendit', 'dana')),
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('received', 'processed', 'ignored', 'failed')),
    processed_at TEXT,
    created_at TEXT NOT NULL,
    CONSTRAINT uq_webhook_provider_event UNIQUE (provider, event_id)
);

INSERT INTO webhook_events_new (
    id, provider, event_id, event_type, payload, status, processed_at, created_at
)
SELECT 
    id, provider, event_id, event_type, payload, status, processed_at, created_at
FROM webhook_events;

DROP TABLE webhook_events;
ALTER TABLE webhook_events_new RENAME TO webhook_events;

CREATE INDEX idx_webhook_events_status ON webhook_events(status);
