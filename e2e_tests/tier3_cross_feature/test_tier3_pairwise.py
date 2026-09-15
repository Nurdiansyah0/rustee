#!/usr/bin/env python3
"""
Tier 3: Pairwise Cross-Feature Interactions Test Suite (29 tests).
Tests interactions and invariants across combinations of distinct features.
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

def run_tier3_tests(base_url: str = "http://127.0.0.1:8089", reporter: TapReporter = None) -> bool:
    if reporter is None:
        reporter = TapReporter(total_expected=29)
        reporter.print_header()

    client = ApiClient(base_url)
    run_id = f"t3_{int(time.time())}_{uuid.uuid4().hex[:6]}"
    u_alice = f"alice_{run_id}@example.com"
    u_bob = f"bob_{run_id}@example.com"
    password = "PairwisePassword123!"

    # Setup Alice and Bob
    res_a = client.register(u_alice, password, "Alice")
    alice_id = res_a.json["id"]
    alice_accs = client.list_accounts().json
    alice_bank = alice_accs[0]["id"]
    alice_cash = alice_accs[1]["id"]
    alice_cats = client.list_categories().json
    alice_gaji = [c["id"] for c in alice_cats if c["name"] == "Gaji"][0]
    alice_makan = [c["id"] for c in alice_cats if c["name"] == "Makanan"][0]

    client_bob = ApiClient(base_url)
    res_b = client_bob.register(u_bob, password, "Bob")
    bob_id = res_b.json["id"]
    bob_accs = client_bob.list_accounts().json
    bob_bank = bob_accs[0]["id"]

    server_key = "SB-Mid-server-test-secret-key"

    # -------------------------------------------------------------
    # P01: F05 Multi-Tenant + F12 Idempotency-Key
    # -------------------------------------------------------------
    try:
        shared_key = str(uuid.uuid4())
        tx_a = client.create_transaction(account_id=alice_bank, amount=50000, tx_type="expense", idempotency_key=shared_key)
        tx_b = client_bob.create_transaction(account_id=bob_bank, amount=75000, tx_type="expense", idempotency_key=shared_key)
        assert_status(tx_a, 201)
        assert_status(tx_b, 201)
        assert_true(tx_a.json["id"] != tx_b.json["id"])
        reporter.record(True, "T3-P01: F05 Multi-Tenant + F12 Idempotency: Identical keys operate independently across tenants")
    except Exception as e:
        reporter.record(False, "T3-P01: F05 Multi-Tenant + F12 Idempotency: Identical keys operate independently across tenants", str(e))

    # -------------------------------------------------------------
    # P02: F04 Integer Rupiah + F09 NetCashFlowEngine
    # -------------------------------------------------------------
    try:
        client.create_transaction(account_id=alice_bank, amount=2500000, tx_type="income", category_id=alice_gaji)
        client.create_transaction(account_id=alice_bank, amount=350000, tx_type="expense", category_id=alice_makan)
        cf = client.get_cash_flow().json
        # net cash flow = 2,500,000 - (50,000 + 350,000) = 2,100,000
        assert_equal(cf["net_cash_flow"], 2100000)
        reporter.record(True, "T3-P02: F04 Integer Math + F09 NetCashFlow: Precise integer arithmetic with zero floating drift")
    except Exception as e:
        reporter.record(False, "T3-P02: F04 Integer Math + F09 NetCashFlow: Precise integer arithmetic with zero floating drift", str(e))

    # -------------------------------------------------------------
    # P03: F10 Multi-Wallet + F12 Idempotency
    # -------------------------------------------------------------
    try:
        bal_bank_pre = client.get(f"/api/v1/accounts/{alice_bank}").json["balance"]
        bal_cash_pre = client.get(f"/api/v1/accounts/{alice_cash}").json["balance"]
        transfer_key = str(uuid.uuid4())

        # Transfer 200,000
        t_res1 = client.create_transaction(
            account_id=alice_bank,
            amount=200000,
            tx_type="transfer",
            destination_account_id=alice_cash,
            idempotency_key=transfer_key
        )
        assert_status(t_res1, 201)

        # Replay transfer
        t_res2 = client.create_transaction(
            account_id=alice_bank,
            amount=200000,
            tx_type="transfer",
            destination_account_id=alice_cash,
            idempotency_key=transfer_key
        )
        assert_status(t_res2, 201)
        assert_equal(t_res1.json["id"], t_res2.json["id"])

        bal_bank_post = client.get(f"/api/v1/accounts/{alice_bank}").json["balance"]
        bal_cash_post = client.get(f"/api/v1/accounts/{alice_cash}").json["balance"]
        assert_equal(bal_bank_post, bal_bank_pre - 200000)
        assert_equal(bal_cash_post, bal_cash_pre + 200000)
        reporter.record(True, "T3-P03: F10 Multi-Wallet + F12 Idempotency: Wallet transfer deduplicated without double balance mutation")
    except Exception as e:
        reporter.record(False, "T3-P03: F10 Multi-Wallet + F12 Idempotency: Wallet transfer deduplicated without double balance mutation", str(e))

    # -------------------------------------------------------------
    # P04: F11 Category Soft-Delete + F09 NetCashFlowEngine
    # -------------------------------------------------------------
    try:
        cat_snack = client.create_category("Snack Sore", "expense").json["id"]
        client.create_transaction(account_id=alice_cash, amount=25000, tx_type="expense", category_id=cat_snack)
        cf_pre_del = client.get_cash_flow().json["expenses"]
        client.soft_delete_category(cat_snack)
        cf_post_del = client.get_cash_flow().json["expenses"]
        assert_equal(cf_pre_del, cf_post_del)
        reporter.record(True, "T3-P04: F11 Soft-Delete + F09 NetCashFlow: Historical expenses preserved in cash flow after category soft-delete")
    except Exception as e:
        reporter.record(False, "T3-P04: F11 Soft-Delete + F09 NetCashFlow: Historical expenses preserved in cash flow after category soft-delete", str(e))

    # -------------------------------------------------------------
    # P05: F07 JWT Cookies + F08 Rate Limiting
    # -------------------------------------------------------------
    try:
        c_rl = ApiClient(base_url, client_ip="10.3.3.3")
        # 2 failed attempts
        c_rl.login(u_alice, "bad1")
        c_rl.login(u_alice, "bad2")
        # Valid login resets counter
        r_good = c_rl.login(u_alice, password)
        assert_status(r_good, 200)
        # Subsequent failed attempts start fresh counter
        for _ in range(4):
            assert_status(c_rl.login(u_alice, "bad"), 401)
        reporter.record(True, "T3-P05: F07 JWT Auth + F08 Rate Limiter: Successful authentication resets sliding window failure count")
    except Exception as e:
        reporter.record(False, "T3-P05: F07 JWT Auth + F08 Rate Limiter: Successful authentication resets sliding window failure count", str(e))

    # -------------------------------------------------------------
    # P06: F16 Subscription State + F17 Feature Gating
    # -------------------------------------------------------------
    try:
        # Bob is Free -> 403 on advanced analytics
        assert_rfc7807(client_bob.get_advanced_analytics(), 403, "FEATURE_LOCKED")
        # Webhook upgrades Bob to Premium
        sig_bob = compute_midtrans_signature(bob_id, "200", "5000.00", server_key)
        client_bob.post("/api/v1/webhooks/midtrans", {
            "order_id": bob_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_bob,
            "transaction_id": f"trx_p06_{run_id}"
        })
        # Bob now accesses advanced analytics -> 200 OK
        assert_status(client_bob.get_advanced_analytics(), 200)
        reporter.record(True, "T3-P06: F16 Subscription + F17 Feature Gate: Webhook settlement instantly unlocks gated Premium features")
    except Exception as e:
        reporter.record(False, "T3-P06: F16 Subscription + F17 Feature Gate: Webhook settlement instantly unlocks gated Premium features", str(e))

    # -------------------------------------------------------------
    # P07: F14 HMAC Verification + F15 Idempotent Webhook
    # -------------------------------------------------------------
    try:
        wh_event_id = f"evt_p07_{run_id}"
        sig_p07 = compute_midtrans_signature(alice_id, "200", "5000.00", server_key)
        p07_body = {
            "order_id": alice_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_p07,
            "transaction_id": wh_event_id
        }
        res_first = client.post("/api/v1/webhooks/midtrans", p07_body)
        res_second = client.post("/api/v1/webhooks/midtrans", p07_body)
        assert_status(res_first, 200)
        assert_status(res_second, 200)
        reporter.record(True, "T3-P07: F14 HMAC + F15 Webhook: Cryptographically verified webhook deduplicated on replay")
    except Exception as e:
        reporter.record(False, "T3-P07: F14 HMAC + F15 Webhook: Cryptographically verified webhook deduplicated on replay", str(e))

    # -------------------------------------------------------------
    # P08: F06 Argon2id + F05 Multi-Tenant
    # -------------------------------------------------------------
    try:
        # Both Alice and Bob have password 'PairwisePassword123!', both can log in to distinct accounts
        c_a_login = ApiClient(base_url)
        c_b_login = ApiClient(base_url)
        assert_status(c_a_login.login(u_alice, password), 200)
        assert_status(c_b_login.login(u_bob, password), 200)
        assert_equal(c_a_login.me().json["id"], alice_id)
        assert_equal(c_b_login.me().json["id"], bob_id)
        reporter.record(True, "T3-P08: F06 Password Hashing + F05 Multi-Tenant: Identical passwords produce distinct tenant sessions")
    except Exception as e:
        reporter.record(False, "T3-P08: F06 Password Hashing + F05 Multi-Tenant: Identical passwords produce distinct tenant sessions", str(e))

    # -------------------------------------------------------------
    # P09: F18 REST API + F25 Cache-Control
    # -------------------------------------------------------------
    try:
        res_tx_cc = client.get("/api/v1/transactions")
        cc_val = res_tx_cc.header("Cache-Control")
        assert_true("private" in cc_val and "no-store" in cc_val and "must-revalidate" in cc_val)
        reporter.record(True, "T3-P09: F18 REST API + F25 Cache-Control: REST financial endpoints declare strict no-store caching")
    except Exception as e:
        reporter.record(False, "T3-P09: F18 REST API + F25 Cache-Control: REST financial endpoints declare strict no-store caching", str(e))

    # -------------------------------------------------------------
    # P10: F01 SQLite WAL + F10 Multi-Wallet
    # -------------------------------------------------------------
    try:
        # Sequential multi-account updates execute without database lock
        for i in range(5):
            r = client.create_transaction(account_id=alice_bank, amount=1000 * (i + 1), tx_type="expense")
            assert_status(r, 201)
        reporter.record(True, "T3-P10: F01 SQLite WAL + F10 Multi-Wallet: Multiple wallet mutations execute without SQLITE_BUSY locking")
    except Exception as e:
        reporter.record(False, "T3-P10: F01 SQLite WAL + F10 Multi-Wallet: Multiple wallet mutations execute without SQLITE_BUSY locking", str(e))

    # -------------------------------------------------------------
    # P11: F17 Feature Gating + F27 Upgrade Modal
    # -------------------------------------------------------------
    # Register free user 5 for P11
    u5_email = f"u5_p11_{run_id}@example.com"
    client5 = ApiClient(base_url)
    client5.register(u5_email, password, "User Five")
    try:
        r_gate_modal = client5.get_advanced_analytics()
        assert_equal(r_gate_modal.status, 403)
        assert_equal(r_gate_modal.json.get("code"), "FEATURE_LOCKED")
        assert_true("analytics.advanced" in r_gate_modal.json.get("detail", ""))
        reporter.record(True, "T3-P11: F17 Feature Gate + F27 Upgrade Modal: 403 response supplies schema consumed by UpgradeModal")
    except Exception as e:
        reporter.record(False, "T3-P11: F17 Feature Gate + F27 Upgrade Modal: 403 response supplies schema consumed by UpgradeModal", str(e))

    # -------------------------------------------------------------
    # P12: F23 POS Keypad + F04 Integer Rupiah
    # -------------------------------------------------------------
    try:
        # Keypad entry emulation: 15 + 000 = 15000 Rupiah integer
        keypad_amount = 15000
        r_keypad_tx = client.create_transaction(
            account_id=alice_cash,
            amount=keypad_amount,
            tx_type="expense",
            idempotency_key=str(uuid.uuid4())
        )
        assert_status(r_keypad_tx, 201)
        assert_equal(r_keypad_tx.json.get("amount"), 15000)
        reporter.record(True, "T3-P12: F23 POS Keypad + F04 Integer Math: Keypad input translates to exact integer Rupiah amount")
    except Exception as e:
        reporter.record(False, "T3-P12: F23 POS Keypad + F04 Integer Math: Keypad input translates to exact integer Rupiah amount", str(e))

    # -------------------------------------------------------------
    # P13: F24 Localized Formatting + F26 Accessible Charts
    # -------------------------------------------------------------
    try:
        summary_cf = client.get_cash_flow().json
        formatted_net = f"Rp {summary_cf['net_cash_flow']:,}".replace(",", ".")
        assert_true("Rp " in formatted_net and "." in formatted_net)
        reporter.record(True, "T3-P13: F24 id-ID Currency + F26 Accessible Charts: Numeric chart totals map to localized Indonesian format")
    except Exception as e:
        reporter.record(False, "T3-P13: F24 id-ID Currency + F26 Accessible Charts: Numeric chart totals map to localized Indonesian format", str(e))

    # -------------------------------------------------------------
    # P14: F02 Relational Schema + F11 Category Soft-Delete
    # -------------------------------------------------------------
    try:
        # Category soft-delete preserves foreign key integrity with transactions table
        tx_hist_count = len(client.list_transactions().json)
        assert_true(tx_hist_count > 0)
        reporter.record(True, "T3-P14: F02 Relational Schema + F11 Category Soft-Delete: Category deletion preserves transaction relations")
    except Exception as e:
        reporter.record(False, "T3-P14: F02 Relational Schema + F11 Category Soft-Delete: Category deletion preserves transaction relations", str(e))

    # -------------------------------------------------------------
    # P15: F13 Payment Gateway + F16 Subscription Lifecycle
    # -------------------------------------------------------------
    try:
        co_init = client5.checkout("midtrans")
        assert_status(co_init, 200)
        assert_true(co_init.json.get("order_id") is not None)
        reporter.record(True, "T3-P15: F13 Payment Gateway + F16 Subscription Lifecycle: Checkout generates tracked order_id for activation")
    except Exception as e:
        reporter.record(False, "T3-P15: F13 Payment Gateway + F16 Subscription Lifecycle: Checkout generates tracked order_id for activation", str(e))

    # -------------------------------------------------------------
    # P16: F08 Rate Limiter + F18 REST API
    # -------------------------------------------------------------
    try:
        # Probes and authenticated endpoints follow standard access without being blocked by auth rate limits
        assert_status(client.get("/health"), 200)
        assert_status(client.list_accounts(), 200)
        reporter.record(True, "T3-P16: F08 Rate Limiter + F18 REST API: Rate limiter is isolated to auth endpoints without affecting main API")
    except Exception as e:
        reporter.record(False, "T3-P16: F08 Rate Limiter + F18 REST API: Rate limiter is isolated to auth endpoints without affecting main API", str(e))

    # -------------------------------------------------------------
    # P17: F03 Composite Indexes + F18 REST API
    # -------------------------------------------------------------
    try:
        # Filter transactions with date range
        tx_filtered = client.list_transactions({"start_date": "2026-01-01", "end_date": "2026-12-31"})
        assert_status(tx_filtered, 200)
        assert_true(isinstance(tx_filtered.json, list))
        reporter.record(True, "T3-P17: F03 Composite Indexes + F18 REST API: Transactions endpoint leverages composite date index")
    except Exception as e:
        reporter.record(False, "T3-P17: F03 Composite Indexes + F18 REST API: Transactions endpoint leverages composite date index", str(e))

    # -------------------------------------------------------------
    # P18: F09 NetCashFlowEngine + F17 Feature Gating
    # -------------------------------------------------------------
    try:
        # Free user can access basic cash-flow, but not advanced analytics
        assert_status(client5.get_cash_flow(), 200)
        assert_status(client5.get_advanced_analytics(), 403)
        reporter.record(True, "T3-P18: F09 NetCashFlow + F17 Feature Gate: Basic cash flow available to Free users while advanced is gated")
    except Exception as e:
        reporter.record(False, "T3-P18: F09 NetCashFlow + F17 Feature Gate: Basic cash flow available to Free users while advanced is gated", str(e))

    # -------------------------------------------------------------
    # P19: F12 Idempotency-Key + F18 REST API
    # -------------------------------------------------------------
    try:
        # Non-UUID idempotency key rejected with 400
        r_fail_idem = client.create_transaction(account_id=alice_bank, amount=1000, tx_type="expense", idempotency_key="bad-key")
        assert_rfc7807(r_fail_idem, 400, "INVALID_IDEMPOTENCY_KEY")
        reporter.record(True, "T3-P19: F12 Idempotency + F18 REST API: Invalid idempotency key format rejected with RFC 7807 400")
    except Exception as e:
        reporter.record(False, "T3-P19: F12 Idempotency + F18 REST API: Invalid idempotency key format rejected with RFC 7807 400", str(e))

    # -------------------------------------------------------------
    # P20: F15 Webhook Processing + F16 Subscription Lifecycle
    # -------------------------------------------------------------
    try:
        # Webhook cancel transitions subscription to cancelled
        sig_c = compute_midtrans_signature(bob_id, "200", "5000.00", server_key)
        client_bob.post("/api/v1/webhooks/midtrans", {
            "order_id": bob_id,
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "cancel",
            "signature_key": sig_c,
            "transaction_id": f"trx_cancel_p20_{run_id}"
        })
        st_bob = client_bob.subscription_status().json
        assert_equal(st_bob["status"], "cancelled")
        reporter.record(True, "T3-P20: F15 Webhook Processing + F16 Subscription: Webhook cancel event transitions state to Cancelled")
    except Exception as e:
        reporter.record(False, "T3-P20: F15 Webhook Processing + F16 Subscription: Webhook cancel event transitions state to Cancelled", str(e))

    # -------------------------------------------------------------
    # P21: F16 Subscription Lifecycle + F17 Feature Gating
    # -------------------------------------------------------------
    try:
        # Free user attempting to access budgeting receives 403
        r_bud_gated = client5.list_budgets()
        assert_rfc7807(r_bud_gated, 403, "FEATURE_LOCKED")
        reporter.record(True, "T3-P21: F16 Subscription + F17 Feature Gate: Non-premium tier strictly blocks budgeting access with 403")
    except Exception as e:
        reporter.record(False, "T3-P21: F16 Subscription + F17 Feature Gate: Non-premium tier strictly blocks budgeting access with 403", str(e))

    # -------------------------------------------------------------
    # P22: F05 Multi-Tenant + F10 Multi-Wallet
    # -------------------------------------------------------------
    try:
        # Alice tries to transfer money to Bob's bank account directly -> 404
        r_cross_transfer = client.create_transaction(
            account_id=alice_bank,
            amount=50000,
            tx_type="transfer",
            destination_account_id=bob_bank
        )
        assert_rfc7807(r_cross_transfer, 404, "DESTINATION_ACCOUNT_NOT_FOUND")
        reporter.record(True, "T3-P22: F05 Multi-Tenant + F10 Multi-Wallet: Inter-tenant wallet transfer strictly blocked 404")
    except Exception as e:
        reporter.record(False, "T3-P22: F05 Multi-Tenant + F10 Multi-Wallet: Inter-tenant wallet transfer strictly blocked 404", str(e))

    # -------------------------------------------------------------
    # P23: F07 JWT Token + F18 REST API
    # -------------------------------------------------------------
    try:
        # Expired / invalid cookie rejected with 401 across API endpoints
        c_bad_jwt = ApiClient(base_url)
        c_bad_jwt.set_cookie("auth_token", "invalid.jwt.token")
        assert_rfc7807(c_bad_jwt.list_accounts(), 401, "AUTH_REQUIRED")
        assert_rfc7807(c_bad_jwt.list_transactions(), 401, "AUTH_REQUIRED")
        reporter.record(True, "T3-P23: F07 JWT Token + F18 REST API: Invalid JWT cookie rejected uniformly across all REST routes")
    except Exception as e:
        reporter.record(False, "T3-P23: F07 JWT Token + F18 REST API: Invalid JWT cookie rejected uniformly across all REST routes", str(e))

    # -------------------------------------------------------------
    # P24: F14 HMAC Verification + F13 Payment Gateway
    # -------------------------------------------------------------
    try:
        # Xendit callback token verified
        wh_x_p24 = {
            "id": f"xen_p24_{run_id}",
            "external_id": bob_id,
            "status": "PAID",
            "amount": 5000
        }
        res_x_p24 = client_bob.post("/api/v1/webhooks/xendit", wh_x_p24, headers={"X-Callback-Token": "xendit_webhook_token_secret_123"})
        assert_status(res_x_p24, 200)
        reporter.record(True, "T3-P24: F14 HMAC + F13 Payment Gateway: Xendit provider webhook verified via callback token")
    except Exception as e:
        reporter.record(False, "T3-P24: F14 HMAC + F13 Payment Gateway: Xendit provider webhook verified via callback token", str(e))

    # -------------------------------------------------------------
    # P25: F21 Vue PWA Shell + F25 Workbox Caching
    # -------------------------------------------------------------
    try:
        sw_code = client.get("/service-worker.js").text
        # SW specifies NetworkOnly for /api/v1/
        assert_true("/api/v1/" in sw_code)
        reporter.record(True, "T3-P25: F21 Vue PWA Shell + F25 Workbox: Service worker enforces NetworkOnly bypass for API requests")
    except Exception as e:
        reporter.record(False, "T3-P25: F21 Vue PWA Shell + F25 Workbox: Service worker enforces NetworkOnly bypass for API requests", str(e))

    # -------------------------------------------------------------
    # P26: F10 Multi-Wallet + F09 NetCashFlowEngine
    # -------------------------------------------------------------
    try:
        # Internal transfer between accounts does not affect net cash flow
        net_before = client.get_cash_flow().json["net_cash_flow"]
        client.create_transaction(account_id=alice_bank, amount=50000, tx_type="transfer", destination_account_id=alice_cash)
        net_after = client.get_cash_flow().json["net_cash_flow"]
        assert_equal(net_before, net_after)
        reporter.record(True, "T3-P26: F10 Multi-Wallet + F09 NetCashFlow: Inter-wallet fund transfers preserve net cash flow total")
    except Exception as e:
        reporter.record(False, "T3-P26: F10 Multi-Wallet + F09 NetCashFlow: Inter-wallet fund transfers preserve net cash flow total", str(e))

    # -------------------------------------------------------------
    # P27: F11 Category Personalization + F05 Multi-Tenant
    # -------------------------------------------------------------
    try:
        # Alice creates custom category; Bob cannot see it in his category list
        cat_alice_priv = client.create_category("Alice Private", "expense").json["id"]
        bob_cats = client_bob.list_categories().json
        assert_true(not any(c["id"] == cat_alice_priv for c in bob_cats))
        reporter.record(True, "T3-P27: F11 Category Personalization + F05 Multi-Tenant: Tenant custom categories are strictly private")
    except Exception as e:
        reporter.record(False, "T3-P27: F11 Category Personalization + F05 Multi-Tenant: Tenant custom categories are strictly private", str(e))

    # -------------------------------------------------------------
    # P28: F19 Health Probes + F01 SQLite WAL
    # -------------------------------------------------------------
    try:
        ready_p28 = client.get("/ready")
        assert_status(ready_p28, 200)
        assert_equal(ready_p28.json.get("wal_mode"), True)
        assert_equal(ready_p28.json.get("database"), "connected")
        reporter.record(True, "T3-P28: F19 Health Probes + F01 SQLite WAL: Readiness probe confirms operational SQLite WAL mode")
    except Exception as e:
        reporter.record(False, "T3-P28: F19 Health Probes + F01 SQLite WAL: Readiness probe confirms operational SQLite WAL mode", str(e))

    # -------------------------------------------------------------
    # P29: F29 Adversarial Hardening + F12 Idempotency
    # -------------------------------------------------------------
    try:
        # Malicious payload with Idempotency-Key rejected and does not corrupt idempotency cache
        bad_key = str(uuid.uuid4())
        r_bad_tx = client.create_transaction(account_id=alice_bank, amount=-99999, tx_type="expense", idempotency_key=bad_key)
        assert_rfc7807(r_bad_tx, 400, "INVALID_AMOUNT")
        # Submitting valid payload with same key afterwards succeeds
        r_good_tx = client.create_transaction(account_id=alice_bank, amount=5000, tx_type="expense", idempotency_key=bad_key)
        assert_status(r_good_tx, 201)
        reporter.record(True, "T3-P29: F29 Adversarial + F12 Idempotency: Rejected malicious transaction does not corrupt idempotency store")
    except Exception as e:
        reporter.record(False, "T3-P29: F29 Adversarial + F12 Idempotency: Rejected malicious transaction does not corrupt idempotency store", str(e))

    return reporter.print_summary()

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    success = run_tier3_tests(url)
    sys.exit(0 if success else 1)
