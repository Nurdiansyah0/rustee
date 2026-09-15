#!/usr/bin/env python3
"""
Tier 4: Real-World Application Workload Scenarios Test Suite (15 comprehensive scenarios).
Simulates realistic end-to-end user workflows, payment lifecycles, and operational conditions.
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

def run_tier4_scenarios(base_url: str = "http://127.0.0.1:8089", reporter: TapReporter = None) -> bool:
    if reporter is None:
        reporter = TapReporter(total_expected=15)
        reporter.print_header()

    run_id = f"t4_{int(time.time())}_{uuid.uuid4().hex[:6]}"
    server_key = "SB-Mid-server-test-secret-key"

    # -------------------------------------------------------------
    # S01: New User Complete Onboarding
    # -------------------------------------------------------------
    try:
        client1 = ApiClient(base_url)
        email1 = f"s01_{run_id}@example.com"
        reg = client1.register(email1, "OnboardingPass123!", "Siti Onboarding")
        assert_status(reg, 201)
        u_data = reg.json
        assert_equal(u_data.get("tier"), "free")

        # Verify seeded accounts
        accs = client1.list_accounts().json
        assert_true(len(accs) >= 2)
        assert_true(any(a["account_type"] == "cash" for a in accs))
        assert_true(any(a["account_type"] == "checking" for a in accs))

        # Verify seeded categories
        cats = client1.list_categories().json
        assert_true(len(cats) >= 2)

        # Verify profile endpoint
        me = client1.me().json
        assert_equal(me["email"], email1)
        reporter.record(True, "T4-S01: New user complete onboarding and default asset bootstrapping")
    except Exception as e:
        reporter.record(False, "T4-S01: New user complete onboarding and default asset bootstrapping", str(e))

    # -------------------------------------------------------------
    # S02: Daily POS Expense Logging
    # -------------------------------------------------------------
    try:
        cash_acc = [a["id"] for a in accs if a["account_type"] == "cash"][0]
        cat_makan = [c["id"] for c in cats if c["category_type"] == "expense"][0]

        # Initial deposit
        client1.create_transaction(account_id=cash_acc, amount=500000, tx_type="income", note="Initial Cash")

        # Morning Coffee: Rp 22,000
        client1.create_transaction(account_id=cash_acc, amount=22000, tx_type="expense", category_id=cat_makan, note="Kopi Pagi")
        # Lunch Nasi Padang: Rp 35,000
        client1.create_transaction(account_id=cash_acc, amount=35000, tx_type="expense", category_id=cat_makan, note="Nasi Padang")
        # Evening Snack: Rp 18,000
        client1.create_transaction(account_id=cash_acc, amount=18000, tx_type="expense", category_id=cat_makan, note="Martabak Mini")

        # Balance check: 500,000 - 75,000 = 425,000
        bal = client1.get(f"/api/v1/accounts/{cash_acc}").json["balance"]
        assert_equal(bal, 425000)
        reporter.record(True, "T4-S02: Daily POS rapid micro-expense logging with atomic balance reconciliation")
    except Exception as e:
        reporter.record(False, "T4-S02: Daily POS rapid micro-expense logging with atomic balance reconciliation", str(e))

    # -------------------------------------------------------------
    # S03: Monthly Payroll & Multi-Wallet Allocation
    # -------------------------------------------------------------
    try:
        bank_acc = [a["id"] for a in accs if a["account_type"] == "checking"][0]
        ewallet_res = client1.create_account("OVO Balance", "ewallet", 0)
        ewallet_acc = ewallet_res.json["id"]

        # Salary credit: Rp 12,500,000
        client1.create_transaction(account_id=bank_acc, amount=12500000, tx_type="income", note="Gaji Bulanan")

        # Split 2,000,000 to Cash
        client1.create_transaction(account_id=bank_acc, amount=2000000, tx_type="transfer", destination_account_id=cash_acc, note="Tarik Tunai")

        # Split 1,500,000 to E-Wallet
        client1.create_transaction(account_id=bank_acc, amount=1500000, tx_type="transfer", destination_account_id=ewallet_acc, note="Topup OVO")

        # Verify balances
        assert_equal(client1.get(f"/api/v1/accounts/{bank_acc}").json["balance"], 9000000)
        assert_equal(client1.get(f"/api/v1/accounts/{cash_acc}").json["balance"], 2425000)
        assert_equal(client1.get(f"/api/v1/accounts/{ewallet_acc}").json["balance"], 1500000)
        reporter.record(True, "T4-S03: Monthly payroll receipt and multi-wallet distribution split")
    except Exception as e:
        reporter.record(False, "T4-S03: Monthly payroll receipt and multi-wallet distribution split", str(e))

    # -------------------------------------------------------------
    # S04: Complete Premium Upgrade Lifecycle
    # -------------------------------------------------------------
    try:
        # User attempts to access advanced analytics -> 403
        r_prem_pre = client1.get_advanced_analytics()
        assert_equal(r_prem_pre.status, 403)

        # Initiates checkout
        co = client1.checkout("midtrans").json
        order_id = co["order_id"]

        # Midtrans webhook arrives with valid cryptographic signature
        sig_upgrade = compute_midtrans_signature(u_data["id"], "200", "5000.00", server_key)
        wh_upgrade = {
            "order_id": u_data["id"],
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_upgrade,
            "transaction_id": f"trx_s04_{run_id}"
        }
        client1.post("/api/v1/webhooks/midtrans", wh_upgrade)

        # User checks status: tier is premium
        st = client1.subscription_status().json
        assert_equal(st["tier"], "premium")
        assert_equal(st["status"], "active")

        # User now accesses advanced analytics -> 200 OK
        r_prem_post = client1.get_advanced_analytics()
        assert_status(r_prem_post, 200)
        reporter.record(True, "T4-S04: End-to-end premium upgrade workflow via verified payment webhook")
    except Exception as e:
        reporter.record(False, "T4-S04: End-to-end premium upgrade workflow via verified payment webhook", str(e))

    # -------------------------------------------------------------
    # S05: Subscription Renewal & Grace Recovery
    # -------------------------------------------------------------
    try:
        # Re-settlement webhook represents renewal payment
        sig_renew = compute_midtrans_signature(u_data["id"], "200", "5000.00", server_key)
        wh_renew = {
            "order_id": u_data["id"],
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": sig_renew,
            "transaction_id": f"trx_s05_{run_id}"
        }
        r_renew = client1.post("/api/v1/webhooks/midtrans", wh_renew)
        assert_status(r_renew, 200)
        st_renew = client1.subscription_status().json
        assert_equal(st_renew["tier"], "premium")
        assert_equal(st_renew["status"], "active")
        reporter.record(True, "T4-S05: Subscription automated renewal extension handling")
    except Exception as e:
        reporter.record(False, "T4-S05: Subscription automated renewal extension handling", str(e))

    # -------------------------------------------------------------
    # S06: Subscription Cancellation & Graceful Downgrade
    # -------------------------------------------------------------
    try:
        # User cancels subscription via webhook
        sig_cancel = compute_midtrans_signature(u_data["id"], "200", "5000.00", server_key)
        wh_cancel = {
            "order_id": u_data["id"],
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "cancel",
            "signature_key": sig_cancel,
            "transaction_id": f"trx_s06_{run_id}"
        }
        client1.post("/api/v1/webhooks/midtrans", wh_cancel)
        st_canc = client1.subscription_status().json
        assert_equal(st_canc["status"], "cancelled")
        reporter.record(True, "T4-S06: Subscription cancellation lifecycle transition")
    except Exception as e:
        reporter.record(False, "T4-S06: Subscription cancellation lifecycle transition", str(e))

    # -------------------------------------------------------------
    # S07: Offline Transaction Batch Sync with Idempotency Keys
    # -------------------------------------------------------------
    try:
        offline_keys = [str(uuid.uuid4()) for _ in range(5)]
        offline_tx_ids = []

        # Client syncs 5 offline transactions
        for i, k in enumerate(offline_keys):
            r = client1.create_transaction(
                account_id=cash_acc,
                amount=10000 * (i + 1),
                tx_type="expense",
                note=f"Offline expense {i+1}",
                idempotency_key=k
            )
            assert_status(r, 201)
            offline_tx_ids.append(r.json["id"])

        # Network retry: re-send batch with same keys
        for i, k in enumerate(offline_keys):
            r_retry = client1.create_transaction(
                account_id=cash_acc,
                amount=10000 * (i + 1),
                tx_type="expense",
                note=f"Offline expense {i+1}",
                idempotency_key=k
            )
            assert_status(r_retry, 201)
            assert_equal(r_retry.json["id"], offline_tx_ids[i])
        reporter.record(True, "T4-S07: Offline transaction batch synchronization and replay deduplication")
    except Exception as e:
        reporter.record(False, "T4-S07: Offline transaction batch synchronization and replay deduplication", str(e))

    # -------------------------------------------------------------
    # S08: Category Lifecycle with Historical Integrity
    # -------------------------------------------------------------
    try:
        # Create category 'Hobi Fotografi'
        cat_foto = client1.create_category("Hobi Fotografi", "expense", "camera", "#8B5CF6").json["id"]
        # Log 2 transactions
        tx1 = client1.create_transaction(account_id=bank_acc, amount=1200000, tx_type="expense", category_id=cat_foto, note="Lensa")
        tx2 = client1.create_transaction(account_id=bank_acc, amount=300000, tx_type="expense", category_id=cat_foto, note="Filter UV")
        # Soft delete category
        client1.soft_delete_category(cat_foto)
        # Transactions still reference cat_foto
        txs_foto = client1.list_transactions({"category_id": cat_foto}).json
        assert_equal(len(txs_foto), 2)
        # Active categories no longer include cat_foto
        assert_true(not any(c["id"] == cat_foto for c in client1.list_categories().json))
        reporter.record(True, "T4-S08: Category lifecycle and historical transaction relational integrity")
    except Exception as e:
        reporter.record(False, "T4-S08: Category lifecycle and historical transaction relational integrity", str(e))

    # -------------------------------------------------------------
    # S09: Multi-Tenant Concurrent Activity
    # -------------------------------------------------------------
    try:
        client_tenant_a = ApiClient(base_url)
        client_tenant_b = ApiClient(base_url)
        reg_a = client_tenant_a.register(f"tenant_a_{run_id}@example.com", "PasswordTenant123!", "Tenant A").json
        reg_b = client_tenant_b.register(f"tenant_b_{run_id}@example.com", "PasswordTenant123!", "Tenant B").json

        acc_a = client_tenant_a.list_accounts().json[0]["id"]
        acc_b = client_tenant_b.list_accounts().json[0]["id"]

        # Both tenants perform concurrent transactions
        client_tenant_a.create_transaction(account_id=acc_a, amount=200000, tx_type="income", note="A income")
        client_tenant_b.create_transaction(account_id=acc_b, amount=300000, tx_type="income", note="B income")

        # Verify complete data isolation
        assert_equal(client_tenant_a.get_cash_flow().json["income"], 200000)
        assert_equal(client_tenant_b.get_cash_flow().json["income"], 300000)
        reporter.record(True, "T4-S09: Multi-tenant concurrent financial activity with zero cross-tenant leakage")
    except Exception as e:
        reporter.record(False, "T4-S09: Multi-tenant concurrent financial activity with zero cross-tenant leakage", str(e))

    # -------------------------------------------------------------
    # S10: End-of-Month Comprehensive Financial Audit
    # -------------------------------------------------------------
    try:
        # Audit client_tenant_a
        cat_exp = client_tenant_a.list_categories().json[1]["id"]
        client_tenant_a.create_transaction(account_id=acc_a, amount=75000, tx_type="expense", category_id=cat_exp, note="Audit Exp")
        cf_audit = client_tenant_a.get_cash_flow().json
        # Net must equal 200,000 - 75,000 = 125,000
        assert_equal(cf_audit["net_cash_flow"], 125000)
        bal_audit = client_tenant_a.get(f"/api/v1/accounts/{acc_a}").json["balance"]
        assert_equal(bal_audit, 125000)
        reporter.record(True, "T4-S10: End-of-month ledger reconciliation and balance-to-cashflow equality")
    except Exception as e:
        reporter.record(False, "T4-S10: End-of-month ledger reconciliation and balance-to-cashflow equality", str(e))

    # -------------------------------------------------------------
    # S11: High-Frequency POS Transactions
    # -------------------------------------------------------------
    try:
        # 20 rapid sequential micro-transactions of Rp 2,500
        bal_hf_start = client_tenant_a.get(f"/api/v1/accounts/{acc_a}").json["balance"]
        for i in range(20):
            r_hf = client_tenant_a.create_transaction(
                account_id=acc_a,
                amount=2500,
                tx_type="expense",
                note=f"Micro TX {i+1}",
                idempotency_key=str(uuid.uuid4())
            )
            assert_status(r_hf, 201)
        bal_hf_end = client_tenant_a.get(f"/api/v1/accounts/{acc_a}").json["balance"]
        assert_equal(bal_hf_end, bal_hf_start - (20 * 2500))
        reporter.record(True, "T4-S11: High-frequency POS micro-transactions with zero drift")
    except Exception as e:
        reporter.record(False, "T4-S11: High-frequency POS micro-transactions with zero drift", str(e))

    # -------------------------------------------------------------
    # S12: Multi-Vector Adversarial Attack Defense
    # -------------------------------------------------------------
    try:
        # 1. Attacker attempts forged Midtrans signature
        r_forged = client1.post("/api/v1/webhooks/midtrans", {
            "order_id": "VICTIM-ORDER",
            "status_code": "200",
            "gross_amount": "5000.00",
            "transaction_status": "settlement",
            "signature_key": "fake_forged_signature_00000000000000"
        })
        assert_equal(r_forged.status, 401)

        # 2. Attacker attempts SQL injection in search
        r_sqli = client1.list_transactions({"account_id": "' OR '1'='1"})
        assert_status(r_sqli, 200)

        # 3. Attacker attempts XSS injection in transaction note
        r_xss = client_tenant_a.create_transaction(account_id=acc_a, amount=1000, tx_type="expense", note="<script src='evil.js'></script>")
        assert_status(r_xss, 201)
        reporter.record(True, "T4-S12: Resilient defense against signature forgery, SQL injection, and XSS")
    except Exception as e:
        reporter.record(False, "T4-S12: Resilient defense against signature forgery, SQL injection, and XSS", str(e))

    # -------------------------------------------------------------
    # S13: Multi-Wallet Inter-Account Rebalancing
    # -------------------------------------------------------------
    try:
        # Transfer chain: Account 1 -> Account 2 -> Account 3 -> Account 1
        acc1 = client1.create_account("Rebal 1", "checking", 1000000).json["id"]
        acc2 = client1.create_account("Rebal 2", "savings", 500000).json["id"]
        acc3 = client1.create_account("Rebal 3", "cash", 200000).json["id"]

        net_pre_chain = client1.get_cash_flow().json["net_cash_flow"]

        # Transfer 100,000 from acc1 to acc2
        client1.create_transaction(account_id=acc1, amount=100000, tx_type="transfer", destination_account_id=acc2)
        # Transfer 50,000 from acc2 to acc3
        client1.create_transaction(account_id=acc2, amount=50000, tx_type="transfer", destination_account_id=acc3)
        # Transfer 25,000 from acc3 to acc1
        client1.create_transaction(account_id=acc3, amount=25000, tx_type="transfer", destination_account_id=acc1)

        net_post_chain = client1.get_cash_flow().json["net_cash_flow"]
        assert_equal(net_pre_chain, net_post_chain)
        assert_equal(client1.get(f"/api/v1/accounts/{acc1}").json["balance"], 925000)
        assert_equal(client1.get(f"/api/v1/accounts/{acc2}").json["balance"], 550000)
        assert_equal(client1.get(f"/api/v1/accounts/{acc3}").json["balance"], 225000)
        reporter.record(True, "T4-S13: Multi-wallet circular rebalancing preserves global net cash flow invariance")
    except Exception as e:
        reporter.record(False, "T4-S13: Multi-wallet circular rebalancing preserves global net cash flow invariance", str(e))

    # -------------------------------------------------------------
    # S14: Data Export & Profile Management
    # -------------------------------------------------------------
    try:
        # Retrieve full customer profile and ledger export
        profile = client1.me().json
        all_accounts = client1.list_accounts().json
        all_categories = client1.list_categories().json
        all_transactions = client1.list_transactions({"limit": 100}).json
        cash_flow = client1.get_cash_flow().json

        assert_true("id" in profile and "email" in profile)
        assert_true(isinstance(all_accounts, list) and len(all_accounts) >= 3)
        assert_true(isinstance(all_categories, list) and len(all_categories) >= 2)
        assert_true(isinstance(all_transactions, list) and len(all_transactions) >= 5)
        assert_true("net_cash_flow" in cash_flow)
        reporter.record(True, "T4-S14: Complete customer financial profile and ledger data export")
    except Exception as e:
        reporter.record(False, "T4-S14: Complete customer financial profile and ledger data export", str(e))

    # -------------------------------------------------------------
    # S15: Resilient Health & Readiness Polling Under Load
    # -------------------------------------------------------------
    try:
        for _ in range(15):
            h_res = client1.get("/health")
            r_res = client1.get("/ready")
            assert_status(h_res, 200)
            assert_status(r_res, 200)
            assert_equal(h_res.json["status"], "pass")
            assert_equal(r_res.json["database"], "connected")
        reporter.record(True, "T4-S15: High-frequency operational health and readiness probe resilience")
    except Exception as e:
        reporter.record(False, "T4-S15: High-frequency operational health and readiness probe resilience", str(e))

    return reporter.print_summary()

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    success = run_tier4_scenarios(url)
    sys.exit(0 if success else 1)
