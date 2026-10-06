-- 0015: Username and Phone Support for Flexible Login
-- Allows staff and business owners to log in using either their Email, Username, or Phone number.

ALTER TABLE users ADD COLUMN username TEXT;
ALTER TABLE users ADD COLUMN phone TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users(LOWER(username)) WHERE username IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_phone ON users(phone) WHERE phone IS NOT NULL;
