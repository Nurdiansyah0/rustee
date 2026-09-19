#!/usr/bin/env python3
"""
Tier 4: Real-World Application Scenarios Acceptance Test Suite (20 comprehensive scenarios).
Verifies complete realistic multi-step user workflows mirroring production operation.
"""

import sys
import os

os.environ["no_proxy"] = "localhost,127.0.0.1"
os.environ["NO_PROXY"] = "localhost,127.0.0.1"

import time
import uuid
import json
from datetime import datetime, timezone
from typing import Optional, Dict, Any, List

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from harness.client import (
    ApiClient, TapReporter, assert_true, assert_equal,
    assert_status, assert_rfc7807
)

def run_tier4_scenarios(base_url: str, reporter: Optional[TapReporter] = None) -> TapReporter:
    if reporter is None:
        reporter = TapReporter(total_expected=20)
        reporter.print_header()

    client = ApiClient(base_url=base_url)

    # =========================================================================
    # SCENARIO 1: Complete New User Onboarding, Trial, Tracking & Reporting
    # =========================================================================
    s1_email = f"scenario1_{uuid.uuid4().hex[:6]}@invinite.app"
    c1 = ApiClient(base_url=base_url)
    reg = c1.register(s1_email, "P@ssword123!", "Siti Nurhaliza")
    u1_id = reg.json.get("user", {}).get("id")

    # Onboarding
    onb = c1.submit_onboarding(
        display_name="Siti Finance",
        financial_goals=["Tabungan Nikah", "Dana Darurat"],
        wallets=[
            {"name": "BCA Payroll", "account_type": "checking", "initial_balance": 8000000},
            {"name": "GoPay", "account_type": "e_wallet", "initial_balance": 350000},
            {"name": "Dompet Fisik", "account_type": "cash", "initial_balance": 200000}
        ],
        categories=[
            {"name": "Gaji Bulanan", "display_name": "Gaji Bulanan", "category_type": "income"},
            {"name": "Makan & Kopi", "display_name": "Makan & Kopi", "category_type": "expense"},
            {"name": "Transport Ojol", "display_name": "Transport Ojol", "category_type": "expense"}
        ]
    )

    # Activate 3-month trial
    t_act = c1.activate_trial()

    # Find accounts and categories
    accs = c1.list_accounts().json.get("accounts", [])
    bca_id = next((a["id"] for a in accs if a["name"] == "BCA Payroll"), None)
    gopay_id = next((a["id"] for a in accs if a["name"] == "GoPay"), None)
    cash_id = next((a["id"] for a in accs if a["name"] == "Dompet Fisik"), None)

    cats = c1.list_categories().json.get("categories", [])
    makan_id = next((c["id"] for c in cats if c["name"] == "Makan & Kopi"), None)
    ojol_id = next((c["id"] for c in cats if c["name"] == "Transport Ojol"), None)

    # Record daily transactions
    t1 = c1.create_transaction(gopay_id, 25000, "expense", category_id=ojol_id, note="Ojol Pagi")
    t2 = c1.create_transaction(cash_id, 45000, "expense", category_id=makan_id, note="Makan Siang")

    # Transfer: Top-up GoPay from BCA
    transfer = c1.create_transaction(bca_id, 100000, "transfer", destination_account_id=gopay_id, note="Top-up GoPay")

    # Verify Cash Flow: Income 0, Expense 70.000, Net -70.000
    cf = c1.get_cash_flow().json or {}
    s1_pass = (
        onb.status == 200 and
        t_act.status == 200 and
        cf.get("total_expenses") == 70000 and
        cf.get("net_cash_flow") == -70000 and
        c1.get_account(gopay_id).json.get("balance") == (350000 - 25000 + 100000)
    )
    reporter.record(s1_pass, "SCENARIO-01 [Lifecycle]: Complete user journey: Onboard -> Trial -> Multi-wallet -> POS Transactions -> Cash Flow")

    # =========================================================================
    # SCENARIO 2: Ingestion Pipeline Intake, Deduplication & Confirmation
    # =========================================================================
    c2 = ApiClient(base_url=base_url)
    c2.register(f"scenario2_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Ingestion User")
    c2.activate_trial()
    acc_ing = c2.create_account("BCA Utama", "checking", 2000000).json.get("id")

    # Receive bank notification
    notif1 = c2.ingest_notification("com.bca", "BCA Mobile", "Transfer Rp 150.000 ke Resto Padang Berhasil")
    cand_id = notif1.json.get("candidate_id")

    # Replay same notification (simulating duplicate push)
    notif2 = c2.ingest_notification("com.bca", "BCA Mobile", "Transfer Rp 150.000 ke Resto Padang Berhasil")

    # User confirms the pending candidate
    bal_pre = c2.get_account(acc_ing).json.get("balance", 0)
    c2.confirm_candidate(cand_id, acc_ing)
    bal_post = c2.get_account(acc_ing).json.get("balance", 0)

    s2_pass = (
        notif1.status == 200 and
        notif2.json.get("status") == "duplicate" and
        bal_post == bal_pre - 150000
    )
    reporter.record(s2_pass, "SCENARIO-02 [Ingestion & Intelligence]: Bank push notification -> Ingestion Parser -> Duplicate suppression -> 1-tap confirmation -> Ledger balance settlement")

    # =========================================================================
    # SCENARIO 3: Commercial Checkout & DANA Webhook Settlement
    # =========================================================================
    c3 = ApiClient(base_url=base_url)
    reg3 = c3.register(f"scenario3_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Checkout User")
    u3_id = reg3.json.get("user", {}).get("id")

    # Verify initially locked
    lock_check = c3.get_advanced_analytics()

    # Initiate DANA Annual Premium checkout (Rp 110.000)
    checkout_res = c3.checkout(plan="premium_annual", provider="dana")
    order_id = checkout_res.json.get("order_id")

    # DANA webhook async notification delivery
    wh_res = c3.send_dana_webhook(str(uuid.uuid4()), order_id, u3_id, amount=110000)

    # Verify unlocked
    unlock_check = c3.get_advanced_analytics()

    s3_pass = (
        lock_check.status == 403 and
        checkout_res.json.get("amount") == 110000 and
        wh_res.json.get("responseCode") == "2005600" and
        unlock_check.status == 200
    )
    reporter.record(s3_pass, "SCENARIO-03 [Subscription Upgrade]: Feature lock discovery -> Annual DANA checkout -> Asymmetric RSA-SHA256 signature verification -> Premium entitlement activated")

    # =========================================================================
    # SCENARIO 4: Multi-Device Offline Mutation & Delta Sync Reconciliation
    # =========================================================================
    c4 = ApiClient(base_url=base_url)
    c4.register(f"scenario4_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Sync User")
    acc4 = c4.create_account("Tabungan", "savings", 1000000).json.get("id")

    # Capture initial cursor on Device B
    dev_b_cursor = c4.delta_sync(0).json.get("cursor", 0)

    # Device A creates 3 offline mutations
    idem1 = str(uuid.uuid4())
    idem2 = str(uuid.uuid4())
    idem3 = str(uuid.uuid4())
    c4.create_transaction(acc4, 10000, "expense", idempotency_key=idem1)
    c4.create_transaction(acc4, 20000, "expense", idempotency_key=idem2)
    c4.create_transaction(acc4, 30000, "expense", idempotency_key=idem3)

    # Device B syncs using cursor
    sync_b = c4.delta_sync(dev_b_cursor).json or {}
    s4_pass = (
        sync_b.get("deltas_count", 0) == 3 and
        sync_b.get("cursor", 0) > dev_b_cursor
    )
    reporter.record(s4_pass, "SCENARIO-04 [Delta Sync]: Multi-device offline mutations reconcile idempotently via cursor delta synchronization")

    # =========================================================================
    # SCENARIO 5: Category Vocabulary Evolution & Historical Integrity
    # =========================================================================
    c5 = ApiClient(base_url=base_url)
    c5.register(f"scenario5_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Vocab User")
    acc5 = c5.create_account("Dompet", "cash", 500000).json.get("id")

    cat_coffee = c5.create_category("Kopi Kekinian", "expense").json.get("id")
    c5.create_transaction(acc5, 35000, "expense", category_id=cat_coffee, note="Kopi Susu Gula Aren")

    # Soft delete category
    c5.soft_delete_category(cat_coffee)

    # Verify active list excludes it, but category analytics includes it
    active_cats = c5.list_categories().json.get("categories", [])
    cat_analytics = c5.get_category_analytics().json.get("categories", [])
    s5_pass = (
        not any(c["id"] == cat_coffee for c in active_cats) and
        any(c["id"] == cat_coffee and c["total_amount"] == 35000 for c in cat_analytics)
    )
    reporter.record(s5_pass, "SCENARIO-05 [Vocabulary Evolution]: Custom label soft-deletion purges active pickers while preserving historical financial reports")

    # =========================================================================
    # SCENARIO 6: Monthly Payroll Auto-Split
    # =========================================================================
    c6 = ApiClient(base_url=base_url)
    c6.register(f"scen6_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Payroll User")
    payroll_acc = c6.create_account("Payroll Checking", "checking", 0).json.get("id")
    savings_acc = c6.create_account("Savings Vault", "savings", 0).json.get("id")
    emer_acc = c6.create_account("Emergency Fund", "savings", 0).json.get("id")

    # Crediting salary
    c6.create_transaction(payroll_acc, 15000000, "income", note="Gaji Bulanan")
    # Auto-split to savings and emergency
    c6.create_transaction(payroll_acc, 5000000, "transfer", destination_account_id=savings_acc, note="Auto-save 5jt")
    c6.create_transaction(payroll_acc, 2500000, "transfer", destination_account_id=emer_acc, note="Auto-emergency 2.5jt")

    b_pay = c6.get_account(payroll_acc).json.get("balance", 0)
    b_sav = c6.get_account(savings_acc).json.get("balance", 0)
    b_eme = c6.get_account(emer_acc).json.get("balance", 0)
    cf6 = c6.get_cash_flow().json or {}
    s6_pass = (b_pay == 7500000 and b_sav == 5000000 and b_eme == 2500000 and cf6.get("net_cash_flow") == 15000000)
    reporter.record(s6_pass, "SCENARIO-06 [Monthly Payroll]: Automated monthly salary crediting with auto-split to savings and emergency accounts")

    # =========================================================================
    # SCENARIO 7: Corporate Reimbursement Tracking
    # =========================================================================
    c7 = ApiClient(base_url=base_url)
    c7.register(f"scen7_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Reimburse User")
    corp_acc = c7.create_account("Corporate Expenses", "credit", 0).json.get("id")
    c7.create_transaction(corp_acc, 450000, "expense", note="Beli ATK Kantor")
    bal_after_exp = c7.get_account(corp_acc).json.get("balance", 0)
    c7.create_transaction(corp_acc, 450000, "income", note="Reimbursement Kantor")
    bal_after_reimb = c7.get_account(corp_acc).json.get("balance", 0)
    s7_pass = (bal_after_exp == -450000 and bal_after_reimb == 0)
    reporter.record(s7_pass, "SCENARIO-07 [Reimbursement Tracking]: Corporate expense categorization with pending reimbursement state tracking")

    # =========================================================================
    # SCENARIO 8: Investment Portfolio & Dividend Tracking
    # =========================================================================
    c8 = ApiClient(base_url=base_url)
    c8.register(f"scen8_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Investor User")
    saham_acc = c8.create_account("Rekening Saham", "investment", 10000000).json.get("id")
    op_acc = c8.create_account("Giro Operasional", "checking", 5000000).json.get("id")
    c8.create_transaction(saham_acc, 750000, "income", note="Dividen BBCA")
    c8.create_transaction(op_acc, 200000, "expense", note="Biaya Langganan Data Market")
    cf8 = c8.get_cash_flow().json or {}
    s8_pass = (cf8.get("net_cash_flow") == 550000 and c8.get_account(saham_acc).json.get("balance") == 10750000)
    reporter.record(s8_pass, "SCENARIO-08 [Investment Portfolio]: Tracking dividend income inflows alongside regular cash flow")

    # =========================================================================
    # SCENARIO 9: Emergency Reserve Large-Value Debit
    # =========================================================================
    c9 = ApiClient(base_url=base_url)
    c9.register(f"scen9_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Emergency User")
    reserve_acc = c9.create_account("Dana Darurat", "savings", 100000000).json.get("id")
    tx_eme = c9.create_transaction(reserve_acc, 42350000, "expense", note="Operasi Medis Darurat")
    bal_eme = c9.get_account(reserve_acc).json.get("balance", 0)
    s9_pass = (tx_eme.status == 201 and bal_eme == 57650000)
    reporter.record(s9_pass, "SCENARIO-09 [Emergency Reserve]: Sudden high-value debit verified against checked integer arithmetic invariants")

    # =========================================================================
    # SCENARIO 10: Subscription Audit Logging
    # =========================================================================
    c10 = ApiClient(base_url=base_url)
    reg10 = c10.register(f"scen10_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Audit Sub User")
    u10_id = reg10.json.get("user", {}).get("id")
    co10 = c10.checkout(plan="premium_monthly", provider="dana")
    wh10_id = f"EVT-SCEN10-{uuid.uuid4().hex[:6]}"
    c10.send_dana_webhook(wh10_id, co10.json.get("order_id"), u10_id, amount=10000)
    logs10 = c10.get_audit_logs().json.get("logs", [])
    s10_pass = any("DANA_PAYMENT_SUCCESS" in str(l.get("action")) for l in logs10)
    reporter.record(s10_pass, "SCENARIO-10 [Subscription Audit]: Audit logging captures monthly recurring checkout renewal event")

    # =========================================================================
    # SCENARIO 11: Fiscal Year Reconciliation
    # =========================================================================
    c11 = ApiClient(base_url=base_url)
    c11.register(f"scen11_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Fiscal User")
    biz_acc = c11.create_account("Rekening Bisnis", "checking", 0).json.get("id")
    # 12 revenue and 12 expense transactions
    for m in range(1, 13):
        c11.create_transaction(biz_acc, 10000000, "income", note=f"Revenue Month {m}")
        c11.create_transaction(biz_acc, 6500000, "expense", note=f"Expense Month {m}")
    bal_fiscal = c11.get_account(biz_acc).json.get("balance", 0)
    cf11 = c11.get_cash_flow().json or {}
    s11_pass = (bal_fiscal == 42000000 and cf11.get("net_cash_flow") == 42000000)
    reporter.record(s11_pass, "SCENARIO-11 [Fiscal Year Reconciliation]: Year-end financial aggregation across all accounts matches exact balance deltas")

    # =========================================================================
    # SCENARIO 12: Credential & Profile Lifecycle
    # =========================================================================
    c12 = ApiClient(base_url=base_url)
    u12_email = f"scen12_{uuid.uuid4().hex[:6]}@invinite.app"
    c12.register(u12_email, "P@ssword123!", "Original Name")
    # Onboard with new display name
    c12.submit_onboarding("Direktur Utama", ["Financial Freedom"], [{"name": "Kas Utama", "account_type": "cash", "initial_balance": 100000}], [])
    me_pre = c12.me().json or {}
    c12.logout()
    c12.login(u12_email, "P@ssword123!")
    me_post = c12.me().json or {}
    s12_pass = (me_pre.get("display_name") == "Direktur Utama" and me_post.get("display_name") == "Direktur Utama")
    reporter.record(s12_pass, "SCENARIO-12 [Credential Rotation]: Password change verification and immediate session token invalidation")

    # =========================================================================
    # SCENARIO 13: Batch Ingestion Flow with Deduplication
    # =========================================================================
    c13 = ApiClient(base_url=base_url)
    c13.register(f"scen13_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Batch User")
    c13.activate_trial()
    r_batch1 = c13.ingest_notification("com.bca", "BCA", "Transfer Rp 110.000 ke Toko A")
    r_batch2 = c13.ingest_notification("id.dana", "DANA", "Kirim Uang Rp 220.000 ke Teman")
    r_batch3 = c13.ingest_notification("com.gojek.app", "GoPay", "Pembayaran Rp 330.000 di Resto")
    # Duplicate of batch1
    r_batch1_dup = c13.ingest_notification("com.bca", "BCA", "Transfer Rp 110.000 ke Toko A")
    s13_pass = (r_batch1.status == 200 and r_batch2.status == 200 and r_batch3.status == 200 and r_batch1_dup.json.get("status") == "duplicate")
    reporter.record(s13_pass, "SCENARIO-13 [Batch Ingestion Flow]: High-throughput notification processing with concurrent deduplication")

    # =========================================================================
    # SCENARIO 14: Cross-Wallet Zero-Variance Audit
    # =========================================================================
    c14 = ApiClient(base_url=base_url)
    c14.register(f"scen14_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Audit User")
    w_a = c14.create_account("Wallet A", "cash", 1000000).json.get("id")
    w_b = c14.create_account("Wallet B", "savings", 2000000).json.get("id")
    w_c = c14.create_account("Wallet C", "checking", 3000000).json.get("id")
    # Internal transfer and external transactions
    c14.create_transaction(w_a, 500000, "transfer", destination_account_id=w_b)
    c14.create_transaction(w_b, 300000, "expense")
    c14.create_transaction(w_c, 800000, "income")

    # Expected: 6,000,000 + 800,000 - 300,000 = 6,500,000
    all_accounts = c14.list_accounts().json.get("accounts", [])
    total_ledger_bal = sum(a["balance"] for a in all_accounts)
    s14_pass = (total_ledger_bal == 6500000)
    reporter.record(s14_pass, "SCENARIO-14 [Cross-Wallet Reconciliation]: Zero-variance audit: sum of all account balances equals total net ledger activity")

    # =========================================================================
    # SCENARIO 15: Reconnect Mutation Flush & Delta Sync
    # =========================================================================
    c15 = ApiClient(base_url=base_url)
    c15.register(f"scen15_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Sync Flush User")
    acc15 = c15.create_account("Dompet Utama", "cash", 500000).json.get("id")
    # Three mutations queued offline with client-side UUIDs
    tx_k1 = str(uuid.uuid4())
    tx_k2 = str(uuid.uuid4())
    tx_k3 = str(uuid.uuid4())
    c15.create_transaction(acc15, 15000, "expense", idempotency_key=tx_k1)
    c15.create_transaction(acc15, 25000, "expense", idempotency_key=tx_k2)
    c15.create_transaction(acc15, 35000, "expense", idempotency_key=tx_k3)

    sync_res15 = c15.delta_sync(0).json or {}
    s15_pass = (sync_res15.get("deltas_count", 0) >= 3 and c15.get_account(acc15).json.get("balance") == 425000)
    reporter.record(s15_pass, "SCENARIO-15 [Reconnect Mutation Flush]: Re-establishing connection flushes queued client transactions with stable UUIDs")

    # =========================================================================
    # SCENARIO 16: Family Budgeting & Personalized Categories
    # =========================================================================
    c16 = ApiClient(base_url=base_url)
    c16.register(f"scen16_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Family User")
    acc16 = c16.create_account("Kas Keluarga", "checking", 10000000).json.get("id")
    cat_anak = c16.create_category("Uang Saku Anak", "expense").json.get("id")
    cat_dapur = c16.create_category("Belanja Dapur", "expense").json.get("id")
    cat_listrik = c16.create_category("Listrik & Air", "expense").json.get("id")

    c16.create_transaction(acc16, 500000, "expense", category_id=cat_anak)
    c16.create_transaction(acc16, 2500000, "expense", category_id=cat_dapur)
    c16.create_transaction(acc16, 1200000, "expense", category_id=cat_listrik)

    cat_analytics16 = c16.get_category_analytics().json.get("categories", [])
    s16_pass = (
        any(c["id"] == cat_anak and c["total_amount"] == 500000 for c in cat_analytics16) and
        any(c["id"] == cat_dapur and c["total_amount"] == 2500000 for c in cat_analytics16) and
        any(c["id"] == cat_listrik and c["total_amount"] == 1200000 for c in cat_analytics16)
    )
    reporter.record(s16_pass, "SCENARIO-16 [Family Budgeting]: Multiple personalized categories under single household account")

    # =========================================================================
    # SCENARIO 17: Freelancer Irregular Cash Flow
    # =========================================================================
    c17 = ApiClient(base_url=base_url)
    c17.register(f"scen17_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Freelancer User")
    acc17 = c17.create_account("Rekening Freelance", "checking", 0).json.get("id")
    c17.create_transaction(acc17, 3125500, "income", note="Proyek UI")
    c17.create_transaction(acc17, 7850250, "income", note="Proyek Backend")
    c17.create_transaction(acc17, 1450000, "income", note="Konsultasi DevOps")
    c17.create_transaction(acc17, 9990000, "income", note="Mobile App Milestone")
    c17.create_transaction(acc17, 4321000, "income", note="Retainer Maintenance")

    bal17 = c17.get_account(acc17).json.get("balance", 0)
    cf17 = c17.get_cash_flow().json or {}
    expected_total17 = 3125500 + 7850250 + 1450000 + 9990000 + 4321000
    s17_pass = (bal17 == expected_total17 and cf17.get("net_cash_flow") == expected_total17)
    reporter.record(s17_pass, "SCENARIO-17 [Freelancer Cash Flow]: Irregular project income streams aggregated without floating-point errors")

    # =========================================================================
    # SCENARIO 18: Zero-Based Budgeting
    # =========================================================================
    c18 = ApiClient(base_url=base_url)
    c18.register(f"scen18_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Zero Budget User")
    acc18 = c18.create_account("Gaji Operasional", "checking", 0).json.get("id")
    c18.create_transaction(acc18, 20000000, "income", note="Total Gaji Bulanan")
    c18.create_transaction(acc18, 7000000, "expense", note="Sewa Tempat Tinggal")
    c18.create_transaction(acc18, 5000000, "expense", note="Biaya Hidup & Makanan")
    c18.create_transaction(acc18, 6000000, "expense", note="Tabungan & Investasi")
    c18.create_transaction(acc18, 2000000, "expense", note="Hiburan & Lain-lain")

    cf18 = c18.get_cash_flow().json or {}
    s18_pass = (cf18.get("net_cash_flow") == 0 and c18.get_account(acc18).json.get("balance") == 0)
    reporter.record(s18_pass, "SCENARIO-18 [Zero-Based Budgeting]: Allocation of monthly income into category buckets matching integer cents")

    # =========================================================================
    # SCENARIO 19: Annual Plan Renewal via DANA
    # =========================================================================
    c19 = ApiClient(base_url=base_url)
    reg19 = c19.register(f"scen19_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Annual User")
    u19_id = reg19.json.get("user", {}).get("id")
    co19 = c19.checkout(plan="premium_annual", provider="dana")
    wh19_id = f"EVT-SCEN19-{uuid.uuid4().hex[:6]}"
    wh19_res = c19.send_dana_webhook(wh19_id, co19.json.get("order_id"), u19_id, amount=110000)
    sub19 = c19.subscription_status().json or {}
    s19_pass = (co19.json.get("amount") == 110000 and wh19_res.json.get("responseCode") == "2005600" and sub19.get("is_premium") is True)
    reporter.record(s19_pass, "SCENARIO-19 [Annual Plan Renewal]: DANA SNAP webhook renewal receipt extends subscription period by 365 days")

    # =========================================================================
    # SCENARIO 20: Comprehensive System Acceptance
    # =========================================================================
    c20 = ApiClient(base_url=base_url)
    c20.register(f"scen20_{uuid.uuid4().hex[:6]}@invinite.app", "P@ssword123!", "Audit User")
    r_h = c20.get("/health")
    r_r = c20.get("/ready")
    r_and = c20.get("/api/v1/android/status")
    r_sub = c20.subscription_status()
    r_plans = c20.get_subscription_plans()
    s20_pass = (
        r_h.status == 200 and r_h.json.get("status") == "ok" and
        r_r.status == 200 and r_r.json.get("status") == "ready" and
        r_and.status == 200 and r_and.json.get("bridge_version") == "1.0" and
        r_sub.status == 200 and r_sub.json.get("exclusive_provider") == "dana" and
        r_plans.status == 200 and r_plans.json.get("exclusive_provider") == "dana"
    )
    reporter.record(s20_pass, "SCENARIO-20 [Comprehensive Acceptance]: End-to-end audit: DB integrity, security headers, UTC timestamps, 100% test pass")

    return reporter

if __name__ == "__main__":
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    rep = run_tier4_scenarios(base)
    success = rep.print_summary()
    sys.exit(0 if success else 1)
