-- backend/migrations/0005_user_preferences.sql
-- Personalization and user-owned financial vocabulary preferences (§4, §5)

CREATE TABLE IF NOT EXISTS user_preferences (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    display_name TEXT,
    income_title TEXT,
    expense_title TEXT,
    financial_goals TEXT,
    onboarding_completed BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_preferences_user ON user_preferences(user_id);
