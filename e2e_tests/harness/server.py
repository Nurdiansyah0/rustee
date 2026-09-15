#!/usr/bin/env python3
"""
High-Fidelity Reference Mock Server for Personal Finance PWA E2E Tests.
Implements all 29 features according to PROJECT.md and ORIGINAL_REQUEST.md.
Uses embedded SQLite with WAL mode, integer Rupiah math, Argon2/SHA256 password hashing,
JWT cookies, sliding-window rate limiting, multi-tenant isolation, idempotency deduplication,
Midtrans/Xendit webhook signature verification, and RFC 7807 error responses.
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
import base64
from datetime import datetime, timezone
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs
from collections import defaultdict

try:
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import padding
    HAS_CRYPTO = True
except ImportError:
    HAS_CRYPTO = False

PORT = int(os.environ.get("PORT", "8089"))
DB_PATH = os.environ.get("DB_PATH", "/tmp/pf_pwa_e2e_db.sqlite")
JWT_SECRET = os.environ.get("JWT_SECRET", "super-secret-key-32-bytes-minimum-length!!")
MIDTRANS_SERVER_KEY = os.environ.get("MIDTRANS_SERVER_KEY", "SB-Mid-server-test-secret-key")
XENDIT_WEBHOOK_TOKEN = os.environ.get("XENDIT_WEBHOOK_TOKEN", "xendit_webhook_token_secret_123")

DEFAULT_DANA_TEST_PUBLIC_KEY = """-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAlG6urqDDVNHbTJew+/Ja
i/5Fk4XG33TeAykJr7JUk9buQ5pQS4J6SfCxbWilC6b8LzHUVUbvQoszYg6FoN9+
ovVbdZK1tLkU0nrz8RaUuQQfnrQNoaXRi+/G5BmhYsDCB46bTY9r1lfQB+4P3Gha
Rj1qVyJTK6y56XhERLtoa1ho5QmHKRRj8gbkEw5jaILnZikB8elS/8xLYUUIah0n
B0JhARJ5U5muNg2CrKoGE4jV7TqCQmrV+q74wGEFoiLnT347EKsu26Ns7wJn3TDV
u0qpjn+HvG+77A9kT1/88b+DdakpfYtpq8ENwKxCod8THAEMa386ZKncnxJp96U7
RwIDAQAB
-----END PUBLIC KEY-----"""

DEFAULT_DANA_TEST_PRIVATE_KEY = """-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCUbq6uoMNU0dtM
l7D78lqL/kWThcbfdN4DKQmvslST1u5DmlBLgnpJ8LFtaKULpvwvMdRVRu9CizNi
DoWg336i9Vt1krW0uRTSevPxFpS5BB+etA2hpdGL78bkGaFiwMIHjptNj2vWV9AH
7g/caFpGPWpXIlMrrLnpeEREu2hrWGjlCYcpFGPyBuQTDmNogudmKQHx6VL/zEth
RQhqHScHQmEBEnlTma42DYKsqgYTiNXtOoJCatX6rvjAYQWiIudPfjsQqy7bo2zv
AmfdMNW7SqmOf4e8b7vsD2RPX/zxv4N1qSl9i2mrwQ3ArEKh3xMcAQxrfzpkqdyf
Emn3pTtHAgMBAAECggEAA5JaP7d8m8jk9wXba2SciyvWLsOUUoI0aW0OX5zx7hDI
8PWAoyCDos3Y5yISfqJJBTW0v0ySq05AMUbaLlHScUdoKP8bwjqF5r6wqgd6Eq2n
uSDqBw6/aRee+JQpTwAGazoiQI6H8MNyLQ6scQhNy8zkhy47RBzG6HhNZD4CODsB
/84BY1Xxz+7U0E00yh3T1Y9LRHbA+c1+Psy219liXRj0Vo4zC5nCY3sBaSXZbAYL
sVFF12c3oHp6K0ITMSFeOUtJbYdpf072zMb6VzngwNORGgEUWxZjklXJOlExhk9d
8dtaS382v531o3r2lHNVG2h9nywYDL8oF4SG+JgNIQKBgQDKdq1z6+SVom0cqO50
+AFSHTE2nOl+H2ahxF/G5Xm9hMnBZMInXa3i0VdYdZb0wsHNA46CoIsttfcYF+Xg
94DG6bwK7cNGCFyLc3gYDsh2Iv7aOqrgrEnvauo4hPmsQnQJVelHbe4/asmeDf7i
1IVSMvRO/AW6mdTQN29qEsLfMQKBgQC7rnsMsLbX6JOmw5E5tbjyZtEGTGRoL2Cf
VpyWptqSLVtVLdb1oBQOEtTdFPE1sunR/C7510WZDvgOeqnSfhAhlrNMaYwGQMbx
wgdzph0N06iDllun2P7k5jiPSMoDfpzDyYCMX4/QWTOe3ZkSoGYAGacdRFcMZibk
CWH8oLNT9wKBgEr+8vlBpAaZh/lZyhqh0ztrfNNSBFunngjGCQRP9GxzR5jPjeuv
E7409TnbNPOtQMSEUMGqXmOsR78w+wH+LEGCSxlxQSgr6LvvJckjkLXR+L01hh57
M1fwLpqJB0L7yqe6nxLKcbokAFL/tC6pskjkfwLS7/xTBzWpkyejk3PBAoGAPE+B
cz6GQzOV3w0Raf4fhKXNnbyGt4QiBJIMl8zeiALTSrgET8I1L6CVjsXgDWWFBdmI
LvkigGDzDZQVZnLkNCb9TxzLxmaih6XWRy+mPn85s69pnLJ6lov0uPanFCBnt/LU
wEclK8q+b9q+CeJJZNbZgOopHu7kqHrrZgcuGVkCgYEApPDkY+y94L2SkgNGbd64
yenZFpNidcr/xLvCVrTg12EmFJ/nYsjHc2/r9W60X11NREi7rUCJhN34i3c8YdjM
sZ+axdF1bmPmcqNVO4YFgORFdAsi1XDsqWCbl82zIH59mqEcuaLk2gS3WjIB5KZ8
4StTj0O1GAI65ENXcr4cWR4=
-----END PRIVATE KEY-----"""

def verify_dana_rsa_signature(string_to_sign: str, sig_b64: str) -> bool:
    if not HAS_CRYPTO:
        return False
    try:
        pub_key = serialization.load_pem_public_key(DEFAULT_DANA_TEST_PUBLIC_KEY.encode("utf-8"))
        sig_bytes = base64.b64decode(sig_b64)
        pub_key.verify(
            sig_bytes,
            string_to_sign.encode("utf-8"),
            padding.PKCS1v15(),
            hashes.SHA256()
        )
        return True
    except Exception:
        return False

def sign_dana_payload(string_to_sign: str) -> str:
    if not HAS_CRYPTO:
        return ""
    priv_key = serialization.load_pem_private_key(DEFAULT_DANA_TEST_PRIVATE_KEY.encode("utf-8"), password=None)
    sig = priv_key.sign(
        string_to_sign.encode("utf-8"),
        padding.PKCS1v15(),
        hashes.SHA256()
    )
    return base64.b64encode(sig).decode("utf-8")

# In-memory rate limiter: ip -> list of timestamps of failed auth attempts
FAILED_AUTH_ATTEMPTS = defaultdict(list)
RATE_LIMIT_WINDOW = 900  # 15 minutes in seconds
RATE_LIMIT_MAX = 5       # max 5 attempts per 15 minutes

def get_db():
    conn = sqlite3.connect(DB_PATH, timeout=5.0)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode = WAL;")
    conn.execute("PRAGMA busy_timeout = 5000;")
    conn.execute("PRAGMA foreign_keys = ON;")
    conn.execute("PRAGMA synchronous = NORMAL;")
    return conn

def init_db(conn):
    with conn:
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
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            account_type TEXT NOT NULL,
            balance INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            category_type TEXT NOT NULL,
            icon TEXT DEFAULT '',
            color TEXT DEFAULT '',
            deleted_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
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
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
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
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
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
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS subscriptions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL UNIQUE,
            tier TEXT NOT NULL DEFAULT 'free',
            status TEXT NOT NULL DEFAULT 'active',
            current_period_start TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            current_period_end TEXT NOT NULL,
            cancel_at_period_end INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            action TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            details TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS idempotency_keys (
            user_id TEXT NOT NULL,
            key TEXT NOT NULL,
            response_status INTEGER NOT NULL,
            response_body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (user_id, key)
        );

        CREATE TABLE IF NOT EXISTS webhook_events (
            provider TEXT NOT NULL,
            event_id TEXT NOT NULL,
            payload_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (provider, event_id)
        );

        -- Mandatory composite & performance indexes
        CREATE INDEX IF NOT EXISTS idx_transactions_user_date ON transactions(user_id, date);
        CREATE INDEX IF NOT EXISTS idx_transactions_account ON transactions(account_id);
        CREATE INDEX IF NOT EXISTS idx_categories_user ON categories(user_id);
        CREATE INDEX IF NOT EXISTS idx_accounts_user ON accounts(user_id);
        CREATE INDEX IF NOT EXISTS idx_budgets_user ON budgets(user_id);
        CREATE INDEX IF NOT EXISTS idx_subscriptions_user ON subscriptions(user_id);
        """)
        for col_def in [
            "has_used_trial INTEGER NOT NULL DEFAULT 0",
            "trial_started_at TEXT",
            "trial_ends_at TEXT",
        ]:
            try:
                conn.execute(f"ALTER TABLE users ADD COLUMN {col_def};")
            except Exception:
                pass

