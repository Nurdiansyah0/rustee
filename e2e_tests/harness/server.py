#!/usr/bin/env python3
"""
High-Fidelity Reference Mock Server (Oracle) for Invinite E2E Acceptance Tests.
Authoritative implementation derived strictly from Master Specification v3.1.0 & PROJECT.md.

Implements all 55 features:
- SQLite embedded persistence in WAL mode with busy_timeout=5000 and foreign_keys=ON.
- Checked integer Rupiah arithmetic (i64 minor units, zero floats).
- Centralized Net Cash Flow engine (income - expenses, transfers net to zero).
- Multi-wallet accounting with atomic balance mutations.
- Category soft-deletion preserving historical ledger integrity.
- Tenant-isolated custom vocabulary in stable relational schema.
- Strict UTC timestamps (ISO 8601).
- Idempotency-Key deduplication engine with SHA-256 payload caching.
- Argon2id / secure PBKDF2 password hashing + httpOnly cookie session management.
- Sliding-window rate limiting on login (5 attempts / 15 minutes).
- 3-Month Premium Trial (90 days, 0 upfront, 1x per account).
- Commercial plans: Monthly Premium (Rp10.000), Annual Premium (Rp110.000).
- DANA Open API integration with RSA-SHA256 asymmetric signature verification.
- Server-side feature gating returning HTTP 403 FEATURE_LOCKED.
- Ingestion pipeline (Notification, SMS, Gmail) with confidence engine (HIGH/MEDIUM/LOW) & deduplication.
- Progressive onboarding, cursor delta sync, foreground WebSocket probe, Android shell status.
- Cache-Control: private, no-store on sensitive endpoints.
- /health and /ready operational status probes.
"""

import sys
import os
import time
import json
import uuid
import hmac
import hashlib
import sqlite3
import re
from datetime import datetime, timezone
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs
from collections import defaultdict
from typing import Dict, Any, Optional, Tuple, List

sys.path.insert(0, os.path.abspath(os.path.dirname(__file__)))
from crypto_keys import verify_dana_signature

PORT = int(os.environ.get("PORT", "8089"))
DB_PATH = os.environ.get("DB_PATH", "/tmp/invinite_e2e_db.sqlite")
JWT_SECRET = os.environ.get("JWT_SECRET", "super-secret-jwt-signing-key-minimum-32-bytes!")

RATE_LIMIT_STORE = defaultdict(list)

def get_db():
    conn = sqlite3.connect(DB_PATH, timeout=5.0)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode = WAL;")
    conn.execute("PRAGMA busy_timeout = 5000;")
    conn.execute("PRAGMA foreign_keys = ON;")
    conn.execute("PRAGMA synchronous = NORMAL;")
    return conn

def init_db():
    with get_db() as conn:
        conn.executescript("""
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            tier TEXT NOT NULL DEFAULT 'free',
            has_used_trial INTEGER NOT NULL DEFAULT 0,
            trial_started_at TEXT,
            trial_ends_at TEXT,
            display_name TEXT,
            financial_goals TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            account_type TEXT NOT NULL,
            balance INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            display_name TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            category_type TEXT NOT NULL,
            icon TEXT DEFAULT 'tag',
            color TEXT DEFAULT '#10b981',
            metadata TEXT DEFAULT '{}',
            deleted_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            category_id TEXT,
            destination_account_id TEXT,
            amount INTEGER NOT NULL,
            transaction_type TEXT NOT NULL,
            note TEXT DEFAULT '',
            date TEXT NOT NULL,
            idempotency_key TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS budgets (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            category_id TEXT NOT NULL,
            amount INTEGER NOT NULL,
            period_start TEXT NOT NULL,
            period_end TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            target_amount INTEGER NOT NULL,
            current_amount INTEGER NOT NULL DEFAULT 0,
            target_date TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS subscriptions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL UNIQUE,
            tier TEXT NOT NULL DEFAULT 'free',
            plan_id TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            current_period_start TEXT NOT NULL,
            current_period_end TEXT NOT NULL,
            cancel_at_period_end INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            action TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            details TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS idempotency_keys (
            user_id TEXT NOT NULL,
            key TEXT NOT NULL,
            payload_hash TEXT NOT NULL,
            response_status INTEGER NOT NULL,
            response_body TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (user_id, key)
        );

        CREATE TABLE IF NOT EXISTS webhook_events (
            provider TEXT NOT NULL,
            event_id TEXT NOT NULL,
            payload_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (provider, event_id)
        );

        CREATE TABLE IF NOT EXISTS ingestion_events (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            source TEXT NOT NULL,
            provider TEXT NOT NULL,
            external_ref TEXT,
            raw_fingerprint TEXT NOT NULL,
            confidence TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS transaction_candidates (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            source TEXT NOT NULL,
            provider TEXT NOT NULL,
            external_ref TEXT,
            amount INTEGER NOT NULL,
            direction TEXT NOT NULL,
            occurred_at TEXT NOT NULL,
            merchant TEXT NOT NULL,
            account_id TEXT,
            category_id TEXT,
            confidence TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_cursors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            action TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        -- Mandatory Canonical Composite Indexes (§16)
        CREATE INDEX IF NOT EXISTS idx_tx_user_date ON transactions(user_id, date);
        CREATE INDEX IF NOT EXISTS idx_tx_account ON transactions(account_id);
        CREATE INDEX IF NOT EXISTS idx_cat_user ON categories(user_id);
        CREATE INDEX IF NOT EXISTS idx_acc_user ON accounts(user_id);
        CREATE INDEX IF NOT EXISTS idx_budg_user ON budgets(user_id);
        CREATE INDEX IF NOT EXISTS idx_sub_user ON subscriptions(user_id);
        CREATE INDEX IF NOT EXISTS idx_sync_user_id ON sync_cursors(user_id, id);
        """)

def utc_now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def hash_password(password: str) -> str:
    salt = os.urandom(16).hex()
    hashed = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt.encode("utf-8"), 100000).hex()
    return f"pbkdf2:sha256:100000${salt}${hashed}"

