-- backend/migrations/0004_subscription_lifecycle_v3_1_0.sql
-- Invinite Personal Finance Master Specification v3.1.0 Subscription Lifecycle & Pricing Support
-- Expands status to support FREE, TRIALING, ACTIVE, GRACE, CANCELLED, EXPIRED, PENDING, UNVERIFIED

CREATE TABLE subscriptions_new (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('midtrans', 'xendit', 'dana', 'trial', 'system')),
    provider_subscription_id TEXT,
    plan_id TEXT NOT NULL DEFAULT 'premium_monthly',
    status TEXT NOT NULL CHECK (status IN ('free', 'trialing', 'active', 'grace', 'cancelled', 'expired', 'pending', 'unverified',
                                           'FREE', 'TRIALING', 'ACTIVE', 'GRACE', 'CANCELLED', 'EXPIRED', 'PENDING', 'UNVERIFIED')),
    amount INTEGER NOT NULL DEFAULT 10000 CHECK (amount >= 0),
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
