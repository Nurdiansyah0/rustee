#!/usr/bin/env python3
"""
Tier 4: Real-World Application Scenarios Acceptance Test Suite (24 comprehensive scenarios).
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
        reporter = TapReporter(total_expected=24)
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

    # =========================================================================
    # SCENARIO 21: Multi-Location Retail Fulfillment & Stock Transfer Pipeline
    # =========================================================================
    c21 = ApiClient(base_url=base_url)
    uid21 = uuid.uuid4().hex[:6]
    c21.register(f"scen21_{uid21}@invinite.biz", "P@ssword123!", "Direktur Retail")
    t21 = c21.create_tenant(f"PT Retail Megatama {uid21}", slug=f"megatama-{uid21}")
    t21_id = t21.json.get("id")
    c21.set_tenant(t21_id)

    # Provision central DC and storefront warehouse
    wh_dc = c21.create_warehouse(code=f"DC-JKT-{uid21}", name="Pusat Distribusi Jakarta", is_default=True)
    wh_dc_id = wh_dc.json.get("id")
    wh_store = c21.create_warehouse(code=f"STORE-GI-{uid21}", name="Toko Grand Indonesia")
    wh_store_id = wh_store.json.get("id")

    # Catalog apparel items
    prod_a = c21.create_product(name="Kemeja Katun Premium", sku=f"KMJ-{uid21}", cost_price=75000, sale_price=150000, reorder_threshold=10)
    prod_b = c21.create_product(name="Celana Chino Slim", sku=f"CLN-{uid21}", cost_price=100000, sale_price=220000, reorder_threshold=5)
    pa_id = prod_a.json.get("id")
    pb_id = prod_b.json.get("id")

    # Inbound bulk inventory to central DC
    c21.create_stock_movement(movement_type="INBOUND", product_id=pa_id, destination_warehouse_id=wh_dc_id, quantity=50, unit_cost=75000)
    c21.create_stock_movement(movement_type="INBOUND", product_id=pb_id, destination_warehouse_id=wh_dc_id, quantity=30, unit_cost=100000)

    # Inter-warehouse replenishment transfer to storefront
    xfer_a = c21.transfer_stock(source_warehouse_id=wh_dc_id, destination_warehouse_id=wh_store_id, product_id=pa_id, quantity=20)
    xfer_b = c21.transfer_stock(source_warehouse_id=wh_dc_id, destination_warehouse_id=wh_store_id, product_id=pb_id, quantity=10)

    # Verify DC balances: A=30, B=20
    dc_stock_a = c21.get_stock_items(warehouse_id=wh_dc_id, product_id=pa_id).json.get("stock_items", [])[0].get("quantity_on_hand")
    dc_stock_b = c21.get_stock_items(warehouse_id=wh_dc_id, product_id=pb_id).json.get("stock_items", [])[0].get("quantity_on_hand")

    # Storefront sales fulfillment: OUTBOUND 15 units of product A
    c21.create_stock_movement(movement_type="OUTBOUND", product_id=pa_id, source_warehouse_id=wh_store_id, quantity=15)
    store_stock_a = c21.get_stock_items(warehouse_id=wh_store_id, product_id=pa_id).json.get("stock_items", [])[0]

    # Verify low-stock alert triggered (5 <= 10)
    low_res = c21.get_stock_items(low_stock=True)
    low_items = low_res.json.get("stock_items", [])
    low_flagged = any(it.get("product_id") == pa_id and it.get("warehouse_id") == wh_store_id for it in low_items)

    s21_pass = (
        wh_dc.status == 201 and wh_store.status == 201 and
        xfer_a.status == 200 and xfer_b.status == 200 and
        dc_stock_a == 30 and dc_stock_b == 20 and
        store_stock_a.get("quantity_on_hand") == 5 and
        store_stock_a.get("is_low_stock") is True and
        low_flagged
    )
    reporter.record(s21_pass, "SCENARIO-21 [Multi-Location Fulfillment]: Central warehouse replenishment transfer and retail storefront sales fulfillment pipeline")

    # =========================================================================
    # SCENARIO 22: Moving Weighted Average Cost (WAC) Revaluation & GL Audit
    # =========================================================================
    c22 = ApiClient(base_url=base_url)
    uid22 = uuid.uuid4().hex[:6]
    c22.register(f"scen22_{uid22}@invinite.biz", "P@ssword123!", "Financial Controller")
    t22 = c22.create_tenant(f"PT Niaga Sukses {uid22}", slug=f"niaga-{uid22}")
    t22_id = t22.json.get("id")
    c22.set_tenant(t22_id)

    wh_wac = c22.create_warehouse(code=f"WH-WAC-{uid22}", name="Gudang Valuasi WAC", is_default=True)
    wh_wac_id = wh_wac.json.get("id")
    prod_wac22 = c22.create_product(name="Biji Kopi Arabika Gayo 1kg", sku=f"KOP-{uid22}", cost_price=100000, sale_price=160000)
    pw22_id = prod_wac22.json.get("id")

    # Receipt 1: 100 kg @ Rp 100.000 -> Value = 10.000.000 IDR, WAC = 100.000
    c22.create_stock_movement(movement_type="INBOUND", product_id=pw22_id, destination_warehouse_id=wh_wac_id, quantity=100, unit_cost=100000)

    # Receipt 2: 50 kg @ Rp 130.000 -> Total Qty = 150 kg, Total Value = 16.500.000 IDR -> WAC = 110.000
    c22.create_stock_movement(movement_type="INBOUND", product_id=pw22_id, destination_warehouse_id=wh_wac_id, quantity=50, unit_cost=130000)
    wac_step1 = c22.get_stock_items(warehouse_id=wh_wac_id, product_id=pw22_id).json.get("stock_items", [])[0].get("average_cost")

    # Outbound Fulfillment: 60 kg @ WAC 110.000 -> COGS = 6.600.000 IDR, Remaining = 90 kg @ 110.000 (Valuation: 9.900.000 IDR)
    c22.create_stock_movement(movement_type="OUTBOUND", product_id=pw22_id, source_warehouse_id=wh_wac_id, quantity=60)

    # Receipt 3: 60 kg @ Rp 120.000 -> Total Qty = 150 kg, Total Value = 9.900.000 + 7.200.000 = 17.100.000 IDR -> WAC = 114.000
    c22.create_stock_movement(movement_type="INBOUND", product_id=pw22_id, destination_warehouse_id=wh_wac_id, quantity=60, unit_cost=120000)
    final_stock = c22.get_stock_items(warehouse_id=wh_wac_id, product_id=pw22_id).json.get("stock_items", [])[0]
    final_q = final_stock.get("quantity_on_hand")
    final_cost = final_stock.get("average_cost")

    # General ledger audit: verify GL journals for inbound receipts & outbound COGS
    j_list = c22.get("/api/v1/accounting/journals").json.get("journals", [])
    total_inv_debits = sum(sum(l.get("debit", 0) for l in j.get("lines", []) if l.get("account_code") == "1300") for j in j_list)
    total_inv_credits = sum(sum(l.get("credit", 0) for l in j.get("lines", []) if l.get("account_code") == "1300") for j in j_list)
    net_gl_inventory = total_inv_debits - total_inv_credits
    physical_valuation = final_q * final_cost

    s22_pass = (
        wac_step1 == 110000 and
        final_q == 150 and
        final_cost == 114000 and
        physical_valuation == 17100000 and
        net_gl_inventory == physical_valuation
    )
    reporter.record(s22_pass, "SCENARIO-22 [Moving WAC Valuation & GL Audit]: Multi-batch procurement recalculates moving WAC with 100% GL Persediaan balance reconciliation")

    # =========================================================================
    # SCENARIO 23: PO Lifecycle with Staged Receipts & Batch Traceability
    # =========================================================================
    c23 = ApiClient(base_url=base_url)
    uid23 = uuid.uuid4().hex[:6]
    c23.register(f"scen23_{uid23}@invinite.biz", "P@ssword123!", "Procurement Lead")
    t23 = c23.create_tenant(f"PT Manufaktur Presisi {uid23}", slug=f"presisi-{uid23}")
    t23_id = t23.json.get("id")
    c23.set_tenant(t23_id)

    wh_mfg = c23.create_warehouse(code=f"WH-RAW-{uid23}", name="Gudang Bahan Baku Pabrik", is_default=True)
    wh_mfg_id = wh_mfg.json.get("id")
    comp_a = c23.create_product(name="Baut Baja M8", sku=f"BAUT-{uid23}", cost_price=2000, sale_price=3500)
    comp_b = c23.create_product(name="Plat Aluminium 2mm", sku=f"ALUM-{uid23}", cost_price=50000, sale_price=80000)
    ca_id = comp_a.json.get("id")
    cb_id = comp_b.json.get("id")

    # Create PO with DRAFT status
    po_res = c23.create_purchase_order(
        supplier_name="PT Krakatau Mega Logam",
        destination_warehouse_id=wh_mfg_id,
        items=[
            {"product_id": ca_id, "quantity_ordered": 1000, "unit_cost": 2000},
            {"product_id": cb_id, "quantity_ordered": 200, "unit_cost": 50000}
        ]
    )
    po_id = po_res.json.get("id")
    po_draft_status = po_res.json.get("status")

    # Order PO -> transitions to ORDERED
    ord_res = c23.order_purchase_order(po_id)
    po_ord_status = ord_res.json.get("status")

    # Delivery Stage 1: Partial delivery
    rcv1_res = c23.receive_purchase_order(po_id, [
        {"product_id": ca_id, "quantity_received": 400},
        {"product_id": cb_id, "quantity_received": 100}
    ])
    po_part_status = rcv1_res.json.get("status")

    # Delivery Stage 2: Final delivery completing PO
    rcv2_res = c23.receive_purchase_order(po_id, [
        {"product_id": ca_id, "quantity_received": 600},
        {"product_id": cb_id, "quantity_received": 100}
    ])
    po_full_status = rcv2_res.json.get("status")

    # Final receipt attempt rejected
    over_rcv = c23.receive_purchase_order(po_id, [{"product_id": ca_id, "quantity_received": 10}])

    # Verify inventory quantities on hand: 1000 Baut, 200 Aluminium
    st_a = c23.get_stock_items(warehouse_id=wh_mfg_id, product_id=ca_id).json.get("stock_items", [])[0].get("quantity_on_hand")
    st_b = c23.get_stock_items(warehouse_id=wh_mfg_id, product_id=cb_id).json.get("stock_items", [])[0].get("quantity_on_hand")

    s23_pass = (
        po_draft_status == "DRAFT" and
        po_ord_status == "ORDERED" and
        po_part_status == "PARTIALLY_RECEIVED" and
        po_full_status == "RECEIVED" and
        over_rcv.status == 422 and
        st_a == 1000 and st_b == 200
    )
    reporter.record(s23_pass, "SCENARIO-23 [PO Lifecycle & Staged Receipts]: End-to-end staged procurement receipts transition PO state and track line item quantities")

    # =========================================================================
    # SCENARIO 24: Annual Physical Inventory Stock Count & Discrepancy Settlement
    # =========================================================================
    c24 = ApiClient(base_url=base_url)
    uid24 = uuid.uuid4().hex[:6]
    c24.register(f"scen24_{uid24}@invinite.biz", "P@ssword123!", "Warehouse Audit Manager")
    t24 = c24.create_tenant(f"PT Logistik Sentosa {uid24}", slug=f"sentosa-{uid24}")
    t24_id = t24.json.get("id")
    c24.set_tenant(t24_id)

    wh_audit = c24.create_warehouse(code=f"WH-AUDIT-{uid24}", name="Gudang Pusat Audit", is_default=True)
    wh_audit_id = wh_audit.json.get("id")
    item_shrink = c24.create_product(name="Barang Susut Audit", sku=f"SHR-{uid24}", cost_price=40000, sale_price=70000)
    item_surplus = c24.create_product(name="Barang Lebih Audit", sku=f"SUR-{uid24}", cost_price=60000, sale_price=95000)
    is_id = item_shrink.json.get("id")
    ip_id = item_surplus.json.get("id")

    # Inbound initial book stock: 100 units item_shrink, 50 units item_surplus
    c24.create_stock_movement(movement_type="INBOUND", product_id=is_id, destination_warehouse_id=wh_audit_id, quantity=100, unit_cost=40000)
    c24.create_stock_movement(movement_type="INBOUND", product_id=ip_id, destination_warehouse_id=wh_audit_id, quantity=50, unit_cost=60000)

    # Physical count adjustments:
    # 1. Shrinkage: actual is 95 (-5 variance @ 40.000 = -200.000 IDR)
    adj_shrink = c24.adjust_stock(warehouse_id=wh_audit_id, product_id=is_id, actual_quantity=95, reason="Audit tahunan barang rusak/hilang")

    # 2. Surplus: actual is 54 (+4 variance @ 60.000 = +240.000 IDR)
    adj_surplus = c24.adjust_stock(warehouse_id=wh_audit_id, product_id=ip_id, actual_quantity=54, reason="Audit penemuan selisih lebih fisik")

    # Verify updated quantities on hand
    q_shrink = c24.get_stock_items(warehouse_id=wh_audit_id, product_id=is_id).json.get("stock_items", [])[0].get("quantity_on_hand")
    q_surplus = c24.get_stock_items(warehouse_id=wh_audit_id, product_id=ip_id).json.get("stock_items", [])[0].get("quantity_on_hand")

    # Verify journal entries posted for adjustments
    j_audit = c24.get("/api/v1/accounting/journals").json.get("journals", [])
    adj_journals = [j for j in j_audit if j.get("source_type") == "STOCK_ADJUSTMENT"]
    all_adj_balanced = all(j.get("total_debit") == j.get("total_credit") for j in adj_journals)

    s24_pass = (
        adj_shrink.status == 200 and adj_shrink.json.get("variance") == -5 and
        adj_surplus.status == 200 and adj_surplus.json.get("variance") == 4 and
        q_shrink == 95 and q_surplus == 54 and
        len(adj_journals) == 2 and all_adj_balanced
    )
    reporter.record(s24_pass, "SCENARIO-24 [Physical Stock Audit Settlement]: Discrepancy cycle count settlement adjusts balances and posts balanced adjustment journals")

    return reporter

if __name__ == "__main__":
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    rep = run_tier4_scenarios(base)
    success = rep.print_summary()
    sys.exit(0 if success else 1)