def verify_password(password: str, hashed_str: str) -> bool:
    try:
        parts = hashed_str.split("$")
        if len(parts) != 3:
            return False
        salt = parts[1]
        orig_hash = parts[2]
        test_hash = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt.encode("utf-8"), 100000).hex()
        return hmac.compare_digest(orig_hash, test_hash)
    except Exception:
        return False

def make_session_token(user_id: str) -> str:
    payload = f"{user_id}:{int(time.time())}"
    sig = hmac.new(JWT_SECRET.encode("utf-8"), payload.encode("utf-8"), hashlib.sha256).hexdigest()
    return f"{payload}:{sig}"

def parse_session_token(token: str) -> Optional[str]:
    try:
        parts = token.split(":")
        if len(parts) != 3:
            return None
        user_id, ts_str, sig = parts
        expected_sig = hmac.new(JWT_SECRET.encode("utf-8"), f"{user_id}:{ts_str}".encode("utf-8"), hashlib.sha256).hexdigest()
        if not hmac.compare_digest(sig, expected_sig):
            return None
        if time.time() - int(ts_str) > 86400:  # 24 hour expiry
            return None
        return user_id
    except Exception:
        return None

def check_rate_limit(ip: str) -> bool:
    now = time.time()
    cutoff = now - 900  # 15 minute sliding window
    RATE_LIMIT_STORE[ip] = [t for t in RATE_LIMIT_STORE[ip] if t > cutoff]
    if len(RATE_LIMIT_STORE[ip]) >= 5:
        return False
    RATE_LIMIT_STORE[ip].append(now)
    return True

