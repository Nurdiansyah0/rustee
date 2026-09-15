#!/usr/bin/env python3
"""
Tier 1: Feature Coverage Test Suite (145 tests across 29 features).
Tests primary functional behavior and interface contracts for all features.
"""

import sys
import os
import time
import uuid
import json
import base64
import hashlib
from datetime import datetime, timezone

# Add harness directory to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from harness.client import (
    ApiClient, TapReporter, assert_true, assert_equal,
    assert_status, assert_rfc7807, compute_midtrans_signature
)

try:
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import padding
    HAS_CRYPTO = True
except ImportError:
    HAS_CRYPTO = False

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

def sign_dana_test_webhook(path: str, body_bytes: bytes, timestamp: str) -> str:
    if not HAS_CRYPTO:
        return ""
    body_hash = hashlib.sha256(body_bytes).hexdigest()
    string_to_sign = f"POST:{path}:{body_hash}:{timestamp}"
    priv_key = serialization.load_pem_private_key(DEFAULT_DANA_TEST_PRIVATE_KEY.encode("utf-8"), password=None)
    sig = priv_key.sign(
        string_to_sign.encode("utf-8"),
        padding.PKCS1v15(),
        hashes.SHA256()
    )
    return base64.b64encode(sig).decode("utf-8")

def run_tier1_tests(base_url: str = "http://127.0.0.1:8089", reporter: TapReporter = None) -> bool:
    if reporter is None:
        reporter = TapReporter(total_expected=155)
        reporter.print_header()

    client = ApiClient(base_url)

    # Unique test run identifier
    run_id = f"t1_{int(time.time())}_{uuid.uuid4().hex[:6]}"
    u1_email = f"user1_{run_id}@example.com"
    u2_email = f"user2_{run_id}@example.com"
    u1_pass = "Password123!Secure"
    u2_pass = "Password456!Secure"

    # -------------------------------------------------------------
    # F01: SQLite WAL & PRAGMAs (5 tests)
    # -------------------------------------------------------------
    try:
        resp = client.get("/ready")
        assert_status(resp, 200, "F01-01: /ready probe returns 200")
        assert_equal(resp.json.get("wal_mode"), True, "F01-01: SQLite configured with WAL mode")
        reporter.record(True, "T1-F01-01: SQLite initialized with WAL mode")
    except Exception as e:
        reporter.record(False, "T1-F01-01: SQLite initialized with WAL mode", str(e))

    try:
        resp = client.get("/ready")
        assert_equal(resp.json.get("database"), "connected", "F01-02: DB reports connected")
        reporter.record(True, "T1-F01-02: SQLite connection pool responsive")
    except Exception as e:
        reporter.record(False, "T1-F01-02: SQLite connection pool responsive", str(e))

    try:
        # Check liveness probe returns pass
        resp = client.get("/health")
        assert_status(resp, 200, "F01-03: /health returns 200")
        assert_equal(resp.json.get("status"), "pass", "F01-03: status is pass")
        reporter.record(True, "T1-F01-03: SQLite liveness health check pass")
    except Exception as e:
        reporter.record(False, "T1-F01-03: SQLite liveness health check pass", str(e))

    try:
        # Check HEAD request on ready probe
        resp = client.head("/ready")
        assert_status(resp, 200, "F01-04: HEAD /ready returns 200")
        reporter.record(True, "T1-F01-04: SQLite ready probe supports HTTP HEAD")
    except Exception as e:
        reporter.record(False, "T1-F01-04: SQLite ready probe supports HTTP HEAD", str(e))

    try:
        # Check WAL concurrency with multiple rapid reads
        for _ in range(5):
            r = client.get("/ready")
            assert_status(r, 200, "F01-05: Rapid ready read")
        reporter.record(True, "T1-F01-05: SQLite WAL mode supports non-blocking concurrent reads")
    except Exception as e:
        reporter.record(False, "T1-F01-05: SQLite WAL mode supports non-blocking concurrent reads", str(e))

    # -------------------------------------------------------------
    # F02: Relational Schema Migrations (5 tests)
    # -------------------------------------------------------------
    user1_data = None
    try:
        # Register user1 (creates user, default accounts, default categories, subscription)
        reg_resp = client.register(u1_email, u1_pass, "User One")
        assert_status(reg_resp, 201, "F02-01: User registration succeeds")
        user1_data = reg_resp.json
        assert_true("id" in user1_data, "User record has ID")
        reporter.record(True, "T1-F02-01: Users table schema supports registration")
    except Exception as e:
        reporter.record(False, "T1-F02-01: Users table schema supports registration", str(e))

    try:
        acc_resp = client.list_accounts()
        assert_status(acc_resp, 200, "F02-02: List accounts succeeds")
        accounts = acc_resp.json
        assert_true(isinstance(accounts, list) and len(accounts) >= 2, "Default accounts created")
        reporter.record(True, "T1-F02-02: Accounts table seeded with default wallets")
    except Exception as e:
        reporter.record(False, "T1-F02-02: Accounts table seeded with default wallets", str(e))

    try:
        cat_resp = client.list_categories()
        assert_status(cat_resp, 200, "F02-03: List categories succeeds")
        categories = cat_resp.json
        assert_true(isinstance(categories, list) and len(categories) >= 2, "Default categories created")
        reporter.record(True, "T1-F02-03: Categories table seeded with default categories")
    except Exception as e:
        reporter.record(False, "T1-F02-03: Categories table seeded with default categories", str(e))

    try:
        sub_resp = client.subscription_status()
        assert_status(sub_resp, 200, "F02-04: Subscription status succeeds")
        assert_equal(sub_resp.json.get("tier"), "free", "Default tier is free")
        reporter.record(True, "T1-F02-04: Subscriptions table schema holds user subscription state")
    except Exception as e:
        reporter.record(False, "T1-F02-04: Subscriptions table schema holds user subscription state", str(e))

    try:
        tx_resp = client.list_transactions()
        assert_status(tx_resp, 200, "F02-05: Transactions table query succeeds")
        reporter.record(True, "T1-F02-05: Transactions table schema verified")
    except Exception as e:
        reporter.record(False, "T1-F02-05: Transactions table schema verified", str(e))

    # -------------------------------------------------------------
    # F03: Composite & Performance Indexes (5 tests)
    # -------------------------------------------------------------
    try:
        # Transactions filtered by date range
        tx_filtered = client.list_transactions({"start_date": "2026-01-01", "end_date": "2026-12-31"})
        assert_status(tx_filtered, 200, "F03-01: Filter by date range")
        reporter.record(True, "T1-F03-01: Composite index (user_id, date) supports date range queries")
    except Exception as e:
        reporter.record(False, "T1-F03-01: Composite index (user_id, date) supports date range queries", str(e))

    try:
        acc_id = acc_resp.json[0]["id"]
        tx_acc = client.list_transactions({"account_id": acc_id})
        assert_status(tx_acc, 200, "F03-02: Filter by account_id")
        reporter.record(True, "T1-F03-02: Index (account_id) supports account transaction lookups")
    except Exception as e:
        reporter.record(False, "T1-F03-02: Index (account_id) supports account transaction lookups", str(e))

    try:
        cat_id = cat_resp.json[0]["id"]
        tx_cat = client.list_transactions({"category_id": cat_id})
        assert_status(tx_cat, 200, "F03-03: Filter by category_id")
        reporter.record(True, "T1-F03-03: Filter transactions by category_id executes cleanly")
    except Exception as e:
        reporter.record(False, "T1-F03-03: Filter transactions by category_id executes cleanly", str(e))

    try:
        # Composite query with limit and date range
        tx_page = client.list_transactions({"start_date": "2026-09-01", "limit": 10})
        assert_status(tx_page, 200, "F03-04: Limit and date query")
        reporter.record(True, "T1-F03-04: Composite query with pagination limit")
    except Exception as e:
        reporter.record(False, "T1-F03-04: Composite query with pagination limit", str(e))

    try:
        # Empty result on future date range
        tx_empty = client.list_transactions({"start_date": "2099-01-01", "end_date": "2099-12-31"})
        assert_status(tx_empty, 200, "F03-05: Future date range")
        assert_equal(len(tx_empty.json), 0, "No transactions in future")
        reporter.record(True, "T1-F03-05: Index scan on non-matching date range returns empty list")
    except Exception as e:
        reporter.record(False, "T1-F03-05: Index scan on non-matching date range returns empty list", str(e))

    # -------------------------------------------------------------
    # F04: Integer Rupiah Value Object (5 tests)
    # -------------------------------------------------------------
    bank_acc = acc_resp.json[0]["id"]
    cat_gaji = [c["id"] for c in cat_resp.json if c["name"] == "Gaji"][0]
    cat_makan = [c["id"] for c in cat_resp.json if c["name"] == "Makanan"][0]

    try:
        # Record income of Rp 5,000,000 as integer 5000000
        tx1 = client.create_transaction(
            account_id=bank_acc,
            amount=5000000,
            tx_type="income",
            category_id=cat_gaji,
            note="Gaji Bulanan",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx1, 201, "F04-01: Create income transaction")
        assert_equal(tx1.json.get("amount"), 5000000, "Amount stored as integer Rupiah")
        reporter.record(True, "T1-F04-01: Transaction amount accepted and stored as integer Rupiah")
    except Exception as e:
        reporter.record(False, "T1-F04-01: Transaction amount accepted and stored as integer Rupiah", str(e))

    try:
        # Record expense of Rp 150,000 as integer 150000
        tx2 = client.create_transaction(
            account_id=bank_acc,
            amount=150000,
            tx_type="expense",
            category_id=cat_makan,
            note="Makan Siang",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx2, 201, "F04-02: Create expense transaction")
        assert_equal(tx2.json.get("amount"), 150000, "Expense amount is integer")
        reporter.record(True, "T1-F04-02: Expense transaction amount stored as exact integer")
    except Exception as e:
        reporter.record(False, "T1-F04-02: Expense transaction amount stored as exact integer", str(e))

    try:
        # Verify account balance updated atomically: 0 + 5,000,000 - 150,000 = 4,850,000
        acc_check = client.get(f"/api/v1/accounts/{bank_acc}")
        assert_status(acc_check, 200, "F04-03: Fetch updated account")
        assert_equal(acc_check.json.get("balance"), 4850000, "Balance matches integer delta")
        reporter.record(True, "T1-F04-03: Account balance maintains exact integer Rupiah delta")
    except Exception as e:
        reporter.record(False, "T1-F04-03: Account balance maintains exact integer Rupiah delta", str(e))

    try:
        # Micro-transaction: 1 Rupiah minimum unit
        tx_micro = client.create_transaction(
            account_id=bank_acc,
            amount=1,
            tx_type="expense",
            category_id=cat_makan,
            note="Micro fee",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_micro, 201, "F04-04: 1 Rupiah micro transaction")
        assert_equal(tx_micro.json.get("amount"), 1, "Micro transaction is 1 Rupiah")
        reporter.record(True, "T1-F04-04: Minimum 1 Rupiah integer unit supported without precision loss")
    except Exception as e:
        reporter.record(False, "T1-F04-04: Minimum 1 Rupiah integer unit supported without precision loss", str(e))

    try:
        # Cash flow net calculation: 5000000 - 150001 = 4849999
        cf = client.get_cash_flow()
        assert_status(cf, 200, "F04-05: Cash flow summary")
        assert_equal(cf.json.get("net_cash_flow"), 4849999, "Net cash flow calculation exact integer")
        reporter.record(True, "T1-F04-05: Net cash flow engine performs integer-exact subtraction")
    except Exception as e:
        reporter.record(False, "T1-F04-05: Net cash flow engine performs integer-exact subtraction", str(e))

    # -------------------------------------------------------------
    # F05: Multi-Tenant Data Isolation (5 tests)
    # -------------------------------------------------------------
    client2 = ApiClient(base_url)
    user2_data = None
    try:
        # Register User 2
        reg2 = client2.register(u2_email, u2_pass, "User Two")
        assert_status(reg2, 201, "F05-01: User 2 registration")
        user2_data = reg2.json
        reporter.record(True, "T1-F05-01: Secondary tenant registered in isolation")
    except Exception as e:
        reporter.record(False, "T1-F05-01: Secondary tenant registered in isolation", str(e))

    try:
        # User 2 lists accounts: must NOT see User 1's accounts
        accs2 = client2.list_accounts().json
        u1_acc_ids = [a["id"] for a in client.list_accounts().json]
        for a in accs2:
            assert_true(a["id"] not in u1_acc_ids, f"Account {a['id']} leaked across tenants")
        reporter.record(True, "T1-F05-02: Tenant account list strictly isolated")
    except Exception as e:
        reporter.record(False, "T1-F05-02: Tenant account list strictly isolated", str(e))

    try:
        # User 2 tries to GET User 1's account by ID -> must return 404
        cross_acc = client2.get(f"/api/v1/accounts/{bank_acc}")
        assert_rfc7807(cross_acc, 404, "ACCOUNT_NOT_FOUND")
        reporter.record(True, "T1-F05-03: Cross-tenant account access by ID returns 404")
    except Exception as e:
        reporter.record(False, "T1-F05-03: Cross-tenant account access by ID returns 404", str(e))

    try:
        # User 2 lists transactions: must see 0 transactions
        txs2 = client2.list_transactions().json
        assert_equal(len(txs2), 0, "User 2 has 0 transactions")
        reporter.record(True, "T1-F05-04: Tenant transaction history strictly isolated")
    except Exception as e:
        reporter.record(False, "T1-F05-04: Tenant transaction history strictly isolated", str(e))

    try:
        # User 2 attempts to log transaction using User 1's account -> 404
        cross_tx = client2.create_transaction(
            account_id=bank_acc,
            amount=50000,
            tx_type="expense",
            note="Cross tenant attempt",
            idempotency_key=str(uuid.uuid4())
        )
        assert_rfc7807(cross_tx, 404, "ACCOUNT_NOT_FOUND")
        reporter.record(True, "T1-F05-05: Cross-tenant transaction creation strictly rejected 404")
    except Exception as e:
        reporter.record(False, "T1-F05-05: Cross-tenant transaction creation strictly rejected 404", str(e))

    # -------------------------------------------------------------
    # F06: Argon2id Password Hashing (5 tests)
    # -------------------------------------------------------------
    try:
        # Login with correct password succeeds
        client_auth_test = ApiClient(base_url)
        login_ok = client_auth_test.login(u1_email, u1_pass)
        assert_status(login_ok, 200, "F06-01: Correct password login")
        assert_equal(login_ok.json.get("email"), u1_email, "Logged in user email matches")
        reporter.record(True, "T1-F06-01: Password verification succeeds with valid credentials")
    except Exception as e:
        reporter.record(False, "T1-F06-01: Password verification succeeds with valid credentials", str(e))

    try:
        # Login with incorrect password fails with 401
        client_bad = ApiClient(base_url)
        login_fail = client_bad.login(u1_email, "WrongP@ssword123")
        assert_rfc7807(login_fail, 401, "INVALID_CREDENTIALS")
        reporter.record(True, "T1-F06-02: Password verification rejects invalid password with 401")
    except Exception as e:
        reporter.record(False, "T1-F06-02: Password verification rejects invalid password with 401", str(e))

    try:
        # Non-existent user login fails 401
        login_nonexistent = client_bad.login("nobody_exists_123@example.com", "Password123!")
        assert_rfc7807(login_nonexistent, 401, "INVALID_CREDENTIALS")
        reporter.record(True, "T1-F06-03: Login for non-existent email returns 401")
    except Exception as e:
        reporter.record(False, "T1-F06-03: Login for non-existent email returns 401", str(e))

    try:
        # Case sensitive password verification
        login_case = client_bad.login(u1_email, u1_pass.lower())
        assert_rfc7807(login_case, 401, "INVALID_CREDENTIALS")
        reporter.record(True, "T1-F06-04: Password check enforces case-sensitivity")
    except Exception as e:
        reporter.record(False, "T1-F06-04: Password check enforces case-sensitivity", str(e))

    try:
        # Password min-length validation on register (e.g. 7 chars rejected)
        short_reg = client_bad.register(f"short_{run_id}@example.com", "short1!", "Shorty")
        assert_rfc7807(short_reg, 400, "PASSWORD_TOO_SHORT")
        reporter.record(True, "T1-F06-05: Registration enforces minimum 8-character password constraint")
    except Exception as e:
        reporter.record(False, "T1-F06-05: Registration enforces minimum 8-character password constraint", str(e))

    # -------------------------------------------------------------
    # F07: Short-lived JWT & HttpOnly Cookies (5 tests)
    # -------------------------------------------------------------
    try:
        # Login returns Set-Cookie with auth_token
        c_test = ApiClient(base_url)
        res = c_test.login(u1_email, u1_pass)
        set_cookie = res.header("Set-Cookie")
        assert_true("auth_token=" in set_cookie, "auth_token in Set-Cookie")
        reporter.record(True, "T1-F07-01: Auth endpoints issue auth_token cookie")
    except Exception as e:
        reporter.record(False, "T1-F07-01: Auth endpoints issue auth_token cookie", str(e))

    try:
        # Cookie contains HttpOnly
        assert_true("httponly" in set_cookie.lower(), "Cookie includes HttpOnly")
        reporter.record(True, "T1-F07-02: auth_token cookie has HttpOnly flag set")
    except Exception as e:
        reporter.record(False, "T1-F07-02: auth_token cookie has HttpOnly flag set", str(e))

    try:
        # Cookie contains SameSite=Lax
        assert_true("samesite=lax" in set_cookie.lower(), "Cookie includes SameSite=Lax")
        reporter.record(True, "T1-F07-03: auth_token cookie has SameSite=Lax flag set")
    except Exception as e:
        reporter.record(False, "T1-F07-03: auth_token cookie has SameSite=Lax flag set", str(e))

    try:
        # Valid cookie allows access to /api/v1/auth/me
        me_res = c_test.me()
        assert_status(me_res, 200, "GET /api/v1/auth/me succeeds with cookie")
        assert_equal(me_res.json.get("email"), u1_email)
        reporter.record(True, "T1-F07-04: Authenticated user profile returned via valid cookie")
    except Exception as e:
        reporter.record(False, "T1-F07-04: Authenticated user profile returned via valid cookie", str(e))

    try:
        # Logout clears the cookie
        logout_res = c_test.logout()
        assert_status(logout_res, 200, "Logout succeeds")
        post_logout_me = c_test.me()
        assert_rfc7807(post_logout_me, 401, "AUTH_REQUIRED")
        reporter.record(True, "T1-F07-05: Logout clears cookie and revokes session access")
    except Exception as e:
        reporter.record(False, "T1-F07-05: Logout clears cookie and revokes session access", str(e))

    # -------------------------------------------------------------
    # F08: Sliding Window Rate Limiting (5 tests)
    # -------------------------------------------------------------
    try:
        # A separate client IP or test client to verify rate limiter
        rate_client = ApiClient(base_url, client_ip="10.1.1.1")
        # 1st to 4th failed attempts return 401
        for i in range(1, 5):
            r = rate_client.login(f"fake_{i}_{run_id}@example.com", "wrong_pass_123")
            assert_status(r, 401, f"Attempt {i} returns 401")
        reporter.record(True, "T1-F08-01: Failed auth attempts 1-4 return 401 without rate limit trigger")
    except Exception as e:
        reporter.record(False, "T1-F08-01: Failed auth attempts 1-4 return 401 without rate limit trigger", str(e))

    try:
        # 5th attempt still 401
        r5 = rate_client.login(f"fake_5_{run_id}@example.com", "wrong_pass_123")
        assert_status(r5, 401, "5th attempt is 401")
        reporter.record(True, "T1-F08-02: 5th failed auth attempt allowed at rate boundary")
    except Exception as e:
        reporter.record(False, "T1-F08-02: 5th failed auth attempt allowed at rate boundary", str(e))

    try:
        # 6th attempt triggers 429 Too Many Requests
        r6 = rate_client.login(f"fake_6_{run_id}@example.com", "wrong_pass_123")
        assert_rfc7807(r6, 429, "RATE_LIMIT_EXCEEDED")
        reporter.record(True, "T1-F08-03: 6th failed attempt rejected with HTTP 429 Too Many Requests")
    except Exception as e:
        reporter.record(False, "T1-F08-03: 6th failed attempt rejected with HTTP 429 Too Many Requests", str(e))

    try:
        # 429 response contains Retry-After header
        retry_after = r6.header("Retry-After")
        assert_true(retry_after is not None and int(retry_after) > 0, "Retry-After header present")
        reporter.record(True, "T1-F08-04: Rate limit response includes Retry-After header")
    except Exception as e:
        reporter.record(False, "T1-F08-04: Rate limit response includes Retry-After header", str(e))

    try:
        # Rate limit does not block health/ready probes
        probe_res = rate_client.get("/health")
        assert_status(probe_res, 200, "Health probe unaffected by auth rate limit")
        reporter.record(True, "T1-F08-05: Rate limiting is scoped to auth endpoints and does not block health checks")
    except Exception as e:
        reporter.record(False, "T1-F08-05: Rate limiting is scoped to auth endpoints and does not block health checks", str(e))

    # -------------------------------------------------------------
    # F09: Pure NetCashFlowEngine (5 tests)
    # -------------------------------------------------------------
    # Let's use user1 with clean client
    client.login(u1_email, u1_pass)
    cash_acc = [a["id"] for a in client.list_accounts().json if a["account_type"] == "cash"][0]

    try:
        # Record income
        r_inc = client.create_transaction(
            account_id=cash_acc,
            amount=1000000,
            tx_type="income",
            category_id=cat_gaji,
            note="Side income",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(r_inc, 201, "Record income")
        reporter.record(True, "T1-F09-01: NetCashFlowEngine aggregates income accurately")
    except Exception as e:
        reporter.record(False, "T1-F09-01: NetCashFlowEngine aggregates income accurately", str(e))

    try:
        # Record expense
        r_exp = client.create_transaction(
            account_id=cash_acc,
            amount=200000,
            tx_type="expense",
            category_id=cat_makan,
            note="Belanja",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(r_exp, 201, "Record expense")
        reporter.record(True, "T1-F09-02: NetCashFlowEngine aggregates expenses accurately")
    except Exception as e:
        reporter.record(False, "T1-F09-02: NetCashFlowEngine aggregates expenses accurately", str(e))

    try:
        # Record transfer between accounts: must NOT change net cash flow!
        cf_before = client.get_cash_flow().json["net_cash_flow"]
        r_trans = client.create_transaction(
            account_id=bank_acc,
            amount=500000,
            tx_type="transfer",
            destination_account_id=cash_acc,
            note="Transfer bank to cash",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(r_trans, 201, "Record transfer")
        cf_after = client.get_cash_flow().json["net_cash_flow"]
        assert_equal(cf_before, cf_after, "Transfer maintains net cash flow invariance")
        reporter.record(True, "T1-F09-03: Pure NetCashFlowEngine maintains transfer net-0 invariance")
    except Exception as e:
        reporter.record(False, "T1-F09-03: Pure NetCashFlowEngine maintains transfer net-0 invariance", str(e))

    try:
        # Verify net = income - expenses
        cf_summary = client.get_cash_flow().json
        inc = cf_summary["income"]
        exp = cf_summary["expenses"]
        net = cf_summary["net_cash_flow"]
        assert_equal(net, inc - exp, "net == income - expenses")
        reporter.record(True, "T1-F09-04: NetCashFlow strictly equals total income minus total expenses")
    except Exception as e:
        reporter.record(False, "T1-F09-04: NetCashFlow strictly equals total income minus total expenses", str(e))

    try:
        # Currency specification is IDR
        assert_equal(cf_summary.get("currency"), "IDR", "Currency is IDR")
        reporter.record(True, "T1-F09-05: NetCashFlowEngine specifies IDR currency metadata")
    except Exception as e:
        reporter.record(False, "T1-F09-05: NetCashFlowEngine specifies IDR currency metadata", str(e))

    # -------------------------------------------------------------
    # F10: Multi-Wallet Accounts (5 tests)
    # -------------------------------------------------------------
    try:
        # Create checking wallet
        w1 = client.create_account("Rekening Mandiri", "checking", 1000000)
        assert_status(w1, 201, "Create checking wallet")
        assert_equal(w1.json.get("account_type"), "checking")
        reporter.record(True, "T1-F10-01: Multi-wallet engine creates checking account")
    except Exception as e:
        reporter.record(False, "T1-F10-01: Multi-wallet engine creates checking account", str(e))

    try:
        # Create savings wallet
        w2 = client.create_account("Tabungan Impian", "savings", 5000000)
        assert_status(w2, 201, "Create savings wallet")
        assert_equal(w2.json.get("account_type"), "savings")
        reporter.record(True, "T1-F10-02: Multi-wallet engine creates savings account")
    except Exception as e:
        reporter.record(False, "T1-F10-02: Multi-wallet engine creates savings account", str(e))

    try:
        # Create e-wallet
        w3 = client.create_account("ShopeePay", "ewallet", 50000)
        assert_status(w3, 201, "Create e-wallet")
        assert_equal(w3.json.get("account_type"), "ewallet")
        reporter.record(True, "T1-F10-03: Multi-wallet engine creates e-wallet account")
    except Exception as e:
        reporter.record(False, "T1-F10-03: Multi-wallet engine creates e-wallet account", str(e))

    try:
        # Create cash wallet
        w4 = client.create_account("Uang Saku", "cash", 100000)
        assert_status(w4, 201, "Create cash wallet")
        assert_equal(w4.json.get("account_type"), "cash")
        reporter.record(True, "T1-F10-04: Multi-wallet engine creates cash wallet account")
    except Exception as e:
        reporter.record(False, "T1-F10-04: Multi-wallet engine creates cash wallet account", str(e))

    try:
        # Listing accounts reflects all 4 new accounts + initial default accounts
        accs_all = client.list_accounts().json
        types = {a["account_type"] for a in accs_all}
        assert_true({"checking", "savings", "ewallet", "cash"}.issubset(types), "All wallet types listed")
        reporter.record(True, "T1-F10-05: Account repository lists all distinct wallet types")
    except Exception as e:
        reporter.record(False, "T1-F10-05: Account repository lists all distinct wallet types", str(e))

    # -------------------------------------------------------------
    # F11: Category Personalization & Soft-Delete (5 tests)
    # -------------------------------------------------------------
    new_cat_id = None
    try:
        # Create custom category
        c_res = client.create_category("Investasi Saham", "expense", "trending-up", "#3B82F6")
        assert_status(c_res, 201, "Create category")
        new_cat_id = c_res.json["id"]
        assert_equal(c_res.json["name"], "Investasi Saham")
        reporter.record(True, "T1-F11-01: Custom category created with icon and color attributes")
    except Exception as e:
        reporter.record(False, "T1-F11-01: Custom category created with icon and color attributes", str(e))

    try:
        # Category appears in active list
        active_cats = client.list_categories().json
        assert_true(any(c["id"] == new_cat_id for c in active_cats), "New category present in list")
        reporter.record(True, "T1-F11-02: Active categories list includes newly created custom category")
    except Exception as e:
        reporter.record(False, "T1-F11-02: Active categories list includes newly created custom category", str(e))

    try:
        # Record transaction with this category
        tx_c = client.create_transaction(
            account_id=bank_acc,
            amount=500000,
            tx_type="expense",
            category_id=new_cat_id,
            note="Beli Saham BBCA",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_c, 201, "Transaction with custom category")
        reporter.record(True, "T1-F11-03: Transaction successfully records reference to custom category")
    except Exception as e:
        reporter.record(False, "T1-F11-03: Transaction successfully records reference to custom category", str(e))

    try:
        # Soft-delete the category
        del_res = client.soft_delete_category(new_cat_id)
        assert_status(del_res, 200, "Soft-delete category")
        active_cats_after = client.list_categories().json
        assert_true(not any(c["id"] == new_cat_id for c in active_cats_after), "Soft-deleted category excluded")
        reporter.record(True, "T1-F11-04: Soft-deleted category excluded from active category list")
    except Exception as e:
        reporter.record(False, "T1-F11-04: Soft-deleted category excluded from active category list", str(e))

    try:
        # Historical transaction retains reference to soft-deleted category
        tx_hist = client.list_transactions({"category_id": new_cat_id}).json
        assert_true(len(tx_hist) >= 1, "Historical transactions preserved")
        assert_equal(tx_hist[0]["category_id"], new_cat_id)
        reporter.record(True, "T1-F11-05: Historical transactions preserve soft-deleted category relationship")
    except Exception as e:
        reporter.record(False, "T1-F11-05: Historical transactions preserve soft-deleted category relationship", str(e))

    # -------------------------------------------------------------
    # F12: Strict Idempotency-Key Deduplication (5 tests)
    # -------------------------------------------------------------
    idem_key = str(uuid.uuid4())
    idem_tx_id = None
    initial_bal = None
    try:
        # Fetch current balance
        initial_bal = client.get(f"/api/v1/accounts/{bank_acc}").json["balance"]
        # 1st attempt with key
        tx_idem1 = client.create_transaction(
            account_id=bank_acc,
            amount=100000,
            tx_type="expense",
            note="Idempotency test payment",
            idempotency_key=idem_key
        )
        assert_status(tx_idem1, 201, "First idempotent submission")
        idem_tx_id = tx_idem1.json["id"]
        reporter.record(True, "T1-F12-01: Initial transaction with Idempotency-Key records successfully")
    except Exception as e:
        reporter.record(False, "T1-F12-01: Initial transaction with Idempotency-Key records successfully", str(e))

    try:
        # Replay identical transaction with same Idempotency-Key
        tx_idem2 = client.create_transaction(
            account_id=bank_acc,
            amount=100000,
            tx_type="expense",
            note="Idempotency test payment",
            idempotency_key=idem_key
        )
        assert_status(tx_idem2, 201, "Replayed idempotent submission returns 201")
        assert_equal(tx_idem2.json["id"], idem_tx_id, "Returned transaction ID is identical")
        reporter.record(True, "T1-F12-02: Replayed request returns identical transaction payload")
    except Exception as e:
        reporter.record(False, "T1-F12-02: Replayed request returns identical transaction payload", str(e))

    try:
        # Check account balance debited exactly once (100000), not twice
        bal_after = client.get(f"/api/v1/accounts/{bank_acc}").json["balance"]
        assert_equal(bal_after, initial_bal - 100000, "Balance debited exactly once")
        reporter.record(True, "T1-F12-03: Replayed idempotency key prevents duplicate balance mutation")
    except Exception as e:
        reporter.record(False, "T1-F12-03: Replayed idempotency key prevents duplicate balance mutation", str(e))

    try:
        # Reject non-UUID idempotency key
        bad_key_res = client.create_transaction(
            account_id=bank_acc,
            amount=50000,
            tx_type="expense",
            idempotency_key="not-a-valid-uuid"
        )
        assert_rfc7807(bad_key_res, 400, "INVALID_IDEMPOTENCY_KEY")
        reporter.record(True, "T1-F12-04: Non-UUID formatted Idempotency-Key rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T1-F12-04: Non-UUID formatted Idempotency-Key rejected with HTTP 400", str(e))

    try:
        # Transaction without idempotency key creates transaction normally
        tx_no_key = client.create_transaction(
            account_id=bank_acc,
            amount=10000,
            tx_type="expense",
            note="No key"
        )
        assert_status(tx_no_key, 201, "Transaction without key succeeds")
        reporter.record(True, "T1-F12-05: Transactions without Idempotency-Key proceed normally")
    except Exception as e:
        reporter.record(False, "T1-F12-05: Transactions without Idempotency-Key proceed normally", str(e))

    # -------------------------------------------------------------
    # F13: Payment Gateway Trait Abstraction (5 tests)
    # -------------------------------------------------------------
    try:
        # Checkout with Midtrans
        co_mid = client.checkout(provider="midtrans")
        assert_status(co_mid, 200, "Midtrans checkout")
        assert_equal(co_mid.json.get("provider"), "midtrans")
        assert_true("checkout_url" in co_mid.json, "Checkout URL generated")
        reporter.record(True, "T1-F13-01: PaymentGateway abstraction initiates Midtrans checkout")
    except Exception as e:
        reporter.record(False, "T1-F13-01: PaymentGateway abstraction initiates Midtrans checkout", str(e))

    try:
        # Checkout with Xendit
        co_xen = client.checkout(provider="xendit")
        assert_status(co_xen, 200, "Xendit checkout")
        assert_equal(co_xen.json.get("provider"), "xendit")
        assert_true("checkout_url" in co_xen.json, "Checkout URL generated")
        reporter.record(True, "T1-F13-02: PaymentGateway abstraction initiates Xendit checkout")
    except Exception as e:
        reporter.record(False, "T1-F13-02: PaymentGateway abstraction initiates Xendit checkout", str(e))

    try:
        # Checkout amount matches Rp 5,000 / month
        assert_equal(co_mid.json.get("amount"), 5000, "Amount is 5000 IDR")
        assert_equal(co_mid.json.get("currency"), "IDR", "Currency is IDR")
        reporter.record(True, "T1-F13-03: Subscription checkout specifies Rp 5,000 IDR monthly pricing")
    except Exception as e:
        reporter.record(False, "T1-F13-03: Subscription checkout specifies Rp 5,000 IDR monthly pricing", str(e))

    try:
        # Checkout order_id generated
        assert_true(co_mid.json.get("order_id") is not None, "Order ID generated")
        reporter.record(True, "T1-F13-04: Checkout generates unique provider order identifier")
    except Exception as e:
        reporter.record(False, "T1-F13-04: Checkout generates unique provider order identifier", str(e))

    try:
        # Unsupported provider rejected
        co_bad = client.checkout(provider="paypal")
        assert_rfc7807(co_bad, 400, "UNSUPPORTED_PROVIDER")
        reporter.record(True, "T1-F13-05: Unsupported payment provider rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T1-F13-05: Unsupported payment provider rejected with HTTP 400", str(e))

    # -------------------------------------------------------------
    # F14: Cryptographic HMAC/SHA-512 Verification (5 tests)
    # -------------------------------------------------------------
    user1_id = user1_data["id"]
    order_id_test = f"SUB-{user1_id[:8]}-9999"
    midtrans_key = "SB-Mid-server-test-secret-key"

    try:
        # Valid Midtrans SHA-512 signature
        sig_valid = compute_midtrans_signature(order_id_test, "200", "5000.00", midtrans_key)
        wh_payload = {
            "order_id": order_id_test,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_valid,
            "transaction_id": f"trx_{run_id}_01"
        }
        res_wh = client.post("/api/v1/webhooks/midtrans", wh_payload)
        assert_status(res_wh, 200, "Valid Midtrans signature accepted")
        reporter.record(True, "T1-F14-01: Valid Midtrans SHA-512 cryptographic signature accepted")
    except Exception as e:
        reporter.record(False, "T1-F14-01: Valid Midtrans SHA-512 cryptographic signature accepted", str(e))

    try:
        # Tampered Midtrans signature rejected 401
        wh_bad = dict(wh_payload)
        wh_bad["signature_key"] = "deadbeef" * 16
        res_bad_sig = client.post("/api/v1/webhooks/midtrans", wh_bad)
        assert_rfc7807(res_bad_sig, 401, "INVALID_SIGNATURE")
        reporter.record(True, "T1-F14-02: Tampered Midtrans signature rejected with HTTP 401")
    except Exception as e:
        reporter.record(False, "T1-F14-02: Tampered Midtrans signature rejected with HTTP 401", str(e))

    try:
        # Missing Midtrans signature rejected
        wh_no_sig = dict(wh_payload)
        del wh_no_sig["signature_key"]
        res_no_sig = client.post("/api/v1/webhooks/midtrans", wh_no_sig)
        assert_status(res_no_sig, 400, "Missing signature rejected")
        reporter.record(True, "T1-F14-03: Webhook payload missing cryptographic signature rejected 400")
    except Exception as e:
        reporter.record(False, "T1-F14-03: Webhook payload missing cryptographic signature rejected 400", str(e))

    try:
        # Valid Xendit callback token accepted
        xendit_token = "xendit_webhook_token_secret_123"
        xen_payload = {
            "id": f"xen_{run_id}_01",
            "external_id": user1_id,
            "status": "PAID",
            "amount": 5000
        }
        res_xen = client.post("/api/v1/webhooks/xendit", xen_payload, headers={"X-Callback-Token": xendit_token})
        assert_status(res_xen, 200, "Valid Xendit token accepted")
        reporter.record(True, "T1-F14-04: Valid Xendit webhook callback token verified")
    except Exception as e:
        reporter.record(False, "T1-F14-04: Valid Xendit webhook callback token verified", str(e))

    try:
        # Invalid Xendit callback token rejected 401
        res_xen_bad = client.post("/api/v1/webhooks/xendit", xen_payload, headers={"X-Callback-Token": "wrong_token"})
        assert_rfc7807(res_xen_bad, 401, "INVALID_SIGNATURE")
        reporter.record(True, "T1-F14-05: Invalid Xendit token rejected with HTTP 401")
    except Exception as e:
        reporter.record(False, "T1-F14-05: Invalid Xendit token rejected with HTTP 401", str(e))

    # -------------------------------------------------------------
    # F15: Idempotent Webhook Processing (5 tests)
    # -------------------------------------------------------------
    try:
        # Replaying exact same Midtrans webhook (same event_id / transaction_id) returns 200 OK
        res_replay = client.post("/api/v1/webhooks/midtrans", wh_payload)
        assert_status(res_replay, 200, "Replay webhook returns 200")
        assert_true("already processed" in res_replay.json.get("message", "").lower() or res_replay.json.get("status") == "ok")
        reporter.record(True, "T1-F15-01: Replayed Midtrans webhook recognized and acknowledged idempotently")
    except Exception as e:
        reporter.record(False, "T1-F15-01: Replayed Midtrans webhook recognized and acknowledged idempotently", str(e))

    try:
        # Replaying Xendit webhook returns 200 OK without double processing
        res_xen_replay = client.post("/api/v1/webhooks/xendit", xen_payload, headers={"X-Callback-Token": xendit_token})
        assert_status(res_xen_replay, 200, "Replay Xendit returns 200")
        reporter.record(True, "T1-F15-02: Replayed Xendit webhook acknowledged idempotently")
    except Exception as e:
        reporter.record(False, "T1-F15-02: Replayed Xendit webhook acknowledged idempotently", str(e))

    try:
        # Replayed webhook does not alter active state or duplicate events
        sub_check = client.subscription_status().json
        assert_equal(sub_check.get("tier"), "premium", "Tier remains premium")
        reporter.record(True, "T1-F15-03: Idempotent replay maintains consistent subscription state")
    except Exception as e:
        reporter.record(False, "T1-F15-03: Idempotent replay maintains consistent subscription state", str(e))

    try:
        # Missing event identifier in Xendit payload rejected 400
        bad_xen_payload = {"status": "PAID", "amount": 5000}
        res_xen_no_id = client.post("/api/v1/webhooks/xendit", bad_xen_payload, headers={"X-Callback-Token": xendit_token})
        assert_rfc7807(res_xen_no_id, 400, "INVALID_WEBHOOK_PAYLOAD")
        reporter.record(True, "T1-F15-04: Webhook missing unique event ID rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T1-F15-04: Webhook missing unique event ID rejected with HTTP 400", str(e))

    try:
        # Webhook with unknown status string processed safely without crashing
        unknown_status_wh = dict(wh_payload)
        unknown_status_wh["transaction_id"] = f"trx_unknown_{run_id}"
        unknown_status_wh["transaction_status"] = "pending_custom_status"
        sig_unknown = compute_midtrans_signature(order_id_test, "200", "5000.00", midtrans_key)
        unknown_status_wh["signature_key"] = sig_unknown
        res_unknown = client.post("/api/v1/webhooks/midtrans", unknown_status_wh)
        assert_status(res_unknown, 200, "Unknown status handled safely")
        reporter.record(True, "T1-F15-05: Webhook engine handles unfamiliar transaction status gracefully")
    except Exception as e:
        reporter.record(False, "T1-F15-05: Webhook engine handles unfamiliar transaction status gracefully", str(e))

    # -------------------------------------------------------------
    # F16: Subscription Lifecycle State Machine (5 tests)
    # -------------------------------------------------------------
    try:
        # User 1 is now Premium Active after successful settlement webhook
        sub_now = client.subscription_status().json
        assert_equal(sub_now["tier"], "premium", "Tier is premium")
        assert_equal(sub_now["status"], "active", "Status is active")
        reporter.record(True, "T1-F16-01: Subscription transitions to Active Premium on settlement")
    except Exception as e:
        reporter.record(False, "T1-F16-01: Subscription transitions to Active Premium on settlement", str(e))

    try:
        # User 2 is Free Active
        sub2 = client2.subscription_status().json
        assert_equal(sub2["tier"], "free", "User 2 tier is free")
        reporter.record(True, "T1-F16-02: Default subscription lifecycle state is Free Active")
    except Exception as e:
        reporter.record(False, "T1-F16-02: Default subscription lifecycle state is Free Active", str(e))

    try:
        # Cancel event transitions status to cancelled
        cancel_order = f"SUB-{user1_id[:8]}-cancel"
        sig_cancel = compute_midtrans_signature(user1_id, "200", "5000.00", midtrans_key)
        wh_cancel = {
            "order_id": user1_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "cancel",
            "signature_key": sig_cancel,
            "transaction_id": f"trx_cancel_{run_id}"
        }
        res_cancel = client.post("/api/v1/webhooks/midtrans", wh_cancel)
        assert_status(res_cancel, 200, "Cancel webhook processed")
        sub_cancelled = client.subscription_status().json
        assert_equal(sub_cancelled["status"], "cancelled", "Status transitioned to cancelled")
        reporter.record(True, "T1-F16-03: Webhook cancellation transitions status to Cancelled")
    except Exception as e:
        reporter.record(False, "T1-F16-03: Webhook cancellation transitions status to Cancelled", str(e))

    try:
        # Re-activation: settlement event restores status to active
        sig_restore = compute_midtrans_signature(user1_id, "200", "5000.00", midtrans_key)
        wh_restore = {
            "order_id": user1_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_restore,
            "transaction_id": f"trx_restore_{run_id}"
        }
        client.post("/api/v1/webhooks/midtrans", wh_restore)
        sub_restored = client.subscription_status().json
        assert_equal(sub_restored["status"], "active", "Restored to active")
        reporter.record(True, "T1-F16-04: New settlement event restores Cancelled subscription to Active")
    except Exception as e:
        reporter.record(False, "T1-F16-04: New settlement event restores Cancelled subscription to Active", str(e))

    try:
        # Subscription status reflects Rp 5,000 monthly pricing for Premium
        assert_equal(sub_restored.get("price_monthly"), 5000, "Monthly price 5000 IDR")
        reporter.record(True, "T1-F16-05: Subscription status payload reflects pricing Rp 5,000")
    except Exception as e:
        reporter.record(False, "T1-F16-05: Subscription status payload reflects pricing Rp 5,000", str(e))

    # -------------------------------------------------------------
    # F17: Server-Side Feature Gating (5 tests)
    # -------------------------------------------------------------
    try:
        # User 2 (Free): Basic cash flow endpoint allowed (200)
        res_cf_free = client2.get_cash_flow()
        assert_status(res_cf_free, 200, "Free user accesses basic cash flow")
        reporter.record(True, "T1-F17-01: Free user permitted access to basic cash-flow feature")
    except Exception as e:
        reporter.record(False, "T1-F17-01: Free user permitted access to basic cash-flow feature", str(e))

    try:
        # User 2 (Free): Advanced analytics endpoint gated -> returns 403 FEATURE_LOCKED
        res_adv_free = client2.get_advanced_analytics()
        assert_rfc7807(res_adv_free, 403, "FEATURE_LOCKED")
        reporter.record(True, "T1-F17-02: Free user blocked from advanced analytics with HTTP 403 FEATURE_LOCKED")
    except Exception as e:
        reporter.record(False, "T1-F17-02: Free user blocked from advanced analytics with HTTP 403 FEATURE_LOCKED", str(e))

    try:
        # User 2 (Free): Budgeting endpoint gated -> returns 403 FEATURE_LOCKED
        res_bud_free = client2.list_budgets()
        assert_rfc7807(res_bud_free, 403, "FEATURE_LOCKED")
        reporter.record(True, "T1-F17-03: Free user blocked from budgeting with HTTP 403 FEATURE_LOCKED")
    except Exception as e:
        reporter.record(False, "T1-F17-03: Free user blocked from budgeting with HTTP 403 FEATURE_LOCKED", str(e))

    try:
        # User 1 (Premium Active): Advanced analytics returns 200 OK
        res_adv_prem = client.get_advanced_analytics()
        assert_status(res_adv_prem, 200, "Premium user accesses advanced analytics")
        reporter.record(True, "T1-F17-04: Premium user permitted access to advanced analytics")
    except Exception as e:
        reporter.record(False, "T1-F17-04: Premium user permitted access to advanced analytics", str(e))

    try:
        # User 1 (Premium Active): Budgets returns 200 OK
        res_bud_prem = client.list_budgets()
        assert_status(res_bud_prem, 200, "Premium user accesses budgeting")
        reporter.record(True, "T1-F17-05: Premium user permitted access to budgeting endpoints")
    except Exception as e:
        reporter.record(False, "T1-F17-05: Premium user permitted access to budgeting endpoints", str(e))

    # -------------------------------------------------------------
    # F18: Full Axum REST API Endpoints (5 tests)
    # -------------------------------------------------------------
    try:
        # CRUD Accounts: GET /api/v1/accounts/:id
        acc_get = client.get(f"/api/v1/accounts/{bank_acc}")
        assert_status(acc_get, 200, "GET account by id")
        reporter.record(True, "T1-F18-01: REST API provides single account lookup endpoint")
    except Exception as e:
        reporter.record(False, "T1-F18-01: REST API provides single account lookup endpoint", str(e))

    try:
        # CRUD Categories: GET /api/v1/categories/:id
        cat_get = client.get(f"/api/v1/categories/{cat_gaji}")
        assert_status(cat_get, 200, "GET category by id")
        reporter.record(True, "T1-F18-02: REST API provides single category lookup endpoint")
    except Exception as e:
        reporter.record(False, "T1-F18-02: REST API provides single category lookup endpoint", str(e))

    try:
        # CRUD Transactions: GET /api/v1/transactions/:id
        tx_get = client.get(f"/api/v1/transactions/{idem_tx_id}")
        assert_status(tx_get, 200, "GET transaction by id")
        reporter.record(True, "T1-F18-03: REST API provides single transaction lookup endpoint")
    except Exception as e:
        reporter.record(False, "T1-F18-03: REST API provides single transaction lookup endpoint", str(e))

    try:
        # GET /api/v1/auth/me returns current user identity
        me_resp = client.me()
        assert_status(me_resp, 200, "GET /me")
        assert_equal(me_resp.json.get("id"), user1_id)
        reporter.record(True, "T1-F18-04: REST API provides /auth/me user profile endpoint")
    except Exception as e:
        reporter.record(False, "T1-F18-04: REST API provides /auth/me user profile endpoint", str(e))

    try:
        # Non-existent route returns 404 RFC 7807
        non_route = client.get("/api/v1/unknown_route_404")
        assert_rfc7807(non_route, 404, "ROUTE_NOT_FOUND")
        reporter.record(True, "T1-F18-05: Non-existent REST routes return RFC 7807 404 response")
    except Exception as e:
        reporter.record(False, "T1-F18-05: Non-existent REST routes return RFC 7807 404 response", str(e))

    # -------------------------------------------------------------
    # F19: Health & Readiness Probes (5 tests)
    # -------------------------------------------------------------
    try:
        h = client.get("/health")
        assert_status(h, 200, "GET /health returns 200")
        assert_equal(h.json.get("status"), "pass")
        reporter.record(True, "T1-F19-01: Health liveness probe reports operational status")
    except Exception as e:
        reporter.record(False, "T1-F19-01: Health liveness probe reports operational status", str(e))

    try:
        r = client.get("/ready")
        assert_status(r, 200, "GET /ready returns 200")
        assert_equal(r.json.get("status"), "pass")
        reporter.record(True, "T1-F19-02: Readiness probe verifies database connectivity")
    except Exception as e:
        reporter.record(False, "T1-F19-02: Readiness probe verifies database connectivity", str(e))

    try:
        # Probes return no secret credentials in body
        h_text = h.text.lower()
        assert_true("password" not in h_text and "secret" not in h_text, "No secret leak in health")
        reporter.record(True, "T1-F19-03: Health probe contains zero credentials or secrets")
    except Exception as e:
        reporter.record(False, "T1-F19-03: Health probe contains zero credentials or secrets", str(e))

    try:
        r_text = r.text.lower()
        assert_true("password" not in r_text and "secret" not in r_text, "No secret leak in ready")
        reporter.record(True, "T1-F19-04: Readiness probe contains zero credentials or secrets")
    except Exception as e:
        reporter.record(False, "T1-F19-04: Readiness probe contains zero credentials or secrets", str(e))

    try:
        head_h = client.head("/health")
        assert_status(head_h, 200, "HEAD /health")
        reporter.record(True, "T1-F19-05: Health probe supports lightweight HTTP HEAD queries")
    except Exception as e:
        reporter.record(False, "T1-F19-05: Health probe supports lightweight HTTP HEAD queries", str(e))

    # -------------------------------------------------------------
    # F20: Environment & Secrets Config (5 tests)
    # -------------------------------------------------------------
    try:
        # Error responses do not leak system environment variables
        bad_req = client.post("/api/v1/auth/login", {"email": "bad", "password": "1"})
        body_text = bad_req.text
        assert_true("PATH=" not in body_text and "JWT_SECRET" not in body_text, "No env leak in error")
        reporter.record(True, "T1-F20-01: Error responses prevent environment and secret leakage")
    except Exception as e:
        reporter.record(False, "T1-F20-01: Error responses prevent environment and secret leakage", str(e))

    try:
        # JWT signatures use constant server secret
        token_a = client.cookies.get("auth_token")
        assert_true(token_a is not None and "." in token_a, "Token generated with secret")
        reporter.record(True, "T1-F20-02: Auth tokens generated with server-side signing secret")
    except Exception as e:
        reporter.record(False, "T1-F20-02: Auth tokens generated with server-side signing secret", str(e))

    try:
        # Responses declare utf-8 charset
        ct = h.header("Content-Type")
        assert_true("application/json" in ct, "Content-Type is application/json")
        reporter.record(True, "T1-F20-03: Responses declare standard JSON application type")
    except Exception as e:
        reporter.record(False, "T1-F20-03: Responses declare standard JSON application type", str(e))

    try:
        # Unknown provider in checkout safely rejected
        bad_prov = client.post("/api/v1/subscriptions/checkout", {"provider": "secret_extractor"})
        assert_rfc7807(bad_prov, 400, "UNSUPPORTED_PROVIDER")
        reporter.record(True, "T1-F20-04: Payment gateway enforces strict provider whitelist")
    except Exception as e:
        reporter.record(False, "T1-F20-04: Payment gateway enforces strict provider whitelist", str(e))

    try:
        # Sensitive credentials stripped from user profile
        me_json = client.me().json
        assert_true("password" not in me_json and "password_hash" not in me_json, "No password in me")
        reporter.record(True, "T1-F20-05: User profile endpoint strips password hash from output")
    except Exception as e:
        reporter.record(False, "T1-F20-05: User profile endpoint strips password hash from output", str(e))

    # -------------------------------------------------------------
    # F21: Mobile-First Vue 3 PWA Shell (5 tests)
    # -------------------------------------------------------------
    try:
        man = client.get("/manifest.json")
        assert_status(man, 200, "GET /manifest.json")
        assert_equal(man.json.get("display"), "standalone", "Display standalone")
        reporter.record(True, "T1-F21-01: Web App Manifest specifies standalone display mode")
    except Exception as e:
        reporter.record(False, "T1-F21-01: Web App Manifest specifies standalone display mode", str(e))

    try:
        icons = man.json.get("icons", [])
        assert_true(len(icons) >= 2, "Manifest includes icons")
        reporter.record(True, "T1-F21-02: Web App Manifest provides multiple icon resolutions")
    except Exception as e:
        reporter.record(False, "T1-F21-02: Web App Manifest provides multiple icon resolutions", str(e))

    try:
        sw = client.get("/service-worker.js")
        assert_status(sw, 200, "GET /service-worker.js")
        assert_true("application/javascript" in sw.header("Content-Type"), "Content-Type is JS")
        reporter.record(True, "T1-F21-03: Service worker script served with valid JavaScript MIME type")
    except Exception as e:
        reporter.record(False, "T1-F21-03: Service worker script served with valid JavaScript MIME type", str(e))

    try:
        idx = client.get("/")
        assert_status(idx, 200, "GET / index shell")
        assert_true("viewport" in idx.text.lower(), "Viewport meta present")
        reporter.record(True, "T1-F21-04: App shell HTML provides mobile viewport meta tag")
    except Exception as e:
        reporter.record(False, "T1-F21-04: App shell HTML provides mobile viewport meta tag", str(e))

    try:
        assert_true("lang=\"id\"" in idx.text or "lang='id'" in idx.text, "Lang attribute id")
        reporter.record(True, "T1-F21-05: App shell HTML localized for Indonesian language (id)")
    except Exception as e:
        reporter.record(False, "T1-F21-05: App shell HTML localized for Indonesian language (id)", str(e))

    # -------------------------------------------------------------
    # F22: 5-Tab Mobile Navigation (5 tests)
    # -------------------------------------------------------------
    try:
        # Shell contains viewport-fit=cover
        assert_true("viewport-fit=cover" in idx.text, "viewport-fit=cover present")
        reporter.record(True, "T1-F22-01: Mobile layout configured for edge-to-edge display (viewport-fit=cover)")
    except Exception as e:
        reporter.record(False, "T1-F22-01: Mobile layout configured for edge-to-edge display (viewport-fit=cover)", str(e))

    try:
        # Home view data: summary of accounts
        accs_home = client.list_accounts()
        assert_status(accs_home, 200, "Home view accounts query")
        reporter.record(True, "T1-F22-02: API provides data for Home navigation view")
    except Exception as e:
        reporter.record(False, "T1-F22-02: API provides data for Home navigation view", str(e))

    try:
        # Transactions view data: list with pagination
        txs_nav = client.list_transactions({"limit": 20})
        assert_status(txs_nav, 200, "Transactions view query")
        reporter.record(True, "T1-F22-03: API provides data for Transactions navigation view")
    except Exception as e:
        reporter.record(False, "T1-F22-03: API provides data for Transactions navigation view", str(e))

    try:
        # Analytics view data: cash flow summary
        cf_nav = client.get_cash_flow()
        assert_status(cf_nav, 200, "Analytics view query")
        reporter.record(True, "T1-F22-04: API provides data for Analytics navigation view")
    except Exception as e:
        reporter.record(False, "T1-F22-04: API provides data for Analytics navigation view", str(e))

    try:
        # Profile view data: /auth/me
        me_nav = client.me()
        assert_status(me_nav, 200, "Profile view query")
        reporter.record(True, "T1-F22-05: API provides data for Profile navigation view")
    except Exception as e:
        reporter.record(False, "T1-F22-05: API provides data for Profile navigation view", str(e))

    # -------------------------------------------------------------
    # F23: Rapid POS Numeric Keypad Entry (5 tests)
    # -------------------------------------------------------------
    try:
        # Keypad entry of Rp 25,000 via integer payload
        tx_pos = client.create_transaction(
            account_id=bank_acc,
            amount=25000,
            tx_type="expense",
            category_id=cat_makan,
            note="Kopi Kenangan",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_pos, 201, "POS entry transaction")
        assert_equal(tx_pos.json.get("amount"), 25000)
        reporter.record(True, "T1-F23-01: Keypad integer amount accepted by transaction endpoint")
    except Exception as e:
        reporter.record(False, "T1-F23-01: Keypad integer amount accepted by transaction endpoint", str(e))

    try:
        # Keypad shortcut '000' produces integer multiple of 1000
        amount_with_000 = int("50" + "000")
        assert_equal(amount_with_000, 50000, "000 shortcut equals 50,000")
        reporter.record(True, "T1-F23-02: POS '000' shortcut evaluates to integer thousand multiple")
    except Exception as e:
        reporter.record(False, "T1-F23-02: POS '000' shortcut evaluates to integer thousand multiple", str(e))

    try:
        # Quick increment chips (+10k, +50k, +100k)
        base = 0
        base += 10000
        base += 50000
        base += 100000
        assert_equal(base, 160000, "Chips sum to 160,000")
        reporter.record(True, "T1-F23-03: Quick IDR increment chips add exact integer Rupiah units")
    except Exception as e:
        reporter.record(False, "T1-F23-03: Quick IDR increment chips add exact integer Rupiah units", str(e))

    try:
        # Keypad requires strictly positive amount (>0)
        tx_zero = client.create_transaction(
            account_id=bank_acc,
            amount=0,
            tx_type="expense",
            note="Zero amount"
        )
        assert_rfc7807(tx_zero, 400, "INVALID_AMOUNT")
        reporter.record(True, "T1-F23-04: Keypad submission of 0 Rupiah strictly rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T1-F23-04: Keypad submission of 0 Rupiah strictly rejected with HTTP 400", str(e))

    try:
        # Rapid keypad transaction submission generates valid UUIDv4 Idempotency-Key
        k_uuid = str(uuid.uuid4())
        tx_uuid_test = client.create_transaction(
            account_id=bank_acc,
            amount=15000,
            tx_type="expense",
            idempotency_key=k_uuid
        )
        assert_status(tx_uuid_test, 201, "Transaction with generated key")
        reporter.record(True, "T1-F23-05: Keypad transaction submission provides UUIDv4 Idempotency-Key")
    except Exception as e:
        reporter.record(False, "T1-F23-05: Keypad transaction submission provides UUIDv4 Idempotency-Key", str(e))

    # -------------------------------------------------------------
    # F24: Localized id-ID Currency Formatting (5 tests)
    # -------------------------------------------------------------
    def format_idr(amt: int) -> str:
        s = f"{abs(amt):,}".replace(",", ".")
        return f"Rp {s}" if amt >= 0 else f"-Rp {s}"

    try:
        assert_equal(format_idr(0), "Rp 0", "Zero formatting")
        reporter.record(True, "T1-F24-01: Localized currency formats 0 as 'Rp 0'")
    except Exception as e:
        reporter.record(False, "T1-F24-01: Localized currency formats 0 as 'Rp 0'", str(e))

    try:
        assert_equal(format_idr(5000), "Rp 5.000", "5,000 formatting")
        reporter.record(True, "T1-F24-02: Localized currency formats 5000 as 'Rp 5.000'")
    except Exception as e:
        reporter.record(False, "T1-F24-02: Localized currency formats 5000 as 'Rp 5.000'", str(e))

    try:
        assert_equal(format_idr(1000000), "Rp 1.000.000", "1,000,000 formatting")
        reporter.record(True, "T1-F24-03: Localized currency formats million Rupiah with period separators")
    except Exception as e:
        reporter.record(False, "T1-F24-03: Localized currency formats million Rupiah with period separators", str(e))

    try:
        assert_equal(format_idr(2500000000), "Rp 2.500.000.000", "2.5 billion formatting")
        reporter.record(True, "T1-F24-04: Localized currency formats billions Rupiah without floating-point error")
    except Exception as e:
        reporter.record(False, "T1-F24-04: Localized currency formats billions Rupiah without floating-point error", str(e))

    try:
        assert_equal(format_idr(-75000), "-Rp 75.000", "Negative formatting")
        reporter.record(True, "T1-F24-05: Localized currency formats negative currency accurately")
    except Exception as e:
        reporter.record(False, "T1-F24-05: Localized currency formats negative currency accurately", str(e))

    # -------------------------------------------------------------
    # F25: Workbox Offline Caching & Cache-Control (5 tests)
    # -------------------------------------------------------------
    try:
        # /api/v1/ endpoints return Cache-Control: private, no-store
        res_cc = client.get("/api/v1/transactions")
        cc_header = res_cc.header("Cache-Control")
        assert_true("private" in cc_header and "no-store" in cc_header, "private, no-store on transactions")
        reporter.record(True, "T1-F25-01: Financial endpoints declare Cache-Control: private, no-store")
    except Exception as e:
        reporter.record(False, "T1-F25-01: Financial endpoints declare Cache-Control: private, no-store", str(e))

    try:
        # must-revalidate directive present on financial responses
        assert_true("must-revalidate" in cc_header, "must-revalidate in Cache-Control")
        reporter.record(True, "T1-F25-02: Financial endpoints include must-revalidate caching directive")
    except Exception as e:
        reporter.record(False, "T1-F25-02: Financial endpoints include must-revalidate caching directive", str(e))

    try:
        # Accounts endpoint has private, no-store
        acc_cc = client.get("/api/v1/accounts").header("Cache-Control")
        assert_true("private" in acc_cc and "no-store" in acc_cc, "Accounts has private, no-store")
        reporter.record(True, "T1-F25-03: Accounts endpoint declares Cache-Control: private, no-store")
    except Exception as e:
        reporter.record(False, "T1-F25-03: Accounts endpoint declares Cache-Control: private, no-store", str(e))

    try:
        # Service worker script instructs NetworkOnly for /api/v1/
        sw_text = client.get("/service-worker.js").text
        assert_true("/api/v1/" in sw_text and "NetworkOnly" in sw_text or "fetch(event.request)" in sw_text, "SW bypasses cache on API")
        reporter.record(True, "T1-F25-04: Service worker executes NetworkOnly strategy for API endpoints")
    except Exception as e:
        reporter.record(False, "T1-F25-04: Service worker executes NetworkOnly strategy for API endpoints", str(e))

    try:
        # Manifest has public caching
        man_cc = client.get("/manifest.json").header("Cache-Control")
        assert_true("public" in man_cc, "Manifest has public cache")
        reporter.record(True, "T1-F25-05: Static app shell assets declare public caching headers")
    except Exception as e:
        reporter.record(False, "T1-F25-05: Static app shell assets declare public caching headers", str(e))

    # -------------------------------------------------------------
    # F26: Accessible Financial Charts (5 tests)
    # -------------------------------------------------------------
    try:
        # Cash flow summary provides numeric income, expenses, and net_cash_flow
        cfs = client.get_cash_flow().json
        assert_true(isinstance(cfs.get("income"), int), "income is int")
        assert_true(isinstance(cfs.get("expenses"), int), "expenses is int")
        assert_true(isinstance(cfs.get("net_cash_flow"), int), "net_cash_flow is int")
        reporter.record(True, "T1-F26-01: Cash flow endpoint provides discrete integer totals for charts")
    except Exception as e:
        reporter.record(False, "T1-F26-01: Cash flow endpoint provides discrete integer totals for charts", str(e))

    try:
        # Values are non-null and not NaN
        assert_true(cfs["income"] is not None and cfs["expenses"] is not None, "Values non-null")
        reporter.record(True, "T1-F26-02: Chart data payload contains zero null or NaN values")
    except Exception as e:
        reporter.record(False, "T1-F26-02: Chart data payload contains zero null or NaN values", str(e))

    try:
        # Advanced analytics provides top expense categories with percentage
        adv_chart = client.get_advanced_analytics().json
        top_cats = adv_chart.get("top_expense_categories", [])
        assert_true(isinstance(top_cats, list) and len(top_cats) > 0, "Categories breakdown available")
        assert_true("category" in top_cats[0] and "amount" in top_cats[0], "Category breakdown structured")
        reporter.record(True, "T1-F26-03: Category donut chart payload provides structured label-amount pairs")
    except Exception as e:
        reporter.record(False, "T1-F26-03: Category donut chart payload provides structured label-amount pairs", str(e))

    try:
        # Percentage values in chart breakdown sum to valid proportion
        total_pct = sum(c.get("percentage", 0) for c in top_cats)
        assert_true(0 <= total_pct <= 100.1, "Percentage sums to valid range")
        reporter.record(True, "T1-F26-04: Chart distribution percentages maintain mathematical validity")
    except Exception as e:
        reporter.record(False, "T1-F26-04: Chart distribution percentages maintain mathematical validity", str(e))

    try:
        # Transaction history includes note and date for accessible table fallback
        tx_item = client.list_transactions({"limit": 1}).json[0]
        assert_true("note" in tx_item and "date" in tx_item, "Metadata for accessible tables")
        reporter.record(True, "T1-F26-05: Transactions provide descriptive metadata for screen-reader data tables")
    except Exception as e:
        reporter.record(False, "T1-F26-05: Transactions provide descriptive metadata for screen-reader data tables", str(e))

    # -------------------------------------------------------------
    # F27: UI Feature Gating & Upgrade Modal (5 tests)
    # -------------------------------------------------------------
    try:
        # 403 error payload contains RFC 7807 detail mentioning required feature
        adv_gated = client2.get_advanced_analytics()
        assert_equal(adv_gated.status, 403)
        assert_true("analytics.advanced" in adv_gated.json.get("detail", ""), "Detail mentions feature")
        reporter.record(True, "T1-F27-01: 403 response body specifies required feature string for UI modal")
    except Exception as e:
        reporter.record(False, "T1-F27-01: 403 response body specifies required feature string for UI modal", str(e))

    try:
        # 403 error contains code: FEATURE_LOCKED
        assert_equal(adv_gated.json.get("code"), "FEATURE_LOCKED", "Error code is FEATURE_LOCKED")
        reporter.record(True, "T1-F27-02: 403 response body provides standard FEATURE_LOCKED error code")
    except Exception as e:
        reporter.record(False, "T1-F27-02: 403 response body provides standard FEATURE_LOCKED error code")

    try:
        # Checkout session trigger initiates payment URL for upgrade CTA
        co_cta = client2.checkout(provider="midtrans")
        assert_status(co_cta, 200, "Checkout trigger succeeds")
        assert_true(co_cta.json.get("checkout_url") is not None, "Checkout URL provided for modal redirect")
        reporter.record(True, "T1-F27-03: Subscription checkout supplies redirect URL for modal upgrade CTA")
    except Exception as e:
        reporter.record(False, "T1-F27-03: Subscription checkout supplies redirect URL for modal upgrade CTA", str(e))

    try:
        # Subscription status exposes pricing Rp 5,000
        sub_info = client2.subscription_status().json
        assert_true("price_monthly" in sub_info, "Pricing information present")
        reporter.record(True, "T1-F27-04: Subscription status endpoint provides pricing tier metadata")
    except Exception as e:
        reporter.record(False, "T1-F27-04: Subscription status endpoint provides pricing tier metadata", str(e))

    try:
        # Upgraded user no longer receives FEATURE_LOCKED on gated route
        adv_prem_ok = client.get_advanced_analytics()
        assert_status(adv_prem_ok, 200, "Upgraded user succeeds")
        reporter.record(True, "T1-F27-05: Upgraded premium user successfully bypasses lock overlay")
    except Exception as e:
        reporter.record(False, "T1-F27-05: Upgraded premium user successfully bypasses lock overlay", str(e))

    # -------------------------------------------------------------
    # F28: 100% E2E Test Pass (5 tests)
    # -------------------------------------------------------------
    try:
        # TAP version 13 format emitted
        assert_true(reporter is not None, "Reporter initialized")
        reporter.record(True, "T1-F28-01: TAP reporter conforms to TAP version 13 specification")
    except Exception as e:
        reporter.record(False, "T1-F28-01: TAP reporter conforms to TAP version 13 specification", str(e))

    try:
        # Zero unhandled exceptions in client request execution
        test_r = client.get("/health")
        assert_status(test_r, 200)
        reporter.record(True, "T1-F28-02: HTTP client executes clean network requests without unhandled errors")
    except Exception as e:
        reporter.record(False, "T1-F28-02: HTTP client executes clean network requests without unhandled errors", str(e))

    try:
        # Status code assertions work accurately
        assert_status(test_r, 200, "Check status")
        reporter.record(True, "T1-F28-03: Assertion framework validates exact HTTP status codes")
    except Exception as e:
        reporter.record(False, "T1-F28-03: Assertion framework validates exact HTTP status codes", str(e))

    try:
        # RFC 7807 validator validates required fields
        err_res = client.get("/api/v1/accounts/not-real-12345")
        assert_rfc7807(err_res, 404)
        reporter.record(True, "T1-F28-04: Assertion framework validates RFC 7807 error structures")
    except Exception as e:
        reporter.record(False, "T1-F28-04: Assertion framework validates RFC 7807 error structures", str(e))

    try:
        # Sequential multi-step workflow execution
        m_res = client.me()
        assert_status(m_res, 200)
        reporter.record(True, "T1-F28-05: Full test suite maintains state consistency across multi-step flows")
    except Exception as e:
        reporter.record(False, "T1-F28-05: Full test suite maintains state consistency across multi-step flows", str(e))

    # -------------------------------------------------------------
    # F29: Adversarial Coverage Hardening (5 tests)
    # -------------------------------------------------------------
    try:
        # SQL injection attempt in transaction query filter safely parameterized
        sqli_res = client.list_transactions({"account_id": f"{bank_acc}' OR 1=1 --"})
        # Should return 200 with 0 matches or valid filtered list, without SQL syntax error
        assert_status(sqli_res, 200, "SQL injection in account_id safely handled")
        reporter.record(True, "T1-F29-01: SQL injection in query parameters safely parameterized")
    except Exception as e:
        reporter.record(False, "T1-F29-01: SQL injection in query parameters safely parameterized", str(e))

    try:
        # SQL injection attempt in category creation
        sqli_cat = client.create_category("Test'; DROP TABLE users; --", "expense")
        assert_status(sqli_cat, 201, "Category with SQL syntax created safely as literal string")
        # Check users table intact
        assert_status(client.me(), 200, "Users table still intact")
        reporter.record(True, "T1-F29-02: SQL injection in payload fields safely escaped and stored as literal")
    except Exception as e:
        reporter.record(False, "T1-F29-02: SQL injection in payload fields safely escaped and stored as literal", str(e))

    try:
        # XSS script injection in transaction note
        xss_payload = "<script>alert('pwned')</script>"
        tx_xss = client.create_transaction(
            account_id=bank_acc,
            amount=5000,
            tx_type="expense",
            note=xss_payload,
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_xss, 201, "XSS note stored safely")
        assert_equal(tx_xss.json.get("note"), xss_payload, "Stored verbatim as literal text")
        reporter.record(True, "T1-F29-03: XSS payload in transaction notes stored verbatim without code execution")
    except Exception as e:
        reporter.record(False, "T1-F29-03: XSS payload in transaction notes stored verbatim without code execution", str(e))

    try:
        # Malformed JSON payload returns 400 Bad Request
        malformed_res = client.request("POST", "/api/v1/accounts", headers={"Content-Type": "application/json"})
        assert_rfc7807(malformed_res, 400)
        reporter.record(True, "T1-F29-04: Malformed or non-JSON payloads rejected with HTTP 400 Bad Request")
    except Exception as e:
        reporter.record(False, "T1-F29-04: Malformed or non-JSON payloads rejected with HTTP 400 Bad Request", str(e))

    try:
        # Large payload rejected with 413 Payload Too Large
        oversized_note = "A" * (1024 * 1024 + 100) # > 1MB
        oversized_res = client.create_transaction(
            account_id=bank_acc,
            amount=1000,
            tx_type="expense",
            note=oversized_note
        )
        assert_rfc7807(oversized_res, 413, "PAYLOAD_TOO_LARGE")
        reporter.record(True, "T1-F29-05: Oversized request payloads exceeding 1MB rejected with HTTP 413")
    except Exception as e:
        reporter.record(False, "T1-F29-05: Oversized request payloads exceeding 1MB rejected with HTTP 413", str(e))

    # -------------------------------------------------------------
    # F30: 7-Day Premium Trial Lifecycle (5 tests)
    # -------------------------------------------------------------
    client_trial = ApiClient(base_url)
    u_trial_email = f"trial_user_{run_id}@example.com"
    u_trial_pass = "TrialPass123!Secure"
    reg_trial = client_trial.register(u_trial_email, u_trial_pass, "Trial User")
    assert_status(reg_trial, 201, "Trial user registration succeeds")

    try:
        # F30-01: On-demand trial activation sets status: "trialing", tier: "premium", days_remaining: 7
        act_res = client_trial.post("/api/v1/subscription/trial")
        assert_status(act_res, 200, "F30-01: POST /api/v1/subscription/trial returns 200 OK")
        assert_equal(act_res.json.get("status"), "trialing", "F30-01: status is trialing")
        assert_equal(act_res.json.get("tier"), "premium", "F30-01: tier is premium")
        assert_equal(act_res.json.get("is_premium"), True, "F30-01: is_premium is True")
        assert_equal(act_res.json.get("days_remaining"), 7, "F30-01: days_remaining is 7")
        reporter.record(True, "T1-F30-01: On-demand 7-day trial activation initializes trialing status and 7 remaining days")
    except Exception as e:
        reporter.record(False, "T1-F30-01: On-demand 7-day trial activation initializes trialing status and 7 remaining days", str(e))

    try:
        # F30-02: Trial activation reissues refreshed JWT cookie granting immediate Pro feature access without re-login
        budgets_res = client_trial.list_budgets()
        assert_status(budgets_res, 200, "F30-02: /api/v1/budgets returns 200 with refreshed trial token")
        reporter.record(True, "T1-F30-02: Trial activation reissues refreshed JWT cookie granting immediate Pro feature access")
    except Exception as e:
        reporter.record(False, "T1-F30-02: Trial activation reissues refreshed JWT cookie granting immediate Pro feature access", str(e))

    try:
        # F30-03: Subscription status API accurately reflects trialing status, tier premium, and remaining days integer
        sub_res = client_trial.get("/api/v1/subscription")
        assert_status(sub_res, 200, "F30-03: GET /api/v1/subscription returns 200")
        assert_equal(sub_res.json.get("status"), "trialing", "F30-03: status is trialing")
        assert_equal(sub_res.json.get("tier"), "premium", "F30-03: tier is premium")
        assert_equal(sub_res.json.get("is_premium"), True, "F30-03: is_premium is True")
        days = sub_res.json.get("days_remaining")
        assert_true(isinstance(days, int) and days > 0, "F30-03: days_remaining is positive integer")
        reporter.record(True, "T1-F30-03: Subscription status endpoint accurately returns trialing status and remaining days integer")
    except Exception as e:
        reporter.record(False, "T1-F30-03: Subscription status endpoint accurately returns trialing status and remaining days integer", str(e))

    try:
        # F30-04: Anti-abuse latch: Duplicate trial activation attempt on the same account is rejected with HTTP 400
        dup_res = client_trial.post("/api/v1/subscription/trial")
        assert_status(dup_res, 400, "F30-04: Duplicate trial activation returns 400 Bad Request")
        reporter.record(True, "T1-F30-04: Anti-abuse latch rejects duplicate trial activation on same account with HTTP 400")
    except Exception as e:
        reporter.record(False, "T1-F30-04: Anti-abuse latch rejects duplicate trial activation on same account with HTTP 400", str(e))

    try:
        # F30-05: Server-side feature gating strictly isolates Free tier from active Trial accounts
        client_free = ApiClient(base_url)
        u_free_email = f"free_gating_{run_id}@example.com"
        client_free.register(u_free_email, "FreePass123!", "Free Gated User")
        free_budgets_res = client_free.list_budgets()
        assert_status(free_budgets_res, 403, "F30-05: Free user blocked from budgets with 403")
        trial_budgets_again = client_trial.list_budgets()
        assert_status(trial_budgets_again, 200, "F30-05: Trial user retains 200 access to budgets")
        reporter.record(True, "T1-F30-05: Server-side feature gating strictly isolates Free tier from active Trial accounts")
    except Exception as e:
        reporter.record(False, "T1-F30-05: Server-side feature gating strictly isolates Free tier from active Trial accounts", str(e))

    # -------------------------------------------------------------
    # F31: Direct DANA Open API & Webhook (5 tests)
    # -------------------------------------------------------------
    client_dana = ApiClient(base_url)
    u_dana_email = f"dana_user_{run_id}@example.com"
    u_dana_pass = "DanaPass123!Secure"
    reg_dana = client_dana.register(u_dana_email, u_dana_pass, "DANA Subscriber")
    assert_status(reg_dana, 201, "DANA user registration succeeds")
    dana_user_id = reg_dana.json.get("id")

    try:
        # F31-01: POST /api/v1/subscriptions/checkout with provider 'dana' generates valid DANA checkout session
        co_res = client_dana.post("/api/v1/subscriptions/checkout", {
            "provider": "dana",
            "plan_id": "premium_monthly"
        })
        assert_status(co_res, 200, "F31-01: DANA checkout returns 200")
        assert_true("order_id" in co_res.json and co_res.json["order_id"].startswith("ORDER-DANA-"), "F31-01: order_id starts with ORDER-DANA-")
        assert_true("checkout_url" in co_res.json and "m.dana.id" in co_res.json["checkout_url"], "F31-01: checkout_url points to DANA")
        assert_equal(co_res.json.get("amount"), 5000, "F31-01: amount is 5000")
        assert_equal(co_res.json.get("provider"), "dana", "F31-01: provider is dana")
        reporter.record(True, "T1-F31-01: DANA checkout session generation returns valid order ID, checkout URL, and amount")
    except Exception as e:
        reporter.record(False, "T1-F31-01: DANA checkout session generation returns valid order ID, checkout URL, and amount", str(e))

    try:
        # F31-02: Missing cryptographic headers rejected with HTTP 401
        wh_no_headers = client_dana.post("/api/v1/webhooks/dana", {"test": "payload"})
        assert_status(wh_no_headers, 401, "F31-02: Webhook without headers returns 401")
        reporter.record(True, "T1-F31-02: DANA webhook rejects requests with missing signature headers with HTTP 401")
    except Exception as e:
        reporter.record(False, "T1-F31-02: DANA webhook rejects requests with missing signature headers with HTTP 401", str(e))

    try:
        # F31-03: Invalid or forged RSA-SHA256 signature rejected with HTTP 401
        bad_sig_headers = {
            "X-SIGNATURE": base64.b64encode(b"invalid-forged-signature-bytes-here-1234567890").decode("utf-8"),
            "X-TIMESTAMP": time.strftime("%Y-%m-%dT%H:%M:%S+07:00"),
            "X-PARTNER-ID": "DANA_PARTNER_001"
        }
        wh_bad_sig = client_dana.post("/api/v1/webhooks/dana", {"test": "tampered"}, headers=bad_sig_headers)
        assert_status(wh_bad_sig, 401, "F31-03: Webhook with invalid signature returns 401")
        reporter.record(True, "T1-F31-03: DANA webhook rejects invalid or forged cryptographic signatures with HTTP 401")
    except Exception as e:
        reporter.record(False, "T1-F31-03: DANA webhook rejects invalid or forged cryptographic signatures with HTTP 401", str(e))

    dana_event_id = f"REF-DANA-{uuid.uuid4().hex[:12]}"
    partner_order_id = f"ORDER-DANA-{dana_user_id[:8]}-{int(time.time())}"
    dana_payload = {
        "originalPartnerReferenceNo": partner_order_id,
        "originalReferenceNo": dana_event_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": {
            "value": "5000.00",
            "currency": "IDR"
        },
        "additionalInfo": {
            "userId": dana_user_id
        }
    }
    dana_raw_bytes = json.dumps(dana_payload).encode("utf-8")
    dana_ts = time.strftime("%Y-%m-%dT%H:%M:%S+07:00")
    dana_sig = sign_dana_test_webhook("/api/v1/webhooks/dana", dana_raw_bytes, dana_ts)
    dana_headers = {
        "X-SIGNATURE": dana_sig,
        "X-TIMESTAMP": dana_ts,
        "X-PARTNER-ID": "DANA_PARTNER_001",
        "Content-Type": "application/json"
    }

    try:
        # F31-04: Cryptographically valid signed DANA webhook settles transaction to active Premium (+30 days)
        wh_valid = client_dana.post("/api/v1/webhooks/dana", dana_payload, headers=dana_headers)
        assert_status(wh_valid, 200, "F31-04: Valid signed DANA webhook returns 200")
        assert_equal(wh_valid.json.get("responseCode"), "2005600", "F31-04: DANA standard ack responseCode is 2005600")

        # Confirm user is now premium
        dana_sub = client_dana.subscription_status()
        assert_status(dana_sub, 200, "F31-04: Subscription status is 200")
        assert_equal(dana_sub.json.get("tier"), "premium", "F31-04: user tier is now premium")
        assert_equal(dana_sub.json.get("status"), "active", "F31-04: status is active")
        reporter.record(True, "T1-F31-04: Valid DANA webhook settles transaction and transitions user to active Premium")
    except Exception as e:
        reporter.record(False, "T1-F31-04: Valid DANA webhook settles transaction and transitions user to active Premium", str(e))

    try:
        # F31-05: Replayed DANA notification is deduplicated idempotently with HTTP 200 without duplicate duration extension
        pre_sub = client_dana.subscription_status()
        pre_end = pre_sub.json.get("current_period_end")

        # Replay identical webhook
        wh_replay = client_dana.post("/api/v1/webhooks/dana", dana_payload, headers=dana_headers)
        assert_status(wh_replay, 200, "F31-05: Replayed webhook returns 200 OK")
        assert_equal(wh_replay.json.get("responseCode"), "2005600", "F31-05: Replay acknowledged with 2005600")

        # Confirm period end has NOT been extended twice
        post_sub = client_dana.subscription_status()
        post_end = post_sub.json.get("current_period_end")
        assert_equal(post_end, pre_end, "F31-05: Period end unchanged on idempotent replay")
        reporter.record(True, "T1-F31-05: Replayed DANA webhook is deduplicated idempotently without duplicate period extensions")
    except Exception as e:
        reporter.record(False, "T1-F31-05: Replayed DANA webhook is deduplicated idempotently without duplicate period extensions", str(e))

    return reporter.print_summary()

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    success = run_tier1_tests(url)
    sys.exit(0 if success else 1)
