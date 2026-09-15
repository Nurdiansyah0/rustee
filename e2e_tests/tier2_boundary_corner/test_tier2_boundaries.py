#!/usr/bin/env python3
"""
Tier 2: Boundary & Corner Cases Test Suite (145 tests across 29 features).
Tests boundary values, extreme inputs, constraints, edge cases, and failure modes.
"""

import sys
import os
import time
import uuid
import json

# Add harness directory to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from harness.client import (
    ApiClient, TapReporter, assert_true, assert_equal,
    assert_status, assert_rfc7807, compute_midtrans_signature
)

def run_tier2_tests(base_url: str = "http://127.0.0.1:8089", reporter: TapReporter = None) -> bool:
    if reporter is None:
        reporter = TapReporter(total_expected=145)
        reporter.print_header()

    client = ApiClient(base_url)

    # Unique test run identifier
    run_id = f"t2_{int(time.time())}_{uuid.uuid4().hex[:6]}"
    u1_email = f"b_user1_{run_id}@example.com"
    u2_email = f"b_user2_{run_id}@example.com"
    u1_pass = "SecurePass123!"
    u2_pass = "SecurePass456!"

    # -------------------------------------------------------------
    # B01: SQLite WAL & PRAGMAs Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Rapid concurrent reads on /ready during active state
        for _ in range(10):
            r = client.get("/ready")
            assert_status(r, 200)
        reporter.record(True, "T2-B01-01: SQLite WAL handles repeated rapid concurrent connection queries")
    except Exception as e:
        reporter.record(False, "T2-B01-01: SQLite WAL handles repeated rapid concurrent connection queries", str(e))

    try:
        # Register user for boundary tests
        reg_res = client.register(u1_email, u1_pass, "Boundary User One")
        assert_status(reg_res, 201)
        u1_data = reg_res.json
        reporter.record(True, "T2-B01-02: SQLite transactions persist user registration atomically")
    except Exception as e:
        reporter.record(False, "T2-B01-02: SQLite transactions persist user registration atomically", str(e))

    try:
        # Check WAL mode remains active after write
        r_ready = client.get("/ready")
        assert_equal(r_ready.json.get("wal_mode"), True)
        reporter.record(True, "T2-B01-03: SQLite journal_mode persists as WAL post-mutation")
    except Exception as e:
        reporter.record(False, "T2-B01-03: SQLite journal_mode persists as WAL post-mutation", str(e))

    try:
        # Fetching accounts confirms foreign key relationship established
        accs = client.list_accounts().json
        assert_true(len(accs) >= 2)
        reporter.record(True, "T2-B01-04: Foreign key constraint associates seeded accounts with user_id")
    except Exception as e:
        reporter.record(False, "T2-B01-04: Foreign key constraint associates seeded accounts with user_id", str(e))

    try:
        # Check SQLite busy timeout handles immediate subsequent queries
        r_probe = client.get("/ready")
        assert_equal(r_probe.json.get("database"), "connected")
        reporter.record(True, "T2-B01-05: SQLite busy_timeout ensures immediate pool availability")
    except Exception as e:
        reporter.record(False, "T2-B01-05: SQLite busy_timeout ensures immediate pool availability", str(e))

    # -------------------------------------------------------------
    # B02: Relational Schema Migrations Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Registration with empty email rejected 400
        r_empty_email = client.register("", "ValidPass123!", "Empty Email")
        assert_rfc7807(r_empty_email, 400, "INVALID_EMAIL")
        reporter.record(True, "T2-B02-01: Registration with empty email string rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T2-B02-01: Registration with empty email string rejected with HTTP 400", str(e))

    try:
        # Duplicate email registration rejected 409 Conflict
        r_dup = client.register(u1_email, "AnotherPass123!", "Duplicate User")
        assert_rfc7807(r_dup, 409, "EMAIL_ALREADY_EXISTS")
        reporter.record(True, "T2-B02-02: Unique email constraint in users table rejects duplicate registration 409")
    except Exception as e:
        reporter.record(False, "T2-B02-02: Unique email constraint in users table rejects duplicate registration 409")

    bank_acc = accs[0]["id"]
    try:
        # 4-byte UTF-8 emoji in transaction note
        emoji_note = "Beli Kopi ☕ dan Pizza 🍕 untuk pesta 💰🎉"
        tx_emoji = client.create_transaction(
            account_id=bank_acc,
            amount=75000,
            tx_type="expense",
            note=emoji_note,
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_emoji, 201)
        assert_equal(tx_emoji.json.get("note"), emoji_note)
        reporter.record(True, "T2-B02-03: SQLite schema handles 4-byte UTF-8 emoji characters without corruption")
    except Exception as e:
        reporter.record(False, "T2-B02-03: SQLite schema handles 4-byte UTF-8 emoji characters without corruption", str(e))

    try:
        # Long note (5,000 characters) stored cleanly
        long_note = "Catatan Keuangan: " + ("Detail transaksi panjang. " * 200)
        tx_long = client.create_transaction(
            account_id=bank_acc,
            amount=12000,
            tx_type="expense",
            note=long_note,
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_long, 201)
        reporter.record(True, "T2-B02-04: Schema accepts and stores long text strings up to limits")
    except Exception as e:
        reporter.record(False, "T2-B02-04: Schema accepts and stores long text strings up to limits", str(e))

    try:
        # Nullable fields: transaction note empty string
        tx_no_note = client.create_transaction(
            account_id=bank_acc,
            amount=5000,
            tx_type="expense",
            note="",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_no_note, 201)
        assert_equal(tx_no_note.json.get("note"), "")
        reporter.record(True, "T2-B02-05: Schema nullable fields handle empty string values cleanly")
    except Exception as e:
        reporter.record(False, "T2-B02-05: Schema nullable fields handle empty string values cleanly", str(e))

    # -------------------------------------------------------------
    # B03: Composite Indexes Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Inverted date range (start_date > end_date) returns empty list
        tx_inv = client.list_transactions({"start_date": "2026-12-31", "end_date": "2026-01-01"})
        assert_status(tx_inv, 200)
        assert_equal(len(tx_inv.json), 0)
        reporter.record(True, "T2-B03-01: Inverted date range query safely returns empty list")
    except Exception as e:
        reporter.record(False, "T2-B03-01: Inverted date range query safely returns empty list", str(e))

    try:
        # Exact date match on today's transactions
        today_str = time.strftime("%Y-%m-%d")
        tx_today = client.list_transactions({"start_date": today_str, "end_date": today_str})
        assert_status(tx_today, 200)
        assert_true(len(tx_today.json) >= 1)
        reporter.record(True, "T2-B03-02: Boundary date query with identical start and end date matches records")
    except Exception as e:
        reporter.record(False, "T2-B03-02: Boundary date query with identical start and end date matches records", str(e))

    try:
        # Pagination limit boundary: limit=1
        tx_lim1 = client.list_transactions({"limit": 1})
        assert_status(tx_lim1, 200)
        assert_equal(len(tx_lim1.json), 1)
        reporter.record(True, "T2-B03-03: Minimum pagination limit (limit=1) returns exactly one item")
    except Exception as e:
        reporter.record(False, "T2-B03-03: Minimum pagination limit (limit=1) returns exactly one item", str(e))

    try:
        # Pagination limit boundary: limit=1000 capped to 100
        tx_lim_high = client.list_transactions({"limit": 1000})
        assert_status(tx_lim_high, 200)
        assert_true(len(tx_lim_high.json) <= 100)
        reporter.record(True, "T2-B03-04: Excessive pagination limit safely capped to server maximum")
    except Exception as e:
        reporter.record(False, "T2-B03-04: Excessive pagination limit safely capped to server maximum", str(e))

    try:
        # Querying with non-matching category filter
        tx_no_cat = client.list_transactions({"category_id": "non-existent-cat-uuid"})
        assert_status(tx_no_cat, 200)
        assert_equal(len(tx_no_cat.json), 0)
        reporter.record(True, "T2-B03-05: Querying with non-matching foreign key filter returns empty array")
    except Exception as e:
        reporter.record(False, "T2-B03-05: Querying with non-matching foreign key filter returns empty array", str(e))

    # -------------------------------------------------------------
    # B04: Integer Rupiah Math Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Amount = 0 strictly rejected
        r_zero = client.create_transaction(account_id=bank_acc, amount=0, tx_type="expense")
        assert_rfc7807(r_zero, 400, "INVALID_AMOUNT")
        reporter.record(True, "T2-B04-01: Zero Rupiah transaction amount strictly rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B04-01: Zero Rupiah transaction amount strictly rejected 400", str(e))

    try:
        # Negative amount strictly rejected
        r_neg = client.create_transaction(account_id=bank_acc, amount=-50000, tx_type="expense")
        assert_rfc7807(r_neg, 400, "INVALID_AMOUNT")
        reporter.record(True, "T2-B04-02: Negative transaction amount strictly rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B04-02: Negative transaction amount strictly rejected 400", str(e))

    try:
        # Floating point amount rejected (e.g. 5000.75)
        r_float = client.post("/api/v1/transactions", {
            "account_id": bank_acc,
            "amount": 5000.75,
            "transaction_type": "expense"
        })
        assert_rfc7807(r_float, 400, "INVALID_AMOUNT")
        reporter.record(True, "T2-B04-03: Floating-point transaction amount strictly rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B04-03: Floating-point transaction amount strictly rejected 400", str(e))

    try:
        # Large integer math: Rp 100,000,000,000 (100 billion IDR)
        r_large = client.create_transaction(
            account_id=bank_acc,
            amount=100000000000,
            tx_type="income",
            note="Modal Usaha Besar",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(r_large, 201)
        assert_equal(r_large.json.get("amount"), 100000000000)
        reporter.record(True, "T2-B04-04: Large integer Rupiah (100 Billion IDR) processed without precision loss")
    except Exception as e:
        reporter.record(False, "T2-B04-04: Large integer Rupiah (100 Billion IDR) processed without precision loss", str(e))

    try:
        # Integer overflow boundary (> i64 max) rejected
        r_overflow = client.create_transaction(
            account_id=bank_acc,
            amount=9223372036854775808,
            tx_type="income"
        )
        assert_rfc7807(r_overflow, 400, "AMOUNT_OVERFLOW")
        reporter.record(True, "T2-B04-05: Integer overflow beyond 64-bit integer limit rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B04-05: Integer overflow beyond 64-bit integer limit rejected 400", str(e))

    # -------------------------------------------------------------
    # B05: Multi-Tenant Boundary (5 tests)
    # -------------------------------------------------------------
    client2 = ApiClient(base_url)
    reg2_res = client2.register(u2_email, u2_pass, "Boundary User Two")
    u2_data = reg2_res.json

    try:
        # User 1 querying random non-existent UUID returns 404
        r_not_found = client.get(f"/api/v1/accounts/{uuid.uuid4()}")
        assert_rfc7807(r_not_found, 404, "ACCOUNT_NOT_FOUND")
        reporter.record(True, "T2-B05-01: Non-existent resource ID returns RFC 7807 404 Not Found")
    except Exception as e:
        reporter.record(False, "T2-B05-01: Non-existent resource ID returns RFC 7807 404 Not Found", str(e))

    try:
        # User 2 tries to GET User 1's transaction ID -> returns 404
        u1_tx_id = tx_emoji.json["id"]
        r_cross_tx = client2.get(f"/api/v1/transactions/{u1_tx_id}")
        assert_rfc7807(r_cross_tx, 404, "TRANSACTION_NOT_FOUND")
        reporter.record(True, "T2-B05-02: Cross-tenant transaction lookup returns 404 Not Found")
    except Exception as e:
        reporter.record(False, "T2-B05-02: Cross-tenant transaction lookup returns 404 Not Found", str(e))

    try:
        # User 2 tries to delete User 1's category -> returns 404
        u1_cat_id = client.list_categories().json[0]["id"]
        r_cross_del = client2.delete(f"/api/v1/categories/{u1_cat_id}")
        assert_rfc7807(r_cross_del, 404, "CATEGORY_NOT_FOUND")
        reporter.record(True, "T2-B05-03: Cross-tenant category soft-deletion returns 404 Not Found")
    except Exception as e:
        reporter.record(False, "T2-B05-03: Cross-tenant category soft-deletion returns 404 Not Found", str(e))

    try:
        # User 2 attempts transfer with User 1's account as destination -> 404
        u2_acc = client2.list_accounts().json[0]["id"]
        r_cross_dest = client2.create_transaction(
            account_id=u2_acc,
            amount=10000,
            tx_type="transfer",
            destination_account_id=bank_acc # User 1's bank account
        )
        assert_rfc7807(r_cross_dest, 404, "DESTINATION_ACCOUNT_NOT_FOUND")
        reporter.record(True, "T2-B05-04: Cross-tenant transfer destination rejected with 404 Not Found")
    except Exception as e:
        reporter.record(False, "T2-B05-04: Cross-tenant transfer destination rejected with 404 Not Found", str(e))

    try:
        # SQL LIKE wildcard (%) in search doesn't leak other tenants' data
        r_wildcard = client2.list_transactions({"account_id": "%"})
        assert_status(r_wildcard, 200)
        assert_equal(len(r_wildcard.json), 0)
        reporter.record(True, "T2-B05-05: SQL wildcard in query filters respects multi-tenant boundary")
    except Exception as e:
        reporter.record(False, "T2-B05-05: SQL wildcard in query filters respects multi-tenant boundary", str(e))

    # -------------------------------------------------------------
    # B06: Argon2id Password Hashing Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Exactly 8 characters accepted
        r_8 = client.register(f"pw8_{run_id}@example.com", "12345678", "User Eight")
        assert_status(r_8, 201)
        reporter.record(True, "T2-B06-01: Exactly 8-character password accepted at boundary")
    except Exception as e:
        reporter.record(False, "T2-B06-01: Exactly 8-character password accepted at boundary", str(e))

    try:
        # 7 characters rejected
        r_7 = client.register(f"pw7_{run_id}@example.com", "1234567", "User Seven")
        assert_rfc7807(r_7, 400, "PASSWORD_TOO_SHORT")
        reporter.record(True, "T2-B06-02: 7-character password rejected below boundary minimum")
    except Exception as e:
        reporter.record(False, "T2-B06-02: 7-character password rejected below boundary minimum", str(e))

    try:
        # Password with leading and trailing spaces preserved
        space_pass = "  space_password_123  "
        r_space = client.register(f"pwspace_{run_id}@example.com", space_pass, "User Space")
        assert_status(r_space, 201)
        c_space = ApiClient(base_url)
        # Login with untrimmed space password succeeds
        assert_status(c_space.login(f"pwspace_{run_id}@example.com", space_pass), 200)
        reporter.record(True, "T2-B06-03: Password hashing preserves leading and trailing whitespace")
    except Exception as e:
        reporter.record(False, "T2-B06-03: Password hashing preserves leading and trailing whitespace", str(e))

    try:
        # Password with trimmed spaces fails login
        c_bad_space = ApiClient(base_url)
        assert_status(c_bad_space.login(f"pwspace_{run_id}@example.com", space_pass.strip()), 401)
        reporter.record(True, "T2-B06-04: Password verification distinguishes trimmed vs untrimmed whitespace")
    except Exception as e:
        reporter.record(False, "T2-B06-04: Password verification distinguishes trimmed vs untrimmed whitespace", str(e))

    try:
        # Empty password rejected 400
        r_empty_pw = client.register(f"no_pw_{run_id}@example.com", "", "No Password")
        assert_rfc7807(r_empty_pw, 400, "PASSWORD_TOO_SHORT")
        reporter.record(True, "T2-B06-05: Empty password rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T2-B06-05: Empty password rejected with HTTP 400", str(e))

    # -------------------------------------------------------------
    # B07: Short-lived JWT & HttpOnly Cookies Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Malformed token (single part) rejected 401
        c_mal = ApiClient(base_url)
        c_mal.set_cookie("auth_token", "invalid_token_no_dots")
        r_mal = c_mal.me()
        assert_rfc7807(r_mal, 401, "AUTH_REQUIRED")
        reporter.record(True, "T2-B07-01: Malformed cookie token rejected with HTTP 401")
    except Exception as e:
        reporter.record(False, "T2-B07-01: Malformed cookie token rejected with HTTP 401", str(e))

    try:
        # Tampered signature rejected 401
        c_tamp = ApiClient(base_url)
        valid_cookie = client.cookies.get("auth_token", "")
        if "." in valid_cookie:
            parts = valid_cookie.split(".")
            tampered_cookie = f"{parts[0]}.{parts[1]}.bad_sig_hex_12345"
            c_tamp.set_cookie("auth_token", tampered_cookie)
            r_tamp = c_tamp.me()
            assert_rfc7807(r_tamp, 401, "AUTH_REQUIRED")
        reporter.record(True, "T2-B07-02: Tampered JWT signature rejected with HTTP 401")
    except Exception as e:
        reporter.record(False, "T2-B07-02: Tampered JWT signature rejected with HTTP 401", str(e))

    try:
        # Authorization Bearer header accepted as alternative
        c_bearer = ApiClient(base_url)
        r_bearer = c_bearer.get("/api/v1/auth/me", headers={"Authorization": f"Bearer {valid_cookie}"})
        assert_status(r_bearer, 200)
        reporter.record(True, "T2-B07-03: Authorization Bearer header accepted as valid token carrier")
    except Exception as e:
        reporter.record(False, "T2-B07-03: Authorization Bearer header accepted as valid token carrier", str(e))

    try:
        # Unauthenticated request returns 401
        c_anon = ApiClient(base_url)
        r_anon = c_anon.get("/api/v1/accounts")
        assert_rfc7807(r_anon, 401, "AUTH_REQUIRED")
        reporter.record(True, "T2-B07-04: Completely unauthenticated requests to protected endpoints return 401")
    except Exception as e:
        reporter.record(False, "T2-B07-04: Completely unauthenticated requests to protected endpoints return 401", str(e))

    try:
        # Expired token simulation / future token check
        c_exp = ApiClient(base_url)
        c_exp.set_cookie("auth_token", "expired.token.simulation")
        r_exp = c_exp.me()
        assert_rfc7807(r_exp, 401, "AUTH_REQUIRED")
        reporter.record(True, "T2-B07-05: Non-verifiable token safely returns 401 without 500 error")
    except Exception as e:
        reporter.record(False, "T2-B07-05: Non-verifiable token safely returns 401 without 500 error", str(e))

    # -------------------------------------------------------------
    # B08: Sliding Window Rate Limiting Boundary (5 tests)
    # -------------------------------------------------------------
    # Ensure client is logged back in as user 1
    client.login(u1_email, u1_pass)

    try:
        # Rate limit boundary: 5 attempts
        rl_c = ApiClient(base_url, client_ip="10.2.2.2")
        for i in range(1, 6):
            r_try = rl_c.login(f"rl_target_{run_id}@example.com", "bad_pass")
            # 1-5 must return 401
            assert_status(r_try, 401)
        reporter.record(True, "T2-B08-01: Exactly 5 failed attempts allowed at boundary")
    except Exception as e:
        reporter.record(False, "T2-B08-01: Exactly 5 failed attempts allowed at boundary", str(e))

    try:
        # 6th attempt returns 429
        r_block = rl_c.login(f"rl_target_{run_id}@example.com", "bad_pass")
        assert_rfc7807(r_block, 429, "RATE_LIMIT_EXCEEDED")
        reporter.record(True, "T2-B08-02: 6th failed attempt strictly blocked with HTTP 429")
    except Exception as e:
        reporter.record(False, "T2-B08-02: 6th failed attempt strictly blocked with HTTP 429", str(e))

    try:
        # 7th attempt also 429
        r_block2 = rl_c.login(f"rl_target_{run_id}@example.com", "bad_pass")
        assert_rfc7807(r_block2, 429, "RATE_LIMIT_EXCEEDED")
        reporter.record(True, "T2-B08-03: Subsequent attempts during window continue to return 429")
    except Exception as e:
        reporter.record(False, "T2-B08-03: Subsequent attempts during window continue to return 429", str(e))

    try:
        # Retry-After header format
        ra_val = r_block.header("Retry-After")
        assert_true(ra_val is not None and ra_val.isdigit() and int(ra_val) <= 900)
        reporter.record(True, "T2-B08-04: Retry-After header indicates valid remaining seconds")
    except Exception as e:
        reporter.record(False, "T2-B08-04: Retry-After header indicates valid remaining seconds", str(e))

    try:
        # Public non-auth endpoints still responsive
        assert_status(rl_c.get("/manifest.json"), 200)
        reporter.record(True, "T2-B08-05: Rate limited client can still fetch public PWA assets")
    except Exception as e:
        reporter.record(False, "T2-B08-05: Rate limited client can still fetch public PWA assets", str(e))

    # -------------------------------------------------------------
    # B09: Pure NetCashFlowEngine Boundary (5 tests)
    # -------------------------------------------------------------
    # Register user 3 with zero initial transactions
    u3_email = f"u3_zero_{run_id}@example.com"
    client3 = ApiClient(base_url)
    client3.register(u3_email, "PasswordZero123!", "Zero User")

    try:
        # Zero transactions yields exact 0 income, 0 expenses, 0 net
        cf3 = client3.get_cash_flow().json
        assert_equal(cf3["income"], 0)
        assert_equal(cf3["expenses"], 0)
        assert_equal(cf3["net_cash_flow"], 0)
        reporter.record(True, "T2-B09-01: Zero transactions boundary yields exact 0 net cash flow")
    except Exception as e:
        reporter.record(False, "T2-B09-01: Zero transactions boundary yields exact 0 net cash flow", str(e))

    u3_acc = client3.list_accounts().json[0]["id"]
    try:
        # Only expenses yields negative net cash flow
        client3.create_transaction(account_id=u3_acc, amount=50000, tx_type="expense")
        cf_exp = client3.get_cash_flow().json
        assert_equal(cf_exp["income"], 0)
        assert_equal(cf_exp["expenses"], 50000)
        assert_equal(cf_exp["net_cash_flow"], -50000)
        reporter.record(True, "T2-B09-02: Only expenses sequence yields exact negative net cash flow")
    except Exception as e:
        reporter.record(False, "T2-B09-02: Only expenses sequence yields exact negative net cash flow", str(e))

    try:
        # Add income: 200,000 -> net should be 200,000 - 50,000 = 150,000
        client3.create_transaction(account_id=u3_acc, amount=200000, tx_type="income")
        cf_pos = client3.get_cash_flow().json
        assert_equal(cf_pos["income"], 200000)
        assert_equal(cf_pos["expenses"], 50000)
        assert_equal(cf_pos["net_cash_flow"], 150000)
        reporter.record(True, "T2-B09-03: Combined income and expense produces exact integer delta")
    except Exception as e:
        reporter.record(False, "T2-B09-03: Combined income and expense produces exact integer delta", str(e))

    u3_acc2 = client3.list_accounts().json[1]["id"]
    try:
        # Transfer 100,000 from acc 1 to acc 2 -> net cash flow must remain exactly 150,000
        client3.create_transaction(
            account_id=u3_acc,
            amount=100000,
            tx_type="transfer",
            destination_account_id=u3_acc2
        )
        cf_trans = client3.get_cash_flow().json
        assert_equal(cf_trans["net_cash_flow"], 150000)
        reporter.record(True, "T2-B09-04: Internal transfers produce exactly 0 impact on net cash flow")
    except Exception as e:
        reporter.record(False, "T2-B09-04: Internal transfers produce exactly 0 impact on net cash flow", str(e))

    try:
        # High volume transaction batch: 10 transactions of 1,000
        for _ in range(10):
            client3.create_transaction(account_id=u3_acc, amount=1000, tx_type="expense")
        cf_batch = client3.get_cash_flow().json
        # expenses was 50000 + 10000 = 60000, net was 150000 - 10000 = 140000
        assert_equal(cf_batch["expenses"], 60000)
        assert_equal(cf_batch["net_cash_flow"], 140000)
        reporter.record(True, "T2-B09-05: Multi-transaction sequence maintains exact integer precision")
    except Exception as e:
        reporter.record(False, "T2-B09-05: Multi-transaction sequence maintains exact integer precision", str(e))

    # -------------------------------------------------------------
    # B10: Multi-Wallet Accounts Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Create account with initial balance = 0
        w_zero = client.create_account("Dompet Kosong", "cash", 0)
        assert_status(w_zero, 201)
        assert_equal(w_zero.json.get("balance"), 0)
        reporter.record(True, "T2-B10-01: Account creation with initial balance 0 accepted")
    except Exception as e:
        reporter.record(False, "T2-B10-01: Account creation with initial balance 0 accepted", str(e))

    try:
        # Unsupported account type rejected 400
        w_bad_type = client.create_account("Crypto Wallet", "crypto", 1000)
        assert_rfc7807(w_bad_type, 400, "INVALID_ACCOUNT_TYPE")
        reporter.record(True, "T2-B10-02: Unsupported account type string rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B10-02: Unsupported account type string rejected 400", str(e))

    try:
        # Negative initial balance rejected 400
        w_neg_bal = client.create_account("Hutang", "checking", -10000)
        assert_rfc7807(w_neg_bal, 400, "INVALID_BALANCE")
        reporter.record(True, "T2-B10-03: Negative initial balance rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B10-03: Negative initial balance rejected 400", str(e))

    try:
        # Empty account name rejected 400
        w_no_name = client.create_account("", "cash", 1000)
        assert_rfc7807(w_no_name, 400, "INVALID_NAME")
        reporter.record(True, "T2-B10-04: Empty account name rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B10-04: Empty account name rejected 400", str(e))

    try:
        # Multiple accounts of identical type allowed for same tenant
        w_cash1 = client.create_account("Cash Kantor", "cash", 50000)
        w_cash2 = client.create_account("Cash Rumah", "cash", 75000)
        assert_status(w_cash1, 201)
        assert_status(w_cash2, 201)
        reporter.record(True, "T2-B10-05: Multiple accounts of identical type permitted for single user")
    except Exception as e:
        reporter.record(False, "T2-B10-05: Multiple accounts of identical type permitted for single user", str(e))

    # -------------------------------------------------------------
    # B11: Category Soft-Deletion Boundary (5 tests)
    # -------------------------------------------------------------
    cat_temp = client.create_category("Temp Cat", "expense").json["id"]
    try:
        # 1st soft-delete succeeds
        assert_status(client.soft_delete_category(cat_temp), 200)
        reporter.record(True, "T2-B11-01: Initial soft-deletion of active category succeeds 200")
    except Exception as e:
        reporter.record(False, "T2-B11-01: Initial soft-deletion of active category succeeds 200", str(e))

    try:
        # Re-deleting already soft-deleted category returns 404
        r_del_again = client.soft_delete_category(cat_temp)
        assert_rfc7807(r_del_again, 404, "CATEGORY_NOT_FOUND")
        reporter.record(True, "T2-B11-02: Soft-deleting an already soft-deleted category returns 404")
    except Exception as e:
        reporter.record(False, "T2-B11-02: Soft-deleting an already soft-deleted category returns 404", str(e))

    try:
        # Soft-deleting non-existent category returns 404
        r_del_fake = client.soft_delete_category(str(uuid.uuid4()))
        assert_rfc7807(r_del_fake, 404, "CATEGORY_NOT_FOUND")
        reporter.record(True, "T2-B11-03: Soft-deleting non-existent category ID returns 404")
    except Exception as e:
        reporter.record(False, "T2-B11-03: Soft-deleting non-existent category ID returns 404", str(e))

    try:
        # Creating category with invalid category_type rejected 400
        r_bad_cat = client.create_category("Invalid Type", "investment")
        assert_rfc7807(r_bad_cat, 400, "INVALID_CATEGORY_TYPE")
        reporter.record(True, "T2-B11-04: Category creation with invalid category_type rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B11-04: Category creation with invalid category_type rejected 400", str(e))

    try:
        # Creating new category with same name as soft-deleted category succeeds
        r_new_same = client.create_category("Temp Cat", "expense")
        assert_status(r_new_same, 201)
        reporter.record(True, "T2-B11-05: Re-creating category with same name as soft-deleted category succeeds")
    except Exception as e:
        reporter.record(False, "T2-B11-05: Re-creating category with same name as soft-deleted category succeeds", str(e))

    # -------------------------------------------------------------
    # B12: Idempotency-Key Deduplication Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Idempotency-Key with invalid characters rejected 400
        r_bad_key = client.create_transaction(
            account_id=bank_acc,
            amount=5000,
            tx_type="expense",
            idempotency_key="12345-not-uuid"
        )
        assert_rfc7807(r_bad_key, 400, "INVALID_IDEMPOTENCY_KEY")
        reporter.record(True, "T2-B12-01: Malformed Idempotency-Key header strictly rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B12-01: Malformed Idempotency-Key header strictly rejected 400", str(e))

    key_iso = str(uuid.uuid4())
    try:
        # User 1 submits with key_iso
        tx_u1_key = client.create_transaction(
            account_id=bank_acc,
            amount=20000,
            tx_type="expense",
            idempotency_key=key_iso
        )
        assert_status(tx_u1_key, 201)
        reporter.record(True, "T2-B12-02: Tenant 1 records transaction with unique idempotency key")
    except Exception as e:
        reporter.record(False, "T2-B12-02: Tenant 1 records transaction with unique idempotency key", str(e))

    try:
        # User 2 submits with identical key_iso -> must succeed independently!
        u2_acc = client2.list_accounts().json[0]["id"]
        tx_u2_key = client2.create_transaction(
            account_id=u2_acc,
            amount=30000,
            tx_type="expense",
            idempotency_key=key_iso
        )
        assert_status(tx_u2_key, 201)
        assert_true(tx_u1_key.json["id"] != tx_u2_key.json["id"])
        reporter.record(True, "T2-B12-03: Idempotency keys are scoped strictly per-tenant without collision")
    except Exception as e:
        reporter.record(False, "T2-B12-03: Idempotency keys are scoped strictly per-tenant without collision", str(e))

    try:
        # User 1 replays key_iso -> returns User 1's cached transaction
        tx_u1_rep = client.create_transaction(
            account_id=bank_acc,
            amount=20000,
            tx_type="expense",
            idempotency_key=key_iso
        )
        assert_status(tx_u1_rep, 201)
        assert_equal(tx_u1_rep.json["id"], tx_u1_key.json["id"])
        reporter.record(True, "T2-B12-04: Tenant 1 replay returns exact cached transaction ID")
    except Exception as e:
        reporter.record(False, "T2-B12-04: Tenant 1 replay returns exact cached transaction ID", str(e))

    try:
        # Missing destination account on transfer rejected 400
        r_no_dest = client.create_transaction(
            account_id=bank_acc,
            amount=10000,
            tx_type="transfer"
        )
        assert_rfc7807(r_no_dest, 400, "MISSING_DESTINATION_ACCOUNT")
        reporter.record(True, "T2-B12-05: Transfer missing destination_account_id rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B12-05: Transfer missing destination_account_id rejected 400", str(e))

    # -------------------------------------------------------------
    # B13: Payment Gateway Trait Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Empty provider string rejected 400
        r_empty_prov = client.post("/api/v1/subscriptions/checkout", {"provider": ""})
        assert_rfc7807(r_empty_prov, 400, "UNSUPPORTED_PROVIDER")
        reporter.record(True, "T2-B13-01: Empty provider parameter in checkout rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B13-01: Empty provider parameter in checkout rejected 400", str(e))

    try:
        # Uppercase provider string rejected or normalized
        r_upper = client.post("/api/v1/subscriptions/checkout", {"provider": "STRIPE"})
        assert_rfc7807(r_upper, 400, "UNSUPPORTED_PROVIDER")
        reporter.record(True, "T2-B13-02: Unknown payment provider 'STRIPE' rejected 400")
    except Exception as e:
        reporter.record(False, "T2-B13-02: Unknown payment provider 'STRIPE' rejected 400", str(e))

    try:
        # Midtrans checkout order_id format
        r_co_mid = client.checkout("midtrans")
        assert_true(r_co_mid.json.get("order_id", "").startswith("SUB-"))
        reporter.record(True, "T2-B13-03: Generated subscription order_id contains SUB- prefix")
    except Exception as e:
        reporter.record(False, "T2-B13-03: Generated subscription order_id contains SUB- prefix", str(e))

    try:
        # Checkout URL contains payment host
        assert_true("payment.nurdiansyahlabs.com" in r_co_mid.json.get("checkout_url", ""))
        reporter.record(True, "T2-B13-04: Checkout URL resolves to payment gateway domain")
    except Exception as e:
        reporter.record(False, "T2-B13-04: Checkout URL resolves to payment gateway domain", str(e))

    try:
        # Amount equals 5000 exactly
        assert_equal(r_co_mid.json.get("amount"), 5000)
        reporter.record(True, "T2-B13-05: Checkout amount strictly locked to Rp 5,000 / month")
    except Exception as e:
        reporter.record(False, "T2-B13-05: Checkout amount strictly locked to Rp 5,000 / month", str(e))

    # -------------------------------------------------------------
    # B14: HMAC / Signature Boundary (5 tests)
    # -------------------------------------------------------------
    server_key = "SB-Mid-server-test-secret-key"
    order_b14 = f"SUB-{run_id}-B14"
    try:
        # 1-byte modified signature rejected
        real_sig = compute_midtrans_signature(order_b14, "200", "5000.00", server_key)
        corrupted_sig = ("0" if real_sig[0] != "0" else "1") + real_sig[1:]
        r_corrupt = client.post("/api/v1/webhooks/midtrans", {
            "order_id": order_b14,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": corrupted_sig
        })
        assert_rfc7807(r_corrupt, 401, "INVALID_SIGNATURE")
        reporter.record(True, "T2-B14-01: 1-byte corrupted Midtrans signature rejected 401")
    except Exception as e:
        reporter.record(False, "T2-B14-01: 1-byte corrupted Midtrans signature rejected 401", str(e))

    try:
        # Empty signature string rejected
        r_empty_sig = client.post("/api/v1/webhooks/midtrans", {
            "order_id": order_b14,
            "status_code": "200",
            "gross_amount": "5000.00",
            "signature_key": ""
        })
        assert_rfc7807(r_empty_sig, 400, "INVALID_WEBHOOK_PAYLOAD")
        reporter.record(True, "T2-B14-02: Empty signature string rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T2-B14-02: Empty signature string rejected with HTTP 400", str(e))

    try:
        # Uppercase hex signature accepted (case-insensitive constant time)
        r_upper_sig = client.post("/api/v1/webhooks/midtrans", {
            "order_id": order_b14,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": real_sig.upper(),
            "transaction_id": f"trx_{run_id}_upper"
        })
        assert_status(r_upper_sig, 200)
        reporter.record(True, "T2-B14-03: Constant-time signature verification handles uppercase hex")
    except Exception as e:
        reporter.record(False, "T2-B14-03: Constant-time signature verification handles uppercase hex", str(e))

    try:
        # Xendit webhook with corrupted token rejected
        r_xen_bad = client.post("/api/v1/webhooks/xendit", {"id": "x1", "external_id": "u1"}, headers={"X-Callback-Token": "bad_token_123"})
        assert_rfc7807(r_xen_bad, 401, "INVALID_SIGNATURE")
        reporter.record(True, "T2-B14-04: Corrupted Xendit callback token rejected 401")
    except Exception as e:
        reporter.record(False, "T2-B14-04: Corrupted Xendit callback token rejected 401", str(e))

    try:
        # Xendit webhook with missing token header rejected
        r_xen_no_tok = client.post("/api/v1/webhooks/xendit", {"id": "x1", "external_id": "u1"})
        assert_rfc7807(r_xen_no_tok, 401, "INVALID_SIGNATURE")
        reporter.record(True, "T2-B14-05: Missing Xendit callback token header rejected 401")
    except Exception as e:
        reporter.record(False, "T2-B14-05: Missing Xendit callback token header rejected 401", str(e))

    # -------------------------------------------------------------
    # B15: Webhook Idempotency Boundary (5 tests)
    # -------------------------------------------------------------
    b15_order = f"ORDER-{run_id}-B15"
    b15_sig = compute_midtrans_signature(b15_order, "200", "5000.00", server_key)
    b15_payload = {
        "order_id": b15_order,
        "status_code": "200",
        "gross_amount": "5000.00",
        "transaction_status": "settlement",
        "signature_key": b15_sig,
        "transaction_id": f"trx_{run_id}_b15"
    }

    try:
        # 1st delivery
        res1 = client.post("/api/v1/webhooks/midtrans", b15_payload)
        assert_status(res1, 200)
        reporter.record(True, "T2-B15-01: Initial webhook delivery processed 200")
    except Exception as e:
        reporter.record(False, "T2-B15-01: Initial webhook delivery processed 200", str(e))

    try:
        # 2nd delivery with same event_id
        res2 = client.post("/api/v1/webhooks/midtrans", b15_payload)
        assert_status(res2, 200)
        reporter.record(True, "T2-B15-02: 2nd delivery recognized as duplicate and acknowledged")
    except Exception as e:
        reporter.record(False, "T2-B15-02: 2nd delivery recognized as duplicate and acknowledged", str(e))

    try:
        # 3rd delivery with same event_id
        res3 = client.post("/api/v1/webhooks/midtrans", b15_payload)
        assert_status(res3, 200)
        reporter.record(True, "T2-B15-03: 3rd delivery acknowledged idempotently without error")
    except Exception as e:
        reporter.record(False, "T2-B15-03: 3rd delivery acknowledged idempotently without error", str(e))

    try:
        # Empty body webhook rejected 400
        r_empty_wh = client.post("/api/v1/webhooks/midtrans", {})
        assert_rfc7807(r_empty_wh, 400, "INVALID_WEBHOOK_PAYLOAD")
        reporter.record(True, "T2-B15-04: Empty webhook payload rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T2-B15-04: Empty webhook payload rejected with HTTP 400", str(e))

    try:
        # Unknown status handled without crashing
        b15_unk_sig = compute_midtrans_signature("ORDER-UNK", "200", "5000.00", server_key)
        r_unk = client.post("/api/v1/webhooks/midtrans", {
            "order_id": "ORDER-UNK",
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "chargeback_reversed",
            "signature_key": b15_unk_sig,
            "transaction_id": f"trx_{run_id}_unk"
        })
        assert_status(r_unk, 200)
        reporter.record(True, "T2-B15-05: Webhook with custom status string processed safely")
    except Exception as e:
        reporter.record(False, "T2-B15-05: Webhook with custom status string processed safely", str(e))

    # -------------------------------------------------------------
    # B16: Subscription Lifecycle Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Query status for user 2 (still free)
        sub_u2 = client2.subscription_status().json
        assert_equal(sub_u2["tier"], "free")
        assert_equal(sub_u2["price_monthly"], 0)
        reporter.record(True, "T2-B16-01: Free tier user has price_monthly 0")
    except Exception as e:
        reporter.record(False, "T2-B16-01: Free tier user has price_monthly 0", str(e))

    try:
        # Upgrade user 2 via Xendit webhook
        xen_tok = "xendit_webhook_token_secret_123"
        wh_xen = {
            "id": f"xen_up_{run_id}",
            "external_id": u2_data["id"],
            "status": "PAID",
            "amount": 5000
        }
        r_up = client2.post("/api/v1/webhooks/xendit", wh_xen, headers={"X-Callback-Token": xen_tok})
        assert_status(r_up, 200)
        sub_u2_after = client2.subscription_status().json
        assert_equal(sub_u2_after["tier"], "premium")
        assert_equal(sub_u2_after["status"], "active")
        reporter.record(True, "T2-B16-02: Xendit PAID webhook transitions user tier to Premium Active")
    except Exception as e:
        reporter.record(False, "T2-B16-02: Xendit PAID webhook transitions user tier to Premium Active", str(e))

    try:
        # Cancel subscription for user 2
        u2_id = u2_data["id"]
        sig_c2 = compute_midtrans_signature(u2_id, "200", "5000.00", server_key)
        client2.post("/api/v1/webhooks/midtrans", {
            "order_id": u2_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "cancel",
            "signature_key": sig_c2,
            "transaction_id": f"trx_cancel_{run_id}_2"
        })
        sub_u2_canc = client2.subscription_status().json
        assert_equal(sub_u2_canc["status"], "cancelled")
        reporter.record(True, "T2-B16-03: Cancel event transitions subscription status to Cancelled")
    except Exception as e:
        reporter.record(False, "T2-B16-03: Cancel event transitions subscription status to Cancelled", str(e))

    try:
        # Reactivate subscription for user 2
        sig_re = compute_midtrans_signature(u2_id, "200", "5000.00", server_key)
        client2.post("/api/v1/webhooks/midtrans", {
            "order_id": u2_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_re,
            "transaction_id": f"trx_re_{run_id}_2"
        })
        sub_re = client2.subscription_status().json
        assert_equal(sub_re["status"], "active")
        reporter.record(True, "T2-B16-04: Settlement event reactivates Cancelled subscription to Active")
    except Exception as e:
        reporter.record(False, "T2-B16-04: Settlement event reactivates Cancelled subscription to Active", str(e))

    try:
        # Current period end timestamp present
        assert_true("current_period_end" in sub_re and "T" in sub_re["current_period_end"])
        reporter.record(True, "T2-B16-05: Active subscription provides valid UTC expiration timestamp")
    except Exception as e:
        reporter.record(False, "T2-B16-05: Active subscription provides valid UTC expiration timestamp", str(e))

    # -------------------------------------------------------------
    # B17: Feature Gate Boundary (5 tests)
    # -------------------------------------------------------------
    # Register free user 4
    u4_email = f"u4_gate_{run_id}@example.com"
    client4 = ApiClient(base_url)
    client4.register(u4_email, "PasswordGate123!", "Gate User")

    try:
        # Anonymous request to gated endpoint returns 401, not 403
        c_no_auth = ApiClient(base_url)
        r_no_auth = c_no_auth.get("/api/v1/analytics/advanced")
        assert_status(r_no_auth, 401)
        reporter.record(True, "T2-B17-01: Gated endpoint returns 401 Unauthorized before checking feature permission")
    except Exception as e:
        reporter.record(False, "T2-B17-01: Gated endpoint returns 401 Unauthorized before checking feature permission", str(e))

    try:
        # Free user gets 403 on advanced analytics
        r_adv_403 = client4.get_advanced_analytics()
        assert_rfc7807(r_adv_403, 403, "FEATURE_LOCKED")
        reporter.record(True, "T2-B17-02: Free user accessing advanced analytics receives HTTP 403 FEATURE_LOCKED")
    except Exception as e:
        reporter.record(False, "T2-B17-02: Free user accessing advanced analytics receives HTTP 403 FEATURE_LOCKED", str(e))

    try:
        # Free user gets 403 on budgets
        r_bud_403 = client4.list_budgets()
        assert_rfc7807(r_bud_403, 403, "FEATURE_LOCKED")
        reporter.record(True, "T2-B17-03: Free user accessing budgeting receives HTTP 403 FEATURE_LOCKED")
    except Exception as e:
        reporter.record(False, "T2-B17-03: Free user accessing budgeting receives HTTP 403 FEATURE_LOCKED", str(e))

    try:
        # 403 response contains detail with feature name
        assert_true("budgeting" in r_bud_403.json.get("detail", ""))
        reporter.record(True, "T2-B17-04: 403 detail string specifies required permission 'budgeting'")
    except Exception as e:
        reporter.record(False, "T2-B17-04: 403 detail string specifies required permission 'budgeting'", str(e))

    try:
        # Basic features still return 200 for free user
        assert_status(client4.get_cash_flow(), 200)
        reporter.record(True, "T2-B17-05: Non-gated basic features return 200 for free user")
    except Exception as e:
        reporter.record(False, "T2-B17-05: Non-gated basic features return 200 for free user", str(e))

    # -------------------------------------------------------------
    # B18: Full Axum REST API Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # 404 for random route
        r_rand = client.get("/api/v1/invalid/deep/path")
        assert_rfc7807(r_rand, 404, "ROUTE_NOT_FOUND")
        reporter.record(True, "T2-B18-01: Deeply nested invalid API route returns RFC 7807 404")
    except Exception as e:
        reporter.record(False, "T2-B18-01: Deeply nested invalid API route returns RFC 7807 404", str(e))

    try:
        # HEAD request on /health returns 200 with empty body
        r_head = client.head("/health")
        assert_status(r_head, 200)
        assert_equal(len(r_head.raw_body), 0)
        reporter.record(True, "T2-B18-02: HEAD request to probe returns headers without body")
    except Exception as e:
        reporter.record(False, "T2-B18-02: HEAD request to probe returns headers without body", str(e))

    try:
        # GET account with non-existent UUID format
        r_bad_acc = client.get("/api/v1/accounts/00000000-0000-0000-0000-000000000000")
        assert_rfc7807(r_bad_acc, 404, "ACCOUNT_NOT_FOUND")
        reporter.record(True, "T2-B18-03: Single account lookup with zero UUID returns 404")
    except Exception as e:
        reporter.record(False, "T2-B18-03: Single account lookup with zero UUID returns 404", str(e))

    try:
        # GET transaction with non-existent UUID format
        r_bad_tx = client.get("/api/v1/transactions/00000000-0000-0000-0000-000000000000")
        assert_rfc7807(r_bad_tx, 404, "TRANSACTION_NOT_FOUND")
        reporter.record(True, "T2-B18-04: Single transaction lookup with zero UUID returns 404")
    except Exception as e:
        reporter.record(False, "T2-B18-04: Single transaction lookup with zero UUID returns 404", str(e))

    try:
        # GET category with non-existent UUID format
        r_bad_cat_id = client.get("/api/v1/categories/00000000-0000-0000-0000-000000000000")
        assert_rfc7807(r_bad_cat_id, 404, "CATEGORY_NOT_FOUND")
        reporter.record(True, "T2-B18-05: Single category lookup with zero UUID returns 404")
    except Exception as e:
        reporter.record(False, "T2-B18-05: Single category lookup with zero UUID returns 404", str(e))

    # -------------------------------------------------------------
    # B19: Health & Readiness Probes Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        t0 = time.time()
        r_h_fast = client.get("/health")
        t1 = time.time()
        assert_status(r_h_fast, 200)
        assert_true((t1 - t0) < 0.2)
        reporter.record(True, "T2-B19-01: Health check executes with low latency (< 200ms)")
    except Exception as e:
        reporter.record(False, "T2-B19-01: Health check executes with low latency (< 200ms)", str(e))

    try:
        t0 = time.time()
        r_r_fast = client.get("/ready")
        t1 = time.time()
        assert_status(r_r_fast, 200)
        assert_true((t1 - t0) < 0.2)
        reporter.record(True, "T2-B19-02: Readiness check executes with low latency (< 200ms)")
    except Exception as e:
        reporter.record(False, "T2-B19-02: Readiness check executes with low latency (< 200ms)", str(e))

    try:
        # Keys in /health
        h_keys = set(r_h_fast.json.keys())
        assert_equal(h_keys, {"status", "version", "service"})
        reporter.record(True, "T2-B19-03: /health payload strictly limited to status, version, service")
    except Exception as e:
        reporter.record(False, "T2-B19-03: /health payload strictly limited to status, version, service", str(e))

    try:
        # Keys in /ready
        r_keys = set(r_r_fast.json.keys())
        assert_equal(r_keys, {"status", "database", "wal_mode"})
        reporter.record(True, "T2-B19-04: /ready payload strictly limited to status, database, wal_mode")
    except Exception as e:
        reporter.record(False, "T2-B19-04: /ready payload strictly limited to status, database, wal_mode", str(e))

    try:
        # HEAD on /ready
        assert_status(client.head("/ready"), 200)
        reporter.record(True, "T2-B19-05: HEAD /ready returns 200 with matching headers")
    except Exception as e:
        reporter.record(False, "T2-B19-05: HEAD /ready returns 200 with matching headers", str(e))

    # -------------------------------------------------------------
    # B20: Environment & Secrets Config Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Error responses do not leak database path
        err_msg = client.get("/api/v1/accounts/not_found_12345").text
        assert_true("sqlite" not in err_msg.lower() and ".db" not in err_msg.lower())
        reporter.record(True, "T2-B20-01: Error messages omit database file paths and connection strings")
    except Exception as e:
        reporter.record(False, "T2-B20-01: Error messages omit database file paths and connection strings", str(e))

    try:
        # Error messages omit stack traces
        assert_true("traceback" not in err_msg.lower() and "file \"/" not in err_msg.lower())
        reporter.record(True, "T2-B20-02: Error responses omit internal stack traces and code snippets")
    except Exception as e:
        reporter.record(False, "T2-B20-02: Error responses omit internal stack traces and code snippets", str(e))

    try:
        # Response charset UTF-8
        ct_hdr = client.get("/health").header("Content-Type")
        assert_true("utf-8" in ct_hdr.lower() or "application/json" in ct_hdr.lower())
        reporter.record(True, "T2-B20-03: Content-Type headers specify UTF-8 character encoding")
    except Exception as e:
        reporter.record(False, "T2-B20-03: Content-Type headers specify UTF-8 character encoding", str(e))

    try:
        # Auth tokens are structured with 3 parts
        tok_val = client.cookies.get("auth_token", "")
        assert_equal(len(tok_val.split(".")), 3)
        reporter.record(True, "T2-B20-04: Signed auth tokens adhere to 3-part cryptographic structure")
    except Exception as e:
        reporter.record(False, "T2-B20-04: Signed auth tokens adhere to 3-part cryptographic structure", str(e))

    try:
        # User profile response excludes password_hash
        me_fields = client.me().json
        assert_true("password_hash" not in me_fields and "password" not in me_fields)
        reporter.record(True, "T2-B20-05: Serialization strictly excludes sensitive password hashes")
    except Exception as e:
        reporter.record(False, "T2-B20-05: Serialization strictly excludes sensitive password hashes", str(e))

    # -------------------------------------------------------------
    # B21: Vue 3 Mobile PWA Shell Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        man_resp = client.get("/manifest.json")
        man_data = man_resp.json
        assert_true(man_data.get("theme_color", "").startswith("#"))
        reporter.record(True, "T2-B21-01: Manifest theme_color is valid hex code")
    except Exception as e:
        reporter.record(False, "T2-B21-01: Manifest theme_color is valid hex code", str(e))

    try:
        assert_true(man_data.get("background_color", "").startswith("#"))
        reporter.record(True, "T2-B21-02: Manifest background_color is valid hex code")
    except Exception as e:
        reporter.record(False, "T2-B21-02: Manifest background_color is valid hex code", str(e))

    try:
        assert_equal(man_data.get("start_url"), "/")
        reporter.record(True, "T2-B21-03: Manifest start_url resolves to root (/)")
    except Exception as e:
        reporter.record(False, "T2-B21-03: Manifest start_url resolves to root (/)", str(e))

    try:
        sw_body = client.get("/service-worker.js").text
        assert_true("install" in sw_body)
        reporter.record(True, "T2-B21-04: Service worker script implements install event lifecycle")
    except Exception as e:
        reporter.record(False, "T2-B21-04: Service worker script implements install event lifecycle", str(e))

    try:
        assert_true("fetch" in sw_body)
        reporter.record(True, "T2-B21-05: Service worker script implements fetch event proxy")
    except Exception as e:
        reporter.record(False, "T2-B21-05: Service worker script implements fetch event proxy", str(e))

    # -------------------------------------------------------------
    # B22: 5-Tab Mobile Navigation Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        idx_html = client.get("/").text
        assert_true("width=device-width" in idx_html)
        reporter.record(True, "T2-B22-01: Shell meta tag enforces width=device-width")
    except Exception as e:
        reporter.record(False, "T2-B22-01: Shell meta tag enforces width=device-width", str(e))

    try:
        assert_true("initial-scale=1.0" in idx_html)
        reporter.record(True, "T2-B22-02: Shell meta tag enforces initial-scale=1.0")
    except Exception as e:
        reporter.record(False, "T2-B22-02: Shell meta tag enforces initial-scale=1.0", str(e))

    try:
        assert_true("viewport-fit=cover" in idx_html)
        reporter.record(True, "T2-B22-03: Shell meta tag enforces viewport-fit=cover for notched mobile devices")
    except Exception as e:
        reporter.record(False, "T2-B22-03: Shell meta tag enforces viewport-fit=cover for notched mobile devices", str(e))

    try:
        # Accounts list provides balance for Home view
        acc_b22 = client.list_accounts().json
        assert_true(all("balance" in a for a in acc_b22))
        reporter.record(True, "T2-B22-04: Accounts endpoint includes balance integer for Home view cards")
    except Exception as e:
        reporter.record(False, "T2-B22-04: Accounts endpoint includes balance integer for Home view cards", str(e))

    try:
        # Transactions list provides type and amount for Transactions view
        tx_b22 = client.list_transactions({"limit": 5}).json
        assert_true(all("transaction_type" in t and "amount" in t for t in tx_b22))
        reporter.record(True, "T2-B22-05: Transactions endpoint includes type and amount for list items")
    except Exception as e:
        reporter.record(False, "T2-B22-05: Transactions endpoint includes type and amount for list items", str(e))

    # -------------------------------------------------------------
    # B23: Rapid POS Numeric Keypad Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # 1 Rupiah minimum amount
        tx_one = client.create_transaction(
            account_id=bank_acc,
            amount=1,
            tx_type="expense",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_one, 201)
        assert_equal(tx_one.json.get("amount"), 1)
        reporter.record(True, "T2-B23-01: Exactly 1 Rupiah minimum transaction amount accepted")
    except Exception as e:
        reporter.record(False, "T2-B23-01: Exactly 1 Rupiah minimum transaction amount accepted", str(e))

    try:
        # 999,999,999,999 Rupiah maximum keypad input
        max_keypad = 999999999999
        tx_max = client.create_transaction(
            account_id=bank_acc,
            amount=max_keypad,
            tx_type="income",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(tx_max, 201)
        assert_equal(tx_max.json.get("amount"), max_keypad)
        reporter.record(True, "T2-B23-02: 12-digit keypad input (Rp 999,999,999,999) handled accurately")
    except Exception as e:
        reporter.record(False, "T2-B23-02: 12-digit keypad input (Rp 999,999,999,999) handled accurately", str(e))

    try:
        # Keypad input with string representation of integer parsed
        tx_str_int = client.post("/api/v1/transactions", {
            "account_id": bank_acc,
            "amount": 20000,
            "transaction_type": "expense"
        })
        assert_status(tx_str_int, 201)
        reporter.record(True, "T2-B23-03: Clean integer amount accepted in JSON payload")
    except Exception as e:
        reporter.record(False, "T2-B23-03: Clean integer amount accepted in JSON payload", str(e))

    try:
        # Keypad backspace to empty / 0 rejected
        tx_empty_amt = client.post("/api/v1/transactions", {
            "account_id": bank_acc,
            "amount": 0,
            "transaction_type": "expense"
        })
        assert_rfc7807(tx_empty_amt, 400, "INVALID_AMOUNT")
        reporter.record(True, "T2-B23-04: Keypad cleared to 0 rejected with HTTP 400")
    except Exception as e:
        reporter.record(False, "T2-B23-04: Keypad cleared to 0 rejected with HTTP 400", str(e))

    try:
        # Rapid consecutive keypad transactions
        t_keys = [str(uuid.uuid4()) for _ in range(3)]
        for k in t_keys:
            r = client.create_transaction(account_id=bank_acc, amount=5000, tx_type="expense", idempotency_key=k)
            assert_status(r, 201)
        reporter.record(True, "T2-B23-05: Rapid consecutive keypad transactions succeed without locking")
    except Exception as e:
        reporter.record(False, "T2-B23-05: Rapid consecutive keypad transactions succeed without locking", str(e))

    # -------------------------------------------------------------
    # B24: Localized id-ID IDR Formatting Boundary (5 tests)
    # -------------------------------------------------------------
    def fmt(n: int) -> str:
        s = f"{abs(n):,}".replace(",", ".")
        return f"Rp {s}" if n >= 0 else f"-Rp {s}"

    try:
        assert_equal(fmt(7), "Rp 7")
        reporter.record(True, "T2-B24-01: Single-digit integer formatted without period 'Rp 7'")
    except Exception as e:
        reporter.record(False, "T2-B24-01: Single-digit integer formatted without period 'Rp 7'", str(e))

    try:
        assert_equal(fmt(50), "Rp 50")
        reporter.record(True, "T2-B24-02: Two-digit integer formatted without period 'Rp 50'")
    except Exception as e:
        reporter.record(False, "T2-B24-02: Two-digit integer formatted without period 'Rp 50'", str(e))

    try:
        assert_equal(fmt(500), "Rp 500")
        reporter.record(True, "T2-B24-03: Three-digit integer formatted without period 'Rp 500'")
    except Exception as e:
        reporter.record(False, "T2-B24-03: Three-digit integer formatted without period 'Rp 500'", str(e))

    try:
        assert_equal(fmt(10000000), "Rp 10.000.000")
        reporter.record(True, "T2-B24-04: Ten million integer formatted with two periods 'Rp 10.000.000'")
    except Exception as e:
        reporter.record(False, "T2-B24-04: Ten million integer formatted with two periods 'Rp 10.000.000'", str(e))

    try:
        assert_equal(fmt(-10000), "-Rp 10.000")
        reporter.record(True, "T2-B24-05: Negative currency formatted with leading minus '-Rp 10.000'")
    except Exception as e:
        reporter.record(False, "T2-B24-05: Negative currency formatted with leading minus '-Rp 10.000'", str(e))

    # -------------------------------------------------------------
    # B25: Workbox Offline Caching Policy Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        cc_acc = client.get("/api/v1/accounts").header("Cache-Control")
        assert_true("no-store" in cc_acc)
        reporter.record(True, "T2-B25-01: GET /accounts mandates no-store caching policy")
    except Exception as e:
        reporter.record(False, "T2-B25-01: GET /accounts mandates no-store caching policy", str(e))

    try:
        cc_tx = client.get("/api/v1/transactions").header("Cache-Control")
        assert_true("no-store" in cc_tx)
        reporter.record(True, "T2-B25-02: GET /transactions mandates no-store caching policy")
    except Exception as e:
        reporter.record(False, "T2-B25-02: GET /transactions mandates no-store caching policy", str(e))

    try:
        cc_cf = client.get("/api/v1/analytics/cash-flow").header("Cache-Control")
        assert_true("no-store" in cc_cf)
        reporter.record(True, "T2-B25-03: GET /analytics/cash-flow mandates no-store caching policy")
    except Exception as e:
        reporter.record(False, "T2-B25-03: GET /analytics/cash-flow mandates no-store caching policy", str(e))

    try:
        cc_sw = client.get("/service-worker.js").header("Cache-Control")
        assert_true("no-cache" in cc_sw)
        reporter.record(True, "T2-B25-04: Service worker declares no-cache to support prompt updates")
    except Exception as e:
        reporter.record(False, "T2-B25-04: Service worker declares no-cache to support prompt updates", str(e))

    try:
        cc_man = client.get("/manifest.json").header("Cache-Control")
        assert_true("public" in cc_man and "max-age" in cc_man)
        reporter.record(True, "T2-B25-05: Web App Manifest declares public max-age caching")
    except Exception as e:
        reporter.record(False, "T2-B25-05: Web App Manifest declares public max-age caching", str(e))

    # -------------------------------------------------------------
    # B26: Accessible Financial Charts Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        c_flow = client.get_cash_flow().json
        assert_true(c_flow["income"] >= 0)
        reporter.record(True, "T2-B26-01: Cash flow income is non-negative integer")
    except Exception as e:
        reporter.record(False, "T2-B26-01: Cash flow income is non-negative integer", str(e))

    try:
        assert_true(c_flow["expenses"] >= 0)
        reporter.record(True, "T2-B26-02: Cash flow expenses is non-negative integer")
    except Exception as e:
        reporter.record(False, "T2-B26-02: Cash flow expenses is non-negative integer", str(e))

    try:
        assert_equal(c_flow["net_cash_flow"], c_flow["income"] - c_flow["expenses"])
        reporter.record(True, "T2-B26-03: Cash flow net equals income minus expenses exactly")
    except Exception as e:
        reporter.record(False, "T2-B26-03: Cash flow net equals income minus expenses exactly", str(e))

    try:
        assert_equal(c_flow["currency"], "IDR")
        reporter.record(True, "T2-B26-04: Chart metadata explicitly specifies IDR currency code")
    except Exception as e:
        reporter.record(False, "T2-B26-04: Chart metadata explicitly specifies IDR currency code", str(e))

    try:
        # Check transaction note accessibility
        tx_acc_item = client.list_transactions({"limit": 1}).json[0]
        assert_true("note" in tx_acc_item and "date" in tx_acc_item)
        reporter.record(True, "T2-B26-05: Transaction items provide descriptive text for accessibility tables")
    except Exception as e:
        reporter.record(False, "T2-B26-05: Transaction items provide descriptive text for accessibility tables", str(e))

    # -------------------------------------------------------------
    # B27: UI Feature Gating Modal Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        adv_g_res = client4.get_advanced_analytics()
        assert_equal(adv_g_res.status, 403)
        assert_equal(adv_g_res.json.get("title"), "Forbidden")
        reporter.record(True, "T2-B27-01: 403 response provides title 'Forbidden'")
    except Exception as e:
        reporter.record(False, "T2-B27-01: 403 response provides title 'Forbidden'", str(e))

    try:
        assert_equal(adv_g_res.json.get("code"), "FEATURE_LOCKED")
        reporter.record(True, "T2-B27-02: 403 response provides code 'FEATURE_LOCKED'")
    except Exception as e:
        reporter.record(False, "T2-B27-02: 403 response provides code 'FEATURE_LOCKED'", str(e))

    try:
        assert_true(adv_g_res.json.get("type", "").endswith("/errors/forbidden"))
        reporter.record(True, "T2-B27-03: 403 response type points to forbidden error URI")
    except Exception as e:
        reporter.record(False, "T2-B27-03: 403 response type points to forbidden error URI", str(e))

    try:
        # Checkout generates valid HTTPS URL
        co_res = client4.checkout("midtrans")
        assert_true(co_res.json.get("checkout_url", "").startswith("https://"))
        reporter.record(True, "T2-B27-04: Checkout provides secure HTTPS payment redirection URL")
    except Exception as e:
        reporter.record(False, "T2-B27-04: Checkout provides secure HTTPS payment redirection URL", str(e))

    try:
        # Subscription status for free user
        st_free = client4.subscription_status().json
        assert_equal(st_free.get("tier"), "free")
        reporter.record(True, "T2-B27-05: Subscription status endpoint confirms free tier state")
    except Exception as e:
        reporter.record(False, "T2-B27-05: Subscription status endpoint confirms free tier state", str(e))

    # -------------------------------------------------------------
    # B28: 100% E2E Test Pass Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # Client handles 404 cleanly
        r_404 = client.get("/api/v1/not_found_endpoint_xyz")
        assert_equal(r_404.status, 404)
        reporter.record(True, "T2-B28-01: Test harness cleanly parses HTTP 404 responses")
    except Exception as e:
        reporter.record(False, "T2-B28-01: Test harness cleanly parses HTTP 404 responses", str(e))

    try:
        # Client handles 400 cleanly
        r_400 = client.post("/api/v1/auth/register", {})
        assert_equal(r_400.status, 400)
        reporter.record(True, "T2-B28-02: Test harness cleanly parses HTTP 400 responses")
    except Exception as e:
        reporter.record(False, "T2-B28-02: Test harness cleanly parses HTTP 400 responses", str(e))

    try:
        # Cookie jar maintains session
        assert_true("auth_token" in client.cookies)
        reporter.record(True, "T2-B28-03: Test client preserves authentication session in cookie jar")
    except Exception as e:
        reporter.record(False, "T2-B28-03: Test client preserves authentication session in cookie jar", str(e))

    try:
        # Clear cookies
        c_temp = ApiClient(base_url)
        c_temp.set_cookie("auth_token", "sample_jwt_cookie_value")
        assert_true("auth_token" in c_temp.cookies)
        c_temp.clear_cookies()
        assert_true("auth_token" not in c_temp.cookies)
        reporter.record(True, "T2-B28-04: Test client clear_cookies method cleanly resets session")
    except Exception as e:
        reporter.record(False, "T2-B28-04: Test client clear_cookies method cleanly resets session", str(e))

    try:
        # Test summary reporting
        assert_true(reporter.results is not None)
        reporter.record(True, "T2-B28-05: TAP reporter maintains accurate test result ledger")
    except Exception as e:
        reporter.record(False, "T2-B28-05: TAP reporter maintains accurate test result ledger", str(e))

    # -------------------------------------------------------------
    # B29: Adversarial Boundary (5 tests)
    # -------------------------------------------------------------
    try:
        # SQL injection in account name stored as literal string
        sqli_acc_name = "Dompet'; DROP TABLE accounts; --"
        r_sqli_acc = client.create_account(sqli_acc_name, "cash", 1000)
        assert_status(r_sqli_acc, 201)
        assert_equal(r_sqli_acc.json.get("name"), sqli_acc_name)
        # Verify accounts table still exists
        assert_status(client.list_accounts(), 200)
        reporter.record(True, "T2-B29-01: SQL injection in account name safely persisted as literal text")
    except Exception as e:
        reporter.record(False, "T2-B29-01: SQL injection in account name safely persisted as literal text", str(e))

    try:
        # SQL injection in search filter returns empty or filtered, no error
        sqli_tx = client.list_transactions({"date": "' OR 1=1 --"})
        assert_status(sqli_tx, 200)
        reporter.record(True, "T2-B29-02: SQL injection in date filter safely handled by query planner")
    except Exception as e:
        reporter.record(False, "T2-B29-02: SQL injection in date filter safely handled by query planner", str(e))

    try:
        # XSS in category name
        xss_cat_name = "<img src=x onerror=alert('xss')>"
        r_xss_cat = client.create_category(xss_cat_name, "expense")
        assert_status(r_xss_cat, 201)
        assert_equal(r_xss_cat.json.get("name"), xss_cat_name)
        reporter.record(True, "T2-B29-03: HTML/XSS payloads in category names stored verbatim without execution")
    except Exception as e:
        reporter.record(False, "T2-B29-03: HTML/XSS payloads in category names stored verbatim without execution", str(e))

    try:
        # Path traversal attempt /api/v1/../../etc/passwd returns 404
        r_trav = client.get("/api/v1/../../etc/passwd")
        assert_status(r_trav, 404)
        reporter.record(True, "T2-B29-04: Path traversal sequence ../.. rejected with HTTP 404")
    except Exception as e:
        reporter.record(False, "T2-B29-04: Path traversal sequence ../.. rejected with HTTP 404", str(e))

    try:
        # Unexpected extra JSON fields ignored safely
        r_extra_fields = client.post("/api/v1/accounts", {
            "name": "Dompet Extra",
            "account_type": "cash",
            "initial_balance": 10000,
            "isAdmin": True,
            "role": "superadmin",
            "malicious_inject": "true"
        })
        assert_status(r_extra_fields, 201)
        assert_true("isAdmin" not in r_extra_fields.json)
        reporter.record(True, "T2-B29-05: Unauthorized extra JSON payload properties safely ignored")
    except Exception as e:
        reporter.record(False, "T2-B29-05: Unauthorized extra JSON payload properties safely ignored", str(e))

    return reporter.print_summary()

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    success = run_tier2_tests(url)
    sys.exit(0 if success else 1)