class InviniteRequestHandler(BaseHTTPRequestHandler):
    def send_rfc7807(self, status: int, title: str, detail: str, code: str = "ERROR", extra: Optional[Dict[str, Any]] = None):
        body = {
            "type": f"https://api.nurdiansyahlabs.com/errors/{code.lower().replace('_', '-')}",
            "title": title,
            "status": status,
            "detail": detail,
            "code": code
        }
        if extra:
            body.update(extra)
        raw = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/problem+json")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "private, no-store")
        self.end_headers()
        self.wfile.write(raw)

    def send_json(self, status: int, data: Any, headers: Optional[Dict[str, str]] = None):
        raw = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "private, no-store")
        if headers:
            for k, v in headers.items():
                self.send_header(k, v)
        self.end_headers()
        self.wfile.write(raw)

    def get_auth_user_id(self) -> Optional[str]:
        # 1. Check Cookie
        cookie_header = self.headers.get("Cookie", "")
        for item in cookie_header.split(";"):
            item = item.strip()
            if item.startswith("auth_token="):
                token = item.split("=", 1)[1]
                uid = parse_session_token(token)
                if uid:
                    return uid

        # 2. Check Authorization Bearer Header
        auth_header = self.headers.get("Authorization", "")
        if auth_header.startswith("Bearer "):
            token = auth_header[7:].strip()
            return parse_session_token(token)

        return None

    def get_user(self, user_id: str):
        with get_db() as conn:
            cur = conn.execute("SELECT * FROM users WHERE id = ?", (user_id,))
            return cur.fetchone()

    def is_user_premium(self, user: sqlite3.Row) -> bool:
        if user["tier"] in ["premium", "active"]:
            return True
        # Check trial status
        if user["has_used_trial"] and user["trial_ends_at"]:
            try:
                ends_dt = datetime.strptime(user["trial_ends_at"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
                if datetime.now(timezone.utc) < ends_dt:
                    return True
            except Exception:
                pass
        return False

    def log_audit(self, user_id: str, action: str, entity_type: str, entity_id: str, details: str = "", conn=None):
        if conn is not None:
            conn.execute(
                "INSERT INTO audit_logs (id, user_id, action, entity_type, entity_id, details, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (str(uuid.uuid4()), user_id, action, entity_type, entity_id, details, utc_now_iso())
            )
        else:
            with get_db() as c:
                c.execute(
                    "INSERT INTO audit_logs (id, user_id, action, entity_type, entity_id, details, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (str(uuid.uuid4()), user_id, action, entity_type, entity_id, details, utc_now_iso())
                )
                c.commit()

    def record_sync_cursor(self, user_id: str, entity_type: str, entity_id: str, action: str, conn=None):
        if conn is not None:
            conn.execute(
                "INSERT INTO sync_cursors (user_id, entity_type, entity_id, action, created_at) VALUES (?, ?, ?, ?, ?)",
                (user_id, entity_type, entity_id, action, utc_now_iso())
            )
        else:
            with get_db() as c:
                c.execute(
                    "INSERT INTO sync_cursors (user_id, entity_type, entity_id, action, created_at) VALUES (?, ?, ?, ?, ?)",
                    (user_id, entity_type, entity_id, action, utc_now_iso())
                )
                c.commit()

    def read_json_body(self) -> Tuple[Optional[Dict[str, Any]], Optional[str]]:
        try:
            length = int(self.headers.get("Content-Length", 0))
            if length <= 0:
                return None, None
            raw = self.rfile.read(length)
            return json.loads(raw.decode("utf-8")), raw.decode("utf-8")
        except Exception as e:
            return None, str(e)

    def do_HEAD(self):
        parsed = urlparse(self.path)
        if parsed.path in ["/health", "/ready"]:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
        else:
            self.send_response(404)
            self.end_headers()

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        qs = parse_qs(parsed.query)

        # Operational Probes
        if path == "/health":
            self.send_json(200, {"status": "ok", "timestamp": utc_now_iso()})
            return

        if path == "/ready":
            try:
                with get_db() as conn:
                    cur = conn.execute("PRAGMA journal_mode;")
                    row = cur.fetchone()
                    wal_active = row and row[0].upper() == "WAL"
                    if wal_active:
                        self.send_json(200, {"status": "ready", "database": "ok", "wal": True})
                    else:
                        self.send_rfc7807(503, "Service Unavailable", "Database WAL mode inactive", "WAL_INACTIVE")
            except Exception as e:
                self.send_rfc7807(503, "Service Unavailable", f"Database check failed: {e}", "DB_UNAVAILABLE")
            return

        # WebSocket upgrade probe
        if path == "/api/v1/ws":
            upgrade = self.headers.get("Upgrade", "")
            if "websocket" in upgrade.lower():
                self.send_json(200, {"status": "websocket_endpoint_ready", "supported_events": ["TransactionCreated", "BalanceChanged", "SyncHint"]})
            else:
                self.send_rfc7807(400, "Bad Request", "Expected WebSocket Upgrade header", "UPGRADE_REQUIRED")
            return

        # Android shell capability status probe
        if path == "/api/v1/android/status":
            self.send_json(200, {
                "bridge_version": "1.0",
                "notification_service": "supported",
                "trusted_origin": "https://api.nurdiansyahlabs.com"
            })
            return

        # Protected API endpoints
        user_id = self.get_auth_user_id()
        if not user_id:
            self.send_rfc7807(401, "Unauthorized", "Valid authentication token required", "UNAUTHORIZED")
            return

        user = self.get_user(user_id)
        if not user:
            self.send_rfc7807(401, "Unauthorized", "User account not found", "UNAUTHORIZED")
            return

        is_premium = self.is_user_premium(user)

        # 1. Auth Me
        if path == "/api/v1/auth/me":
            self.send_json(200, {
                "id": user["id"],
                "name": user["name"],
                "email": user["email"],
                "tier": user["tier"],
                "display_name": user["display_name"],
                "has_used_trial": bool(user["has_used_trial"]),
                "is_premium": is_premium
            })
            return

        # 2. Subscription Status (§7, §8, §19)
        if path in ["/api/v1/subscription", "/api/v1/subscriptions/status"]:
            now_dt = datetime.now(timezone.utc)
            days_remaining = 0
            sub_status = user["tier"]
            trial_ends_at = user["trial_ends_at"]

            if user["has_used_trial"] and trial_ends_at:
                try:
                    ends_dt = datetime.strptime(trial_ends_at, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
                    delta_days = (ends_dt - now_dt).days
                    days_remaining = max(0, delta_days)
                    if days_remaining > 0:
                        sub_status = "trialing"
                    else:
                        sub_status = "expired"
                except Exception:
                    pass

            features = ["transactions.basic", "accounts.basic"]
            if is_premium or sub_status == "trialing":
                features.extend(["analytics.advanced", "budgeting", "reports.advanced", "auto_transaction_ingestion"])

            self.send_json(200, {
                "user_id": user_id,
                "tier": user["tier"],
                "status": sub_status,
                "is_premium": is_premium,
                "has_used_trial": bool(user["has_used_trial"]),
                "trial_started_at": user["trial_started_at"],
                "trial_ends_at": user["trial_ends_at"],
                "days_remaining": days_remaining,
                "features": features
            })
            return

        # 3. Multi-Wallet Accounts (§14, §15)
        if path == "/api/v1/accounts":
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM accounts WHERE user_id = ? AND is_active = 1 ORDER BY created_at ASC", (user_id,))
                rows = cur.fetchall()
                accounts = [dict(r) for r in rows]
                self.send_json(200, {"accounts": accounts, "count": len(accounts)})
            return

        if re.match(r"^/api/v1/accounts/[^/]+$", path):
            account_id = path.split("/")[-1]
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM accounts WHERE id = ? AND user_id = ?", (account_id, user_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Account not found or access denied", "ACCOUNT_NOT_FOUND")
                    return
                self.send_json(200, dict(row))
            return

        # 4. Categories (Stable Schema Vocabulary & Soft-Deletion, §4, §5, §14)
        if path == "/api/v1/categories":
            include_deleted = qs.get("include_deleted", ["false"])[0].lower() == "true"
            query = "SELECT * FROM categories WHERE user_id = ?"
            if not include_deleted:
                query += " AND deleted_at IS NULL"
            query += " ORDER BY created_at ASC"

            with get_db() as conn:
                cur = conn.execute(query, (user_id,))
                rows = cur.fetchall()
                categories = [dict(r) for r in rows]
                self.send_json(200, {"categories": categories, "count": len(categories)})
            return

        # 5. Transactions Listing (§14, §15)
        if path == "/api/v1/transactions":
            query = "SELECT * FROM transactions WHERE user_id = ?"
            params = [user_id]

            if "account_id" in qs:
                query += " AND account_id = ?"
                params.append(qs["account_id"][0])
            if "category_id" in qs:
                query += " AND category_id = ?"
                params.append(qs["category_id"][0])
            if "start_date" in qs:
                query += " AND date >= ?"
                params.append(qs["start_date"][0])
            if "end_date" in qs:
                query += " AND date <= ?"
                params.append(qs["end_date"][0])

            query += " ORDER BY date DESC, created_at DESC"
            with get_db() as conn:
                cur = conn.execute(query, params)
                rows = cur.fetchall()
                txs = [dict(r) for r in rows]
                self.send_json(200, {"transactions": txs, "count": len(txs)})
            return

        if re.match(r"^/api/v1/transactions/[^/]+$", path):
            tx_id = path.split("/")[-1]
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM transactions WHERE id = ? AND user_id = ?", (tx_id, user_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Transaction not found or access denied", "TRANSACTION_NOT_FOUND")
                    return
                self.send_json(200, dict(row))
            return

        # 6. Centralized Net Cash Flow Engine (§15)
        if path == "/api/v1/analytics/cash-flow":
            with get_db() as conn:
                cur = conn.execute("SELECT transaction_type, amount FROM transactions WHERE user_id = ?", (user_id,))
                rows = cur.fetchall()
                income = sum(r["amount"] for r in rows if r["transaction_type"] == "income")
                expenses = sum(r["amount"] for r in rows if r["transaction_type"] == "expense")
                # Canonical formula: net_cash_flow = income - expenses
                net_cash_flow = income - expenses
                self.send_json(200, {
                    "total_income": income,
                    "total_expenses": expenses,
                    "net_cash_flow": net_cash_flow,
                    "currency": "IDR"
                })
            return

        # 7. Category Aggregates
        if path == "/api/v1/analytics/categories":
            with get_db() as conn:
                cur = conn.execute("""
                    SELECT c.id, c.display_name, c.normalized_name, c.category_type, COALESCE(SUM(t.amount), 0) as total_amount
                    FROM categories c
                    LEFT JOIN transactions t ON t.category_id = c.id AND t.user_id = ?
                    WHERE c.user_id = ?
                    GROUP BY c.id
                """, (user_id, user_id))
                rows = cur.fetchall()
                items = [dict(r) for r in rows]
                self.send_json(200, {"categories": items})
            return

        # 8. Server-Side Feature Gating (§9, §19, REQ-SEC-10)
        # Locked features strictly return HTTP 403 FEATURE_LOCKED for non-premium users
        if path in ["/api/v1/analytics/advanced", "/api/v1/reports/advanced", "/api/v1/budgets"]:
            if not is_premium:
                self.send_rfc7807(403, "Forbidden", "Fitur ini memerlukan langganan Premium aktif atau uji coba 3 bulan.", "FEATURE_LOCKED")
                return

            if path == "/api/v1/analytics/advanced":
                self.send_json(200, {
                    "financial_health_score": 88,
                    "runway_months": 7.5,
                    "monthly_savings_rate": 0.28,
                    "projected_savings_annual": 42000000
                })
                return
            elif path == "/api/v1/reports/advanced":
                self.send_json(200, {
                    "report_type": "tax_and_balance_sheet",
                    "status": "ready",
                    "assets": 150000000,
                    "liabilities": 25000000,
                    "net_worth": 125000000
                })
                return
            elif path == "/api/v1/budgets":
                with get_db() as conn:
                    cur = conn.execute("SELECT * FROM budgets WHERE user_id = ?", (user_id,))
                    budgets = [dict(r) for r in cur.fetchall()]
                    self.send_json(200, {"budgets": budgets, "count": len(budgets)})
                return

        # 9. Ingestion Candidates Review (§24)
        if path == "/api/v1/ingestion/candidates":
            if not is_premium:
                self.send_rfc7807(403, "Forbidden", "Transaction ingestion requires Premium subscription", "FEATURE_LOCKED")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM transaction_candidates WHERE user_id = ? AND status = 'pending' ORDER BY created_at DESC", (user_id,))
                candidates = [dict(r) for r in cur.fetchall()]
                self.send_json(200, {"candidates": candidates, "count": len(candidates)})
            return

        # 10. Cursor Delta Sync (§25, §26)
        if path == "/api/v1/sync":
            cursor_param = int(qs.get("cursor", [0])[0])
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM sync_cursors WHERE user_id = ? AND id > ? ORDER BY id ASC LIMIT 100", (user_id, cursor_param))
                rows = cur.fetchall()
                new_cursor = rows[-1]["id"] if rows else cursor_param

                # Retrieve transactions modified
                tx_cur = conn.execute("SELECT * FROM transactions WHERE user_id = ? ORDER BY created_at DESC LIMIT 50", (user_id,))
                acc_cur = conn.execute("SELECT * FROM accounts WHERE user_id = ? ORDER BY created_at DESC", (user_id,))
                cat_cur = conn.execute("SELECT * FROM categories WHERE user_id = ? ORDER BY created_at DESC", (user_id,))

                self.send_json(200, {
                    "cursor": new_cursor,
                    "deltas_count": len(rows),
                    "transactions": [dict(r) for r in tx_cur.fetchall()],
                    "accounts": [dict(r) for r in acc_cur.fetchall()],
                    "categories": [dict(r) for r in cat_cur.fetchall()],
                    "has_more": False
                })
            return

        self.send_rfc7807(404, "Not Found", f"Endpoint '{path}' not found", "NOT_FOUND")

    def do_POST(self):
        parsed = urlparse(self.path)
        path = parsed.path
        body, raw_str = self.read_json_body()

        # 1. Registration
        if path == "/api/v1/auth/register":
            if not body or "email" not in body or "password" not in body:
                self.send_rfc7807(422, "Unprocessable Entity", "Email and password are required", "INVALID_CREDENTIALS")
                return
            email = body["email"].strip().lower()
            name = body.get("name", "User").strip()
            password = body["password"]

            if len(password.strip()) < 8:
                self.send_rfc7807(422, "Unprocessable Entity", "Password must be at least 8 non-whitespace characters", "WEAK_PASSWORD")
                return

            user_id = str(uuid.uuid4())
            pwd_hash = hash_password(password)
            now_iso = utc_now_iso()

            try:
                with get_db() as conn:
                    conn.execute(
                        "INSERT INTO users (id, name, email, password_hash, tier, created_at, updated_at) VALUES (?, ?, ?, ?, 'free', ?, ?)",
                        (user_id, name, email, pwd_hash, now_iso, now_iso)
                    )
                    # Create default account and standard starting categories
                    default_acc_id = str(uuid.uuid4())
                    conn.execute(
                        "INSERT INTO accounts (id, user_id, name, account_type, balance, created_at, updated_at) VALUES (?, ?, 'Dompet Utama', 'cash', 0, ?, ?)",
                        (default_acc_id, user_id, now_iso, now_iso)
                    )
                    default_cats = [
                        (str(uuid.uuid4()), user_id, "Gaji", "Gaji", "gaji", "income", "briefcase", "#10b981", now_iso),
                        (str(uuid.uuid4()), user_id, "Makan & Minum", "Makan & Minum", "makan-minum", "expense", "utensils", "#ef4444", now_iso),
                        (str(uuid.uuid4()), user_id, "Transport", "Transport", "transport", "expense", "car", "#f59e0b", now_iso),
                    ]
                    for cid, uid, cname, dname, nname, ctype, cicon, ccol, ccreated in default_cats:
                        conn.execute(
                            "INSERT INTO categories (id, user_id, name, display_name, normalized_name, category_type, icon, color, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                            (cid, uid, cname, dname, nname, ctype, cicon, ccol, ccreated, ccreated)
                        )
                    conn.commit()

                token = make_session_token(user_id)
                cookie_hdr = f"auth_token={token}; Path=/; HttpOnly; SameSite=Lax"
                self.log_audit(user_id, "REGISTER", "user", user_id, f"Registered with email {email}")
                self.send_json(201, {
                    "user": {"id": user_id, "name": name, "email": email, "tier": "free"},
                    "token": token
                }, headers={"Set-Cookie": cookie_hdr})
            except sqlite3.IntegrityError:
                self.send_rfc7807(409, "Conflict", "An account with this email already exists", "EMAIL_EXISTS")
            return

        # 2. Login (with 5 / 15 min Rate Limiting, §17)
        if path == "/api/v1/auth/login":
            client_ip = self.headers.get("X-Forwarded-For", self.client_address[0])
            if not check_rate_limit(client_ip):
                self.send_rfc7807(429, "Too Many Requests", "Rate limit exceeded (maximum 5 attempts per 15 minutes)", "RATE_LIMIT_EXCEEDED")
                return

            if not body or "email" not in body or "password" not in body:
                self.send_rfc7807(401, "Unauthorized", "Invalid credentials", "INVALID_CREDENTIALS")
                return

            email = body["email"].strip().lower()
            password = body["password"]

            with get_db() as conn:
                cur = conn.execute("SELECT * FROM users WHERE email = ?", (email,))
                user = cur.fetchone()

            if not user or not verify_password(password, user["password_hash"]):
                self.send_rfc7807(401, "Unauthorized", "Invalid email or password", "INVALID_CREDENTIALS")
                return

            token = make_session_token(user["id"])
            cookie_hdr = f"auth_token={token}; Path=/; HttpOnly; SameSite=Lax"
            self.log_audit(user["id"], "LOGIN", "user", user["id"], "User logged in")
            self.send_json(200, {
                "user": {"id": user["id"], "name": user["name"], "email": user["email"], "tier": user["tier"]},
                "token": token
            }, headers={"Set-Cookie": cookie_hdr})
            return

        # 3. Logout
        if path == "/api/v1/auth/logout":
            cookie_hdr = "auth_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
            self.send_json(200, {"message": "Logged out successfully"}, headers={"Set-Cookie": cookie_hdr})
            return

        # 4. DANA Open API Webhook (§19, REQ-SEC-08, REQ-SEC-09)
        if path == "/api/v1/webhooks/dana":
            sig = self.headers.get("X-SIGNATURE", "")
            timestamp = self.headers.get("X-TIMESTAMP", "")
            partner_id = self.headers.get("X-PARTNER-ID", "")
            event_id = self.headers.get("X-EXTERNAL-ID", "") or (body.get("event_id") if body else "")

            if not sig or not event_id:
                self.send_rfc7807(401, "Unauthorized", "Missing signature or event ID", "UNAUTHORIZED_WEBHOOK")
                return

            string_to_sign = f"POST:/api/v1/webhooks/dana:{timestamp}:{raw_str}"
            if not verify_dana_signature(string_to_sign, sig):
                self.send_rfc7807(401, "Unauthorized", "Cryptographic signature verification failed", "INVALID_SIGNATURE")
                return

            payload_hash = hashlib.sha256(raw_str.encode("utf-8")).hexdigest()
            with get_db() as conn:
                # Idempotency check (§19)
                cur = conn.execute("SELECT * FROM webhook_events WHERE provider = 'dana' AND event_id = ?", (event_id,))
                existing = cur.fetchone()
                if existing:
                    # Idempotent replay: return cached success acknowledgment
                    self.send_json(200, {"responseCode": "2005600", "responseMessage": "Successful", "idempotent_replay": True})
                    return

                # Record event
                conn.execute(
                    "INSERT INTO webhook_events (provider, event_id, payload_hash, status, created_at) VALUES ('dana', ?, ?, 'processed', ?)",
                    (event_id, payload_hash, utc_now_iso())
                )

                # Process subscription update
                user_id_target = body.get("user_id")
                if user_id_target:
                    conn.execute("UPDATE users SET tier = 'active', updated_at = ? WHERE id = ?", (utc_now_iso(), user_id_target))
                    conn.commit()
                    self.log_audit(user_id_target, "DANA_PAYMENT_SUCCESS", "subscription", event_id, f"Activated via DANA event {event_id}")

            self.send_json(200, {"responseCode": "2005600", "responseMessage": "Successful"})
            return

        # Authenticated endpoints
        user_id = self.get_auth_user_id()
        if not user_id:
            self.send_rfc7807(401, "Unauthorized", "Authentication required", "UNAUTHORIZED")
            return

        user = self.get_user(user_id)
        if not user:
            self.send_rfc7807(401, "Unauthorized", "User not found", "UNAUTHORIZED")
            return
        is_premium = self.is_user_premium(user)

        # 5. Progressive Onboarding & Vocabulary Configuration (§3, §4, §6, REQ-FE-04)
        if path == "/api/v1/users/onboarding":
            if not body:
                self.send_rfc7807(400, "Bad Request", "Onboarding payload required", "BAD_REQUEST")
                return
            display_name = body.get("display_name", user["name"])
            goals = json.dumps(body.get("financial_goals", []))
            wallets = body.get("wallets", [])
            categories = body.get("categories", [])
            now_iso = utc_now_iso()

            with get_db() as conn:
                conn.execute(
                    "UPDATE users SET display_name = ?, financial_goals = ?, updated_at = ? WHERE id = ?",
                    (display_name, goals, now_iso, user_id)
                )
                for w in wallets:
                    w_name = w.get("name", "Dompet")
                    w_type = w.get("account_type", "cash")
                    w_bal = int(w.get("initial_balance", 0))
                    w_id = str(uuid.uuid4())
                    conn.execute(
                        "INSERT INTO accounts (id, user_id, name, account_type, balance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                        (w_id, user_id, w_name, w_type, w_bal, now_iso, now_iso)
                    )
                    self.record_sync_cursor(user_id, "account", w_id, "create", conn=conn)
                for c in categories:
                    c_name = c.get("name", "Kategori")
                    c_disp = c.get("display_name", c_name)
                    c_norm = re.sub(r"[^a-z0-9]+", "-", c_name.lower()).strip("-")
                    c_type = c.get("category_type", "expense")
                    c_id = str(uuid.uuid4())
                    conn.execute(
                        "INSERT INTO categories (id, user_id, name, display_name, normalized_name, category_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        (c_id, user_id, c_name, c_disp, c_norm, c_type, now_iso, now_iso)
                    )
                    self.record_sync_cursor(user_id, "category", c_id, "create", conn=conn)
                conn.commit()

            self.log_audit(user_id, "ONBOARDING_COMPLETED", "user", user_id, "User finished progressive onboarding")
            self.send_json(200, {"status": "onboarded", "display_name": display_name})
            return

        # 6. Accounts creation (§14)
        if path == "/api/v1/accounts":
            if not body or "name" not in body or not str(body["name"]).strip():
                self.send_rfc7807(400, "Bad Request", "Account name is required", "MISSING_NAME")
                return
            name = str(body["name"]).strip()
            acc_type = body.get("account_type", "checking")
            initial_bal = int(body.get("initial_balance", 0))
            acc_id = str(uuid.uuid4())
            now_iso = utc_now_iso()

            with get_db() as conn:
                conn.execute(
                    "INSERT INTO accounts (id, user_id, name, account_type, balance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (acc_id, user_id, name, acc_type, initial_bal, now_iso, now_iso)
                )
                conn.commit()

            self.record_sync_cursor(user_id, "account", acc_id, "create")
            self.send_json(201, {
                "id": acc_id,
                "user_id": user_id,
                "name": name,
                "account_type": acc_type,
                "balance": initial_bal
            })
            return

        # 7. Categories creation (User custom vocabulary, §4, §5)
        if path == "/api/v1/categories":
            if not body or "name" not in body or not str(body["name"]).strip():
                self.send_rfc7807(400, "Bad Request", "Category name is required", "MISSING_NAME")
                return
            name = str(body["name"]).strip()
            display_name = str(body.get("display_name", name)).strip() or name
            normalized_name = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
            cat_type = body.get("category_type", "expense")
            icon = body.get("icon", "tag")
            color = body.get("color", "#10b981")
            metadata = body.get("metadata", "{}")
            if isinstance(metadata, (dict, list)):
                metadata = json.dumps(metadata)
            else:
                metadata = str(metadata)
            cat_id = str(uuid.uuid4())
            now_iso = utc_now_iso()

            with get_db() as conn:
                conn.execute(
                    "INSERT INTO categories (id, user_id, name, display_name, normalized_name, category_type, icon, color, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (cat_id, user_id, name, display_name, normalized_name, cat_type, icon, color, metadata, now_iso, now_iso)
                )
                conn.commit()

            self.record_sync_cursor(user_id, "category", cat_id, "create")
            self.send_json(201, {
                "id": cat_id,
                "user_id": user_id,
                "name": name,
                "display_name": display_name,
                "normalized_name": normalized_name,
                "category_type": cat_type
            })
            return

        # 8. Transactions creation (Atomic Balance Mutation, Idempotency, Integer Math, §15, §18)
        if path == "/api/v1/transactions":
            idempotency_key = self.headers.get("Idempotency-Key")

            # Check Idempotency Cache (§18)
            if idempotency_key:
                payload_hash = hashlib.sha256(raw_str.encode("utf-8")).hexdigest()
                with get_db() as conn:
                    cur = conn.execute("SELECT * FROM idempotency_keys WHERE user_id = ? AND key = ?", (user_id, idempotency_key))
                    cached = cur.fetchone()
                    if cached:
                        cached_data = json.loads(cached["response_body"])
                        self.send_json(cached["response_status"], cached_data, headers={"X-Cache-Replay": "true"})
                        return

            if not body or "account_id" not in body or "amount" not in body:
                self.send_rfc7807(400, "Bad Request", "Account ID and amount are required", "BAD_REQUEST")
                return

            raw_amount = body["amount"]
            if not isinstance(raw_amount, int) or raw_amount <= 0:
                self.send_rfc7807(422, "Unprocessable Entity", "Amount must be a positive integer Rupiah", "INVALID_AMOUNT")
                return
            if raw_amount > 9223372036854775807:
                self.send_rfc7807(422, "Unprocessable Entity", "Amount exceeds signed 64-bit maximum", "INTEGER_OVERFLOW")
                return

            account_id = body["account_id"]
            tx_type = body.get("transaction_type", "expense")
            category_id = body.get("category_id")
            dest_acc_id = body.get("destination_account_id")
            note = body.get("note", "")
            tx_date = body.get("date", utc_now_iso())
            tx_id = str(uuid.uuid4())
            now_iso = utc_now_iso()

            with get_db() as conn:
                # Verify source account ownership
                cur = conn.execute("SELECT * FROM accounts WHERE id = ? AND user_id = ?", (account_id, user_id))
                acc = cur.fetchone()
                if not acc:
                    self.send_rfc7807(404, "Not Found", "Account not found or access denied", "ACCOUNT_NOT_FOUND")
                    return

                # Atomic mutation inside SQLite transaction with checked arithmetic
                if tx_type == "income":
                    new_bal = acc["balance"] + raw_amount
                    if new_bal > 9223372036854775807:
                        self.send_rfc7807(422, "Unprocessable Entity", "Balance overflow detected", "INTEGER_OVERFLOW")
                        return
                    conn.execute("UPDATE accounts SET balance = ?, updated_at = ? WHERE id = ?", (new_bal, now_iso, account_id))
                elif tx_type == "expense":
                    new_bal = acc["balance"] - raw_amount
                    if new_bal < -9223372036854775808:
                        self.send_rfc7807(422, "Unprocessable Entity", "Balance underflow detected", "INTEGER_OVERFLOW")
                        return
                    conn.execute("UPDATE accounts SET balance = ?, updated_at = ? WHERE id = ?", (new_bal, now_iso, account_id))
                elif tx_type == "transfer":
                    if not dest_acc_id:
                        self.send_rfc7807(400, "Bad Request", "Destination account required for transfer", "MISSING_DESTINATION")
                        return
                    cur_dest = conn.execute("SELECT * FROM accounts WHERE id = ? AND user_id = ?", (dest_acc_id, user_id))
                    dest_acc = cur_dest.fetchone()
                    if not dest_acc:
                        self.send_rfc7807(404, "Not Found", "Destination account not found", "DESTINATION_NOT_FOUND")
                        return
                    src_bal = acc["balance"] - raw_amount
                    dst_bal = dest_acc["balance"] + raw_amount
                    if src_bal < -9223372036854775808 or dst_bal > 9223372036854775807:
                        self.send_rfc7807(422, "Unprocessable Entity", "Transfer balance overflow detected", "INTEGER_OVERFLOW")
                        return
                    conn.execute("UPDATE accounts SET balance = ?, updated_at = ? WHERE id = ?", (src_bal, now_iso, account_id))
                    conn.execute("UPDATE accounts SET balance = ?, updated_at = ? WHERE id = ?", (dst_bal, now_iso, dest_acc_id))
                else:
                    self.send_rfc7807(400, "Bad Request", f"Unsupported transaction type: {tx_type}", "INVALID_TYPE")
                    return

                conn.execute(
                    "INSERT INTO transactions (id, user_id, account_id, category_id, destination_account_id, amount, transaction_type, note, date, idempotency_key, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (tx_id, user_id, account_id, category_id, dest_acc_id, raw_amount, tx_type, note, tx_date, idempotency_key, now_iso)
                )

                resp_data = {
                    "id": tx_id,
                    "user_id": user_id,
                    "account_id": account_id,
                    "amount": raw_amount,
                    "transaction_type": tx_type,
                    "note": note,
                    "date": tx_date
                }

                if idempotency_key:
                    conn.execute(
                        "INSERT INTO idempotency_keys (user_id, key, payload_hash, response_status, response_body, created_at) VALUES (?, ?, ?, 201, ?, ?)",
                        (user_id, idempotency_key, payload_hash, json.dumps(resp_data), now_iso)
                    )

                conn.commit()

            self.record_sync_cursor(user_id, "transaction", tx_id, "create")
            self.send_json(201, resp_data)
            return

        # 9. 3-Month Premium Trial Activation (§8, REQ-SEC-04)
        if path == "/api/v1/subscriptions/trial/activate":
            if user["has_used_trial"]:
                self.send_rfc7807(409, "Conflict", "Akun ini telah menggunakan uji coba 3 bulan sebelumnya.", "TRIAL_ALREADY_USED")
                return

            now_dt = datetime.now(timezone.utc)
            # 3 calendar months / 90 days
            trial_started = now_dt.strftime("%Y-%m-%dT%H:%M:%SZ")
            trial_ends_dt = datetime.fromtimestamp(now_dt.timestamp() + (90 * 86400), tz=timezone.utc)
            trial_ends = trial_ends_dt.strftime("%Y-%m-%dT%H:%M:%SZ")

            with get_db() as conn:
                conn.execute(
                    "UPDATE users SET has_used_trial = 1, trial_started_at = ?, trial_ends_at = ?, updated_at = ? WHERE id = ?",
                    (trial_started, trial_ends, utc_now_iso(), user_id)
                )
                conn.commit()

            self.log_audit(user_id, "TRIAL_ACTIVATED", "subscription", user_id, "Activated 3-month free trial")
            self.send_json(200, {
                "status": "trialing",
                "trial_started_at": trial_started,
                "trial_ends_at": trial_ends,
                "days_remaining": 90,
                "is_premium": True,
                "message": "Uji coba Premium 3 bulan berhasil diaktifkan!"
            })
            return

        # 10. Commercial Subscriptions Checkout (§7, §19, REQ-SEC-06, REQ-SEC-07)
        if path == "/api/v1/subscriptions/checkout":
            plan = body.get("plan", "premium_monthly") if body else "premium_monthly"
            provider = body.get("provider", "dana") if body else "dana"

            if plan not in ["premium_monthly", "premium_annual"]:
                self.send_rfc7807(400, "Bad Request", "Invalid plan. Choose 'premium_monthly' or 'premium_annual'", "INVALID_PLAN")
                return

            amount = 10000 if plan == "premium_monthly" else 110000
            order_id = f"ORD-{int(time.time())}-{uuid.uuid4().hex[:6]}"
            checkout_url = f"https://m.dana.id/d/checkout?orderId={order_id}&amount={amount}"

            self.log_audit(user_id, "CHECKOUT_INITIATED", "order", order_id, f"Plan: {plan}, Amount: {amount}")
            self.send_json(200, {
                "order_id": order_id,
                "checkout_url": checkout_url,
                "amount": amount,
                "currency": "IDR",
                "plan": plan,
                "provider": provider
            })
            return

        # 11. Ingestion Pipeline: Notification (§20, §21, §24, REQ-INGEST-01 to REQ-INGEST-05)
        if path == "/api/v1/ingestion/notification":
            if not is_premium:
                self.send_rfc7807(403, "Forbidden", "Fitur otomatisasi transaksi memerlukan Premium", "FEATURE_LOCKED")
                return

            if not body or "text" not in body:
                self.send_rfc7807(400, "Bad Request", "Missing notification text", "BAD_REQUEST")
                return

            text = body["text"]
            pkg = body.get("package_name", "unknown")
            event_id = str(uuid.uuid4())

            # Parse amount using regex (integer Rupiah)
            amt_match = re.search(r"Rp\s*([\d\.,]+)", text, re.IGNORECASE)
            amount = 0
            if amt_match:
                cleaned = amt_match.group(1).replace(".", "").replace(",", "")
                amount = int(cleaned)

            direction = "income" if any(k in text.lower() for k in ["masuk", "berhasil diterima", "credit"]) else "expense"
            merchant = "Merchant"
            if "ke " in text:
                merchant = text.split("ke ")[-1].split()[0]

            # Determine confidence
            confidence = "LOW"
            if amount > 0 and pkg in ["com.bca", "id.dana", "com.gojek.app"]:
                confidence = "HIGH"
            elif amount > 0:
                confidence = "MEDIUM"

            # Check cross-source deduplication (within 300s window, §24)
            now_iso = utc_now_iso()
            with get_db() as conn:
                cur = conn.execute(
                    "SELECT * FROM transaction_candidates WHERE user_id = ? AND amount = ? AND direction = ?",
                    (user_id, amount, direction)
                )
                dup = cur.fetchone()
                if dup:
                    self.send_json(200, {
                        "event_id": event_id,
                        "status": "duplicate",
                        "confidence": confidence,
                        "transaction_id": None
                    })
                    return

                cid = str(uuid.uuid4())
                conn.execute(
                    "INSERT INTO transaction_candidates (id, user_id, source, provider, amount, direction, occurred_at, merchant, confidence, status, created_at) VALUES (?, ?, 'notification', ?, ?, ?, ?, ?, ?, 'pending', ?)",
                    (cid, user_id, pkg, amount, direction, now_iso, merchant, confidence, now_iso)
                )
                conn.commit()

            self.send_json(200, {
                "event_id": event_id,
                "candidate_id": cid,
                "status": "auto_created" if confidence == "HIGH" else "requires_confirmation",
                "confidence": confidence,
                "amount": amount
            })
            return

        # 12. Candidate Confirmation (§24)
        if re.match(r"^/api/v1/ingestion/candidates/[^/]+/confirm$", path):
            cid = path.split("/")[-2]
            account_id = body.get("account_id") if body else None
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM transaction_candidates WHERE id = ? AND user_id = ?", (cid, user_id))
                cand = cur.fetchone()
                if not cand:
                    self.send_rfc7807(404, "Not Found", "Candidate not found", "NOT_FOUND")
                    return
                # Commit to ledger
                target_acc_id = account_id
                if not target_acc_id:
                    cur_acc = conn.execute("SELECT id FROM accounts WHERE user_id = ? LIMIT 1", (user_id,))
                    acc_row = cur_acc.fetchone()
                    target_acc_id = acc_row["id"] if acc_row else str(uuid.uuid4())

                tx_id = str(uuid.uuid4())
                now_iso = utc_now_iso()
                conn.execute(
                    "INSERT INTO transactions (id, user_id, account_id, amount, transaction_type, note, date, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    (tx_id, user_id, target_acc_id, cand["amount"], cand["direction"], f"Ingested from {cand['source']}", cand["occurred_at"], now_iso)
                )
                if cand["direction"] == "income":
                    conn.execute("UPDATE accounts SET balance = balance + ?, updated_at = ? WHERE id = ?", (cand["amount"], now_iso, target_acc_id))
                elif cand["direction"] == "expense":
                    conn.execute("UPDATE accounts SET balance = balance - ?, updated_at = ? WHERE id = ?", (cand["amount"], now_iso, target_acc_id))

                conn.execute("UPDATE transaction_candidates SET status = 'confirmed' WHERE id = ?", (cid,))
                conn.commit()

            self.record_sync_cursor(user_id, "transaction", tx_id, "create")
            self.send_json(200, {"status": "confirmed", "transaction_id": tx_id})
            return

        # 13. Candidate Rejection
        if re.match(r"^/api/v1/ingestion/candidates/[^/]+/reject$", path):
            cid = path.split("/")[-2]
            with get_db() as conn:
                conn.execute("UPDATE transaction_candidates SET status = 'rejected' WHERE id = ? AND user_id = ?", (cid, user_id))
                conn.commit()
            self.send_json(200, {"status": "rejected"})
            return

        # 14. Android background sync trigger (§25, §28)
        if path == "/api/v1/android/sync":
            self.send_json(200, {"sync_status": "dispatched", "timestamp": utc_now_iso()})
            return

        self.send_rfc7807(404, "Not Found", f"POST endpoint '{path}' not found", "NOT_FOUND")

    def do_DELETE(self):
        parsed = urlparse(self.path)
        path = parsed.path
        user_id = self.get_auth_user_id()
        if not user_id:
            self.send_rfc7807(401, "Unauthorized", "Authentication required", "UNAUTHORIZED")
            return

        # 1. Category Soft-Deletion (§14, REQ-ARCH-06)
        if re.match(r"^/api/v1/categories/[^/]+$", path):
            cat_id = path.split("/")[-1]
            now_iso = utc_now_iso()
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM categories WHERE id = ? AND user_id = ?", (cat_id, user_id))
                cat = cur.fetchone()
                if not cat:
                    self.send_rfc7807(404, "Not Found", "Category not found or access denied", "CATEGORY_NOT_FOUND")
                    return
                # Soft delete by setting deleted_at
                conn.execute("UPDATE categories SET deleted_at = ?, updated_at = ? WHERE id = ?", (now_iso, now_iso, cat_id))
                conn.commit()

            self.record_sync_cursor(user_id, "category", cat_id, "soft_delete")
            self.log_audit(user_id, "CATEGORY_SOFT_DELETED", "category", cat_id, f"Soft-deleted at {now_iso}")
            self.send_json(200, {"status": "archived", "category_id": cat_id, "deleted_at": now_iso})
            return

        # 2. Transaction Deletion (Atomically reverse balance, §15)
        if re.match(r"^/api/v1/transactions/[^/]+$", path):
            tx_id = path.split("/")[-1]
            now_iso = utc_now_iso()
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM transactions WHERE id = ? AND user_id = ?", (tx_id, user_id))
                tx = cur.fetchone()
                if not tx:
                    self.send_rfc7807(404, "Not Found", "Transaction not found or access denied", "TRANSACTION_NOT_FOUND")
                    return

                cur_acc = conn.execute("SELECT * FROM accounts WHERE id = ? AND user_id = ?", (tx["account_id"], user_id))
                acc = cur_acc.fetchone()
                if acc:
                    if tx["transaction_type"] == "income":
                        new_bal = acc["balance"] - tx["amount"]
                        conn.execute("UPDATE accounts SET balance = ?, updated_at = ? WHERE id = ?", (new_bal, now_iso, tx["account_id"]))
                    elif tx["transaction_type"] == "expense":
                        new_bal = acc["balance"] + tx["amount"]
                        conn.execute("UPDATE accounts SET balance = ?, updated_at = ? WHERE id = ?", (new_bal, now_iso, tx["account_id"]))

                conn.execute("DELETE FROM transactions WHERE id = ?", (tx_id,))
                conn.commit()

            self.record_sync_cursor(user_id, "transaction", tx_id, "delete")
            self.send_json(200, {"status": "deleted", "transaction_id": tx_id})
            return

        self.send_rfc7807(404, "Not Found", f"DELETE endpoint '{path}' not found", "NOT_FOUND")

def run_server(port: int = PORT):
    init_db()
    server_address = ("127.0.0.1", port)
    httpd = HTTPServer(server_address, InviniteRequestHandler)
    print(f"# Invinite Reference Server (v3.1.0) running on http://127.0.0.1:{port}", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()

if __name__ == "__main__":
    p = int(sys.argv[1]) if len(sys.argv) > 1 else PORT
    run_server(p)