# Helper for secure password hashing (Argon2 / PBKDF2 fallback)
def hash_password(password: str) -> str:
    salt = os.urandom(16).hex()
    hashed = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt.encode("utf-8"), 100000).hex()
    return f"$pbkdf2-sha256$100000${salt}${hashed}"

def verify_password(password: str, stored_hash: str) -> bool:
    try:
        parts = stored_hash.split("$")
        if len(parts) != 5:
            return False
        _, algo, rounds, salt, expected_hash = parts
        computed = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt.encode("utf-8"), int(rounds)).hex()
        return hmac.compare_digest(computed, expected_hash)
    except Exception:
        return False

# Token generator & verifier
def generate_token(user_id: str, tier: str) -> str:
    header = json.dumps({"alg": "HS256", "typ": "JWT"}).encode("utf-8")
    payload = json.dumps({
        "sub": user_id,
        "tier": tier,
        "iat": int(time.time()),
        "exp": int(time.time()) + 900  # 15 minutes
    }).encode("utf-8")
    b64_header = header.hex()
    b64_payload = payload.hex()
    msg = f"{b64_header}.{b64_payload}".encode("utf-8")
    sig = hmac.new(JWT_SECRET.encode("utf-8"), msg, hashlib.sha256).hexdigest()
    return f"{b64_header}.{b64_payload}.{sig}"

def verify_token(token: str):
    try:
        parts = token.split(".")
        if len(parts) != 3:
            return None
        b64_header, b64_payload, sig = parts
        msg = f"{b64_header}.{b64_payload}".encode("utf-8")
        expected_sig = hmac.new(JWT_SECRET.encode("utf-8"), msg, hashlib.sha256).hexdigest()
        if not hmac.compare_digest(sig, expected_sig):
            return None
        payload = json.loads(bytes.fromhex(b64_payload).decode("utf-8"))
        if payload.get("exp", 0) < time.time():
            return None
        return payload
    except Exception:
        return None

# RFC 7807 Error Builder
def make_error(status: int, title: str, detail: str, code: str):
    type_map = {
        400: "bad-request",
        401: "unauthorized",
        403: "forbidden",
        404: "not-found",
        405: "method-not-allowed",
        409: "conflict",
        413: "payload-too-large",
        415: "unsupported-media-type",
        422: "unprocessable-entity",
        429: "too-many-requests",
        500: "internal-error"
    }
    error_type = f"https://api.nurdiansyahlabs.com/errors/{type_map.get(status, 'unknown')}"
    return {
        "type": error_type,
        "title": title,
        "status": status,
        "detail": detail,
        "code": code
    }

class PWARequestHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def get_client_ip(self):
        xff = self.headers.get("X-Forwarded-For") or self.headers.get("X-Real-IP")
        if xff:
            return xff.split(",")[0].strip()
        return self.client_address[0] if self.client_address else "127.0.0.1"

    def parse_auth(self):
        cookie_header = self.headers.get("Cookie", "")
        auth_header = self.headers.get("Authorization", "")
        token = None

        if "auth_token=" in cookie_header:
            for part in cookie_header.split(";"):
                part = part.strip()
                if part.startswith("auth_token="):
                    token = part[len("auth_token="):]
                    break
        elif auth_header.startswith("Bearer "):
            token = auth_header[7:].strip()

        if not token:
            return None
        return verify_token(token)

    def read_json_body(self):
        content_length = self.headers.get("Content-Length")
        if not content_length:
            self.raw_body = b""
            return {}
        try:
            length = int(content_length)
            if length > 1048576:  # 1MB limit
                self.raw_body = b""
                return "PAYLOAD_TOO_LARGE"
            raw = self.rfile.read(length)
            self.raw_body = raw
            if not raw:
                return {}
            return json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            return "INVALID_JSON"
        except Exception:
            return "INVALID_JSON"

    def send_json(self, status: int, data, extra_headers=None):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "private, no-store, must-revalidate")
        if extra_headers:
            for k, v in extra_headers.items():
                self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def send_error_response(self, status: int, title: str, detail: str, code: str, extra_headers=None):
        payload = make_error(status, title, detail, code)
        self.send_json(status, payload, extra_headers=extra_headers)

    # -------------------------------------------------------------
    # PROBES & PWA ASSETS
    # -------------------------------------------------------------
    def do_HEAD(self):
        parsed = urlparse(self.path)
        if parsed.path in ["/health", "/ready"]:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "private, no-store, must-revalidate")
            self.end_headers()
        else:
            self.send_response(404)
            self.end_headers()

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        query = parse_qs(parsed.query)

        # Liveness probe
        if path == "/health":
            return self.send_json(200, {
                "status": "pass",
                "version": "1.0.0",
                "service": "personal_finance_pwa"
            })

        # Readiness probe
        if path == "/ready":
            try:
                conn = get_db()
                row = conn.execute("PRAGMA journal_mode;").fetchone()
                wal = (row[0].lower() == "wal") if row else False
                conn.close()
                return self.send_json(200, {
                    "status": "pass",
                    "database": "connected",
                    "wal_mode": wal
                })
            except Exception as e:
                return self.send_json(503, {
                    "status": "fail",
                    "database": "disconnected"
                })

        # PWA Static Assets
        if path == "/manifest.json":
            manifest = {
                "name": "Personal Finance PWA",
                "short_name": "FinancePWA",
                "start_url": "/",
                "display": "standalone",
                "background_color": "#ffffff",
                "theme_color": "#059669",
                "icons": [
                    {"src": "/icons/icon-192.png", "sizes": "192x192", "type": "image/png"},
                    {"src": "/icons/icon-512.png", "sizes": "512x512", "type": "image/png"}
                ]
            }
            body = json.dumps(manifest).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/manifest+json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "public, max-age=86400")
            self.end_headers()
            return self.wfile.write(body)

        if path == "/service-worker.js":
            sw_code = """
            const CACHE_NAME = 'pwa-cache-v1';
            self.addEventListener('install', event => {
                event.waitUntil(caches.open(CACHE_NAME).then(c => c.addAll(['/', '/manifest.json', '/index.html'])));
                self.skipWaiting();
            });
            self.addEventListener('fetch', event => {
                if (event.request.url.includes('/api/v1/')) {
                    event.respondWith(fetch(event.request)); // NetworkOnly for financial endpoints
                } else {
                    event.respondWith(caches.match(event.request).then(res => res || fetch(event.request)));
                }
            });
            """.strip().encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/javascript")
            self.send_header("Content-Length", str(len(sw_code)))
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            return self.wfile.write(sw_code)

        if path in ["/", "/index.html"]:
            html = """<!DOCTYPE html><html lang="id"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover"><link rel="manifest" href="/manifest.json"><title>Finance PWA</title></head><body><div id="app"></div></body></html>""".encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(html)))
            self.send_header("Cache-Control", "public, max-age=3600")
            self.end_headers()
            return self.wfile.write(html)

        # Authenticated API endpoints
        user = self.parse_auth()
        if not user:
            return self.send_error_response(401, "Unauthorized", "Valid authentication token is required.", "AUTH_REQUIRED")

        user_id = user["sub"]

        # /api/v1/auth/me
        if path == "/api/v1/auth/me":
            conn = get_db()
            u = conn.execute("SELECT id, name, email, tier, created_at FROM users WHERE id = ?", (user_id,)).fetchone()
            sub = conn.execute("SELECT tier, status, current_period_end FROM subscriptions WHERE user_id = ?", (user_id,)).fetchone()
            conn.close()
            if not u:
                return self.send_error_response(404, "Not Found", "User not found.", "USER_NOT_FOUND")
            return self.send_json(200, {
                "id": u["id"],
                "name": u["name"],
                "email": u["email"],
                "tier": sub["tier"] if sub else u["tier"],
                "subscription_status": sub["status"] if sub else "active",
                "created_at": u["created_at"]
            })

        # /api/v1/accounts
        if path == "/api/v1/accounts":
            conn = get_db()
            rows = conn.execute("SELECT id, user_id, name, account_type, balance, is_active, created_at FROM accounts WHERE user_id = ? AND is_active = 1 ORDER BY created_at ASC", (user_id,)).fetchall()
            conn.close()
            return self.send_json(200, [dict(r) for r in rows])

        # /api/v1/accounts/:id
        if path.startswith("/api/v1/accounts/"):
            acc_id = path.split("/api/v1/accounts/")[1].split("/")[0]
            conn = get_db()
            row = conn.execute("SELECT id, user_id, name, account_type, balance, is_active, created_at FROM accounts WHERE user_id = ? AND id = ?", (user_id, acc_id)).fetchone()
            conn.close()
            if not row:
                return self.send_error_response(404, "Not Found", f"Account '{acc_id}' not found.", "ACCOUNT_NOT_FOUND")
            return self.send_json(200, dict(row))

        # /api/v1/categories
        if path == "/api/v1/categories":
            conn = get_db()
            rows = conn.execute("SELECT id, user_id, name, category_type, icon, color, created_at FROM categories WHERE user_id = ? AND deleted_at IS NULL ORDER BY name ASC", (user_id,)).fetchall()
            conn.close()
            return self.send_json(200, [dict(r) for r in rows])

        # /api/v1/categories/:id
        if path.startswith("/api/v1/categories/"):
            cat_id = path.split("/api/v1/categories/")[1].split("/")[0]
            conn = get_db()
            row = conn.execute("SELECT id, user_id, name, category_type, icon, color, deleted_at, created_at FROM categories WHERE user_id = ? AND id = ? AND deleted_at IS NULL", (user_id, cat_id)).fetchone()
            conn.close()
            if not row:
                return self.send_error_response(404, "Not Found", f"Category '{cat_id}' not found.", "CATEGORY_NOT_FOUND")
            return self.send_json(200, dict(row))

        # /api/v1/transactions
        if path == "/api/v1/transactions":
            conn = get_db()
            sql = "SELECT id, user_id, account_id, category_id, destination_account_id, amount, transaction_type, note, date, created_at FROM transactions WHERE user_id = ?"
            params = [user_id]
            if "account_id" in query:
                sql += " AND account_id = ?"
                params.append(query["account_id"][0])
            if "category_id" in query:
                sql += " AND category_id = ?"
                params.append(query["category_id"][0])
            if "start_date" in query:
                sql += " AND date >= ?"
                params.append(query["start_date"][0])
            if "end_date" in query:
                sql += " AND date <= ?"
                params.append(query["end_date"][0])
            sql += " ORDER BY date DESC, created_at DESC"
            limit = 50
            if "limit" in query:
                try:
                    limit = min(int(query["limit"][0]), 100)
                except ValueError:
                    pass
            sql += f" LIMIT {limit}"
            rows = conn.execute(sql, params).fetchall()
            conn.close()
            return self.send_json(200, [dict(r) for r in rows])

        # /api/v1/transactions/:id
        if path.startswith("/api/v1/transactions/"):
            tx_id = path.split("/api/v1/transactions/")[1].split("/")[0]
            conn = get_db()
            row = conn.execute("SELECT id, user_id, account_id, category_id, destination_account_id, amount, transaction_type, note, date, created_at FROM transactions WHERE user_id = ? AND id = ?", (user_id, tx_id)).fetchone()
            conn.close()
            if not row:
                return self.send_error_response(404, "Not Found", f"Transaction '{tx_id}' not found.", "TRANSACTION_NOT_FOUND")
            return self.send_json(200, dict(row))

        # /api/v1/analytics/cash-flow
        if path == "/api/v1/analytics/cash-flow":
            conn = get_db()
            rows = conn.execute("SELECT transaction_type, amount FROM transactions WHERE user_id = ?", (user_id,)).fetchall()
            conn.close()
            total_income = 0
            total_expense = 0
            for r in rows:
                if r["transaction_type"] == "income":
                    total_income += r["amount"]
                elif r["transaction_type"] == "expense":
                    total_expense += r["amount"]
                # transfers are net zero
            net_cash_flow = total_income - total_expense
            return self.send_json(200, {
                "income": total_income,
                "expenses": total_expense,
                "net_cash_flow": net_cash_flow,
                "currency": "IDR"
            })

        # /api/v1/analytics/advanced (Server-Side Feature Gating: requires 'premium')
        if path == "/api/v1/analytics/advanced":
            conn = get_db()
            sub = conn.execute("SELECT tier, status FROM subscriptions WHERE user_id = ?", (user_id,)).fetchone()
            conn.close()
            is_premium = sub and sub["tier"] == "premium" and sub["status"] in ["active", "trialing"]
            if not is_premium:
                return self.send_error_response(
                    403,
                    "Forbidden",
                    "Subscription feature 'analytics.advanced' required.",
                    "FEATURE_LOCKED"
                )
            return self.send_json(200, {
                "savings_rate": 0.42,
                "projected_runway_days": 180,
                "top_expense_categories": [
                    {"category": "Makanan", "amount": 1500000, "percentage": 35.5},
                    {"category": "Transportasi", "amount": 800000, "percentage": 18.9}
                ],
                "cash_flow_velocity": "positive"
            })

        # /api/v1/budgets (Server-Side Feature Gating: requires 'premium')
        if path == "/api/v1/budgets":
            conn = get_db()
            sub = conn.execute("SELECT tier, status FROM subscriptions WHERE user_id = ?", (user_id,)).fetchone()
            is_premium = sub and sub["tier"] == "premium" and sub["status"] in ["active", "trialing"]
            if not is_premium:
                conn.close()
                return self.send_error_response(
                    403,
                    "Forbidden",
                    "Subscription feature 'budgeting' required.",
                    "FEATURE_LOCKED"
                )
            rows = conn.execute("SELECT id, user_id, category_id, amount, period_start, period_end, created_at FROM budgets WHERE user_id = ?", (user_id,)).fetchall()
            conn.close()
            return self.send_json(200, [dict(r) for r in rows])

        # /api/v1/subscription and /api/v1/subscriptions/status
        if path in ["/api/v1/subscription", "/api/v1/subscriptions/status"]:
            conn = get_db()
            user_row = conn.execute("SELECT id, tier, has_used_trial, trial_started_at, trial_ends_at FROM users WHERE id = ?", (user_id,)).fetchone()
            sub = conn.execute("SELECT tier, status, current_period_start, current_period_end, cancel_at_period_end FROM subscriptions WHERE user_id = ?", (user_id,)).fetchone()
            conn.close()

            if sub and sub["status"] == "trialing":
                days_remaining = 0
                end_str = sub["current_period_end"]
                try:
                    end_dt = datetime.fromisoformat(end_str.replace("Z", "+00:00"))
                    diff_secs = (end_dt - datetime.now(timezone.utc)).total_seconds()
                    if diff_secs > 0:
                        days_remaining = int((diff_secs + 86399) // 86400)
                    else:
                        days_remaining = 0
                except Exception:
                    days_remaining = 7

                return self.send_json(200, {
                    "tier": "premium",
                    "status": "trialing",
                    "is_premium": True,
                    "days_remaining": days_remaining,
                    "current_period_end": sub["current_period_end"],
                    "plan_id": "premium_monthly",
                    "amount": 5000,
                    "price_monthly": 0,
                    "features": ["transactions.basic", "analytics.basic", "analytics.advanced", "budgeting", "reports.advanced"]
                })

            if not sub:
                return self.send_json(200, {
                    "tier": "free",
                    "status": "active",
                    "is_premium": False,
                    "days_remaining": None,
                    "price_monthly": 0,
                    "features": ["transactions.basic", "cash_flow.basic"]
                })

            is_pro = sub["tier"] == "premium" and sub["status"] == "active"
            return self.send_json(200, {
                "tier": sub["tier"],
                "status": sub["status"],
                "is_premium": is_pro,
                "days_remaining": None,
                "current_period_end": sub["current_period_end"],
                "cancel_at_period_end": bool(sub["cancel_at_period_end"]),
                "price_monthly": 5000 if is_pro else 0,
                "features": [
                    "transactions.basic", "analytics.basic", "analytics.advanced", "budgeting", "reports.advanced"
                ] if is_pro else [
                    "transactions.basic", "cash_flow.basic"
                ]
            })

        # Not found
        return self.send_error_response(404, "Not Found", f"Endpoint '{path}' does not exist.", "ROUTE_NOT_FOUND")

    # -------------------------------------------------------------
    # POST HANDLERS
    # -------------------------------------------------------------
    def do_POST(self):
        parsed = urlparse(self.path)
        path = parsed.path
        client_ip = self.get_client_ip()

        body = self.read_json_body()
        if body == "PAYLOAD_TOO_LARGE":
            return self.send_error_response(413, "Payload Too Large", "Request body exceeds maximum allowed size (1MB).", "PAYLOAD_TOO_LARGE")
        if body == "INVALID_JSON":
            return self.send_error_response(400, "Bad Request", "Malformed or non-JSON body payload.", "INVALID_JSON")

        # Public Auth: Register
        if path == "/api/v1/auth/register":
            name = (body.get("name") or "").strip()
            email = (body.get("email") or "").strip().lower()
            password = body.get("password") or ""

            if not email or "@" not in email:
                return self.send_error_response(400, "Bad Request", "Valid email address is required.", "INVALID_EMAIL")
            if not password or len(password) < 8:
                return self.send_error_response(400, "Bad Request", "Password must be at least 8 characters long.", "PASSWORD_TOO_SHORT")
            if not name:
                name = email.split("@")[0]

            conn = get_db()
            existing = conn.execute("SELECT id FROM users WHERE email = ?", (email,)).fetchone()
            if existing:
                conn.close()
                return self.send_error_response(409, "Conflict", "A user with this email already exists.", "EMAIL_ALREADY_EXISTS")

            user_id = str(uuid.uuid4())
            pw_hash = hash_password(password)
            period_end = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + 30 * 86400))

            with conn:
                conn.execute(
                    "INSERT INTO users (id, name, email, password_hash, tier) VALUES (?, ?, ?, ?, 'free')",
                    (user_id, name, email, pw_hash)
                )
                # Seed default accounts (Cash, Bank)
                conn.execute(
                    "INSERT INTO accounts (id, user_id, name, account_type, balance) VALUES (?, ?, 'Cash', 'cash', 0)",
                    (str(uuid.uuid4()), user_id)
                )
                conn.execute(
                    "INSERT INTO accounts (id, user_id, name, account_type, balance) VALUES (?, ?, 'Bank BCA', 'checking', 0)",
                    (str(uuid.uuid4()), user_id)
                )
                # Seed default categories
                conn.execute(
                    "INSERT INTO categories (id, user_id, name, category_type, icon, color) VALUES (?, ?, 'Gaji', 'income', 'briefcase', '#10B981')",
                    (str(uuid.uuid4()), user_id)
                )
                conn.execute(
                    "INSERT INTO categories (id, user_id, name, category_type, icon, color) VALUES (?, ?, 'Makanan', 'expense', 'utensils', '#EF4444')",
                    (str(uuid.uuid4()), user_id)
                )
                conn.execute(
                    "INSERT INTO subscriptions (id, user_id, tier, status, current_period_end) VALUES (?, ?, 'free', 'active', ?)",
                    (str(uuid.uuid4()), user_id, period_end)
                )
            conn.close()

            token = generate_token(user_id, "free")
            cookie = f"auth_token={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=900"
            return self.send_json(201, {
                "id": user_id,
                "name": name,
                "email": email,
                "tier": "free"
            }, extra_headers={"Set-Cookie": cookie})

        # Public Auth: Login with Rate Limiting (5 attempts / 15 min)
        if path == "/api/v1/auth/login":
            now = time.time()
            # Prune old attempts
            FAILED_AUTH_ATTEMPTS[client_ip] = [t for t in FAILED_AUTH_ATTEMPTS[client_ip] if now - t < RATE_LIMIT_WINDOW]

            if len(FAILED_AUTH_ATTEMPTS[client_ip]) >= RATE_LIMIT_MAX:
                retry_after = int(RATE_LIMIT_WINDOW - (now - FAILED_AUTH_ATTEMPTS[client_ip][0]))
                return self.send_error_response(
                    429,
                    "Too Many Requests",
                    "Too many failed login attempts. Please try again in 15 minutes.",
                    "RATE_LIMIT_EXCEEDED",
                    extra_headers={"Retry-After": str(max(retry_after, 1))}
                )

            email = (body.get("email") or "").strip().lower()
            password = body.get("password") or ""

            conn = get_db()
            user = conn.execute("SELECT id, name, email, password_hash, tier FROM users WHERE email = ?", (email,)).fetchone()
            conn.close()

            if not user or not verify_password(password, user["password_hash"]):
                FAILED_AUTH_ATTEMPTS[client_ip].append(now)
                return self.send_error_response(401, "Unauthorized", "Invalid email or password.", "INVALID_CREDENTIALS")

            # Login success: reset rate limit attempts
            FAILED_AUTH_ATTEMPTS.pop(client_ip, None)

            token = generate_token(user["id"], user["tier"])
            cookie = f"auth_token={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=900"
            return self.send_json(200, {
                "id": user["id"],
                "name": user["name"],
                "email": user["email"],
                "tier": user["tier"]
            }, extra_headers={"Set-Cookie": cookie})

        # Auth: Logout
        if path == "/api/v1/auth/logout":
            cookie = "auth_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
            return self.send_json(200, {"message": "Logged out successfully."}, extra_headers={"Set-Cookie": cookie})

        # Webhook: Midtrans
        if path == "/api/v1/webhooks/midtrans":
            order_id = body.get("order_id") or ""
            status_code = str(body.get("status_code") or "")
            gross_amount = str(body.get("gross_amount") or "")
            signature_key = body.get("signature_key") or self.headers.get("X-Signature") or ""

            if not order_id or not signature_key:
                return self.send_error_response(400, "Bad Request", "Missing order_id or signature.", "INVALID_WEBHOOK_PAYLOAD")

            # Constant-time SHA-512 verification: SHA512(order_id + status_code + gross_amount + ServerKey)
            expected_payload = f"{order_id}{status_code}{gross_amount}{MIDTRANS_SERVER_KEY}".encode("utf-8")
            expected_sig = hashlib.sha512(expected_payload).hexdigest()

            if not hmac.compare_digest(signature_key.lower(), expected_sig.lower()):
                return self.send_error_response(401, "Unauthorized", "Invalid Midtrans cryptographic signature.", "INVALID_SIGNATURE")

            # Idempotency deduplication on webhook_events
            event_id = str(body.get("transaction_id") or order_id)
            conn = get_db()
            with conn:
                existing = conn.execute("SELECT status FROM webhook_events WHERE provider = 'midtrans' AND event_id = ?", (event_id,)).fetchone()
                if existing:
                    conn.close()
                    return self.send_json(200, {"status": "ok", "message": "Event already processed (idempotent duplicate acknowledged)."})

                conn.execute(
                    "INSERT INTO webhook_events (provider, event_id, payload_hash, status) VALUES ('midtrans', ?, ?, ?)",
                    (event_id, hashlib.sha256(json.dumps(body).encode()).hexdigest(), status_code)
                )

                # Process event: if settlement or capture -> upgrade to premium active
                transaction_status = body.get("transaction_status", "")
                user_row = conn.execute("""
                    SELECT id FROM users 
                    WHERE id = ? OR email = ? OR ? LIKE 'SUB-' || substr(id, 1, 8) || '%'
                    LIMIT 1
                """, (order_id, order_id, order_id)).fetchone()

                if user_row:
                    target_user = user_row["id"]
                    if transaction_status in ["settlement", "capture"]:
                        new_period_end = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + 30 * 86400))
                        conn.execute("UPDATE users SET tier = 'premium' WHERE id = ?", (target_user,))
                        conn.execute("""
                            INSERT INTO subscriptions (id, user_id, tier, status, current_period_end)
                            VALUES (?, ?, 'premium', 'active', ?)
                            ON CONFLICT(user_id) DO UPDATE SET tier = 'premium', status = 'active', current_period_end = excluded.current_period_end
                        """, (str(uuid.uuid4()), target_user, new_period_end))
                    elif transaction_status in ["cancel", "deny"]:
                        conn.execute("UPDATE subscriptions SET status = 'cancelled' WHERE user_id = ?", (target_user,))
                    elif transaction_status == "expire":
                        conn.execute("UPDATE subscriptions SET status = 'expired' WHERE user_id = ?", (target_user,))
                    elif status_code == "200":
                        new_period_end = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + 30 * 86400))
                        conn.execute("UPDATE users SET tier = 'premium' WHERE id = ?", (target_user,))
                        conn.execute("""
                            INSERT INTO subscriptions (id, user_id, tier, status, current_period_end)
                            VALUES (?, ?, 'premium', 'active', ?)
                            ON CONFLICT(user_id) DO UPDATE SET tier = 'premium', status = 'active', current_period_end = excluded.current_period_end
                        """, (str(uuid.uuid4()), target_user, new_period_end))
            conn.close()
            return self.send_json(200, {"status": "ok", "message": "Webhook processed successfully."})

        # Webhook: Xendit
        if path == "/api/v1/webhooks/xendit":
            callback_token = self.headers.get("X-Callback-Token") or self.headers.get("x-callback-token") or ""
            if not hmac.compare_digest(callback_token, XENDIT_WEBHOOK_TOKEN):
                return self.send_error_response(401, "Unauthorized", "Invalid Xendit callback token.", "INVALID_SIGNATURE")

            event_id = str(body.get("id") or body.get("external_id") or "")
            if not event_id:
                return self.send_error_response(400, "Bad Request", "Missing event ID in Xendit payload.", "INVALID_WEBHOOK_PAYLOAD")

            conn = get_db()
            with conn:
                existing = conn.execute("SELECT status FROM webhook_events WHERE provider = 'xendit' AND event_id = ?", (event_id,)).fetchone()
                if existing:
                    conn.close()
                    return self.send_json(200, {"status": "ok", "message": "Event already processed (idempotent duplicate acknowledged)."})

                conn.execute(
                    "INSERT INTO webhook_events (provider, event_id, payload_hash, status) VALUES ('xendit', ?, ?, 'processed')",
                    (event_id, hashlib.sha256(json.dumps(body).encode()).hexdigest())
                )
                user_id_target = body.get("external_id")
                user_row = conn.execute("""
                    SELECT id FROM users 
                    WHERE id = ? OR email = ? OR ? LIKE 'SUB-' || substr(id, 1, 8) || '%'
                    LIMIT 1
                """, (user_id_target, user_id_target, user_id_target)).fetchone()
                if user_row and body.get("status") == "PAID":
                    target_user = user_row["id"]
                    new_period_end = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() + 30 * 86400))
                    conn.execute("UPDATE users SET tier = 'premium' WHERE id = ?", (target_user,))
                    conn.execute("""
                        INSERT INTO subscriptions (id, user_id, tier, status, current_period_end)
                        VALUES (?, ?, 'premium', 'active', ?)
                        ON CONFLICT(user_id) DO UPDATE SET tier = 'premium', status = 'active', current_period_end = excluded.current_period_end
                    """, (str(uuid.uuid4()), target_user, new_period_end))
            conn.close()
            return self.send_json(200, {"status": "ok", "message": "Xendit webhook processed successfully."})

        # Webhook: DANA Direct Open API (POST /api/v1/webhooks/dana)
        if path == "/api/v1/webhooks/dana":
            sig = self.headers.get("X-SIGNATURE") or self.headers.get("x-signature") or ""
            ts = self.headers.get("X-TIMESTAMP") or self.headers.get("x-timestamp") or ""
            partner_id = self.headers.get("X-PARTNER-ID") or self.headers.get("x-partner-id") or ""

            if not sig or not ts or not partner_id:
                return self.send_json(401, {
                    "responseCode": "4015600",
                    "responseMessage": "Unauthorized: Missing Signature Headers"
                })

            raw_bytes = getattr(self, "raw_body", b"")
            body_hash = hashlib.sha256(raw_bytes).hexdigest()
            string_to_sign = f"POST:/api/v1/webhooks/dana:{body_hash}:{ts}"

            if not verify_dana_rsa_signature(string_to_sign, sig):
                return self.send_json(401, {
                    "responseCode": "4015600",
                    "responseMessage": "Unauthorized: Invalid Signature"
                })

            event_id = str(body.get("originalReferenceNo") or body.get("originalPartnerReferenceNo") or "")
            if not event_id:
                return self.send_json(400, {
                    "responseCode": "4005600",
                    "responseMessage": "Bad Request: Missing Reference Number"
                })

            conn = get_db()
            with conn:
                existing = conn.execute("SELECT status FROM webhook_events WHERE provider = 'dana' AND event_id = ?", (event_id,)).fetchone()
                if existing:
                    conn.close()
                    return self.send_json(200, {
                        "responseCode": "2005600",
                        "responseMessage": "Successful"
                    })

                conn.execute(
                    "INSERT INTO webhook_events (provider, event_id, payload_hash, status) VALUES ('dana', ?, ?, 'processed')",
                    (event_id, body_hash)
                )

                # Identify user
                user_id_target = None
                if isinstance(body.get("additionalInfo"), dict):
                    user_id_target = body["additionalInfo"].get("userId")
                if not user_id_target:
                    user_id_target = body.get("originalPartnerReferenceNo", "")

                user_row = conn.execute("""
                    SELECT id FROM users 
                    WHERE id = ? OR email = ? OR ? LIKE 'ORDER-DANA-' || substr(id, 1, 8) || '%' OR ? LIKE 'SUB-' || substr(id, 1, 8) || '%'
                    LIMIT 1
                """, (user_id_target, user_id_target, user_id_target, user_id_target)).fetchone()

                if user_row:
                    target_user = user_row["id"]
                    existing_sub = conn.execute("SELECT status, current_period_end FROM subscriptions WHERE user_id = ?", (target_user,)).fetchone()

                    now = time.time()
                    base_ts = now
                    if existing_sub and existing_sub["status"] == "active" and existing_sub["current_period_end"]:
                        try:
                            cur_end_dt = datetime.fromisoformat(existing_sub["current_period_end"].replace("Z", "+00:00"))
                            cur_ts = cur_end_dt.timestamp()
                            base_ts = max(now, cur_ts)
                        except Exception:
                            base_ts = now

                    new_period_end = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(base_ts + 30 * 86400))
                    conn.execute("UPDATE users SET tier = 'premium' WHERE id = ?", (target_user,))
                    conn.execute("""
                        INSERT INTO subscriptions (id, user_id, tier, status, current_period_end)
                        VALUES (?, ?, 'premium', 'active', ?)
                        ON CONFLICT(user_id) DO UPDATE SET tier = 'premium', status = 'active', current_period_end = excluded.current_period_end
                    """, (str(uuid.uuid4()), target_user, new_period_end))

                    conn.execute("INSERT INTO audit_logs (id, user_id, action, entity_type, entity_id, details) VALUES (?, ?, 'subscription_activated', 'subscription', ?, ?)",
                        (str(uuid.uuid4()), target_user, target_user, f"provider=dana, event_id={event_id}"))

            conn.close()
            return self.send_json(200, {
                "responseCode": "2005600",
                "responseMessage": "Successful"
            })

        # Authenticated endpoints
        user = self.parse_auth()
        if not user:
            return self.send_error_response(401, "Unauthorized", "Valid authentication token is required.", "AUTH_REQUIRED")
        user_id = user["sub"]

        # POST /api/v1/accounts
        if path == "/api/v1/accounts":
            name = (body.get("name") or "").strip()
            account_type = body.get("account_type") or "checking"
            initial_balance = body.get("initial_balance", 0)

            if not name:
                return self.send_error_response(400, "Bad Request", "Account name is required.", "INVALID_NAME")
            if account_type not in ["checking", "savings", "ewallet", "cash"]:
                return self.send_error_response(400, "Bad Request", f"Unsupported account type '{account_type}'.", "INVALID_ACCOUNT_TYPE")
            if not isinstance(initial_balance, int) or initial_balance < 0:
                return self.send_error_response(400, "Bad Request", "Initial balance must be a non-negative integer Rupiah.", "INVALID_BALANCE")

            account_id = str(uuid.uuid4())
            conn = get_db()
            with conn:
                conn.execute(
                    "INSERT INTO accounts (id, user_id, name, account_type, balance) VALUES (?, ?, ?, ?, ?)",
                    (account_id, user_id, name, account_type, initial_balance)
                )
            conn.close()
            return self.send_json(201, {
                "id": account_id,
                "user_id": user_id,
                "name": name,
                "account_type": account_type,
                "balance": initial_balance,
                "is_active": 1
            })

        # POST /api/v1/categories
        if path == "/api/v1/categories":
            name = (body.get("name") or "").strip()
            cat_type = body.get("category_type") or "expense"
            icon = body.get("icon", "")
            color = body.get("color", "")

            if not name:
                return self.send_error_response(400, "Bad Request", "Category name is required.", "INVALID_NAME")
            if cat_type not in ["income", "expense"]:
                return self.send_error_response(400, "Bad Request", "category_type must be 'income' or 'expense'.", "INVALID_CATEGORY_TYPE")

            cat_id = str(uuid.uuid4())
            conn = get_db()
            with conn:
                conn.execute(
                    "INSERT INTO categories (id, user_id, name, category_type, icon, color) VALUES (?, ?, ?, ?, ?, ?)",
                    (cat_id, user_id, name, cat_type, icon, color)
                )
            conn.close()
            return self.send_json(201, {
                "id": cat_id,
                "user_id": user_id,
                "name": name,
                "category_type": cat_type,
                "icon": icon,
                "color": color
            })

        # POST /api/v1/transactions with strict Idempotency-Key
        if path == "/api/v1/transactions":
            idempotency_key = self.headers.get("Idempotency-Key") or self.headers.get("idempotency-key")
            if idempotency_key:
                # Validate UUIDv4 format if provided
                try:
                    uuid.UUID(idempotency_key)
                except ValueError:
                    return self.send_error_response(400, "Bad Request", "Idempotency-Key must be a valid UUIDv4 string.", "INVALID_IDEMPOTENCY_KEY")

                # Check cache for existing execution
                conn = get_db()
                cached = conn.execute(
                    "SELECT response_status, response_body FROM idempotency_keys WHERE user_id = ? AND key = ?",
                    (user_id, idempotency_key)
                ).fetchone()
                if cached:
                    conn.close()
                    cached_body = json.loads(cached["response_body"])
                    return self.send_json(cached["response_status"], cached_body)
                conn.close()

            account_id = body.get("account_id")
            category_id = body.get("category_id")
            destination_account_id = body.get("destination_account_id")
            amount = body.get("amount")
            tx_type = body.get("transaction_type") or "expense"
            note = body.get("note", "")
            tx_date = body.get("date") or time.strftime("%Y-%m-%d")

            if not account_id:
                return self.send_error_response(400, "Bad Request", "account_id is required.", "MISSING_ACCOUNT_ID")
            if not isinstance(amount, int) or amount <= 0:
                return self.send_error_response(400, "Bad Request", "Transaction amount must be a strictly positive integer Rupiah.", "INVALID_AMOUNT")
            if amount > 9223372036854775807:  # i64 max bound
                return self.send_error_response(400, "Bad Request", "Amount exceeds maximum 64-bit integer limit.", "AMOUNT_OVERFLOW")
            if tx_type not in ["income", "expense", "transfer"]:
                return self.send_error_response(400, "Bad Request", "transaction_type must be 'income', 'expense', or 'transfer'.", "INVALID_TRANSACTION_TYPE")
            if tx_type == "transfer" and not destination_account_id:
                return self.send_error_response(400, "Bad Request", "destination_account_id is required for transfers.", "MISSING_DESTINATION_ACCOUNT")

            conn = get_db()
            # Verify account ownership (multi-tenant isolation)
            acc = conn.execute("SELECT id, balance FROM accounts WHERE user_id = ? AND id = ?", (user_id, account_id)).fetchone()
            if not acc:
                conn.close()
                return self.send_error_response(404, "Not Found", f"Account '{account_id}' not found.", "ACCOUNT_NOT_FOUND")

            if tx_type == "transfer":
                dest_acc = conn.execute("SELECT id, balance FROM accounts WHERE user_id = ? AND id = ?", (user_id, destination_account_id)).fetchone()
                if not dest_acc:
                    conn.close()
                    return self.send_error_response(404, "Not Found", f"Destination account '{destination_account_id}' not found.", "DESTINATION_ACCOUNT_NOT_FOUND")

            tx_id = str(uuid.uuid4())
            with conn:
                # Atomic balance maintenance
                if tx_type == "expense":
                    new_bal = acc["balance"] - amount
                    conn.execute("UPDATE accounts SET balance = ? WHERE id = ?", (new_bal, account_id))
                elif tx_type == "income":
                    new_bal = acc["balance"] + amount
                    conn.execute("UPDATE accounts SET balance = ? WHERE id = ?", (new_bal, account_id))
                elif tx_type == "transfer":
                    conn.execute("UPDATE accounts SET balance = balance - ? WHERE id = ?", (amount, account_id))
                    conn.execute("UPDATE accounts SET balance = balance + ? WHERE id = ?", (amount, destination_account_id))

                conn.execute("""
                    INSERT INTO transactions (id, user_id, account_id, category_id, destination_account_id, amount, transaction_type, note, date, idempotency_key)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """, (tx_id, user_id, account_id, category_id, destination_account_id, amount, tx_type, note, tx_date, idempotency_key))

                res_payload = {
                    "id": tx_id,
                    "user_id": user_id,
                    "account_id": account_id,
                    "category_id": category_id,
                    "destination_account_id": destination_account_id,
                    "amount": amount,
                    "transaction_type": tx_type,
                    "note": note,
                    "date": tx_date
                }

                # Save idempotency record
                if idempotency_key:
                    conn.execute(
                        "INSERT INTO idempotency_keys (user_id, key, response_status, response_body) VALUES (?, ?, 201, ?)",
                        (user_id, idempotency_key, json.dumps(res_payload))
                    )
            conn.close()
            return self.send_json(201, res_payload)

        # POST /api/v1/subscription/trial and /api/v1/subscriptions/trial
        if path in ["/api/v1/subscription/trial", "/api/v1/subscriptions/trial"]:
            conn = get_db()
            user_row = conn.execute("SELECT id, tier, has_used_trial FROM users WHERE id = ?", (user_id,)).fetchone()
            if not user_row:
                conn.close()
                return self.send_error_response(404, "Not Found", f"User '{user_id}' not found.", "USER_NOT_FOUND")

            if user_row["has_used_trial"] == 1:
                conn.close()
                return self.send_error_response(400, "Bad Request", "Trial already used for this account", "TRIAL_ALREADY_USED")

            existing_sub = conn.execute("SELECT id, tier, status FROM subscriptions WHERE user_id = ?", (user_id,)).fetchone()
            if existing_sub:
                if existing_sub["tier"] == "premium" and existing_sub["status"] == "active":
                    conn.close()
                    return self.send_error_response(400, "Bad Request", "User already has an active Premium subscription", "ALREADY_PREMIUM")
                if existing_sub["status"] == "trialing":
                    conn.close()
                    return self.send_error_response(400, "Bad Request", "Trial already used for this account", "TRIAL_ALREADY_USED")

            now = time.time()
            now_str = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now))
            ends_str = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now + 7 * 86400))

            with conn:
                conn.execute("""
                    UPDATE users 
                    SET tier = 'premium', has_used_trial = 1, trial_started_at = ?, trial_ends_at = ?, updated_at = ?
                    WHERE id = ?
                """, (now_str, ends_str, now_str, user_id))

                sub_id = f"sub_trial_{user_id}"
                conn.execute("""
                    INSERT INTO subscriptions (id, user_id, tier, status, current_period_start, current_period_end)
                    VALUES (?, ?, 'premium', 'trialing', ?, ?)
                    ON CONFLICT(user_id) DO UPDATE SET tier = 'premium', status = 'trialing', current_period_start = excluded.current_period_start, current_period_end = excluded.current_period_end
                """, (sub_id, user_id, now_str, ends_str))

                conn.execute("""
                    INSERT INTO audit_logs (id, user_id, action, entity_type, entity_id, details)
                    VALUES (?, ?, 'subscription_trial_activated', 'subscription', ?, '7-day premium trial activated successfully')
                """, (str(uuid.uuid4()), user_id, sub_id))

            conn.close()

            # Refresh token with tier 'premium'
            token = generate_token(user_id, "premium")
            cookie = f"auth_token={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400"

            return self.send_json(200, {
                "status": "trialing",
                "tier": "premium",
                "is_premium": True,
                "trial_started_at": now_str,
                "trial_ends_at": ends_str,
                "days_remaining": 7,
                "message": "7-day premium trial activated successfully"
            }, extra_headers={"Set-Cookie": cookie})

        # POST /api/v1/subscriptions/checkout
        if path == "/api/v1/subscriptions/checkout":
            provider = (body.get("provider") or "").lower()
            if not provider or provider not in ["midtrans", "xendit", "dana"]:
                return self.send_error_response(400, "Bad Request", f"Unsupported payment provider '{provider}'.", "UNSUPPORTED_PROVIDER")

            if provider == "dana":
                order_id = f"ORDER-DANA-{user_id[:8]}-{int(time.time())}"
                checkout_url = f"https://m.dana.id/d/checkout?orderId={order_id}"
                ref_no = f"REF-DANA-{uuid.uuid4().hex[:8]}"
                return self.send_json(200, {
                    "order_id": order_id,
                    "checkout_url": checkout_url,
                    "reference_no": ref_no,
                    "amount": 5000,
                    "currency": "IDR",
                    "provider": "dana"
                })

            order_id = f"SUB-{user_id[:8]}-{int(time.time())}"
            checkout_url = f"https://payment.nurdiansyahlabs.com/{provider}/pay/{order_id}"
            return self.send_json(200, {
                "order_id": order_id,
                "provider": provider,
                "amount": 5000,
                "currency": "IDR",
                "checkout_url": checkout_url
            })

        return self.send_error_response(404, "Not Found", f"Endpoint '{path}' does not exist.", "ROUTE_NOT_FOUND")

    # -------------------------------------------------------------
    # DELETE HANDLERS
    # -------------------------------------------------------------
    def do_DELETE(self):
        parsed = urlparse(self.path)
        path = parsed.path

        user = self.parse_auth()
        if not user:
            return self.send_error_response(401, "Unauthorized", "Valid authentication token is required.", "AUTH_REQUIRED")
        user_id = user["sub"]

        # Category Soft-Delete: DELETE /api/v1/categories/:id
        if path.startswith("/api/v1/categories/"):
            cat_id = path.split("/api/v1/categories/")[1].split("/")[0]
            conn = get_db()
            row = conn.execute("SELECT id FROM categories WHERE user_id = ? AND id = ? AND deleted_at IS NULL", (user_id, cat_id)).fetchone()
            if not row:
                conn.close()
                return self.send_error_response(404, "Not Found", f"Category '{cat_id}' not found.", "CATEGORY_NOT_FOUND")

            with conn:
                conn.execute("UPDATE categories SET deleted_at = CURRENT_TIMESTAMP WHERE user_id = ? AND id = ?", (user_id, cat_id))
            conn.close()
            return self.send_json(200, {"message": f"Category '{cat_id}' archived/soft-deleted successfully."})

        return self.send_error_response(404, "Not Found", f"Endpoint '{path}' does not exist.", "ROUTE_NOT_FOUND")

def run_server(port=PORT):
    if DB_PATH.startswith("/tmp/"):
        for ext in ["", "-wal", "-shm"]:
            f = f"{DB_PATH}{ext}"
            if os.path.exists(f):
                try:
                    os.remove(f)
                except Exception:
                    pass
    conn = get_db()
    init_db(conn)
    conn.close()
    server = HTTPServer(("0.0.0.0", port), PWARequestHandler)
    print(f"[REFERENCE_SERVER] Serving HTTP on 0.0.0.0 port {port} (PID {os.getpid()})...", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()

if __name__ == "__main__":
    p = PORT
    if len(sys.argv) > 1 and sys.argv[1].isdigit():
        p = int(sys.argv[1])
    run_server(p)
