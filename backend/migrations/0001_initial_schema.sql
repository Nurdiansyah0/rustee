-- backend/migrations/0001_initial_schema.sql
-- Personal Finance PWA SaaS - Initial Relational Schema
-- Conforms to SQLite WAL mode, integer Rupiah minor units, and multi-tenant isolation.
-- NOTE: PRAGMAs are deliberately configured per-connection via SqliteConnectOptions,
-- NOT in this file, because SQLx executes migrations within a transaction block.

-- 1. Users Table
CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'IDR',
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    subscription_tier TEXT NOT NULL DEFAULT 'free' CHECK (subscription_tier IN ('free', 'premium')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 2. Multi-Wallet Accounts Table
CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    account_type TEXT NOT NULL CHECK (account_type IN ('checking', 'savings', 'credit', 'e_wallet', 'cash', 'investment')),
    currency TEXT NOT NULL DEFAULT 'IDR',
    initial_balance INTEGER NOT NULL DEFAULT 0,     -- Stored in integer Rupiah
    current_balance INTEGER NOT NULL DEFAULT 0,     -- Atomically maintained in integer Rupiah
    color TEXT,
    icon TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 3. Categories Table (Supports System Defaults + User Customization + Soft-Delete)
CREATE TABLE categories (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE, -- NULL for global default system categories
    name TEXT NOT NULL,
    category_type TEXT NOT NULL CHECK (category_type IN ('income', 'expense')),
    icon TEXT,
    color TEXT,
    is_system INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
    deleted_at TEXT,                                -- Soft-delete timestamp; NULL when active
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 4. Transactions Table (Double-Entry / Transfer Aware Ledger)
CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    to_account_id TEXT REFERENCES accounts(id) ON DELETE RESTRICT, -- Target for transfers
    category_id TEXT REFERENCES categories(id) ON DELETE RESTRICT, -- NULL for transfers
    transaction_type TEXT NOT NULL CHECK (transaction_type IN ('income', 'expense', 'transfer')),
    amount INTEGER NOT NULL CHECK (amount > 0),     -- Strictly positive integer Rupiah
    date TEXT NOT NULL,                             -- ISO 8601 UTC timestamp: YYYY-MM-DDTHH:MM:SSZ
    description TEXT NOT NULL,
    notes TEXT,
    is_recurring INTEGER NOT NULL DEFAULT 0 CHECK (is_recurring IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 5. Budgets Table
CREATE TABLE budgets (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
    amount_limit INTEGER NOT NULL CHECK (amount_limit > 0),
    period TEXT NOT NULL DEFAULT 'monthly' CHECK (period IN ('weekly', 'monthly', 'yearly')),
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 6. Financial Goals Table
CREATE TABLE goals (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    target_amount INTEGER NOT NULL CHECK (target_amount > 0),
    current_amount INTEGER NOT NULL DEFAULT 0 CHECK (current_amount >= 0),
    target_date TEXT,
    color TEXT,
    icon TEXT,
    is_completed INTEGER NOT NULL DEFAULT 0 CHECK (is_completed IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 7. Subscriptions Table
CREATE TABLE subscriptions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('midtrans', 'xendit')),
    provider_subscription_id TEXT,
    plan_id TEXT NOT NULL DEFAULT 'premium_monthly',
    status TEXT NOT NULL CHECK (status IN ('active', 'grace', 'cancelled', 'expired')),
    amount INTEGER NOT NULL DEFAULT 5000 CHECK (amount >= 0), -- Rp5,000 / month
    current_period_start TEXT NOT NULL,
    current_period_end TEXT NOT NULL,
    cancel_at_period_end INTEGER NOT NULL DEFAULT 0 CHECK (cancel_at_period_end IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 8. Audit Logs Table
CREATE TABLE audit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    details TEXT,
    created_at TEXT NOT NULL
);

-- 9. Strict Idempotency Keys Table
CREATE TABLE idempotency_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,                      -- SHA-256 of path + payload
    status TEXT NOT NULL CHECK (status IN ('in_progress', 'completed', 'failed')),
    response_code INTEGER,                           -- Cached HTTP status
    response_body TEXT,                              -- Cached JSON response body
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    CONSTRAINT uq_user_idempotency_key UNIQUE (user_id, idempotency_key)
);

-- 10. Webhook Events Table
CREATE TABLE webhook_events (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('midtrans', 'xendit')),
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('received', 'processed', 'ignored', 'failed')),
    processed_at TEXT,
    created_at TEXT NOT NULL,
    CONSTRAINT uq_webhook_provider_event UNIQUE (provider, event_id)
);

-- MANDATORY COMPOSITE & ACCELERATION INDEXES (Per R3)
CREATE INDEX idx_transactions_user_date ON transactions(user_id, date);
CREATE INDEX idx_transactions_account ON transactions(account_id);
CREATE INDEX idx_transactions_to_account ON transactions(to_account_id);
CREATE INDEX idx_transactions_user_category ON transactions(user_id, category_id);

CREATE INDEX idx_accounts_user ON accounts(user_id);
CREATE INDEX idx_categories_user ON categories(user_id);
CREATE INDEX idx_categories_system ON categories(is_system);

CREATE INDEX idx_budgets_user ON budgets(user_id);
CREATE INDEX idx_budgets_user_category ON budgets(user_id, category_id);
CREATE INDEX idx_goals_user ON goals(user_id);

CREATE INDEX idx_subscriptions_user ON subscriptions(user_id);
CREATE INDEX idx_subscriptions_provider_id ON subscriptions(provider, provider_subscription_id);

CREATE INDEX idx_audit_logs_user_date ON audit_logs(user_id, created_at);
CREATE INDEX idx_idempotency_expires ON idempotency_keys(expires_at);
CREATE INDEX idx_webhook_events_status ON webhook_events(status);
