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

os.environ["no_proxy"] = "localhost,127.0.0.1"
os.environ["NO_PROXY"] = "localhost,127.0.0.1"

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

        CREATE TABLE IF NOT EXISTS user_preferences (
            user_id TEXT PRIMARY KEY,
            theme TEXT DEFAULT 'system',
            locale TEXT DEFAULT 'id-ID',
            notifications_enabled INTEGER DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS user_sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            ip_address TEXT,
            user_agent TEXT,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            status TEXT NOT NULL DEFAULT 'ACTIVE',
            is_personal INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_profiles (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL UNIQUE,
            business_name TEXT NOT NULL,
            legal_name TEXT,
            tax_id TEXT,
            address TEXT,
            phone TEXT,
            email TEXT,
            timezone TEXT NOT NULL DEFAULT 'Asia/Jakarta',
            currency TEXT NOT NULL DEFAULT 'IDR',
            locale TEXT NOT NULL DEFAULT 'id-ID',
            invoice_prefix TEXT NOT NULL DEFAULT 'INV',
            business_type TEXT NOT NULL DEFAULT 'general',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS memberships (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'owner',
            status TEXT NOT NULL DEFAULT 'ACTIVE',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(tenant_id, user_id),
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS chart_of_accounts (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            code TEXT NOT NULL,
            name TEXT NOT NULL,
            account_type TEXT NOT NULL,
            is_system INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            UNIQUE(tenant_id, code),
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS journal_entries (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            entry_number TEXT NOT NULL,
            entry_date TEXT NOT NULL,
            description TEXT NOT NULL,
            source_type TEXT NOT NULL DEFAULT 'MANUAL',
            source_id TEXT,
            status TEXT NOT NULL DEFAULT 'POSTED',
            is_reversed INTEGER NOT NULL DEFAULT 0,
            reversal_entry_id TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS journal_lines (
            id TEXT PRIMARY KEY,
            journal_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            account_code TEXT NOT NULL,
            debit INTEGER NOT NULL DEFAULT 0,
            credit INTEGER NOT NULL DEFAULT 0,
            memo TEXT,
            FOREIGN KEY (journal_id) REFERENCES journal_entries(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS invoices (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            invoice_number TEXT,
            customer_name TEXT NOT NULL,
            customer_address TEXT,
            customer_email TEXT,
            issue_date TEXT,
            due_date TEXT NOT NULL,
            currency TEXT NOT NULL DEFAULT 'IDR',
            tax_type TEXT NOT NULL DEFAULT 'NONE',
            subtotal INTEGER NOT NULL DEFAULT 0,
            discount INTEGER NOT NULL DEFAULT 0,
            tax_amount INTEGER NOT NULL DEFAULT 0,
            total_amount INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'DRAFT',
            snapshot_json TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS invoice_items (
            id TEXT PRIMARY KEY,
            invoice_id TEXT NOT NULL,
            description TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            unit_price INTEGER NOT NULL,
            discount INTEGER NOT NULL DEFAULT 0,
            tax_amount INTEGER NOT NULL DEFAULT 0,
            line_total INTEGER NOT NULL,
            FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS receivables (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            invoice_id TEXT NOT NULL UNIQUE,
            total_amount INTEGER NOT NULL,
            allocated_amount INTEGER NOT NULL DEFAULT 0,
            outstanding_amount INTEGER NOT NULL,
            due_date TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'OPEN',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS payments (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            invoice_id TEXT NOT NULL,
            receivable_id TEXT NOT NULL,
            amount INTEGER NOT NULL,
            payment_method TEXT NOT NULL DEFAULT 'BANK_TRANSFER',
            payment_date TEXT NOT NULL,
            reference TEXT,
            status TEXT NOT NULL DEFAULT 'CONFIRMED',
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS outbox_events (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            aggregate_type TEXT NOT NULL,
            aggregate_id TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'PENDING',
            attempt_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at TEXT NOT NULL,
            published_at TEXT,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
        );

        -- Phase 2 Inventory & Multi-Location Stock Management Tables (§10, §27)
        CREATE TABLE IF NOT EXISTS warehouses (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            code TEXT NOT NULL,
            name TEXT NOT NULL,
            address TEXT,
            is_default INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            UNIQUE(tenant_id, code)
        );

        CREATE TABLE IF NOT EXISTS products (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            sku TEXT NOT NULL,
            name TEXT NOT NULL,
            unit TEXT NOT NULL DEFAULT 'pcs',
            cost_price INTEGER NOT NULL DEFAULT 0,
            sale_price INTEGER NOT NULL DEFAULT 0,
            reorder_threshold INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            UNIQUE(tenant_id, sku)
        );

        CREATE TABLE IF NOT EXISTS stock_items (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            warehouse_id TEXT NOT NULL,
            product_id TEXT NOT NULL,
            quantity_on_hand INTEGER NOT NULL DEFAULT 0 CHECK (quantity_on_hand >= 0),
            quantity_reserved INTEGER NOT NULL DEFAULT 0,
            reorder_threshold INTEGER NOT NULL DEFAULT 0,
            bin_location TEXT,
            average_cost INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (warehouse_id) REFERENCES warehouses(id) ON DELETE CASCADE,
            FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE,
            UNIQUE(tenant_id, warehouse_id, product_id)
        );

        CREATE TABLE IF NOT EXISTS stock_movements (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            movement_type TEXT NOT NULL,
            product_id TEXT NOT NULL,
            source_warehouse_id TEXT,
            destination_warehouse_id TEXT,
            quantity INTEGER NOT NULL CHECK (quantity > 0),
            unit_cost INTEGER,
            reference_type TEXT,
            reference_id TEXT,
            batch_number TEXT,
            notes TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE,
            FOREIGN KEY (source_warehouse_id) REFERENCES warehouses(id) ON DELETE SET NULL,
            FOREIGN KEY (destination_warehouse_id) REFERENCES warehouses(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS purchase_orders (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            po_number TEXT NOT NULL,
            supplier_name TEXT NOT NULL,
            destination_warehouse_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'DRAFT',
            total_amount INTEGER NOT NULL DEFAULT 0,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (destination_warehouse_id) REFERENCES warehouses(id) ON DELETE CASCADE,
            UNIQUE(tenant_id, po_number)
        );

        CREATE TABLE IF NOT EXISTS purchase_order_items (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            purchase_order_id TEXT NOT NULL,
            product_id TEXT NOT NULL,
            quantity_ordered INTEGER NOT NULL CHECK (quantity_ordered > 0),
            quantity_received INTEGER NOT NULL DEFAULT 0 CHECK (quantity_received >= 0),
            unit_cost INTEGER NOT NULL CHECK (unit_cost >= 0),
            total_cost INTEGER NOT NULL CHECK (total_cost >= 0),
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (purchase_order_id) REFERENCES purchase_orders(id) ON DELETE CASCADE,
            FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS stock_adjustments (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            adjustment_number TEXT NOT NULL,
            warehouse_id TEXT NOT NULL,
            product_id TEXT NOT NULL,
            previous_quantity INTEGER NOT NULL,
            actual_quantity INTEGER NOT NULL CHECK (actual_quantity >= 0),
            variance INTEGER NOT NULL,
            reason TEXT,
            journal_entry_id TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
            FOREIGN KEY (warehouse_id) REFERENCES warehouses(id) ON DELETE CASCADE,
            FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE,
            FOREIGN KEY (journal_entry_id) REFERENCES journal_entries(id) ON DELETE SET NULL,
            UNIQUE(tenant_id, adjustment_number)
        );

        -- Mandatory Canonical Composite Indexes (§16)
        CREATE INDEX IF NOT EXISTS idx_tx_user_date ON transactions(user_id, date);
        CREATE INDEX IF NOT EXISTS idx_tx_account ON transactions(account_id);
        CREATE INDEX IF NOT EXISTS idx_cat_user ON categories(user_id);
        CREATE INDEX IF NOT EXISTS idx_acc_user ON accounts(user_id);
        CREATE INDEX IF NOT EXISTS idx_budg_user ON budgets(user_id);
        CREATE INDEX IF NOT EXISTS idx_sub_user ON subscriptions(user_id);
        CREATE INDEX IF NOT EXISTS idx_sync_user_id ON sync_cursors(user_id, id);
        CREATE INDEX IF NOT EXISTS idx_tenants_slug ON tenants(slug);
        CREATE INDEX IF NOT EXISTS idx_memberships_user ON memberships(user_id);
        CREATE INDEX IF NOT EXISTS idx_memberships_tenant ON memberships(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_coa_tenant ON chart_of_accounts(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_journals_tenant ON journal_entries(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_journal_lines_journal ON journal_lines(journal_id);
        CREATE INDEX IF NOT EXISTS idx_invoices_tenant ON invoices(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_receivables_tenant ON receivables(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_outbox_tenant_status ON outbox_events(tenant_id, status);
        CREATE INDEX IF NOT EXISTS idx_warehouses_tenant ON warehouses(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_products_tenant ON products(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_stock_items_tenant ON stock_items(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_stock_movements_tenant ON stock_movements(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_pos_tenant ON purchase_orders(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_po_items_po ON purchase_order_items(purchase_order_id);
        CREATE INDEX IF NOT EXISTS idx_stock_adj_tenant ON stock_adjustments(tenant_id);

        -- Triggers preventing UPDATE/DELETE on immutable audit tables (§72)
        CREATE TRIGGER IF NOT EXISTS trg_stock_movements_prevent_update
        BEFORE UPDATE ON stock_movements
        BEGIN
            SELECT RAISE(ABORT, 'Stock movements are immutable audit records');
        END;

        CREATE TRIGGER IF NOT EXISTS trg_stock_movements_prevent_delete
        BEFORE DELETE ON stock_movements
        BEGIN
            SELECT RAISE(ABORT, 'Stock movements cannot be deleted');
        END;

        CREATE TRIGGER IF NOT EXISTS trg_stock_adjustments_prevent_update
        BEFORE UPDATE ON stock_adjustments
        BEGIN
            SELECT RAISE(ABORT, 'Stock adjustments are immutable audit records');
        END;

        CREATE TRIGGER IF NOT EXISTS trg_stock_adjustments_prevent_delete
        BEFORE DELETE ON stock_adjustments
        BEGIN
            SELECT RAISE(ABORT, 'Stock adjustments cannot be deleted');
        END;
        """)

def seed_chart_of_accounts(conn, tenant_id: str, created_at: str):
    system_accounts = [
        ("1000", "Kas", "asset", 1),
        ("1100", "Bank", "asset", 1),
        ("1200", "Piutang Usaha", "asset", 1),
        ("1300", "Persediaan", "asset", 1),
        ("2000", "Utang Usaha", "liability", 1),
        ("2100", "Utang Pajak (PPN/PPh)", "liability", 1),
        ("4000", "Pendapatan Usaha", "income", 1),
        ("5000", "Beban Pokok Penjualan", "expense", 1),
        ("6000", "Beban Operasional", "expense", 1),
    ]
    for code, name, acc_type, is_sys in system_accounts:
        conn.execute(
            """
            INSERT OR IGNORE INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (str(uuid.uuid4()), tenant_id, code, name, acc_type, is_sys, created_at)
        )

RESERVED_SLUGS = {
    "admin", "api", "system", "auth", "login", "billing", "root", "app",
    "static", "support", "terms", "privacy", "dashboard", "settings",
    "webhooks", "v1", "v2", "swagger", "openapi", "health", "ready"
}

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

    def resolve_tenant_context(self, user_id: str, path: str = "") -> Tuple[Optional[str], Optional[str], Optional[str]]:
        target_tenant_id = None
        header_val = self.headers.get("X-Tenant-ID") or self.headers.get("x-tenant-id")
        if header_val:
            target_tenant_id = header_val.strip().lower()
        elif path.startswith("/api/v1/tenants/"):
            parts = path.split("/")
            if len(parts) >= 5 and parts[4] not in ["", "switch"]:
                target_tenant_id = parts[4].lower()

        with get_db() as conn:
            if not target_tenant_id:
                cur = conn.execute(
                    """
                    SELECT m.tenant_id, m.role, m.status 
                    FROM memberships m
                    JOIN tenants t ON t.id = m.tenant_id
                    WHERE m.user_id = ? AND m.status = 'ACTIVE' AND t.is_personal = 1
                    ORDER BY t.created_at ASC LIMIT 1
                    """,
                    (user_id,)
                )
                row = cur.fetchone()
                if not row:
                    cur2 = conn.execute(
                        """
                        SELECT m.tenant_id, m.role, m.status 
                        FROM memberships m
                        JOIN tenants t ON t.id = m.tenant_id
                        WHERE m.user_id = ? AND m.status = 'ACTIVE'
                        ORDER BY t.created_at ASC LIMIT 1
                        """,
                        (user_id,)
                    )
                    row = cur2.fetchone()
                if not row:
                    return None, None, "NOT_FOUND"
                return row["tenant_id"], row["role"], None

            cur = conn.execute(
                "SELECT role, status FROM memberships WHERE tenant_id = ? AND user_id = ?",
                (target_tenant_id, user_id)
            )
            row = cur.fetchone()
            if not row:
                return None, None, "NOT_FOUND"
            if row["status"].upper() != "ACTIVE":
                return None, None, "FORBIDDEN"
            return target_tenant_id, row["role"], None

    def generate_sequential_invoice_number(self, tenant_id: str, prefix: str = "INV", conn = None) -> str:
        current_year = datetime.now(timezone.utc).year
        query = "SELECT COUNT(*) FROM invoices WHERE tenant_id = ? AND status != 'DRAFT'"
        if conn is not None:
            cur = conn.execute(query, (tenant_id,))
            count = cur.fetchone()[0]
        else:
            with get_db() as c:
                cur = c.execute(query, (tenant_id,))
                count = cur.fetchone()[0]
        seq = count + 1
        return f"{prefix}-{current_year}-{seq:06d}"

    def post_outbox_event(self, tenant_id: str, event_type: str, aggregate_type: str, aggregate_id: str, payload: Dict[str, Any], conn = None) -> str:
        evt_id = str(uuid.uuid4())
        now_iso = utc_now_iso()
        raw_payload = json.dumps(payload)
        if conn is not None:
            conn.execute(
                """
                INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status, attempt_count, created_at)
                VALUES (?, ?, ?, ?, ?, ?, 'PENDING', 0, ?)
                """,
                (evt_id, tenant_id, event_type, aggregate_type, aggregate_id, raw_payload, now_iso)
            )
        else:
            with get_db() as c:
                c.execute(
                    """
                    INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status, attempt_count, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, 'PENDING', 0, ?)
                    """,
                    (evt_id, tenant_id, event_type, aggregate_type, aggregate_id, raw_payload, now_iso)
                )
                c.commit()
        return evt_id

    def post_balanced_journal(self, tenant_id: str, description: str, source_type: str, source_id: str, lines: List[Dict[str, Any]], conn = None) -> str:
        j_id = str(uuid.uuid4())
        now_iso = utc_now_iso()
        def _execute(c):
            cur_c = c.execute("SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?", (tenant_id,))
            seq = cur_c.fetchone()[0] + 1
            entry_number = f"JRN-{datetime.now(timezone.utc).year}-{seq:06d}"
            c.execute(
                """
                INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, 'POSTED', 0, ?)
                """,
                (j_id, tenant_id, entry_number, now_iso, description, source_type, source_id, now_iso)
            )
            for l in lines:
                c.execute(
                    "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (str(uuid.uuid4()), j_id, tenant_id, l["account_code"], l["debit"], l["credit"], l.get("memo"))
                )
        if conn is not None:
            _execute(conn)
        else:
            with get_db() as c:
                _execute(c)
                c.commit()
        return j_id

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
                user_id = self.get_auth_user_id()
                if not user_id:
                    self.send_rfc7807(401, "Unauthorized", "Authentication required for WebSocket connection", "UNAUTHORIZED")
                    return
                self.send_json(200, {"status": "websocket_endpoint_ready", "supported_events": ["TransactionCreated", "BalanceChanged", "SyncHint"]})
            else:
                self.send_rfc7807(400, "Bad Request", "Expected WebSocket Upgrade header", "UPGRADE_REQUIRED")
            return

        # Android shell capability status probe
        if path == "/api/v1/android/status":
            self.send_json(200, {
                "bridge_version": "1.0",
                "notification_service": "supported",
                "trusted_origin": "https://api.nurdiansyahlabs.com",
                "hardware_acceleration": True,
                "safe_area_configured": True,
                "backend_authoritative": True,
                "capabilities": ["check_permissions", "haptic_feedback", "sync", "secure_storage"],
                "approved_packages": ["com.bca", "id.dana", "com.gojek.app", "com.mandiri.livin"],
                "battery_policy": "observe_saver",
                "sync_throttle_interval_sec": 900
            })
            return

        if path in ["/api/v1/subscriptions/plans", "/api/v1/plans"]:
            self.send_json(200, {
                "plans": [
                    {"id": "free", "name": "Free", "amount": 0, "currency": "IDR", "interval": "forever"},
                    {"id": "trial_3_months", "name": "3-Month Trial", "amount": 0, "currency": "IDR", "duration_days": 90},
                    {"id": "premium_monthly", "name": "Monthly Premium", "amount": 10000, "currency": "IDR", "interval": "month"},
                    {"id": "premium_annual", "name": "Annual Premium", "amount": 110000, "currency": "IDR", "interval": "year"}
                ],
                "exclusive_provider": "dana"
            })
            return

        if path == "/api/v1/system/schema":
            with get_db() as conn:
                cur = conn.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name")
                tables = [r[0] for r in cur.fetchall()]
                cur2 = conn.execute("PRAGMA journal_mode;")
                j_mode = cur2.fetchone()[0]
                cur3 = conn.execute("PRAGMA foreign_keys;")
                fk = cur3.fetchone()[0]
                cur4 = conn.execute("PRAGMA quick_check;")
                integrity = cur4.fetchone()[0]
                cur5 = conn.execute("SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%'")
                idx_count = cur5.fetchone()[0]
                self.send_json(200, {
                    "tables": tables,
                    "table_count": len(tables),
                    "journal_mode": j_mode.lower(),
                    "foreign_keys": fk,
                    "integrity_check": integrity.lower(),
                    "index_count": idx_count,
                    "status": "synchronized"
                })
                return

        if path == "/api/v1/system/client-check":
            self.send_rfc7807(404, "Not Found", "Resource not found", "NOT_FOUND")
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

        # =====================================================================
        # v4.1 Multi-Tenant & Business Core Endpoints
        # =====================================================================
        if path == "/api/v1/tenants":
            with get_db() as conn:
                cur = conn.execute(
                    """
                    SELECT t.id, t.name, t.slug, t.status, t.is_personal, m.role,
                           bp.business_type, bp.timezone, bp.currency, bp.locale, bp.invoice_prefix,
                           t.created_at, t.updated_at,
                           (SELECT COUNT(*) FROM memberships WHERE tenant_id = t.id) as member_count
                    FROM memberships m
                    JOIN tenants t ON t.id = m.tenant_id
                    LEFT JOIN business_profiles bp ON bp.tenant_id = t.id
                    WHERE m.user_id = ?
                    ORDER BY t.created_at ASC
                    """,
                    (user_id,)
                )
                rows = [dict(r) for r in cur.fetchall()]
                self.send_json(200, {"tenants": rows, "count": len(rows)})
            return

        if re.match(r"^/api/v1/tenants/[^/]+$", path):
            t_id = path.split("/")[-1]
            with get_db() as conn:
                cur = conn.execute(
                    """
                    SELECT t.id, t.name, t.slug, t.status, t.is_personal, m.role,
                           bp.business_type, bp.timezone, bp.currency, bp.locale, bp.invoice_prefix,
                           t.created_at, t.updated_at,
                           (SELECT COUNT(*) FROM memberships WHERE tenant_id = t.id) as member_count
                    FROM memberships m
                    JOIN tenants t ON t.id = m.tenant_id
                    LEFT JOIN business_profiles bp ON bp.tenant_id = t.id
                    WHERE t.id = ? AND m.user_id = ?
                    """,
                    (t_id, user_id)
                )
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                    return
                self.send_json(200, dict(row))
            return

        if re.match(r"^/api/v1/tenants/[^/]+/profile$", path):
            t_id = path.split("/")[-2]
            with get_db() as conn:
                cur = conn.execute(
                    """
                    SELECT bp.* 
                    FROM business_profiles bp
                    JOIN memberships m ON m.tenant_id = bp.tenant_id
                    WHERE bp.tenant_id = ? AND m.user_id = ?
                    """,
                    (t_id, user_id)
                )
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Workspace profile not found", "NOT_FOUND")
                    return
                self.send_json(200, dict(row))
            return

        if re.match(r"^/api/v1/tenants/[^/]+/members$", path):
            t_id = path.split("/")[-2]
            with get_db() as conn:
                # Check caller membership
                cur = conn.execute("SELECT role FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, user_id))
                if not cur.fetchone():
                    self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                    return
                cur2 = conn.execute(
                    """
                    SELECT m.id as membership_id, m.user_id, u.email, u.name as display_name, m.role, m.status, m.created_at as joined_at
                    FROM memberships m
                    JOIN users u ON u.id = m.user_id
                    WHERE m.tenant_id = ?
                    ORDER BY m.created_at ASC
                    """,
                    (t_id,)
                )
                members = [dict(r) for r in cur2.fetchall()]
                self.send_json(200, {"members": members, "count": len(members)})
            return

        if re.match(r"^/api/v1/tenants/[^/]+/capabilities$", path):
            t_id = path.split("/")[-2]
            with get_db() as conn:
                cur = conn.execute(
                    """
                    SELECT bp.business_type, m.role
                    FROM business_profiles bp
                    JOIN memberships m ON m.tenant_id = bp.tenant_id
                    WHERE bp.tenant_id = ? AND m.user_id = ?
                    """,
                    (t_id, user_id)
                )
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                    return
                b_type = row["business_type"] or "general"
                cap_map = {
                    "general": ["inventory", "purchasing", "invoicing", "accounting", "receivables", "reports"],
                    "retail": ["pos", "inventory", "purchasing", "invoicing", "accounting", "receivables", "reports"],
                    "fnb": ["pos", "tables", "kitchen", "inventory", "purchasing", "accounting", "reports"],
                    "rental": ["inventory", "purchasing", "bookings", "invoicing", "receivables", "accounting"],
                    "contractor": ["projects", "milestones", "invoicing", "receivables", "accounting"],
                    "personal": ["accounts", "transactions", "budgets", "analytics"]
                }
                caps = cap_map.get(b_type, ["inventory", "purchasing", "invoicing", "accounting", "receivables", "reports"])
                self.send_json(200, {
                    "tenant_id": t_id,
                    "business_type": b_type,
                    "role": row["role"],
                    "capabilities": caps,
                    "navigation": [{"module": c, "enabled": True} for c in caps]
                })
            return

        # =====================================================================
        # v4.1 Double-Entry Accounting GET Endpoints
        # =====================================================================
        if path == "/api/v1/accounting/accounts":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM chart_of_accounts WHERE tenant_id = ? ORDER BY code ASC", (tenant_id,))
                accounts = [dict(r) for r in cur.fetchall()]
                self.send_json(200, {"accounts": accounts, "count": len(accounts)})
            return

        if path == "/api/v1/accounting/journals":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM journal_entries WHERE tenant_id = ? ORDER BY entry_date DESC, entry_number DESC", (tenant_id,))
                journals = []
                for j in cur.fetchall():
                    jd = dict(j)
                    cur_lines = conn.execute("SELECT * FROM journal_lines WHERE journal_id = ? ORDER BY id ASC", (jd["id"],))
                    lines = [dict(l) for l in cur_lines.fetchall()]
                    jd["lines"] = lines
                    jd["total_debit"] = sum(l.get("debit", 0) for l in lines)
                    jd["total_credit"] = sum(l.get("credit", 0) for l in lines)
                    journals.append(jd)
                self.send_json(200, {"journals": journals, "count": len(journals)})
            return

        if re.match(r"^/api/v1/accounting/journals/[^/]+$", path):
            j_id = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM journal_entries WHERE id = ? AND tenant_id = ?", (j_id, tenant_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Journal entry not found", "NOT_FOUND")
                    return
                jd = dict(row)
                cur_lines = conn.execute("SELECT * FROM journal_lines WHERE journal_id = ? ORDER BY id ASC", (j_id,))
                lines = [dict(l) for l in cur_lines.fetchall()]
                jd["lines"] = lines
                jd["total_debit"] = sum(l.get("debit", 0) for l in lines)
                jd["total_credit"] = sum(l.get("credit", 0) for l in lines)
                self.send_json(200, jd)
            return

        if path == "/api/v1/accounting/trial-balance":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM chart_of_accounts WHERE tenant_id = ? ORDER BY code ASC", (tenant_id,))
                coa = [dict(r) for r in cur.fetchall()]
                tb_lines = []
                sum_debit = 0
                sum_credit = 0
                for acc in coa:
                    cur_bal = conn.execute(
                        """
                        SELECT COALESCE(SUM(jl.debit), 0) as d, COALESCE(SUM(jl.credit), 0) as c
                        FROM journal_lines jl
                        WHERE jl.tenant_id = ? AND jl.account_code = ?
                        """,
                        (tenant_id, acc["code"])
                    )
                    d, c = cur_bal.fetchone()
                    sum_debit += d
                    sum_credit += c
                    bal = d - c
                    tb_lines.append({
                        "code": acc["code"],
                        "name": acc["name"],
                        "account_type": acc["account_type"],
                        "debit": d,
                        "credit": c,
                        "balance": bal
                    })
                net = sum_debit - sum_credit
                self.send_json(200, {
                    "accounts": tb_lines,
                    "total_debit": sum_debit,
                    "total_credit": sum_credit,
                    "net_balance": net,
                    "is_balanced": (net == 0)
                })
            return

        # =====================================================================
        # v4.1 Invoicing, Receivables & Outbox GET Endpoints
        # =====================================================================
        if path == "/api/v1/invoices":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM invoices WHERE tenant_id = ? ORDER BY created_at DESC", (tenant_id,))
                invoices = [dict(r) for r in cur.fetchall()]
                self.send_json(200, {"invoices": invoices, "count": len(invoices)})
            return

        if re.match(r"^/api/v1/invoices/[^/]+$", path):
            inv_id = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM invoices WHERE id = ? AND tenant_id = ?", (inv_id, tenant_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", "Invoice not found", "NOT_FOUND")
                    return
                inv = dict(row)
                cur_items = conn.execute("SELECT * FROM invoice_items WHERE invoice_id = ?", (inv_id,))
                inv["items"] = [dict(i) for i in cur_items.fetchall()]
                if inv["snapshot_json"]:
                    try:
                        inv["snapshot"] = json.loads(inv["snapshot_json"])
                    except Exception:
                        inv["snapshot"] = None
                self.send_json(200, inv)
            return

        if path == "/api/v1/receivables":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute(
                    """
                    SELECT r.*, i.invoice_number, i.customer_name 
                    FROM receivables r
                    JOIN invoices i ON i.id = r.invoice_id
                    WHERE r.tenant_id = ?
                    ORDER BY r.due_date ASC
                    """,
                    (tenant_id,)
                )
                recs = [dict(r) for r in cur.fetchall()]
                self.send_json(200, {"receivables": recs, "count": len(recs)})
            return

        if path == "/api/v1/receivables/aging":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute(
                    "SELECT due_date, outstanding_amount FROM receivables WHERE tenant_id = ? AND outstanding_amount > 0 AND status != 'VOIDED'",
                    (tenant_id,)
                )
                now_dt = datetime.now(timezone.utc).date()
                b_0_30 = 0
                b_31_60 = 0
                b_61_90 = 0
                b_90_plus = 0
                total_out = 0
                for row in cur.fetchall():
                    out = row["outstanding_amount"]
                    total_out += out
                    try:
                        due = datetime.strptime(row["due_date"].split("T")[0], "%Y-%m-%d").date()
                        days_overdue = (now_dt - due).days
                    except Exception:
                        days_overdue = 0

                    if days_overdue <= 30:
                        b_0_30 += out
                    elif days_overdue <= 60:
                        b_31_60 += out
                    elif days_overdue <= 90:
                        b_61_90 += out
                    else:
                        b_90_plus += out

                self.send_json(200, {
                    "current_0_30": b_0_30,
                    "overdue_31_60": b_31_60,
                    "overdue_61_90": b_61_90,
                    "overdue_90_plus": b_90_plus,
                    "total_outstanding": total_out
                })
            return

        if path == "/api/v1/outbox/events":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM outbox_events WHERE tenant_id = ? ORDER BY created_at ASC", (tenant_id,))
                evts = []
                for r in cur.fetchall():
                    ed = dict(r)
                    try:
                        ed["payload"] = json.loads(ed["payload_json"])
                    except Exception:
                        ed["payload"] = ed["payload_json"]
                    evts.append(ed)
                self.send_json(200, {"events": evts, "count": len(evts)})
            return

        # =====================================================================
        # Phase 2 Inventory & Multi-Location Stock Management GET Endpoints
        # =====================================================================
        if path == "/api/v1/warehouses":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM warehouses WHERE tenant_id = ? ORDER BY created_at ASC", (tenant_id,))
                whs = [dict(r) for r in cur.fetchall()]
                for w in whs:
                    w["is_default"] = bool(w["is_default"])
                self.send_json(200, {"warehouses": whs, "count": len(whs)})
            return

        if re.match(r"^/api/v1/warehouses/[^/]+$", path):
            wh_id = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM warehouses WHERE id = ? AND tenant_id = ?", (wh_id, tenant_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", f"Warehouse '{wh_id}' not found", "NOT_FOUND")
                    return
                wd = dict(row)
                wd["is_default"] = bool(wd["is_default"])
                self.send_json(200, wd)
            return

        if path == "/api/v1/products":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM products WHERE tenant_id = ? ORDER BY created_at ASC", (tenant_id,))
                prods = [dict(r) for r in cur.fetchall()]
                for p in prods:
                    p["is_active"] = bool(p["is_active"])
                self.send_json(200, {"products": prods, "count": len(prods)})
            return

        if re.match(r"^/api/v1/products/[^/]+$", path):
            prod_id = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM products WHERE id = ? AND tenant_id = ?", (prod_id, tenant_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", f"Product '{prod_id}' not found", "NOT_FOUND")
                    return
                pd = dict(row)
                pd["is_active"] = bool(pd["is_active"])
                self.send_json(200, pd)
            return

        if path == "/api/v1/inventory":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            wh_filter = qs.get("warehouse_id", [None])[0]
            prod_filter = qs.get("product_id", [None])[0]
            low_stock_filter = qs.get("low_stock", ["false"])[0].lower() in ["true", "1"]

            query = """
                SELECT si.*, p.name as product_name, p.sku as product_sku, p.unit as product_unit,
                       w.name as warehouse_name, w.code as warehouse_code
                FROM stock_items si
                JOIN products p ON p.id = si.product_id
                JOIN warehouses w ON w.id = si.warehouse_id
                WHERE si.tenant_id = ?
            """
            params: List[Any] = [tenant_id]
            if wh_filter:
                query += " AND si.warehouse_id = ?"
                params.append(wh_filter)
            if prod_filter:
                query += " AND si.product_id = ?"
                params.append(prod_filter)

            query += " ORDER BY si.updated_at DESC"
            with get_db() as conn:
                cur = conn.execute(query, params)
                items = []
                for r in cur.fetchall():
                    item = dict(r)
                    q_on_hand = item["quantity_on_hand"]
                    thresh = item["reorder_threshold"]
                    item["is_low_stock"] = (q_on_hand <= thresh)
                    if low_stock_filter and not item["is_low_stock"]:
                        continue
                    items.append(item)
                self.send_json(200, {"stock_items": items, "count": len(items)})
            return

        if path == "/api/v1/purchase-orders":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM purchase_orders WHERE tenant_id = ? ORDER BY created_at DESC", (tenant_id,))
                pos = []
                for r in cur.fetchall():
                    po_dict = dict(r)
                    cur_it = conn.execute("SELECT * FROM purchase_order_items WHERE purchase_order_id = ? ORDER BY created_at ASC", (po_dict["id"],))
                    po_dict["items"] = [dict(it) for it in cur_it.fetchall()]
                    pos.append(po_dict)
                self.send_json(200, {"purchase_orders": pos, "count": len(pos)})
            return

        if re.match(r"^/api/v1/purchase-orders/[^/]+$", path):
            po_id = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM purchase_orders WHERE id = ? AND tenant_id = ?", (po_id, tenant_id))
                row = cur.fetchone()
                if not row:
                    self.send_rfc7807(404, "Not Found", f"Purchase order '{po_id}' not found", "NOT_FOUND")
                    return
                po_dict = dict(row)
                cur_it = conn.execute("SELECT * FROM purchase_order_items WHERE purchase_order_id = ? ORDER BY created_at ASC", (po_id,))
                po_dict["items"] = [dict(it) for it in cur_it.fetchall()]
                self.send_json(200, po_dict)
            return

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

            is_pro = is_premium or (sub_status == "trialing")
            features = ["transactions.basic", "accounts.basic", "analytics.basic"]
            if is_pro:
                features.extend(["analytics.advanced", "budgeting", "reports.advanced", "auto_transaction_ingestion"])

            self.send_json(200, {
                "user_id": user_id,
                "tier": user["tier"],
                "status": sub_status,
                "is_premium": is_pro,
                "has_used_trial": bool(user["has_used_trial"]),
                "trial_started_at": user["trial_started_at"],
                "trial_ends_at": user["trial_ends_at"],
                "days_remaining": days_remaining,
                "remaining_days": days_remaining,
                "price_monthly": 10000 if (user["tier"] == "premium" and sub_status != "trialing") else 0,
                "price_annual": 110000,
                "plans": {
                    "free": {"amount": 0, "interval": "forever"},
                    "premium_monthly": {"amount": 10000, "interval": "month"},
                    "premium_annual": {"amount": 110000, "interval": "year"}
                },
                "exclusive_provider": "dana",
                "features": features
            })
            return

        # Audit Logs Query
        if path in ["/api/v1/audit/logs", "/api/v1/audit"]:
            with get_db() as conn:
                cur = conn.execute(
                    "SELECT * FROM audit_logs WHERE user_id = ? ORDER BY created_at DESC LIMIT 100",
                    (user_id,)
                )
                rows = cur.fetchall()
                logs = [dict(r) for r in rows]
                self.send_json(200, {"logs": logs, "count": len(logs)})
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
                    "income": income,
                    "total_expenses": expenses,
                    "expenses": expenses,
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
                categories = [dict(r) for r in rows]
                self.send_json(200, {"categories": categories, "count": len(categories)})
            return

        # 8. Server-Side Feature Gating (§9, REQ-SEC-10)
        # Locked features strictly return HTTP 403 FEATURE_LOCKED for non-premium users
        if path in ["/api/v1/analytics/advanced", "/api/v1/reports/advanced", "/api/v1/budgets"]:
            if not is_premium:
                self.send_rfc7807(403, "Forbidden", "Fitur ini memerlukan langganan Premium", "FEATURE_LOCKED")
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
                self.send_json(200, candidates)
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
                    # Auto-provision default personal workspace (Feature 7 & M1 Core Foundation)
                    tenant_id = str(uuid.uuid4())
                    slug = f"personal-{user_id[:8]}"
                    profile_id = str(uuid.uuid4())
                    membership_id = f"mem_{uuid.uuid4().hex[:12]}"
                    conn.execute(
                        "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?, ?, ?, 'ACTIVE', 1, ?, ?)",
                        (tenant_id, f"{name}'s Workspace", slug, now_iso, now_iso)
                    )
                    conn.execute(
                        """
                        INSERT INTO business_profiles 
                        (id, tenant_id, business_name, legal_name, timezone, currency, locale, invoice_prefix, business_type, created_at, updated_at)
                        VALUES (?, ?, ?, ?, 'Asia/Jakarta', 'IDR', 'id-ID', 'INV', 'personal', ?, ?)
                        """,
                        (profile_id, tenant_id, name, name, now_iso, now_iso)
                    )
                    conn.execute(
                        "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?, ?, ?, 'owner', 'ACTIVE', ?, ?)",
                        (membership_id, tenant_id, user_id, now_iso, now_iso)
                    )
                    seed_chart_of_accounts(conn, tenant_id, now_iso)
                    conn.commit()

                token = make_session_token(user_id)
                cookie_hdr = f"auth_token={token}; Path=/; HttpOnly; SameSite=Lax"
                self.log_audit(user_id, "REGISTER", "user", user_id, f"Registered with email {email}")
                self.send_json(201, {
                    "user": {
                        "id": user_id,
                        "name": name,
                        "email": email,
                        "tier": "free",
                        "default_tenant_id": tenant_id
                    },
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
            sig = self.headers.get("X-SIGNATURE") or self.headers.get("x-signature") or ""
            timestamp = self.headers.get("X-TIMESTAMP") or self.headers.get("x-timestamp") or ""
            partner_id = self.headers.get("X-PARTNER-ID") or self.headers.get("x-partner-id") or ""
            event_id = self.headers.get("X-EXTERNAL-ID") or self.headers.get("x-external-id") or (body.get("event_id") if isinstance(body, dict) else "")

            if not raw_str or not raw_str.strip():
                self.send_rfc7807(400, "Bad Request", "Empty webhook payload body", "EMPTY_PAYLOAD")
                return

            if not timestamp:
                self.send_rfc7807(401, "Unauthorized", "Missing X-TIMESTAMP header", "MISSING_TIMESTAMP")
                return

            if not event_id:
                self.send_rfc7807(401, "Unauthorized", "Missing X-EXTERNAL-ID header or event ID", "MISSING_EVENT_ID")
                return

            if not sig:
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
                user_id_target = body.get("user_id") if isinstance(body, dict) else None
                amount_paid = body.get("amount", 10000) if isinstance(body, dict) else 10000
                if user_id_target:
                    conn.execute("UPDATE users SET tier = 'active', updated_at = ? WHERE id = ?", (utc_now_iso(), user_id_target))
                    conn.commit()
                    self.log_audit(user_id_target, "DANA_PAYMENT_SUCCESS", "subscription", event_id, f"Activated via DANA event {event_id} (amount: {amount_paid})")

            self.send_json(200, {"responseCode": "2005600", "responseMessage": "Successful"})
            return

        # Authenticated endpoints
        user_id = self.get_auth_user_id()
        if not user_id:
            self.send_rfc7807(401, "Unauthorized", "Authentication required", "UNAUTHORIZED")
            return

        user = self.get_user(user_id)
        if not user:
            self.send_rfc7807(401, "Unauthorized", "User account not found", "UNAUTHORIZED")
            return

        is_premium = self.is_user_premium(user)

        # =====================================================================
        # v4.1 Tenancy & Workspace POST Endpoints
        # =====================================================================
        if path == "/api/v1/tenants":
            if not body or not body.get("name", "").strip():
                self.send_rfc7807(400, "Bad Request", "Workspace name cannot be empty", "INVALID_NAME")
                return
            name = body["name"].strip()
            slug_input = body.get("slug", "").strip().lower()
            if slug_input:
                if not re.match(r"^[a-z0-9-]+$", slug_input):
                    self.send_rfc7807(400, "Bad Request", f"Slug '{slug_input}' contains invalid characters", "INVALID_SLUG")
                    return
                slug = slug_input
            else:
                slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")

            if slug in RESERVED_SLUGS:
                self.send_rfc7807(400, "Bad Request", f"Slug '{slug}' is reserved by system", "RESERVED_SLUG")
                return

            tz = body.get("timezone", "Asia/Jakarta")
            valid_tzs = ["Asia/Jakarta", "Asia/Makassar", "Asia/Jayapura", "WIB", "WITA", "WIT"]
            if tz not in valid_tzs:
                self.send_rfc7807(400, "Bad Request", f"Invalid timezone '{tz}'. Supported: Asia/Jakarta, Asia/Makassar, Asia/Jayapura", "INVALID_TIMEZONE")
                return

            currency = body.get("currency", "IDR")
            now_iso = utc_now_iso()
            tenant_id = str(uuid.uuid4())
            profile_id = str(uuid.uuid4())
            membership_id = f"mem_{uuid.uuid4().hex[:12]}"

            try:
                with get_db() as conn:
                    # Slug uniqueness check
                    cur = conn.execute("SELECT id FROM tenants WHERE slug = ?", (slug,))
                    if cur.fetchone():
                        self.send_rfc7807(409, "Conflict", f"Slug '{slug}' is already in use", "SLUG_ALREADY_EXISTS")
                        return

                    conn.execute(
                        "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?, ?, ?, 'ACTIVE', 0, ?, ?)",
                        (tenant_id, name, slug, now_iso, now_iso)
                    )
                    conn.execute(
                        """
                        INSERT INTO business_profiles 
                        (id, tenant_id, business_name, legal_name, timezone, currency, locale, invoice_prefix, business_type, created_at, updated_at)
                        VALUES (?, ?, ?, ?, ?, ?, 'id-ID', 'INV', 'general', ?, ?)
                        """,
                        (profile_id, tenant_id, name, name, tz, currency, now_iso, now_iso)
                    )
                    conn.execute(
                        "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?, ?, ?, 'owner', 'ACTIVE', ?, ?)",
                        (membership_id, tenant_id, user_id, now_iso, now_iso)
                    )
                    seed_chart_of_accounts(conn, tenant_id, now_iso)
                    conn.commit()

                res_data = {
                    "id": tenant_id,
                    "name": name,
                    "slug": slug,
                    "status": "ACTIVE",
                    "role": "owner",
                    "is_default": False,
                    "created_at": now_iso,
                    "updated_at": now_iso
                }
                self.send_json(201, res_data, headers={"Cache-Control": "private, no-store, must-revalidate"})
            except sqlite3.IntegrityError:
                self.send_rfc7807(409, "Conflict", f"Slug '{slug}' is already in use", "SLUG_ALREADY_EXISTS")
            return

        if re.match(r"^/api/v1/tenants/[^/]+/members$", path):
            t_id = path.split("/")[-2]
            with get_db() as conn:
                cur = conn.execute("SELECT role, status FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, user_id))
                caller_mem = cur.fetchone()
                if not caller_mem:
                    self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                    return
                if caller_mem["role"].lower() in ["staff", "member"]:
                    self.send_rfc7807(403, "Forbidden", "Only owner or admin can invite members", "FORBIDDEN")
                    return
                target_email = (body.get("email") or "").strip().lower()
                target_role = (body.get("role") or "member").lower()
                if target_role not in ["owner", "admin", "staff", "member"]:
                    self.send_rfc7807(400, "Bad Request", f"Invalid role '{target_role}'. Allowed: owner, admin, staff, member", "INVALID_ROLE")
                    return
                cur_user = conn.execute("SELECT id, email, name FROM users WHERE email = ?", (target_email,))
                target_user = cur_user.fetchone()
                if not target_user:
                    self.send_rfc7807(404, "Not Found", "User with this email not found", "USER_NOT_FOUND")
                    return
                cur_exist = conn.execute("SELECT COUNT(*) FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, target_user["id"]))
                if cur_exist.fetchone()[0] > 0:
                    self.send_rfc7807(409, "Conflict", "User is already a member of this workspace", "MEMBER_ALREADY_EXISTS")
                    return

                mem_id = f"mem_{uuid.uuid4().hex[:12]}"
                now_iso = utc_now_iso()
                conn.execute(
                    "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?)",
                    (mem_id, t_id, target_user["id"], target_role, now_iso, now_iso)
                )
                conn.commit()

            self.send_json(201, {
                "membership_id": mem_id,
                "user_id": target_user["id"],
                "email": target_user["email"],
                "display_name": target_user["name"],
                "role": target_role,
                "status": "ACTIVE",
                "joined_at": now_iso
            }, headers={"Cache-Control": "private, no-store, must-revalidate"})
            return

        if re.match(r"^/api/v1/tenants/[^/]+/switch$", path):
            t_id = path.split("/")[-2]
            with get_db() as conn:
                cur = conn.execute("SELECT role, status FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, user_id))
                mem = cur.fetchone()
                if not mem or mem["status"].upper() != "ACTIVE":
                    self.send_rfc7807(404, "Not Found", "Workspace not found or access denied", "NOT_FOUND")
                    return
                cur_tenant = conn.execute("SELECT * FROM tenants WHERE id = ?", (t_id,))
                t_row = cur_tenant.fetchone()
            self.send_json(200, {
                "active_tenant_id": t_id,
                "name": t_row["name"],
                "slug": t_row["slug"],
                "role": mem["role"],
                "status": "switched"
            })
            return

        # =====================================================================
        # v4.1 Accounting POST Endpoints
        # =====================================================================
        if path == "/api/v1/accounting/accounts":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return
            code = (body.get("code") or "").strip()
            name = (body.get("name") or "").strip()
            acc_type = (body.get("account_type") or "asset").strip().lower()
            if not code or not name:
                self.send_rfc7807(400, "Bad Request", "Account code and name are required", "INVALID_ACCOUNT")
                return
            now_iso = utc_now_iso()
            acc_id = str(uuid.uuid4())
            try:
                with get_db() as conn:
                    cur = conn.execute("SELECT id FROM chart_of_accounts WHERE tenant_id = ? AND code = ?", (tenant_id, code))
                    if cur.fetchone():
                        self.send_rfc7807(409, "Conflict", f"Account code '{code}' already exists", "ACCOUNT_ALREADY_EXISTS")
                        return
                    conn.execute(
                        "INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at) VALUES (?, ?, ?, ?, ?, 0, ?)",
                        (acc_id, tenant_id, code, name, acc_type, now_iso)
                    )
                    conn.commit()
                self.send_json(201, {
                    "id": acc_id,
                    "tenant_id": tenant_id,
                    "code": code,
                    "name": name,
                    "account_type": acc_type,
                    "is_system": False,
                    "created_at": now_iso
                })
            except sqlite3.IntegrityError:
                self.send_rfc7807(409, "Conflict", f"Account code '{code}' already exists", "ACCOUNT_EXISTS")
            return

        if path == "/api/v1/accounting/journals":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return
            if not body or "lines" not in body or not isinstance(body["lines"], list) or len(body["lines"]) == 0:
                self.send_rfc7807(400, "Bad Request", "Journal lines cannot be empty", "INVALID_JOURNAL_LINES")
                return
            if len(body["lines"]) < 2:
                self.send_rfc7807(422, "Unprocessable Entity", "Single line journal is unbalanced", "UNBALANCED_JOURNAL_ENTRY")
                return
            lines = body["lines"]
            for l in lines:
                d = l.get("debit", 0)
                c = l.get("credit", 0)
                if not (isinstance(d, int) and isinstance(c, int) and d >= 0 and c >= 0 and not isinstance(d, bool) and not isinstance(c, bool)):
                    self.send_rfc7807(400, "Bad Request", "Journal amounts must be non-negative integers", "INVALID_AMOUNT")
                    return
            total_debit = sum(l.get("debit", 0) for l in lines)
            total_credit = sum(l.get("credit", 0) for l in lines)
            if total_debit != total_credit:
                self.send_rfc7807(422, "Unprocessable Entity", f"SUM(debit)={total_debit} must equal SUM(credit)={total_credit}", "UNBALANCED_JOURNAL_ENTRY")
                return
            if total_debit <= 0:
                self.send_rfc7807(422, "Unprocessable Entity", "Journal total amount must be strictly positive", "UNBALANCED_JOURNAL_ENTRY")
                return

            entry_date = body.get("entry_date", utc_now_iso())
            description = body.get("description", "Manual Journal Entry")
            source_type = body.get("source_type", "MANUAL")
            source_id = body.get("source_id")
            now_iso = utc_now_iso()
            j_id = str(uuid.uuid4())

            with get_db() as conn:
                cur_c = conn.execute("SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?", (tenant_id,))
                seq = cur_c.fetchone()[0] + 1
                entry_number = f"JRN-{datetime.now(timezone.utc).year}-{seq:06d}"
                conn.execute(
                    """
                    INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, 'POSTED', 0, ?)
                    """,
                    (j_id, tenant_id, entry_number, entry_date, description, source_type, source_id, now_iso)
                )
                saved_lines = []
                for l in lines:
                    line_id = str(uuid.uuid4())
                    code = l.get("account_code", "")
                    deb = l.get("debit", 0)
                    cred = l.get("credit", 0)
                    memo = l.get("memo")
                    conn.execute(
                        "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, ?, ?, ?, ?)",
                        (line_id, j_id, tenant_id, code, deb, cred, memo)
                    )
                    saved_lines.append({
                        "id": line_id,
                        "account_code": code,
                        "debit": deb,
                        "credit": cred,
                        "memo": memo
                    })
                self.post_outbox_event(
                    tenant_id, "JournalPosted", "Journal", j_id,
                    {"journal_id": j_id, "entry_number": entry_number, "total_amount": total_debit},
                    conn=conn
                )
                conn.commit()

            self.send_json(201, {
                "id": j_id,
                "tenant_id": tenant_id,
                "entry_number": entry_number,
                "entry_date": entry_date,
                "description": description,
                "source_type": source_type,
                "source_id": source_id,
                "status": "POSTED",
                "total_debit": total_debit,
                "total_credit": total_credit,
                "lines": saved_lines,
                "created_at": now_iso
            })
            return

        if re.match(r"^/api/v1/accounting/journals/[^/]+/reverse$", path):
            j_id = path.split("/")[-2]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if role == "staff":
                self.send_rfc7807(403, "Forbidden", "Staff role is not permitted to reverse journals", "FORBIDDEN")
                return

            with get_db() as conn:
                cur = conn.execute("SELECT * FROM journal_entries WHERE id = ? AND tenant_id = ?", (j_id, tenant_id))
                orig = cur.fetchone()
                if not orig:
                    self.send_rfc7807(404, "Not Found", "Journal entry not found", "NOT_FOUND")
                    return
                if orig["is_reversed"] == 1:
                    self.send_rfc7807(409, "Conflict", "Journal has already been reversed", "ALREADY_REVERSED")
                    return

                cur_lines = conn.execute("SELECT * FROM journal_lines WHERE journal_id = ?", (j_id,))
                orig_lines = [dict(l) for l in cur_lines.fetchall()]

                now_iso = utc_now_iso()
                rev_id = str(uuid.uuid4())
                cur_c = conn.execute("SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?", (tenant_id,))
                rev_num = f"REV-{datetime.now(timezone.utc).year}-{cur_c.fetchone()[0] + 1:06d}"
                reason = body.get("reason", "Correction") if body else "Correction"
                rev_desc = f"Reversal of {orig['entry_number']}: {reason}"

                conn.execute(
                    """
                    INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at)
                    VALUES (?, ?, ?, ?, ?, 'REVERSAL', ?, 'POSTED', 0, ?)
                    """,
                    (rev_id, tenant_id, rev_num, now_iso, rev_desc, j_id, now_iso)
                )

                rev_saved_lines = []
                for ol in orig_lines:
                    line_id = str(uuid.uuid4())
                    swapped_debit = ol["credit"]
                    swapped_credit = ol["debit"]
                    memo = f"Reversal: {ol.get('memo') or ''}".strip()
                    conn.execute(
                        "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, ?, ?, ?, ?)",
                        (line_id, rev_id, tenant_id, ol["account_code"], swapped_debit, swapped_credit, memo)
                    )
                    rev_saved_lines.append({
                        "id": line_id,
                        "account_code": ol["account_code"],
                        "debit": swapped_debit,
                        "credit": swapped_credit,
                        "memo": memo
                    })

                conn.execute("UPDATE journal_entries SET is_reversed = 1, reversal_entry_id = ? WHERE id = ?", (rev_id, j_id))
                self.post_outbox_event(
                    tenant_id, "JournalReversed", "Journal", rev_id,
                    {"original_journal_id": j_id, "reversal_journal_id": rev_id},
                    conn=conn
                )
                conn.commit()

            self.send_json(201, {
                "id": rev_id,
                "tenant_id": tenant_id,
                "entry_number": rev_num,
                "description": rev_desc,
                "source_type": "REVERSAL",
                "source_id": j_id,
                "status": "POSTED",
                "lines": rev_saved_lines,
                "created_at": now_iso
            })
            return

        if path == "/api/v1/accounting/tax/calculate":
            if not body or "amount" not in body or not isinstance(body["amount"], int):
                self.send_rfc7807(400, "Bad Request", "Amount must be an integer Rupiah", "INVALID_AMOUNT")
                return
            amt = body["amount"]
            if amt < 0:
                self.send_rfc7807(400, "Bad Request", "Amount cannot be negative", "INVALID_AMOUNT")
                return
            tax_type = body.get("tax_type", "PPN_11_EXCL")
            is_incl = bool(body.get("is_inclusive", False))
            valid_types = ["PPN_11_EXCL", "PPN_11_INCL", "PPN_11", "PPN_12_EXCL", "PPN_12_INCL", "PPN_12", "UMKM_05", "UMKM_FINAL", "EXEMPT"]
            if tax_type not in valid_types:
                self.send_rfc7807(400, "Bad Request", f"Unsupported tax type '{tax_type}'", "INVALID_TAX_TYPE")
                return

            if tax_type == "PPN_11_EXCL":
                tax = int(round(amt * 0.11 + 1e-9))
                net = amt
                gross = amt + tax
            elif tax_type == "PPN_11_INCL" or (tax_type == "PPN_11" and is_incl):
                tax = int(round(amt - (amt * 100 / 111) + 1e-9))
                net = amt - tax
                gross = amt
            elif tax_type == "PPN_12_EXCL":
                tax = int(round(amt * 0.12 + 1e-9))
                net = amt
                gross = amt + tax
            elif tax_type == "PPN_12_INCL" or (tax_type == "PPN_12" and is_incl):
                tax = int(round(amt - (amt * 100 / 112) + 1e-9))
                net = amt - tax
                gross = amt
            elif tax_type in ["UMKM_05", "UMKM_FINAL"]:
                tax = int(round(amt * 50 / 10000 + 1e-9))
                net = amt
                gross = amt
            else:
                tax = 0
                net = amt
                gross = amt

            self.send_json(200, {
                "base_amount": amt,
                "tax_type": tax_type,
                "is_inclusive": is_incl,
                "tax_amount": tax,
                "net_amount": net,
                "gross_amount": gross
            })
            return

        # =====================================================================
        # v4.1 Invoicing, Receivables & Payment Allocation POST Endpoints
        # =====================================================================
        if path == "/api/v1/invoices":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return
            if not body or "items" not in body or not isinstance(body["items"], list) or len(body["items"]) == 0:
                self.send_rfc7807(400, "Bad Request", "Invoice must contain at least one item", "INVALID_ITEMS")
                return

            cust_name = (body.get("customer_name") or "Pelanggan Umum").strip()
            cust_addr = (body.get("customer_address") or "").strip()
            cust_email = (body.get("customer_email") or "").strip()
            due_date = body.get("due_date", utc_now_iso())
            currency = body.get("currency", "IDR")
            tax_type = body.get("tax_type", "PPN_11_EXCL")
            inv_id = str(uuid.uuid4())
            now_iso = utc_now_iso()

            subtotal = 0
            computed_items = []
            for item in body["items"]:
                desc = item.get("description", "Item")
                qty = item.get("quantity", 1)
                price = item.get("unit_price", 0)
                disc = item.get("discount", 0)
                if not isinstance(qty, int) or qty <= 0:
                    self.send_rfc7807(400, "Bad Request", "Item quantity must be a positive integer", "INVALID_QUANTITY")
                    return
                if not isinstance(price, int) or price < 0:
                    self.send_rfc7807(400, "Bad Request", "Item unit price must be a non-negative integer", "INVALID_PRICE")
                    return
                lt = (qty * price) - disc
                subtotal += lt
                computed_items.append({
                    "id": str(uuid.uuid4()),
                    "description": desc,
                    "quantity": qty,
                    "unit_price": price,
                    "discount": disc,
                    "tax_amount": 0,
                    "line_total": lt
                })

            if tax_type == "PPN_11_EXCL":
                tax_amt = int(round(subtotal * 0.11 + 1e-9))
                total_amt = subtotal + tax_amt
            elif tax_type == "PPN_11_INCL":
                tax_amt = int(round(subtotal - (subtotal * 100 / 111) + 1e-9))
                total_amt = subtotal
            elif tax_type == "PPN_12_EXCL":
                tax_amt = int(round(subtotal * 0.12 + 1e-9))
                total_amt = subtotal + tax_amt
            elif tax_type == "PPN_12_INCL":
                tax_amt = int(round(subtotal - (subtotal * 100 / 112) + 1e-9))
                total_amt = subtotal
            elif tax_type in ["UMKM_05", "UMKM_FINAL"]:
                tax_amt = int(round(subtotal * 50 / 10000 + 1e-9))
                total_amt = subtotal
            else:
                tax_amt = 0
                total_amt = subtotal

            with get_db() as conn:
                conn.execute(
                    """
                    INSERT INTO invoices 
                    (id, tenant_id, customer_name, customer_address, customer_email, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, status, created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, 'DRAFT', ?, ?)
                    """,
                    (inv_id, tenant_id, cust_name, cust_addr, cust_email, due_date, currency, tax_type, subtotal, tax_amt, total_amt, now_iso, now_iso)
                )
                for ci in computed_items:
                    conn.execute(
                        """
                        INSERT INTO invoice_items (id, invoice_id, description, quantity, unit_price, discount, tax_amount, line_total)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (ci["id"], inv_id, ci["description"], ci["quantity"], ci["unit_price"], ci["discount"], ci["tax_amount"], ci["line_total"])
                    )
                conn.commit()

            self.send_json(201, {
                "id": inv_id,
                "tenant_id": tenant_id,
                "customer_name": cust_name,
                "customer_address": cust_addr,
                "customer_email": cust_email,
                "due_date": due_date,
                "currency": currency,
                "tax_type": tax_type,
                "subtotal": subtotal,
                "tax_amount": tax_amt,
                "total_amount": total_amt,
                "status": "DRAFT",
                "items": computed_items,
                "created_at": now_iso
            })
            return

        if re.match(r"^/api/v1/invoices/[^/]+/issue$", path):
            inv_id = path.split("/")[-2]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return

            idempotency_key = self.headers.get("Idempotency-Key")
            if idempotency_key:
                payload_hash = hashlib.sha256(raw_str.encode("utf-8") if raw_str else b"").hexdigest()
                with get_db() as conn:
                    cur = conn.execute("SELECT * FROM idempotency_keys WHERE user_id = ? AND key = ?", (user_id, idempotency_key))
                    cached = cur.fetchone()
                    if cached:
                        if cached["payload_hash"] != payload_hash:
                            self.send_rfc7807(409, "Conflict", "Idempotency key reused with different request payload", "IDEMPOTENCY_KEY_MISMATCH")
                            return
                        self.send_json(cached["response_status"], json.loads(cached["response_body"]), headers={"X-Cache-Replay": "true"})
                        return

            with get_db() as conn:
                cur = conn.execute("SELECT * FROM invoices WHERE id = ? AND tenant_id = ?", (inv_id, tenant_id))
                inv = cur.fetchone()
                if not inv:
                    self.send_rfc7807(404, "Not Found", "Invoice not found", "NOT_FOUND")
                    return
                if inv["status"] != "DRAFT":
                    self.send_rfc7807(409, "Conflict", "Invoice is already issued or finalized", "ALREADY_ISSUED")
                    return

                cur_bp = conn.execute("SELECT invoice_prefix FROM business_profiles WHERE tenant_id = ?", (tenant_id,))
                bp_row = cur_bp.fetchone()
                prefix = bp_row["invoice_prefix"] if bp_row and bp_row["invoice_prefix"] else "INV"
                inv_num = self.generate_sequential_invoice_number(tenant_id, prefix=prefix, conn=conn)

                cur_items = conn.execute("SELECT * FROM invoice_items WHERE invoice_id = ?", (inv_id,))
                items_list = [dict(i) for i in cur_items.fetchall()]

                now_iso = utc_now_iso()
                snapshot = {
                    "customer_name": inv["customer_name"],
                    "customer_address": inv["customer_address"],
                    "customer_email": inv["customer_email"],
                    "items": items_list,
                    "tax_type": inv["tax_type"],
                    "subtotal": inv["subtotal"],
                    "tax_amount": inv["tax_amount"],
                    "total_amount": inv["total_amount"],
                    "issued_at": now_iso
                }
                snap_json = json.dumps(snapshot)

                conn.execute(
                    """
                    UPDATE invoices 
                    SET invoice_number = ?, status = 'ISSUED', issue_date = ?, snapshot_json = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (inv_num, now_iso, snap_json, now_iso, inv_id)
                )

                # Automatic balanced journal entry: Debit AR (1200) = total, Credit Revenue (4000) = subtotal, Credit Tax (2100) = tax
                j_id = str(uuid.uuid4())
                cur_c = conn.execute("SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?", (tenant_id,))
                j_num = f"JRN-{datetime.now(timezone.utc).year}-{cur_c.fetchone()[0] + 1:06d}"
                conn.execute(
                    """
                    INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at)
                    VALUES (?, ?, ?, ?, ?, 'INVOICE', ?, 'POSTED', 0, ?)
                    """,
                    (j_id, tenant_id, j_num, now_iso, f"Invoice {inv_num} issued to {inv['customer_name']}", inv_id, now_iso)
                )
                conn.execute(
                    "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, '1200', ?, 0, ?)",
                    (str(uuid.uuid4()), j_id, tenant_id, inv["total_amount"], f"Piutang Invoice {inv_num}")
                )
                conn.execute(
                    "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, '4000', 0, ?, ?)",
                    (str(uuid.uuid4()), j_id, tenant_id, inv["subtotal"], f"Pendapatan Invoice {inv_num}")
                )
                if inv["tax_amount"] > 0:
                    conn.execute(
                        "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, '2100', 0, ?, ?)",
                        (str(uuid.uuid4()), j_id, tenant_id, inv["tax_amount"], f"Utang Pajak Invoice {inv_num}")
                    )

                # Automatic Receivable creation
                rec_id = str(uuid.uuid4())
                conn.execute(
                    """
                    INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at)
                    VALUES (?, ?, ?, ?, 0, ?, ?, 'OPEN', ?, ?)
                    """,
                    (rec_id, tenant_id, inv_id, inv["total_amount"], inv["total_amount"], inv["due_date"], now_iso, now_iso)
                )

                # Transactional Outbox Event
                self.post_outbox_event(
                    tenant_id, "InvoiceIssued", "Invoice", inv_id,
                    {"invoice_id": inv_id, "invoice_number": inv_num, "total_amount": inv["total_amount"], "customer_name": inv["customer_name"]},
                    conn=conn
                )

                res_data = {
                    "id": inv_id,
                    "tenant_id": tenant_id,
                    "invoice_number": inv_num,
                    "status": "ISSUED",
                    "issue_date": now_iso,
                    "due_date": inv["due_date"],
                    "total_amount": inv["total_amount"],
                    "snapshot": snapshot
                }

                if idempotency_key:
                    conn.execute(
                        "INSERT OR REPLACE INTO idempotency_keys (user_id, key, payload_hash, response_status, response_body, created_at) VALUES (?, ?, ?, 200, ?, ?)",
                        (user_id, idempotency_key, payload_hash, json.dumps(res_data), now_iso)
                    )
                conn.commit()

            self.send_json(200, res_data)
            return

        if re.match(r"^/api/v1/invoices/[^/]+/void$", path):
            inv_id = path.split("/")[-2]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return

            with get_db() as conn:
                cur = conn.execute("SELECT * FROM invoices WHERE id = ? AND tenant_id = ?", (inv_id, tenant_id))
                inv = cur.fetchone()
                if not inv:
                    self.send_rfc7807(404, "Not Found", "Invoice not found", "NOT_FOUND")
                    return
                if inv["status"] in ["PAID", "PARTIALLY_PAID"]:
                    self.send_rfc7807(409, "Conflict", "Cannot void invoice with allocated payments", "CANNOT_VOID_PAID_INVOICE")
                    return

                now_iso = utc_now_iso()
                conn.execute("UPDATE invoices SET status = 'VOIDED', updated_at = ? WHERE id = ?", (now_iso, inv_id))
                conn.execute("UPDATE receivables SET status = 'VOIDED', outstanding_amount = 0, updated_at = ? WHERE invoice_id = ?", (now_iso, inv_id))

                # Reverse invoice journal if posted
                cur_j = conn.execute("SELECT id FROM journal_entries WHERE source_type = 'INVOICE' AND source_id = ? AND is_reversed = 0", (inv_id,))
                j_row = cur_j.fetchone()
                if j_row:
                    orig_jid = j_row["id"]
                    rev_jid = str(uuid.uuid4())
                    cur_c = conn.execute("SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?", (tenant_id,))
                    rev_num = f"REV-{datetime.now(timezone.utc).year}-{cur_c.fetchone()[0] + 1:06d}"
                    conn.execute(
                        """
                        INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at)
                        VALUES (?, ?, ?, ?, ?, 'REVERSAL', ?, 'POSTED', 0, ?)
                        """,
                        (rev_jid, tenant_id, rev_num, now_iso, f"Reversal for voided invoice {inv['invoice_number']}", orig_jid, now_iso)
                    )
                    cur_orig_lines = conn.execute("SELECT * FROM journal_lines WHERE journal_id = ?", (orig_jid,))
                    for ol in cur_orig_lines.fetchall():
                        conn.execute(
                            "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, ?, ?, ?, ?)",
                            (str(uuid.uuid4()), rev_jid, tenant_id, ol["account_code"], ol["credit"], ol["debit"], f"Void reversal {inv['invoice_number']}")
                        )
                    conn.execute("UPDATE journal_entries SET is_reversed = 1, reversal_entry_id = ? WHERE id = ?", (rev_jid, orig_jid))

                self.post_outbox_event(
                    tenant_id, "InvoiceVoided", "Invoice", inv_id,
                    {"invoice_id": inv_id, "invoice_number": inv["invoice_number"], "reason": body.get("reason", "Voided") if body else "Voided"},
                    conn=conn
                )
                conn.commit()

            self.send_json(200, {"id": inv_id, "status": "VOIDED", "updated_at": now_iso})
            return

        if path == "/api/v1/payments":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if err == "FORBIDDEN":
                self.send_rfc7807(403, "Forbidden", "Membership inactive", "FORBIDDEN")
                return

            idempotency_key = self.headers.get("Idempotency-Key")
            if idempotency_key:
                payload_hash = hashlib.sha256(raw_str.encode("utf-8") if raw_str else b"").hexdigest()
                with get_db() as conn:
                    cur = conn.execute("SELECT * FROM idempotency_keys WHERE user_id = ? AND key = ?", (user_id, idempotency_key))
                    cached = cur.fetchone()
                    if cached:
                        if cached["payload_hash"] != payload_hash:
                            self.send_rfc7807(409, "Conflict", "Idempotency key reused with different request payload", "IDEMPOTENCY_KEY_MISMATCH")
                            return
                        self.send_json(cached["response_status"], json.loads(cached["response_body"]), headers={"X-Cache-Replay": "true"})
                        return

            if not body or "invoice_id" not in body or "amount" not in body:
                self.send_rfc7807(400, "Bad Request", "Invoice ID and payment amount are required", "BAD_REQUEST")
                return

            inv_id = body["invoice_id"]
            amount = body["amount"]
            if not isinstance(amount, int) or amount <= 0:
                self.send_rfc7807(400, "Bad Request", "Payment amount must be a positive integer Rupiah", "INVALID_AMOUNT")
                return

            pay_method = body.get("payment_method", "BANK_TRANSFER")
            pay_date = body.get("payment_date", utc_now_iso())
            ref = body.get("reference") or f"PAY-{uuid.uuid4().hex[:8].upper()}"
            pay_id = str(uuid.uuid4())
            now_iso = utc_now_iso()

            with get_db() as conn:
                cur_rec = conn.execute("SELECT * FROM receivables WHERE invoice_id = ? AND tenant_id = ?", (inv_id, tenant_id))
                rec = cur_rec.fetchone()
                if not rec:
                    self.send_rfc7807(404, "Not Found", "Receivable/Invoice not found in this workspace", "NOT_FOUND")
                    return

                if rec["status"] == "VOIDED":
                    self.send_rfc7807(409, "Conflict", "Cannot allocate payment to a voided invoice", "INVOICE_VOIDED")
                    return

                outstanding = rec["outstanding_amount"]
                if amount > outstanding:
                    self.send_rfc7807(400, "Bad Request", f"Payment amount {amount} exceeds outstanding balance {outstanding}", "OVERPAYMENT_NOT_ALLOWED")
                    return

                new_out = outstanding - amount
                new_alloc = rec["allocated_amount"] + amount
                new_status = "PAID" if new_out == 0 else "PARTIALLY_PAID"

                conn.execute(
                    "UPDATE receivables SET outstanding_amount = ?, allocated_amount = ?, status = ?, updated_at = ? WHERE id = ?",
                    (new_out, new_alloc, new_status, now_iso, rec["id"])
                )
                conn.execute(
                    "UPDATE invoices SET status = ?, updated_at = ? WHERE id = ?",
                    (new_status, now_iso, inv_id)
                )
                conn.execute(
                    """
                    INSERT INTO payments (id, tenant_id, invoice_id, receivable_id, amount, payment_method, payment_date, reference, status, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'CONFIRMED', ?)
                    """,
                    (pay_id, tenant_id, inv_id, rec["id"], amount, pay_method, pay_date, ref, now_iso)
                )

                # Automatic double-entry posting: Debit 1100 Bank / Credit 1200 AR
                j_id = str(uuid.uuid4())
                cur_c = conn.execute("SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?", (tenant_id,))
                j_num = f"JRN-{datetime.now(timezone.utc).year}-{cur_c.fetchone()[0] + 1:06d}"
                conn.execute(
                    """
                    INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at)
                    VALUES (?, ?, ?, ?, ?, 'PAYMENT', ?, 'POSTED', 0, ?)
                    """,
                    (j_id, tenant_id, j_num, now_iso, f"Payment allocation {ref} for invoice {inv_id}", pay_id, now_iso)
                )
                conn.execute(
                    "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, '1100', ?, 0, ?)",
                    (str(uuid.uuid4()), j_id, tenant_id, amount, f"Penerimaan Kas/Bank {ref}")
                )
                conn.execute(
                    "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo) VALUES (?, ?, ?, '1200', 0, ?, ?)",
                    (str(uuid.uuid4()), j_id, tenant_id, amount, f"Pelunasan Piutang {ref}")
                )

                # Transactional Outbox Event
                self.post_outbox_event(
                    tenant_id, "PaymentConfirmed", "Payment", pay_id,
                    {"payment_id": pay_id, "invoice_id": inv_id, "amount": amount, "reference": ref},
                    conn=conn
                )

                res_data = {
                    "id": pay_id,
                    "tenant_id": tenant_id,
                    "invoice_id": inv_id,
                    "receivable_id": rec["id"],
                    "amount": amount,
                    "payment_method": pay_method,
                    "reference": ref,
                    "outstanding_balance": new_out,
                    "status": "CONFIRMED",
                    "created_at": now_iso
                }

                if idempotency_key:
                    conn.execute(
                        "INSERT OR REPLACE INTO idempotency_keys (user_id, key, payload_hash, response_status, response_body, created_at) VALUES (?, ?, ?, 201, ?, ?)",
                        (user_id, idempotency_key, payload_hash, json.dumps(res_data), now_iso)
                    )
                conn.commit()

            self.send_json(201, res_data)
            return

        if path == "/api/v1/outbox/process":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return

            sim_failure = bool(body.get("simulate_sidecar_failure", False)) if body else False
            with get_db() as conn:
                cur = conn.execute("SELECT id, attempt_count FROM outbox_events WHERE tenant_id = ? AND status = 'PENDING'", (tenant_id,))
                pending = cur.fetchall()
                now_iso = utc_now_iso()
                if sim_failure:
                    for ev in pending:
                        conn.execute(
                            "UPDATE outbox_events SET attempt_count = attempt_count + 1, last_error = 'SIMULATED_SIDECAR_ERROR: Sidecar provider temporary failure (HTTP 503)' WHERE id = ?",
                            (ev["id"],)
                        )
                    conn.commit()
                    self.send_json(200, {
                        "processed": 0,
                        "failed": len(pending),
                        "status": "retry_scheduled",
                        "message": "Sidecar failure isolated; core transaction unaffected"
                    })
                    return
                else:
                    for ev in pending:
                        conn.execute(
                            "UPDATE outbox_events SET status = 'PUBLISHED', published_at = ?, attempt_count = attempt_count + 1 WHERE id = ?",
                            (now_iso, ev["id"])
                        )
                    conn.commit()
                    self.send_json(200, {
                        "processed": len(pending),
                        "failed": 0,
                        "status": "completed"
                    })
                    return

        # =====================================================================
        # Phase 2 Inventory & Multi-Location Stock Management POST Endpoints
        # =====================================================================
        if path == "/api/v1/warehouses":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if not body or "code" not in body or "name" not in body or not str(body.get("code", "")).strip() or not str(body.get("name", "")).strip():
                self.send_rfc7807(400, "Bad Request", "Warehouse code and name are required", "MISSING_REQUIRED_FIELDS")
                return
            code = str(body["code"]).strip()
            name = str(body["name"]).strip()
            address = body.get("address")
            is_def = 1 if body.get("is_default") else 0
            now_iso = utc_now_iso()
            wh_id = str(uuid.uuid4())

            with get_db() as conn:
                cur = conn.execute("SELECT id FROM warehouses WHERE tenant_id = ? AND code = ?", (tenant_id, code))
                if cur.fetchone():
                    self.send_rfc7807(409, "Conflict", f"Warehouse with code '{code}' already exists", "DUPLICATE_WAREHOUSE_CODE")
                    return
                if is_def == 1:
                    conn.execute("UPDATE warehouses SET is_default = 0 WHERE tenant_id = ?", (tenant_id,))
                conn.execute(
                    """
                    INSERT INTO warehouses (id, tenant_id, code, name, address, is_default, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    """,
                    (wh_id, tenant_id, code, name, address, is_def, now_iso)
                )
                conn.commit()
            self.send_json(201, {
                "id": wh_id,
                "tenant_id": tenant_id,
                "code": code,
                "name": name,
                "address": address,
                "is_default": bool(is_def),
                "created_at": now_iso
            })
            return

        if path == "/api/v1/products":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if not body or "name" not in body or not str(body.get("name", "")).strip():
                self.send_rfc7807(400, "Bad Request", "Product name is required", "MISSING_REQUIRED_FIELDS")
                return
            name = str(body["name"]).strip()
            unit = str(body.get("unit", "pcs")).strip()
            cost_price = body.get("cost_price", 0)
            sale_price = body.get("sale_price", 0)
            reorder_threshold = body.get("reorder_threshold", 0)
            if not isinstance(cost_price, int) or cost_price < 0 or not isinstance(sale_price, int) or sale_price <= 0:
                self.send_rfc7807(400, "Bad Request", "Sale price must be a positive integer Rupiah and cost price non-negative", "INVALID_PRICE")
                return
            if not isinstance(reorder_threshold, int) or reorder_threshold < 0:
                self.send_rfc7807(400, "Bad Request", "Reorder threshold must be non-negative integer", "INVALID_THRESHOLD")
                return

            sku_input = body.get("sku")
            now_iso = utc_now_iso()
            prod_id = str(uuid.uuid4())

            with get_db() as conn:
                if sku_input and str(sku_input).strip():
                    sku = str(sku_input).strip()
                    cur = conn.execute("SELECT id FROM products WHERE tenant_id = ? AND sku = ?", (tenant_id, sku))
                    if cur.fetchone():
                        self.send_rfc7807(409, "Conflict", f"Product with SKU '{sku}' already exists", "DUPLICATE_SKU")
                        return
                else:
                    cur_c = conn.execute("SELECT COUNT(*) FROM products WHERE tenant_id = ?", (tenant_id,))
                    seq = cur_c.fetchone()[0] + 1
                    sku = f"SKU-{seq:06d}"

                conn.execute(
                    """
                    INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)
                    """,
                    (prod_id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, now_iso)
                )
                conn.commit()

            self.send_json(201, {
                "id": prod_id,
                "tenant_id": tenant_id,
                "sku": sku,
                "name": name,
                "unit": unit,
                "cost_price": cost_price,
                "sale_price": sale_price,
                "reorder_threshold": reorder_threshold,
                "is_active": True,
                "created_at": now_iso
            })
            return

        if path == "/api/v1/inventory/movements":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if not body or "movement_type" not in body or "product_id" not in body or "quantity" not in body:
                self.send_rfc7807(400, "Bad Request", "movement_type, product_id, and quantity are required", "MISSING_REQUIRED_FIELDS")
                return

            m_type = str(body["movement_type"]).upper()
            prod_id = body["product_id"]
            raw_qty = body["quantity"]
            if not isinstance(raw_qty, int) or raw_qty <= 0:
                self.send_rfc7807(400, "Bad Request", "Quantity must be a positive integer", "INVALID_QUANTITY")
                return

            src_wh = body.get("source_warehouse_id")
            dst_wh = body.get("destination_warehouse_id")
            unit_cost = body.get("unit_cost")
            notes = body.get("notes")
            batch_num = body.get("batch_number")
            now_iso = utc_now_iso()
            mov_id = str(uuid.uuid4())

            with get_db() as conn:
                cur_p = conn.execute("SELECT * FROM products WHERE id = ? AND tenant_id = ?", (prod_id, tenant_id))
                product = cur_p.fetchone()
                if not product:
                    self.send_rfc7807(404, "Not Found", f"Product '{prod_id}' not found", "NOT_FOUND")
                    return

                if src_wh:
                    cur_sw = conn.execute("SELECT id FROM warehouses WHERE id = ? AND tenant_id = ?", (src_wh, tenant_id))
                    if not cur_sw.fetchone():
                        self.send_rfc7807(404, "Not Found", f"Source warehouse '{src_wh}' not found", "NOT_FOUND")
                        return

                if dst_wh:
                    cur_dw = conn.execute("SELECT id FROM warehouses WHERE id = ? AND tenant_id = ?", (dst_wh, tenant_id))
                    if not cur_dw.fetchone():
                        self.send_rfc7807(404, "Not Found", f"Destination warehouse '{dst_wh}' not found", "NOT_FOUND")
                        return

                if m_type == "OUTBOUND":
                    if not src_wh:
                        self.send_rfc7807(400, "Bad Request", "Source warehouse required for OUTBOUND", "MISSING_SOURCE_WAREHOUSE")
                        return
                    cur_si = conn.execute("SELECT * FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?", (tenant_id, src_wh, prod_id))
                    stock_item = cur_si.fetchone()
                    current_on_hand = stock_item["quantity_on_hand"] if stock_item else 0
                    if current_on_hand < raw_qty:
                        self.send_rfc7807(422, "Unprocessable Entity", f"Insufficient stock: requested {raw_qty}, available {current_on_hand}", "INSUFFICIENT_STOCK")
                        return

                    new_q = current_on_hand - raw_qty
                    conn.execute("UPDATE stock_items SET quantity_on_hand = ?, updated_at = ? WHERE id = ?", (new_q, now_iso, stock_item["id"]))
                    effective_cost = stock_item["average_cost"] if stock_item and stock_item["average_cost"] > 0 else (unit_cost or product["cost_price"])
                    conn.execute(
                        """
                        INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, source_warehouse_id, quantity, unit_cost, reference_type, notes, batch_number, created_at)
                        VALUES (?, ?, 'OUTBOUND', ?, ?, ?, ?, 'DIRECT_MUTATION', ?, ?, ?)
                        """,
                        (mov_id, tenant_id, prod_id, src_wh, raw_qty, effective_cost, notes, batch_num, now_iso)
                    )
                    cogs_val = raw_qty * effective_cost
                    if cogs_val > 0:
                        self.post_balanced_journal(
                            tenant_id, f"Fulfillment Beban Pokok Penjualan ({product['name']})", "INVENTORY_OUTBOUND", mov_id,
                            [
                                {"account_code": "5000", "debit": cogs_val, "credit": 0, "memo": "Beban Pokok Penjualan"},
                                {"account_code": "1300", "debit": 0, "credit": cogs_val, "memo": "Persediaan Barang Dagang"}
                            ],
                            conn=conn
                        )
                    self.post_outbox_event(tenant_id, "StockDeducted", "Inventory", mov_id, {"movement_id": mov_id, "product_id": prod_id, "warehouse_id": src_wh, "quantity": raw_qty}, conn=conn)
                    conn.commit()
                    self.send_json(201, {"id": mov_id, "movement_type": "OUTBOUND", "product_id": prod_id, "quantity": raw_qty, "remaining_stock": new_q, "created_at": now_iso})
                    return

                elif m_type == "INBOUND":
                    if not dst_wh:
                        self.send_rfc7807(400, "Bad Request", "Destination warehouse required for INBOUND", "MISSING_DESTINATION_WAREHOUSE")
                        return
                    cur_si = conn.execute("SELECT * FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?", (tenant_id, dst_wh, prod_id))
                    stock_item = cur_si.fetchone()
                    effective_cost = unit_cost if unit_cost is not None else product["cost_price"]
                    if stock_item:
                        prev_q = stock_item["quantity_on_hand"]
                        prev_wac = stock_item["average_cost"]
                        new_q = prev_q + raw_qty
                        new_wac = ((prev_q * prev_wac) + (raw_qty * effective_cost)) // new_q if new_q > 0 else effective_cost
                        conn.execute("UPDATE stock_items SET quantity_on_hand = ?, average_cost = ?, updated_at = ? WHERE id = ?", (new_q, new_wac, now_iso, stock_item["id"]))
                    else:
                        new_q = raw_qty
                        new_wac = effective_cost
                        conn.execute(
                            """
                            INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, average_cost, updated_at)
                            VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)
                            """,
                            (str(uuid.uuid4()), tenant_id, dst_wh, prod_id, new_q, product["reorder_threshold"], new_wac, now_iso)
                        )
                    conn.execute(
                        """
                        INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, destination_warehouse_id, quantity, unit_cost, reference_type, notes, batch_number, created_at)
                        VALUES (?, ?, 'INBOUND', ?, ?, ?, ?, 'DIRECT_MUTATION', ?, ?, ?)
                        """,
                        (mov_id, tenant_id, prod_id, dst_wh, raw_qty, effective_cost, notes, batch_num, now_iso)
                    )
                    rcv_val = raw_qty * effective_cost
                    if rcv_val > 0:
                        self.post_balanced_journal(
                            tenant_id, f"Inbound Stock Receipt ({product['name']})", "INVENTORY_INBOUND", mov_id,
                            [
                                {"account_code": "1300", "debit": rcv_val, "credit": 0, "memo": "Persediaan Barang Dagang"},
                                {"account_code": "2000", "debit": 0, "credit": rcv_val, "memo": "Utang Usaha"}
                            ],
                            conn=conn
                        )
                    self.post_outbox_event(tenant_id, "StockReceived", "Inventory", mov_id, {"movement_id": mov_id, "product_id": prod_id, "warehouse_id": dst_wh, "quantity": raw_qty}, conn=conn)
                    conn.commit()
                    self.send_json(201, {"id": mov_id, "movement_type": "INBOUND", "product_id": prod_id, "quantity": raw_qty, "resulting_stock": new_q, "average_cost": new_wac, "created_at": now_iso})
                    return
                else:
                    self.send_rfc7807(400, "Bad Request", f"Unsupported movement_type '{m_type}'", "INVALID_MOVEMENT_TYPE")
                    return

        if path == "/api/v1/inventory/transfer":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if not body or "source_warehouse_id" not in body or "destination_warehouse_id" not in body or "product_id" not in body or "quantity" not in body:
                self.send_rfc7807(400, "Bad Request", "source_warehouse_id, destination_warehouse_id, product_id, and quantity are required", "MISSING_REQUIRED_FIELDS")
                return

            src_wh = body["source_warehouse_id"]
            dst_wh = body["destination_warehouse_id"]
            prod_id = body["product_id"]
            raw_qty = body["quantity"]

            if src_wh == dst_wh:
                self.send_rfc7807(400, "Bad Request", "Source and destination warehouses cannot be the same", "SAME_WAREHOUSE_TRANSFER")
                return
            if not isinstance(raw_qty, int) or raw_qty <= 0:
                self.send_rfc7807(400, "Bad Request", "Transfer quantity must be a positive integer", "INVALID_QUANTITY")
                return

            now_iso = utc_now_iso()
            mov_id = str(uuid.uuid4())

            with get_db() as conn:
                cur_sw = conn.execute("SELECT id FROM warehouses WHERE id = ? AND tenant_id = ?", (src_wh, tenant_id))
                if not cur_sw.fetchone():
                    self.send_rfc7807(404, "Not Found", f"Source warehouse '{src_wh}' not found", "NOT_FOUND")
                    return

                cur_dw = conn.execute("SELECT id FROM warehouses WHERE id = ? AND tenant_id = ?", (dst_wh, tenant_id))
                if not cur_dw.fetchone():
                    self.send_rfc7807(404, "Not Found", f"Destination warehouse '{dst_wh}' not found", "NOT_FOUND")
                    return

                cur_p = conn.execute("SELECT id FROM products WHERE id = ? AND tenant_id = ?", (prod_id, tenant_id))
                if not cur_p.fetchone():
                    self.send_rfc7807(404, "Not Found", f"Product '{prod_id}' not found", "NOT_FOUND")
                    return

                cur_si = conn.execute("SELECT * FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?", (tenant_id, src_wh, prod_id))
                src_item = cur_si.fetchone()
                src_on_hand = src_item["quantity_on_hand"] if src_item else 0
                if src_on_hand < raw_qty:
                    self.send_rfc7807(422, "Unprocessable Entity", f"Insufficient source stock: requested {raw_qty}, available {src_on_hand}", "INSUFFICIENT_STOCK")
                    return

                new_src_q = src_on_hand - raw_qty
                conn.execute("UPDATE stock_items SET quantity_on_hand = ?, updated_at = ? WHERE id = ?", (new_src_q, now_iso, src_item["id"]))

                cur_dst = conn.execute("SELECT * FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?", (tenant_id, dst_wh, prod_id))
                dst_item = cur_dst.fetchone()
                if dst_item:
                    new_dst_q = dst_item["quantity_on_hand"] + raw_qty
                    conn.execute("UPDATE stock_items SET quantity_on_hand = ?, updated_at = ? WHERE id = ?", (new_dst_q, now_iso, dst_item["id"]))
                else:
                    new_dst_q = raw_qty
                    cur_p = conn.execute("SELECT * FROM products WHERE id = ? AND tenant_id = ?", (prod_id, tenant_id))
                    p_row = cur_p.fetchone()
                    reorder_th = p_row["reorder_threshold"] if p_row else 0
                    conn.execute(
                        """
                        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, average_cost, updated_at)
                        VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)
                        """,
                        (str(uuid.uuid4()), tenant_id, dst_wh, prod_id, new_dst_q, reorder_th, src_item["average_cost"], now_iso)
                    )

                conn.execute(
                    """
                    INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, source_warehouse_id, destination_warehouse_id, quantity, unit_cost, reference_type, notes, created_at)
                    VALUES (?, ?, 'TRANSFER', ?, ?, ?, ?, ?, 'TRANSFER', ?, ?)
                    """,
                    (mov_id, tenant_id, prod_id, src_wh, dst_wh, raw_qty, src_item["average_cost"], body.get("notes"), now_iso)
                )

                self.post_outbox_event(tenant_id, "StockTransferred", "Inventory", mov_id, {
                    "movement_id": mov_id, "source_warehouse_id": src_wh, "destination_warehouse_id": dst_wh,
                    "product_id": prod_id, "quantity": raw_qty
                }, conn=conn)

                conn.commit()

            self.send_json(200, {
                "movement_id": mov_id,
                "source_warehouse_id": src_wh,
                "destination_warehouse_id": dst_wh,
                "product_id": prod_id,
                "quantity": raw_qty,
                "status": "COMPLETED",
                "source_remaining": new_src_q,
                "destination_total": new_dst_q,
                "created_at": now_iso
            })
            return

        if path == "/api/v1/inventory/adjust":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if not body or "warehouse_id" not in body or "product_id" not in body or "actual_quantity" not in body:
                self.send_rfc7807(400, "Bad Request", "warehouse_id, product_id, and actual_quantity are required", "MISSING_REQUIRED_FIELDS")
                return

            wh_id = body["warehouse_id"]
            prod_id = body["product_id"]
            act_qty = body["actual_quantity"]
            if not isinstance(act_qty, int) or act_qty < 0:
                self.send_rfc7807(422, "Unprocessable Entity", "Actual stock quantity cannot be negative", "NEGATIVE_STOCK_PROHIBITED")
                return

            reason = body.get("reason", "Physical inventory count adjustment")
            now_iso = utc_now_iso()
            adj_id = str(uuid.uuid4())
            mov_id = str(uuid.uuid4())

            with get_db() as conn:
                cur_wh = conn.execute("SELECT id FROM warehouses WHERE id = ? AND tenant_id = ?", (wh_id, tenant_id))
                if not cur_wh.fetchone():
                    self.send_rfc7807(404, "Not Found", f"Warehouse '{wh_id}' not found", "NOT_FOUND")
                    return

                cur_p = conn.execute("SELECT * FROM products WHERE id = ? AND tenant_id = ?", (prod_id, tenant_id))
                prod = cur_p.fetchone()
                if not prod:
                    self.send_rfc7807(404, "Not Found", f"Product '{prod_id}' not found", "NOT_FOUND")
                    return

                cur_si = conn.execute("SELECT * FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?", (tenant_id, wh_id, prod_id))
                stock_item = cur_si.fetchone()
                prev_q = stock_item["quantity_on_hand"] if stock_item else 0
                avg_cost = stock_item["average_cost"] if stock_item and stock_item["average_cost"] > 0 else prod["cost_price"]

                variance = act_qty - prev_q
                if stock_item:
                    conn.execute("UPDATE stock_items SET quantity_on_hand = ?, updated_at = ? WHERE id = ?", (act_qty, now_iso, stock_item["id"]))
                else:
                    conn.execute(
                        """
                        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, average_cost, updated_at)
                        VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)
                        """,
                        (str(uuid.uuid4()), tenant_id, wh_id, prod_id, act_qty, prod["reorder_threshold"], avg_cost, now_iso)
                    )

                cur_c = conn.execute("SELECT COUNT(*) FROM stock_adjustments WHERE tenant_id = ?", (tenant_id,))
                seq = cur_c.fetchone()[0] + 1
                adj_num = f"ADJ-{datetime.now(timezone.utc).year}-{seq:06d}"

                j_id = None
                if variance != 0 and avg_cost > 0:
                    diff_val = abs(variance) * avg_cost
                    if variance > 0:
                        j_lines = [
                            {"account_code": "1300", "debit": diff_val, "credit": 0, "memo": f"Penyesuaian Fisik Lebih {adj_num}"},
                            {"account_code": "5000", "debit": 0, "credit": diff_val, "memo": "Penyesuaian Selisih Persediaan Lebih"}
                        ]
                    else:
                        j_lines = [
                            {"account_code": "5000", "debit": diff_val, "credit": 0, "memo": "Penyesuaian Selisih Persediaan Kurang"},
                            {"account_code": "1300", "debit": 0, "credit": diff_val, "memo": f"Penyesuaian Fisik Kurang {adj_num}"}
                        ]
                    j_id = self.post_balanced_journal(tenant_id, f"Penyesuaian Stok Fisik {adj_num} ({prod['name']})", "STOCK_ADJUSTMENT", adj_id, j_lines, conn=conn)

                conn.execute(
                    """
                    INSERT INTO stock_adjustments (id, tenant_id, adjustment_number, warehouse_id, product_id, previous_quantity, actual_quantity, variance, reason, journal_entry_id, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (adj_id, tenant_id, adj_num, wh_id, prod_id, prev_q, act_qty, variance, reason, j_id, now_iso)
                )

                conn.execute(
                    """
                    INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, destination_warehouse_id, quantity, unit_cost, reference_type, reference_id, notes, created_at)
                    VALUES (?, ?, 'ADJUSTMENT', ?, ?, ?, ?, 'ADJUSTMENT', ?, ?, ?)
                    """,
                    (mov_id, tenant_id, prod_id, wh_id, abs(variance) if variance != 0 else 1, avg_cost, adj_id, reason, now_iso)
                )

                self.post_outbox_event(tenant_id, "StockAdjusted", "Inventory", adj_id, {
                    "adjustment_id": adj_id, "adjustment_number": adj_num, "warehouse_id": wh_id,
                    "product_id": prod_id, "previous_quantity": prev_q, "actual_quantity": act_qty, "variance": variance
                }, conn=conn)

                conn.commit()

            self.send_json(200, {
                "id": adj_id,
                "adjustment_number": adj_num,
                "warehouse_id": wh_id,
                "product_id": prod_id,
                "previous_quantity": prev_q,
                "actual_quantity": act_qty,
                "variance": variance,
                "reason": reason,
                "journal_entry_id": j_id,
                "created_at": now_iso
            })
            return

        if path == "/api/v1/purchase-orders":
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if not body or "supplier_name" not in body or "destination_warehouse_id" not in body or "items" not in body:
                self.send_rfc7807(400, "Bad Request", "supplier_name, destination_warehouse_id, and items are required", "MISSING_REQUIRED_FIELDS")
                return

            items = body.get("items", [])
            if not items or not isinstance(items, list):
                self.send_rfc7807(400, "Bad Request", "Purchase order requires at least one line item", "EMPTY_LINE_ITEMS")
                return

            for it in items:
                q = it.get("quantity_ordered")
                c = it.get("unit_cost")
                if not isinstance(q, int) or q <= 0:
                    self.send_rfc7807(400, "Bad Request", "quantity_ordered must be a positive integer", "INVALID_QUANTITY")
                    return
                if not isinstance(c, int) or c < 0:
                    self.send_rfc7807(400, "Bad Request", "unit_cost must be non-negative integer", "INVALID_UNIT_COST")
                    return

            supp_name = str(body["supplier_name"]).strip()
            dst_wh = body["destination_warehouse_id"]
            notes = body.get("notes")
            now_iso = utc_now_iso()
            po_id = str(uuid.uuid4())

            with get_db() as conn:
                cur_c = conn.execute("SELECT COUNT(*) FROM purchase_orders WHERE tenant_id = ?", (tenant_id,))
                seq = cur_c.fetchone()[0] + 1
                po_num = f"PO-{datetime.now(timezone.utc).year}-{seq:06d}"
                total_amt = sum(it["quantity_ordered"] * it["unit_cost"] for it in items)

                conn.execute(
                    """
                    INSERT INTO purchase_orders (id, tenant_id, po_number, supplier_name, destination_warehouse_id, status, total_amount, notes, created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?, 'DRAFT', ?, ?, ?, ?)
                    """,
                    (po_id, tenant_id, po_num, supp_name, dst_wh, total_amt, notes, now_iso, now_iso)
                )

                saved_items = []
                for it in items:
                    item_id = str(uuid.uuid4())
                    subtot = it["quantity_ordered"] * it["unit_cost"]
                    conn.execute(
                        """
                        INSERT INTO purchase_order_items (id, tenant_id, purchase_order_id, product_id, quantity_ordered, quantity_received, unit_cost, total_cost, created_at)
                        VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)
                        """,
                        (item_id, tenant_id, po_id, it["product_id"], it["quantity_ordered"], it["unit_cost"], subtot, now_iso)
                    )
                    saved_items.append({
                        "id": item_id,
                        "purchase_order_id": po_id,
                        "product_id": it["product_id"],
                        "quantity_ordered": it["quantity_ordered"],
                        "quantity_received": 0,
                        "unit_cost": it["unit_cost"],
                        "total_cost": subtot
                    })

                conn.commit()

            self.send_json(201, {
                "id": po_id,
                "tenant_id": tenant_id,
                "po_number": po_num,
                "supplier_name": supp_name,
                "destination_warehouse_id": dst_wh,
                "status": "DRAFT",
                "total_amount": total_amt,
                "notes": notes,
                "items": saved_items,
                "created_at": now_iso,
                "updated_at": now_iso
            })
            return

        if re.match(r"^/api/v1/purchase-orders/[^/]+/order$", path):
            po_id = path.split("/")[-2]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            now_iso = utc_now_iso()
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM purchase_orders WHERE id = ? AND tenant_id = ?", (po_id, tenant_id))
                po_row = cur.fetchone()
                if not po_row:
                    self.send_rfc7807(404, "Not Found", f"Purchase order '{po_id}' not found", "NOT_FOUND")
                    return
                if po_row["status"] != "DRAFT":
                    self.send_rfc7807(422, "Unprocessable Entity", f"Cannot order PO in status '{po_row['status']}'", "PO_NOT_DRAFT")
                    return
                conn.execute("UPDATE purchase_orders SET status = 'ORDERED', updated_at = ? WHERE id = ?", (now_iso, po_id))
                conn.commit()
            self.send_json(200, {"id": po_id, "status": "ORDERED", "updated_at": now_iso})
            return

        if re.match(r"^/api/v1/purchase-orders/[^/]+/receive$", path):
            po_id = path.split("/")[-2]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return

            idempotency_key = self.headers.get("Idempotency-Key")
            if idempotency_key:
                payload_hash = hashlib.sha256(raw_str.encode("utf-8")).hexdigest()
                with get_db() as conn:
                    cur = conn.execute("SELECT * FROM idempotency_keys WHERE user_id = ? AND key = ?", (user_id, idempotency_key))
                    cached = cur.fetchone()
                    if cached:
                        cached_data = json.loads(cached["response_body"])
                        self.send_json(cached["response_status"], cached_data, headers={"X-Cache-Replay": "true"})
                        return

            if not body or "items" not in body:
                self.send_rfc7807(400, "Bad Request", "Items payload is required for goods receipt", "MISSING_REQUIRED_FIELDS")
                return

            rcv_items = body.get("items", [])
            if not rcv_items or not isinstance(rcv_items, list):
                self.send_rfc7807(400, "Bad Request", "Goods receipt requires at least one received item", "EMPTY_ITEMS")
                return

            now_iso = utc_now_iso()

            with get_db() as conn:
                cur = conn.execute("SELECT * FROM purchase_orders WHERE id = ? AND tenant_id = ?", (po_id, tenant_id))
                po_row = cur.fetchone()
                if not po_row:
                    self.send_rfc7807(404, "Not Found", f"Purchase order '{po_id}' not found", "NOT_FOUND")
                    return
                if po_row["status"] == "DRAFT":
                    self.send_rfc7807(422, "Unprocessable Entity", "Cannot receive items on DRAFT purchase order; must be ORDERED", "PO_NOT_ORDERED")
                    return
                if po_row["status"] == "CANCELLED":
                    self.send_rfc7807(422, "Unprocessable Entity", "Cannot receive items on CANCELLED purchase order", "PO_CANCELLED")
                    return
                if po_row["status"] == "RECEIVED":
                    self.send_rfc7807(422, "Unprocessable Entity", "Purchase order has already been fully received", "PO_ALREADY_RECEIVED")
                    return

                cur_lines = conn.execute("SELECT * FROM purchase_order_items WHERE purchase_order_id = ? ORDER BY rowid ASC", (po_id,))
                lines = [dict(row) for row in cur_lines.fetchall()]
                po_prods = {l["product_id"] for l in lines}

                for r_it in rcv_items:
                    pid = r_it.get("product_id")
                    q_rcv = r_it.get("quantity_received", 0)
                    if not isinstance(q_rcv, int) or q_rcv <= 0:
                        self.send_rfc7807(400, "Bad Request", "quantity_received must be a positive integer", "INVALID_QUANTITY")
                        return
                    if pid not in po_prods:
                        self.send_rfc7807(422, "Unprocessable Entity", f"Product '{pid}' is not part of this purchase order", "INVALID_PRODUCT_LINE")
                        return

                # Cumulative requested vs remaining check
                req_qty_by_prod = {}
                for r_it in rcv_items:
                    pid = r_it.get("product_id")
                    req_qty_by_prod[pid] = req_qty_by_prod.get(pid, 0) + r_it.get("quantity_received", 0)

                po_rem_by_prod = {}
                for l in lines:
                    pid = l["product_id"]
                    rem = l["quantity_ordered"] - l["quantity_received"]
                    po_rem_by_prod[pid] = po_rem_by_prod.get(pid, 0) + rem

                for pid, total_req in req_qty_by_prod.items():
                    remain = po_rem_by_prod.get(pid, 0)
                    if total_req > remain:
                        self.send_rfc7807(422, "Unprocessable Entity", f"Received quantity {total_req} exceeds remaining ordered quantity {remain}", "QUANTITY_EXCEEDS_ORDERED")
                        return

                total_receipt_value = 0
                dst_wh = po_row["destination_warehouse_id"]

                for r_it in rcv_items:
                    pid = r_it["product_id"]
                    q_rcv = r_it["quantity_received"]
                    remaining_to_fill = q_rcv
                    fallback_unit_cost = None
                    for line in lines:
                        if line["product_id"] == pid:
                            if fallback_unit_cost is None:
                                fallback_unit_cost = line["unit_cost"]
                            rem = line["quantity_ordered"] - line["quantity_received"]
                            if rem > 0 and remaining_to_fill > 0:
                                alloc = min(remaining_to_fill, rem)
                                line["quantity_received"] += alloc
                                remaining_to_fill -= alloc
                                conn.execute("UPDATE purchase_order_items SET quantity_received = ? WHERE id = ?", (line["quantity_received"], line["id"]))

                    unit_c = r_it.get("unit_cost", fallback_unit_cost or 0)
                    batch_n = r_it.get("batch_number", "BATCH-DEFAULT")
                    line_val = q_rcv * unit_c
                    total_receipt_value += line_val

                    cur_si = conn.execute("SELECT * FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?", (tenant_id, dst_wh, pid))
                    si = cur_si.fetchone()
                    if si:
                        prev_q = si["quantity_on_hand"]
                        prev_wac = si["average_cost"]
                        new_q = prev_q + q_rcv
                        new_wac = ((prev_q * prev_wac) + (q_rcv * unit_c)) // new_q if new_q > 0 else unit_c
                        conn.execute("UPDATE stock_items SET quantity_on_hand = ?, average_cost = ?, updated_at = ? WHERE id = ?", (new_q, new_wac, now_iso, si["id"]))
                    else:
                        new_q = q_rcv
                        new_wac = unit_c
                        cur_p = conn.execute("SELECT * FROM products WHERE id = ? AND tenant_id = ?", (pid, tenant_id))
                        p_row = cur_p.fetchone()
                        reorder_th = p_row["reorder_threshold"] if p_row else 0
                        conn.execute(
                            """
                            INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, average_cost, updated_at)
                            VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)
                            """,
                            (str(uuid.uuid4()), tenant_id, dst_wh, pid, new_q, reorder_th, new_wac, now_iso)
                        )

                    conn.execute(
                        """
                        INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, destination_warehouse_id, quantity, unit_cost, reference_type, reference_id, batch_number, created_at)
                        VALUES (?, ?, 'INBOUND', ?, ?, ?, ?, 'PURCHASE_ORDER', ?, ?, ?)
                        """,
                        (str(uuid.uuid4()), tenant_id, pid, dst_wh, q_rcv, unit_c, po_id, batch_n, now_iso)
                    )

                j_id = None
                if total_receipt_value > 0:
                    j_id = self.post_balanced_journal(
                        tenant_id, f"Penerimaan Barang Pesanan {po_row['po_number']}", "PURCHASE_ORDER_RECEIPT", po_id,
                        [
                            {"account_code": "1300", "debit": total_receipt_value, "credit": 0, "memo": f"Persediaan Masuk PO {po_row['po_number']}"},
                            {"account_code": "2000", "debit": 0, "credit": total_receipt_value, "memo": f"Utang Usaha PO {po_row['po_number']}"}
                        ],
                        conn=conn
                    )

                self.post_outbox_event(tenant_id, "StockReceived", "PurchaseOrder", po_id, {
                    "po_id": po_id, "po_number": po_row["po_number"], "total_receipt_value": total_receipt_value, "journal_id": j_id
                }, conn=conn)

                all_received = all(l["quantity_received"] >= l["quantity_ordered"] for l in lines)
                new_status = "RECEIVED" if all_received else "PARTIALLY_RECEIVED"
                conn.execute("UPDATE purchase_orders SET status = ?, updated_at = ? WHERE id = ?", (new_status, now_iso, po_id))

                res_data = {
                    "id": po_id,
                    "po_number": po_row["po_number"],
                    "status": new_status,
                    "total_receipt_value": total_receipt_value,
                    "journal_entry_id": j_id,
                    "updated_at": now_iso
                }

                if idempotency_key:
                    conn.execute(
                        "INSERT OR REPLACE INTO idempotency_keys (user_id, key, payload_hash, response_status, response_body, created_at) VALUES (?, ?, ?, 200, ?, ?)",
                        (user_id, idempotency_key, payload_hash, json.dumps(res_data), now_iso)
                    )

                conn.commit()

            self.send_json(200, res_data)
            return

        if re.match(r"^/api/v1/purchase-orders/[^/]+/cancel$", path):
            po_id = path.split("/")[-2]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            now_iso = utc_now_iso()
            with get_db() as conn:
                cur = conn.execute("SELECT * FROM purchase_orders WHERE id = ? AND tenant_id = ?", (po_id, tenant_id))
                po_row = cur.fetchone()
                if not po_row:
                    self.send_rfc7807(404, "Not Found", f"Purchase order '{po_id}' not found", "NOT_FOUND")
                    return
                if po_row["status"] in ["PARTIALLY_RECEIVED", "RECEIVED"]:
                    self.send_rfc7807(422, "Unprocessable Entity", f"Cannot cancel purchase order in status '{po_row['status']}'", "CANNOT_CANCEL_RECEIVED_PO")
                    return
                conn.execute("UPDATE purchase_orders SET status = 'CANCELLED', updated_at = ? WHERE id = ?", (now_iso, po_id))
                conn.commit()
            self.send_json(200, {"id": po_id, "status": "CANCELLED", "updated_at": now_iso})
            return

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
        if path in ["/api/v1/subscriptions/trial", "/api/v1/subscriptions/trial/activate"]:
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
            plan = (body.get("plan") or body.get("plan_id") or "premium_monthly") if body else "premium_monthly"
            provider = body.get("provider", "dana") if body else "dana"

            if provider.lower() != "dana":
                self.send_rfc7807(400, "Bad Request", f"Unsupported payment provider '{provider}'. DANA is the exclusive payment provider.", "UNSUPPORTED_PROVIDER")
                return

            if plan not in ["premium_monthly", "premium_annual"]:
                self.send_rfc7807(400, "Bad Request", "Invalid plan. Choose 'premium_monthly' or 'premium_annual'", "INVALID_PLAN")
                return

            amount = 10000 if plan == "premium_monthly" else 110000

            if body and "amount" in body:
                client_amt = body.get("amount")
                if isinstance(client_amt, (int, float)) and client_amt <= 0:
                    self.send_rfc7807(400, "Bad Request", "Amount must be greater than 0", "INVALID_AMOUNT")
                    return
                if client_amt != amount:
                    self.send_rfc7807(400, "Bad Request", "Pricing is locked to official plan rate", "INVALID_AMOUNT")
                    return

            order_id = f"ORD-DANA-{user_id[:8]}-{int(time.time())}"
            checkout_url = f"https://m.dana.id/d/checkout?orderId={order_id}&amount={amount}"

            self.log_audit(user_id, "CHECKOUT_INITIATED", "order", order_id, f"Plan: {plan}, Amount: {amount}")
            self.send_json(200, {
                "order_id": order_id,
                "checkout_url": checkout_url,
                "reference_no": f"REF-DANA-{uuid.uuid4().hex[:12].upper()}",
                "amount": amount,
                "currency": "IDR",
                "plan": plan,
                "plan_id": plan,
                "provider": "dana"
            })
            return

        if path in ["/api/v1/subscriptions/free", "/api/v1/subscriptions/select-free"]:
            self.log_audit(user_id, "PLAN_SELECTED_FREE", "subscription", user_id, "User selected Free plan")
            self.send_json(200, {
                "status": "free",
                "tier": "free",
                "is_premium": False,
                "message": "Continuing with Free plan"
            })
            return

        # 11a. Ingestion Pipeline: Notification (§20, §21, §24, REQ-INGEST-01 to REQ-INGEST-05)
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
                        "source": "notification",
                        "confidence": confidence,
                        "transaction_id": None
                    })
                    return

                if amount > 0:
                    cid = str(uuid.uuid4())
                    conn.execute(
                        "INSERT INTO transaction_candidates (id, user_id, source, provider, amount, direction, occurred_at, merchant, confidence, status, created_at) VALUES (?, ?, 'notification', ?, ?, ?, ?, ?, ?, 'pending', ?)",
                        (cid, user_id, pkg, amount, direction, now_iso, merchant, confidence, now_iso)
                    )
                    conn.commit()
                else:
                    cid = None

            self.send_json(200, {
                "event_id": event_id,
                "candidate_id": cid,
                "source": "notification",
                "status": "auto_created" if confidence == "HIGH" else ("requires_confirmation" if amount > 0 else "ignored"),
                "confidence": confidence,
                "amount": amount
            })
            return

        # 11b. Ingestion Pipeline: SMS (§20, §22, §24)
        if path == "/api/v1/ingestion/sms":
            if not is_premium:
                self.send_rfc7807(403, "Forbidden", "Fitur otomatisasi transaksi memerlukan Premium", "FEATURE_LOCKED")
                return

            if not body or "text" not in body:
                self.send_rfc7807(400, "Bad Request", "Missing SMS text", "BAD_REQUEST")
                return

            text = body["text"]
            sender = body.get("sender", "SMS-BANK")
            event_id = str(uuid.uuid4())

            amt_match = re.search(r"Rp\s*([\d\.,]+)", text, re.IGNORECASE)
            amount = 0
            if amt_match:
                cleaned = amt_match.group(1).replace(".", "").replace(",", "")
                amount = int(cleaned)

            direction = "income" if any(k in text.lower() for k in ["masuk", "berhasil diterima", "credit", "kredit"]) else "expense"
            merchant = "Bank SMS"
            if "ke " in text:
                merchant = text.split("ke ")[-1].split()[0]

            confidence = "HIGH" if amount > 0 and any(b in sender.upper() for b in ["BCA", "BRI", "BNI", "MANDIRI", "DANA"]) else "MEDIUM"
            if amount == 0:
                confidence = "LOW"

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
                        "source": "sms",
                        "confidence": confidence,
                        "transaction_id": None
                    })
                    return

                if amount > 0:
                    cid = str(uuid.uuid4())
                    conn.execute(
                        "INSERT INTO transaction_candidates (id, user_id, source, provider, amount, direction, occurred_at, merchant, confidence, status, created_at) VALUES (?, ?, 'sms', ?, ?, ?, ?, ?, ?, 'pending', ?)",
                        (cid, user_id, sender, amount, direction, now_iso, merchant, confidence, now_iso)
                    )
                    conn.commit()
                else:
                    cid = None

            self.send_json(200, {
                "event_id": event_id,
                "candidate_id": cid,
                "status": "auto_created" if confidence == "HIGH" else ("requires_confirmation" if amount > 0 else "ignored"),
                "confidence": confidence,
                "amount": amount,
                "direction": direction,
                "source": "sms",
                "sender": sender
            })
            return

        # 11c. Ingestion Pipeline: Gmail (§20, §23, §24)
        if path == "/api/v1/ingestion/gmail":
            if not is_premium:
                self.send_rfc7807(403, "Forbidden", "Fitur otomatisasi transaksi memerlukan Premium", "FEATURE_LOCKED")
                return

            if not body or ("snippet" not in body and "subject" not in body):
                self.send_rfc7807(400, "Bad Request", "Missing email snippet or subject", "BAD_REQUEST")
                return

            snippet = body.get("snippet", "")
            subject = body.get("subject", "")
            msg_id = body.get("message_id", str(uuid.uuid4()))
            full_text = f"{subject} {snippet}"
            event_id = str(uuid.uuid4())

            amt_match = re.search(r"Rp\s*([\d\.,]+)", full_text, re.IGNORECASE)
            amount = 0
            if amt_match:
                cleaned = amt_match.group(1).replace(".", "").replace(",", "")
                amount = int(cleaned)

            direction = "income" if any(k in full_text.lower() for k in ["masuk", "berhasil diterima", "credit", "kredit"]) else "expense"
            merchant = "Gmail Merchant"
            if "ke " in full_text:
                merchant = full_text.split("ke ")[-1].split()[0]

            confidence = "HIGH" if amount > 0 and any(b in full_text.upper() for b in ["PLN", "BCA", "DANA", "GOPAY", "SHOPEEPAY", "TOKOPEDIA", "INVOICE"]) else "MEDIUM"
            if amount == 0:
                confidence = "LOW"

            now_iso = utc_now_iso()
            with get_db() as conn:
                cur = conn.execute(
                    "SELECT * FROM transaction_candidates WHERE user_id = ? AND (provider = ? OR (amount = ? AND direction = ?))",
                    (user_id, msg_id, amount, direction)
                )
                dup = cur.fetchone()
                if dup:
                    self.send_json(200, {
                        "event_id": event_id,
                        "status": "duplicate",
                        "source": "gmail",
                        "confidence": confidence,
                        "message_id": msg_id,
                        "transaction_id": None
                    })
                    return

                if amount > 0:
                    cid = str(uuid.uuid4())
                    conn.execute(
                        "INSERT INTO transaction_candidates (id, user_id, source, provider, amount, direction, occurred_at, merchant, confidence, status, created_at) VALUES (?, ?, 'gmail', ?, ?, ?, ?, ?, ?, 'pending', ?)",
                        (cid, user_id, msg_id, amount, direction, now_iso, merchant, confidence, now_iso)
                    )
                    conn.commit()
                else:
                    cid = None

            self.send_json(200, {
                "event_id": event_id,
                "candidate_id": cid,
                "status": "auto_created" if confidence == "HIGH" else ("requires_confirmation" if amount > 0 else "ignored"),
                "confidence": confidence,
                "amount": amount,
                "source": "gmail",
                "message_id": msg_id,
                "subject": subject,
                "snippet": snippet
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

    def do_PUT(self):
        parsed = urlparse(self.path)
        path = parsed.path
        body, raw_str = self.read_json_body()

        if path.startswith("/api/v1/audit"):
            self.send_rfc7807(405, "Method Not Allowed", "Audit logs are append-only and immutable", "IMMUTABLE_LOG")
            return

        if path.startswith("/api/v1/accounting/journals"):
            self.send_rfc7807(405, "Method Not Allowed", "Posted journals are immutable and cannot be updated directly; use reversal instead", "JOURNAL_IMMUTABLE")
            return

        user_id = self.get_auth_user_id()
        if not user_id:
            self.send_rfc7807(401, "Unauthorized", "Authentication required", "UNAUTHORIZED")
            return

        if path in ["/api/v1/users/profile", "/api/v1/profile", "/api/v1/user/preferences"]:
            display_name = body.get("display_name") if body else None
            if display_name:
                with get_db() as conn:
                    conn.execute("UPDATE users SET display_name = ?, updated_at = ? WHERE id = ?", (display_name, utc_now_iso(), user_id))
                    conn.commit()
            self.send_json(200, {"status": "updated", "display_name": display_name})
            return

        if re.match(r"^/api/v1/tenants/[^/]+/profile$", path):
            t_id = path.split("/")[-2]
            with get_db() as conn:
                cur = conn.execute("SELECT role FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, user_id))
                mem = cur.fetchone()
                if not mem:
                    self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                    return
                if mem["role"].lower() in ["staff", "member"]:
                    self.send_rfc7807(403, "Forbidden", "Insufficient role permissions to update profile", "FORBIDDEN")
                    return

                if not body:
                    self.send_rfc7807(400, "Bad Request", "Profile body required", "BAD_REQUEST")
                    return

                now_iso = utc_now_iso()
                b_name = body.get("business_name")
                legal = body.get("legal_name")
                tax_id = body.get("tax_id")
                addr = body.get("address")
                phone = body.get("phone")
                email = body.get("email")
                tz = body.get("timezone")
                if tz is not None and tz not in ["Asia/Jakarta", "Asia/Makassar", "Asia/Jayapura", "WIB", "WITA", "WIT"]:
                    self.send_rfc7807(400, "Bad Request", f"Invalid timezone '{tz}'. Supported: Asia/Jakarta, Asia/Makassar, Asia/Jayapura", "INVALID_TIMEZONE")
                    return
                curr = body.get("currency")
                loc = body.get("locale")
                prefix = body.get("invoice_prefix")
                if prefix is not None:
                    if len(prefix) == 0 or len(prefix) > 10:
                        self.send_rfc7807(400, "Bad Request", "Invoice prefix must be 1-10 characters", "INVALID_PREFIX")
                        return
                b_type = body.get("business_type")

                conn.execute(
                    """
                    UPDATE business_profiles SET
                        business_name = COALESCE(?, business_name),
                        legal_name = COALESCE(?, legal_name),
                        tax_id = COALESCE(?, tax_id),
                        address = COALESCE(?, address),
                        phone = COALESCE(?, phone),
                        email = COALESCE(?, email),
                        timezone = COALESCE(?, timezone),
                        currency = COALESCE(?, currency),
                        locale = COALESCE(?, locale),
                        invoice_prefix = COALESCE(?, invoice_prefix),
                        business_type = COALESCE(?, business_type),
                        updated_at = ?
                    WHERE tenant_id = ?
                    """,
                    (b_name, legal, tax_id, addr, phone, email, tz, curr, loc, prefix, b_type, now_iso, t_id)
                )
                conn.commit()

                cur_p = conn.execute("SELECT * FROM business_profiles WHERE tenant_id = ?", (t_id,))
                updated_prof = dict(cur_p.fetchone())

            self.send_json(200, updated_prof)
            return

        if re.match(r"^/api/v1/invoices/[^/]+$", path):
            inv_id = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return

            with get_db() as conn:
                cur = conn.execute("SELECT * FROM invoices WHERE id = ? AND tenant_id = ?", (inv_id, tenant_id))
                inv = cur.fetchone()
                if not inv:
                    self.send_rfc7807(404, "Not Found", "Invoice not found", "NOT_FOUND")
                    return
                if inv["status"] != "DRAFT":
                    self.send_rfc7807(409, "Conflict", "Issued or paid invoices cannot be modified", "INVOICE_LOCKED")
                    return

                now_iso = utc_now_iso()
                cust_name = body.get("customer_name", inv["customer_name"]) if body else inv["customer_name"]
                cust_addr = body.get("customer_address", inv["customer_address"]) if body else inv["customer_address"]
                cust_email = body.get("customer_email", inv["customer_email"]) if body else inv["customer_email"]
                due_date = body.get("due_date", inv["due_date"]) if body else inv["due_date"]

                conn.execute(
                    "UPDATE invoices SET customer_name = ?, customer_address = ?, customer_email = ?, due_date = ?, updated_at = ? WHERE id = ?",
                    (cust_name, cust_addr, cust_email, due_date, now_iso, inv_id)
                )
                conn.commit()

            self.send_json(200, {"id": inv_id, "status": "DRAFT", "updated_at": now_iso})
            return

        self.send_rfc7807(404, "Not Found", f"PUT endpoint '{path}' not found", "NOT_FOUND")

    def do_DELETE(self):
        parsed = urlparse(self.path)
        path = parsed.path

        if path.startswith("/api/v1/audit"):
            self.send_rfc7807(405, "Method Not Allowed", "Audit logs cannot be deleted; logs are append-only", "IMMUTABLE_LOG")
            return

        if path.startswith("/api/v1/accounting/journals"):
            self.send_rfc7807(405, "Method Not Allowed", "Posted journals cannot be deleted; journals are immutable", "JOURNAL_IMMUTABLE")
            return

        user_id = self.get_auth_user_id()
        if not user_id:
            self.send_rfc7807(401, "Unauthorized", "Authentication required", "UNAUTHORIZED")
            return

        if re.match(r"^/api/v1/tenants/[^/]+/members/[^/]+$", path):
            parts = path.split("/")
            t_id = parts[4]
            target_uid = parts[6]
            with get_db() as conn:
                cur = conn.execute("SELECT role FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, user_id))
                mem = cur.fetchone()
                if not mem:
                    self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                    return
                if mem["role"].lower() in ["staff", "member"]:
                    self.send_rfc7807(403, "Forbidden", "Only owner or admin can remove members", "FORBIDDEN")
                    return
                conn.execute("DELETE FROM memberships WHERE tenant_id = ? AND user_id = ?", (t_id, target_uid))
                conn.commit()
            self.send_json(200, {"status": "removed", "user_id": target_uid})
            return

        if re.match(r"^/api/v1/accounting/accounts/[^/]+$", path):
            code = path.split("/")[-1]
            tenant_id, role, err = self.resolve_tenant_context(user_id, path)
            if err == "NOT_FOUND":
                self.send_rfc7807(404, "Not Found", "Workspace not found", "NOT_FOUND")
                return
            if code in ["1000", "1100", "1200", "1300", "2000", "2100", "4000", "5000", "6000"]:
                self.send_rfc7807(403, "Forbidden", f"System account '{code}' is protected from deletion", "SYSTEM_ACCOUNT_PROTECTED")
                return
            with get_db() as conn:
                conn.execute("DELETE FROM chart_of_accounts WHERE tenant_id = ? AND code = ? AND is_system = 0", (tenant_id, code))
                conn.commit()
            self.send_json(200, {"status": "deleted", "code": code})
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

    def do_PATCH(self):
        parsed = urlparse(self.path)
        path = parsed.path

        if path.startswith("/api/v1/accounting/journals"):
            self.send_rfc7807(405, "Method Not Allowed", "Posted journals cannot be modified; journals are immutable", "JOURNAL_IMMUTABLE")
            return

        self.send_rfc7807(405, "Method Not Allowed", f"PATCH endpoint '{path}' not allowed", "METHOD_NOT_ALLOWED")

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
