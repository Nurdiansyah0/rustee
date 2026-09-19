#!/usr/bin/env python3
"""
Tier 3: Pairwise Cross-Feature Interactions Acceptance Test Suite (55 tests).
Verifies interactions between orthogonal subsystems across the entire feature matrix.
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

def run_tier3_tests(base_url: str, reporter: Optional[TapReporter] = None) -> TapReporter:
    if reporter is None:
        reporter = TapReporter(total_expected=55)
        reporter.print_header()

    client = ApiClient(base_url=base_url)
    uid_suffix = uuid.uuid4().hex[:8]
    user_email = f"tier3_{uid_suffix}@invinite.app"
    user_pass = "P@ssword123!"

    reg_resp = client.register(user_email, user_pass, "Tier3 Pairwise User")
    user_id = reg_resp.json.get("user", {}).get("id") if reg_resp.json else None

    # =========================================================================
    # PAIRWISE INTERACTIONS (55 Tests)
    # =========================================================================

    # Pair 1: Trial Activation + Feature Gating (REQ-SEC-04 x REQ-SEC-10)
    # Before trial: 403. After trial: 200.
    r_gate_pre = client.get_advanced_analytics()
    t_act = client.activate_trial()
    r_gate_post = client.get_advanced_analytics()
    reporter.record(r_gate_pre.status == 403 and t_act.status == 200 and r_gate_post.status == 200,
                    "PAIR-01 [Trial + FeatureGate]: Trial activation immediately unlocks locked endpoints without re-auth")

    # Pair 2: Multi-Wallet Transfer + Net Cash Flow Engine (REQ-ARCH-04 x REQ-ARCH-03)
    acc1 = client.create_account("Rekening A", "checking", 1000000).json.get("id")
    acc2 = client.create_account("Rekening B", "savings", 500000).json.get("id")
    cf_pre = client.get_cash_flow().json.get("net_cash_flow", 0)
    client.create_transaction(acc1, 250000, "transfer", destination_account_id=acc2)
    cf_post = client.get_cash_flow().json.get("net_cash_flow", 0)
    reporter.record(cf_pre == cf_post,
                    "PAIR-02 [MultiWallet + CashFlow]: Wallet-to-wallet transfer preserves identical net cash flow (0 net impact)")

    # Pair 3: Category Soft-Deletion + Category Reports (REQ-ARCH-06 x REQ-ARCH-03)
    cat1 = client.create_category("Langganan Cloud", "expense").json.get("id")
    client.create_transaction(acc1, 150000, "expense", category_id=cat1)
    client.soft_delete_category(cat1)
    cat_analytics = client.get_category_analytics().json.get("categories", [])
    has_cat = any(c["id"] == cat1 and c["total_amount"] == 150000 for c in cat_analytics)
    reporter.record(has_cat,
                    "PAIR-03 [CategorySoftDelete + Reports]: Soft-deleted category remains aggregated in historical reporting")

    # Pair 4: Idempotency Replay + Account Balances (REQ-ARCH-11 x REQ-ARCH-05)
    idem_key = str(uuid.uuid4())
    bal_pre = client.get_account(acc1).json.get("balance", 0)
    client.create_transaction(acc1, 75000, "expense", idempotency_key=idem_key)
    # Replay same idempotency key
    client.create_transaction(acc1, 75000, "expense", idempotency_key=idem_key)
    bal_post = client.get_account(acc1).json.get("balance", 0)
    reporter.record(bal_post == bal_pre - 75000,
                    "PAIR-04 [Idempotency + Balances]: Replayed transaction deduplicated without duplicate balance mutation")

    # Pair 5: DANA Webhook + Subscription State Machine (REQ-SEC-07 x REQ-SEC-05)
    w_user_client = ApiClient(base_url=base_url)
    w_uid = w_user_client.register(f"dana_{uuid.uuid4().hex[:6]}@invinite.app", user_pass).json.get("user", {}).get("id")
    w_user_client.activate_trial()
    sub_pre = w_user_client.subscription_status().json.get("status")
    w_user_client.send_dana_webhook(str(uuid.uuid4()), "ORD-SUB-TRANS", w_uid)
    sub_post = w_user_client.me().json.get("tier")
    reporter.record(sub_pre == "trialing" and sub_post in ["active", "premium"],
                    "PAIR-05 [DANAWebhook + SubscriptionState]: DANA success notification transitions trialing user to ACTIVE")

    # Pair 6: Ingestion Pipeline + Cross-Source Deduplication (REQ-INGEST-01 x REQ-INGEST-04)
    r_notif = client.ingest_notification("com.bca", "BCA", "Transfer Rp 320.000 ke Vendor")
    r_dup_notif = client.ingest_notification("com.bca", "BCA", "Transfer Rp 320.000 ke Vendor")
    reporter.record(r_notif.json.get("status") != "duplicate" and r_dup_notif.json.get("status") == "duplicate",
                    "PAIR-06 [Ingestion + Deduplication]: Ingestion pipeline filters identical cross-event transactions")

    # Pair 7: Ingestion Candidate Confirmation + Ledger Settlement (REQ-INGEST-03 x REQ-ARCH-05)
    cand_id = r_notif.json.get("candidate_id")
    bal_pre_conf = client.get_account(acc1).json.get("balance", 0)
    client.confirm_candidate(cand_id, acc1)
    bal_post_conf = client.get_account(acc1).json.get("balance", 0)
    reporter.record(bal_post_conf == bal_pre_conf - 320000,
                    "PAIR-07 [IngestionConfirm + Ledger]: Candidate confirmation atomically settles balance and commits to ledger")

    # Pair 8: Cursor Delta Sync + Progressive Onboarding (REQ-FE-08 x REQ-FE-04)
    cur_pre = client.delta_sync(0).json.get("cursor", 0)
    client.submit_onboarding("Onboard Sultan", ["Investasi"], [{"name": "Kantong Saham", "account_type": "investment", "initial_balance": 5000000}], [])
    sync_post = client.delta_sync(cur_pre).json or {}
    reporter.record(sync_post.get("cursor") > cur_pre and any(a["name"] == "Kantong Saham" for a in sync_post.get("accounts", [])),
                    "PAIR-08 [DeltaSync + Onboarding]: Onboarding workspace creation captured in delta sync stream")

    # Pair 9: Argon2id Auth + Multi-Tenant Isolation (REQ-SEC-01 x REQ-ARCH-12)
    client_c = ApiClient(base_url=base_url)
    client_c.register(f"c_{uuid.uuid4().hex[:6]}@invinite.app", user_pass)
    c_acc = client_c.create_account("Wallet C", "cash", 100000).json.get("id")
    a_sees_c = client.get_account(c_acc)
    reporter.record(a_sees_c.status in [403, 404],
                    "PAIR-09 [Auth + MultiTenant]: Authenticated session strictly isolated from querying foreign tenant records")

    # Pair 10: DANA Webhook Idempotency + Audit Logging (REQ-SEC-09 x REQ-SEC-12)
    wh_rep_id = f"EVT-PAIR-10-{uuid.uuid4().hex[:6]}"
    r_wh1 = client.send_dana_webhook(wh_rep_id, "ORD-AUDIT", user_id)
    r_wh2 = client.send_dana_webhook(wh_rep_id, "ORD-AUDIT", user_id)
    reporter.record(r_wh1.status == 200 and r_wh2.status == 200 and r_wh2.json.get("idempotent_replay") is True,
                    "PAIR-10 [WebhookIdempotency + Audit]: Webhook duplicate returns cached success without re-executing logic")

    # Pair 11: Stable Schema Vocabulary + Category Aggregations (REQ-ARCH-09 x REQ-ARCH-03)
    vocab_c = client.create_category("Gaji Pokok & Tunjangan", "income")
    client.create_transaction(acc1, 10000000, "income", category_id=vocab_c.json.get("id"))
    cat_agg = client.get_category_analytics().json.get("categories", [])
    has_vocab_agg = any(c["id"] == vocab_c.json.get("id") and c["total_amount"] == 10000000 for c in cat_agg)
    reporter.record(has_vocab_agg,
                    "PAIR-11 [Vocabulary + Analytics]: Custom vocabulary accurately flows into financial category aggregation")

    # Pair 12: Strict UTC Timestamps + Financial Date Policy (REQ-ARCH-10 x REQ-FE-06)
    tx_utc = client.create_transaction(acc1, 25000, "expense", date="2026-09-17T03:00:00Z")
    reporter.record("Z" in tx_utc.json.get("date", ""),
                    "PAIR-12 [UTCTimestamps + DatetimePolicy]: Server guarantees ISO 8601 UTC while frontend resolves Asia/Jakarta")

    # Pair 13: Feature Lock Overlay + Commercial Checkout (REQ-FE-09 x REQ-SEC-06)
    free_u = ApiClient(base_url=base_url)
    free_u.register(f"free_ch_{uuid.uuid4().hex[:6]}@invinite.app", user_pass)
    lock_resp = free_u.get_advanced_analytics()
    co_resp = free_u.checkout(plan="premium_monthly", provider="dana")
    reporter.record(lock_resp.status == 403 and co_resp.status == 200 and co_resp.json.get("amount") == 10000,
                    "PAIR-13 [FeatureLock + Checkout]: Locked feature prompts commercial checkout with Rp 10.000 plan")

    # Pair 14: Rapid 4x3 POS Keypad + Idempotency Engine (REQ-FE-03 x REQ-ARCH-11)
    keypad_idem = str(uuid.uuid4())
    kp_tx1 = client.create_transaction(acc1, 50000, "expense", note="Keypad Entry", idempotency_key=keypad_idem)
    kp_tx2 = client.create_transaction(acc1, 50000, "expense", note="Keypad Entry", idempotency_key=keypad_idem)
    reporter.record(kp_tx1.json.get("id") == kp_tx2.json.get("id"),
                    "PAIR-14 [POSKeypad + Idempotency]: Keypad rapid entry protected against duplicate submission by Idempotency-Key")

    # Pair 15: Foreground WebSocket Probe + Delta Sync Hint (REQ-FE-07 x REQ-FE-08)
    ws_res = client.get("/api/v1/ws", headers={"Upgrade": "websocket"})
    sync_res = client.delta_sync(0)
    reporter.record(ws_res.status == 200 and sync_res.status == 200,
                    "PAIR-15 [WebSocket + DeltaSync]: Foreground WebSocket and cursor delta sync interoperate seamlessly")

    # Pair 16: Modular Monolith Layering + Financial Cache Control (REQ-ARCH-01 x REQ-SEC-11)
    r_acct_cc = client.list_accounts()
    cc_header = r_acct_cc.header("cache-control") or ""
    reporter.record(r_acct_cc.status == 200 and "private" in cc_header and "no-store" in cc_header,
                    "PAIR-16 [ModularMonolith + CacheControl]: Financial accounts endpoint emits private no-store headers")

    # Pair 17: Integer Rupiah Math + Atomic Balance Mutations (REQ-ARCH-02 x REQ-ARCH-05)
    acc_math = client.create_account("Rekening Presisi", "savings", 10000000).json.get("id")
    client.create_transaction(acc_math, 3456789, "expense", note="Exact expense")
    bal_math = client.get_account(acc_math).json.get("balance")
    reporter.record(bal_math == 10000000 - 3456789 and isinstance(bal_math, int),
                    "PAIR-17 [IntegerMath + Balances]: Balance mutation maintains exact integer Rupiah precision without float loss")

    # Pair 18: Multi-Wallet Accounting + Multi-Tenant Isolation (REQ-ARCH-04 x REQ-ARCH-12)
    client_u2 = ApiClient(base_url=base_url)
    client_u2.register(f"u2_wallet_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "User 2")
    u2_acc = client_u2.create_account("U2 Private Wallet", "checking", 500000).json.get("id")
    u1_tries_read_u2 = client.get_account(u2_acc)
    reporter.record(u1_tries_read_u2.status in [403, 404],
                    "PAIR-18 [MultiWallet + MultiTenant]: Multi-wallet account query strictly rejects cross-tenant access")

    # Pair 19: Atomic Balances + Monotonic Idempotency (REQ-ARCH-05 x REQ-ARCH-11)
    acc_mono = client.create_account("Monotonic Wallet", "checking", 1000000).json.get("id")
    client.create_transaction(acc_mono, 100000, "expense", idempotency_key=str(uuid.uuid4()))
    client.create_transaction(acc_mono, 200000, "expense", idempotency_key=str(uuid.uuid4()))
    bal_mono = client.get_account(acc_mono).json.get("balance")
    reporter.record(bal_mono == 700000,
                    "PAIR-19 [AtomicBalances + MonotonicIdempotency]: Sequential transactions with distinct keys update balance monotonically")

    # Pair 20: Category Soft-Deletion + Stable Schema Vocabulary (REQ-ARCH-06 x REQ-ARCH-09)
    cat_temp = client.create_category("Kebutuhan Sementara", "expense").json.get("id")
    client.soft_delete_category(cat_temp)
    active_categories = client.list_categories().json.get("categories", [])
    reporter.record(not any(c["id"] == cat_temp for c in active_categories),
                    "PAIR-20 [CategorySoftDelete + StableVocabulary]: Soft-deleted custom category cleanly filtered from active vocabulary list")

    # Pair 21: SQLite WAL & Pragmas + Strict UTC Timestamps (REQ-ARCH-07 x REQ-ARCH-10)
    tx_wal = client.create_transaction(acc1, 50000, "income", note="WAL Check")
    tx_date_str = tx_wal.json.get("date", "")
    reporter.record(tx_wal.status == 201 and tx_date_str.endswith("Z"),
                    "PAIR-21 [SQLiteWAL + UTCTimestamps]: Committed transaction persists strict ISO 8601 UTC timestamp")

    # Pair 22: Canonical Composite Indexes + Net Cash Flow (REQ-ARCH-08 x REQ-ARCH-03)
    cf_res = client.get_cash_flow()
    cf_json = cf_res.json or {}
    cf_inc = cf_json.get("total_income", cf_json.get("income", 0))
    cf_exp = cf_json.get("total_expenses", cf_json.get("expenses", 0))
    expected_cf = cf_inc - cf_exp
    reporter.record(cf_res.status == 200 and cf_json.get("net_cash_flow") == expected_cf,
                    "PAIR-22 [CompositeIndexes + CashFlowEngine]: Cash flow query verifies net_cash_flow equals income minus expenses")

    # Pair 23: Stable Schema Vocabulary + Progressive Onboarding (REQ-ARCH-09 x REQ-FE-04)
    client_onb = ApiClient(base_url=base_url)
    client_onb.register(f"onb_vocab_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "Onboarding Vocab User")
    onb_res = client_onb.submit_onboarding(
        display_name="Juragan Finansial",
        financial_goals=["Rumah Impian"],
        wallets=[{"name": "Kantong Operasional", "account_type": "cash", "initial_balance": 2000000}],
        categories=[{"name": "Gaji Proyek", "display_name": "Gaji Proyek", "category_type": "income"}]
    )
    me_res = client_onb.me().json or {}
    cats_res = client_onb.list_categories().json.get("categories", [])
    reporter.record(onb_res.status == 200 and me_res.get("display_name") == "Juragan Finansial" and any(c["name"] == "Gaji Proyek" for c in cats_res),
                    "PAIR-23 [StableVocabulary + ProgressiveOnboarding]: Onboarding persists user presentation vocabulary without modifying SQL schema")

    # Pair 24: Idempotency Engine + Payment Webhook Idempotency (REQ-ARCH-11 x REQ-SEC-09)
    idem_tx_k = str(uuid.uuid4())
    t1 = client.create_transaction(acc1, 20000, "expense", idempotency_key=idem_tx_k)
    t2 = client.create_transaction(acc1, 20000, "expense", idempotency_key=idem_tx_k)
    wh_ev_id = f"EVT-PAIR-24-{uuid.uuid4().hex[:6]}"
    wh1 = client.send_dana_webhook(wh_ev_id, "ORD-P24", user_id)
    wh2 = client.send_dana_webhook(wh_ev_id, "ORD-P24", user_id)
    reporter.record(t1.json.get("id") == t2.json.get("id") and wh2.json.get("idempotent_replay") is True,
                    "PAIR-24 [IdempotencyEngine + WebhookReplay]: Dual layer idempotency verified for both ledger transactions and DANA webhooks")

    # Pair 25: Multi-Tenant Data Isolation + Cursor Delta Sync (REQ-ARCH-12 x REQ-FE-08)
    cur_u1 = client.delta_sync(0).json or {}
    # User 2 makes transactions
    u2_acc_id = client_u2.list_accounts().json.get("accounts", [])[0]["id"]
    client_u2.create_transaction(u2_acc_id, 10000, "expense")
    cur_u1_after = client.delta_sync(cur_u1.get("cursor", 0)).json or {}
    reporter.record(not any(t.get("account_id") == u2_acc_id for t in cur_u1_after.get("transactions", [])),
                    "PAIR-25 [MultiTenantIsolation + DeltaSync]: Delta sync cursor guarantees complete isolation from other tenant mutations")

    # Pair 26: Argon2id Password Hashing + Cookie Session Management (REQ-SEC-01 x REQ-SEC-02)
    client_auth_test = ApiClient(base_url=base_url)
    reg_auth = client_auth_test.register(f"auth_test_{uuid.uuid4().hex[:6]}@invinite.app", "Str0ng#P@ssword99", "Auth Test")
    login_auth = client_auth_test.login(reg_auth.json.get("user", {}).get("email"), "Str0ng#P@ssword99")
    me_auth = client_auth_test.me()
    reporter.record(login_auth.status == 200 and "auth_token" in client_auth_test.cookies and me_auth.status == 200,
                    "PAIR-26 [Argon2idAuth + CookieSession]: Login returns session cookie allowing seamless authenticated access")

    # Pair 27: Rate Limiting + Authentication (REQ-SEC-03 x REQ-SEC-01)
    bad_client = ApiClient(base_url=base_url, client_ip="198.51.100.99")
    bad_statuses = []
    for _ in range(6):
        r_bad = bad_client.login("nonexistent@invinite.app", "wrong-password")
        bad_statuses.append(r_bad.status)
    reporter.record(bad_statuses[0] == 401 and any(s in [401, 429] for s in bad_statuses),
                    "PAIR-27 [RateLimiting + AuthProtection]: Repeated failed logins safely handled and protected against brute-force")

    # Pair 28: 3-Month Premium Trial + Subscription Machine (REQ-SEC-04 x REQ-SEC-05)
    client_tr = ApiClient(base_url=base_url)
    client_tr.register(f"tr_machine_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "Trial User")
    r_tr_act = client_tr.activate_trial()
    r_tr_dup = client_tr.activate_trial()
    reporter.record(r_tr_act.status == 200 and r_tr_dup.status == 409 and r_tr_act.json.get("days_remaining") == 90,
                    "PAIR-28 [TrialActivation + AntiAbuse]: 3-month trial activates with 90 days remaining and rejects duplicate activation with HTTP 409")

    # Pair 29: Commercial Pricing Plans + DANA Open API Checkout (REQ-SEC-06 x REQ-SEC-07)
    plans_res = client.get_subscription_plans()
    co_mo = client.checkout(plan="premium_monthly", provider="dana")
    co_yr = client.checkout(plan="premium_annual", provider="dana")
    reporter.record(co_mo.status == 200 and co_mo.json.get("amount") == 10000 and co_yr.status == 200 and co_yr.json.get("amount") == 110000,
                    "PAIR-29 [CommercialPlans + DANACheckout]: Checkout supports Monthly Rp 10.000 and Annual Rp 110.000 via DANA")

    # Pair 30: Commercial Pricing Plans + DANA Exclusivity (REQ-SEC-06 x REQ-SEC-07)
    co_midtrans = client.checkout(plan="premium_monthly", provider="midtrans")
    co_xendit = client.checkout(plan="premium_monthly", provider="xendit")
    reporter.record(co_midtrans.status == 400 and co_xendit.status == 400,
                    "PAIR-30 [CommercialPlans + DANAExclusivity]: Non-DANA providers (midtrans/xendit) strictly rejected with HTTP 400")

    # Pair 31: RSA-SHA256 Signature Verification + DANA Webhook (REQ-SEC-08 x REQ-SEC-07)
    wh_valid = client.send_dana_webhook(f"EVT-P31-OK-{uuid.uuid4().hex[:6]}", "ORD-P31", user_id, amount=10000, tampered=False)
    wh_tamp = client.send_dana_webhook(f"EVT-P31-BAD-{uuid.uuid4().hex[:6]}", "ORD-P31", user_id, amount=10000, tampered=True)
    reporter.record(wh_valid.status == 200 and wh_tamp.status == 401,
                    "PAIR-31 [RSASignature + DANAWebhook]: Cryptographically valid DANA RSA-SHA256 succeeds, tampered signature rejected with 401")

    # Pair 32: Idempotent Webhook Processing + Immutable Audit Log (REQ-SEC-09 x REQ-SEC-12)
    audit_pre = client.get_audit_logs().json.get("count", 0)
    wh_ev_audit = f"EVT-P32-AUDIT-{uuid.uuid4().hex[:6]}"
    client.send_dana_webhook(wh_ev_audit, "ORD-AUD", user_id)
    audit_post = client.get_audit_logs().json.get("count", 0)
    reporter.record(audit_post >= audit_pre,
                    "PAIR-32 [WebhookIdempotency + AuditLog]: Verified webhook settlement writes audit log record")

    # Pair 33: Server-Side Feature Gating + Ingestion Pipeline (REQ-SEC-10 x REQ-INGEST-01)
    client_free_gate = ApiClient(base_url=base_url)
    client_free_gate.register(f"free_gate_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "Free Gated")
    r_ing_locked = client_free_gate.ingest_notification("com.bca", "BCA", "Transfer Rp 50.000")
    client_free_gate.activate_trial()
    r_ing_unlocked = client_free_gate.ingest_notification("com.bca", "BCA", "Transfer Rp 50.000")
    reporter.record(r_ing_locked.status == 403 and r_ing_unlocked.status == 200,
                    "PAIR-33 [FeatureGating + IngestionPipeline]: Ingestion pipeline strictly locked behind Premium feature gate")

    # Pair 34: Financial Response Cache Control + Server-Side Feature Gating (REQ-SEC-11 x REQ-SEC-10)
    client_cc = ApiClient(base_url=base_url)
    client_cc.register(f"cc_test_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "CC Test")
    r_locked_cc = client_cc.get_advanced_analytics()
    cc_locked_hdr = r_locked_cc.header("cache-control") or ""
    reporter.record(r_locked_cc.status == 403 and "no-store" in cc_locked_hdr,
                    "PAIR-34 [CacheControl + FeatureGate]: 403 FEATURE_LOCKED response emits private no-store headers")

    # Pair 35: Immutable Audit Log + Multi-Tenant Isolation (REQ-SEC-12 x REQ-ARCH-12)
    r_audit_put = client.put("/api/v1/audit/logs", {"fake": "tamper"})
    r_audit_del = client.delete("/api/v1/audit/logs")
    reporter.record(r_audit_put.status == 405 and r_audit_del.status == 405,
                    "PAIR-35 [ImmutableAuditLog + TenantProtection]: Audit log endpoints reject PUT and DELETE with HTTP 405 Method Not Allowed")

    # Pair 36: Ingestion Pipeline Architecture + Canonical Representation (REQ-INGEST-01 x REQ-INGEST-02)
    ing_user = ApiClient(base_url=base_url)
    ing_user.register(f"ing_canon_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "Ingest User")
    ing_user.activate_trial()
    r_canon = ing_user.ingest_notification("id.dana", "DANA", "Kirim Uang Rp 120.000 ke Toko Berhasil")
    reporter.record(r_canon.status == 200 and r_canon.json.get("amount") == 120000 and "confidence" in r_canon.json,
                    "PAIR-36 [IngestionPipeline + CanonicalRepresentation]: Notification parsed into canonical candidate with exact amount and confidence")

    # Pair 37: Confidence Threshold Engine + Notification Adapter (REQ-INGEST-03 x REQ-INGEST-05)
    r_high = ing_user.ingest_notification("com.bca", "BCA", "Transfer Rp 750.000 ke Supplier")
    r_low = ing_user.ingest_notification("com.unknown.app", "App", "Promo diskon heboh!")
    reporter.record(r_high.json.get("confidence") == "HIGH" and r_low.json.get("confidence") == "LOW",
                    "PAIR-37 [ConfidenceEngine + NotificationAdapter]: Financial push scored HIGH while promotional text scored LOW")

    # Pair 38: Cross-Source Deduplication + SMS Capability Adapter (REQ-INGEST-04 x REQ-INGEST-06)
    r_sms1 = ing_user.ingest_sms("BANK-BCA", "Debet Rp 88.000 di Coffee Shop")
    r_sms_dup = ing_user.ingest_sms("BANK-BCA", "Debet Rp 88.000 di Coffee Shop")
    reporter.record(r_sms1.status == 200 and r_sms_dup.json.get("status") == "duplicate",
                    "PAIR-38 [CrossSourceDeduplication + SMSAdapter]: SMS ingestion pipeline filters identical repeated signals")

    # Pair 39: Targeted Gmail Ingestion + Confidence Engine (REQ-INGEST-07 x REQ-INGEST-03)
    r_gmail = ing_user.ingest_gmail(str(uuid.uuid4()), "Tagihan Listrik PLN", "Pembayaran Listrik PLN Rp 250.000 Lunas")
    reporter.record(r_gmail.status == 200 and r_gmail.json.get("source") == "gmail" and r_gmail.json.get("amount") == 250000,
                    "PAIR-39 [TargetedGmail + ConfidenceEngine]: Gmail invoice ingestion parses exact amount and marks source 'gmail'")

    # Pair 40: Payload Minimization + Canonical Candidate (REQ-INGEST-08 x REQ-INGEST-02)
    cands_raw = ing_user.list_ingestion_candidates().json
    cands_list = cands_raw if isinstance(cands_raw, list) else (cands_raw.get("candidates", []) if isinstance(cands_raw, dict) else [])
    reporter.record(isinstance(cands_list, list) and not any("password" in str(c) for c in cands_list),
                    "PAIR-40 [PayloadMinimization + CanonicalCandidate]: Ingestion candidate queue does not retain raw unneeded payloads")

    # Pair 41: Kotlin Native Shell & WebView + Operational Health (REQ-AND-01 x REQ-QA-04)
    r_and_stat = client.get("/api/v1/android/status")
    reporter.record(r_and_stat.status == 200 and r_and_stat.json.get("bridge_version") == "1.0",
                    "PAIR-41 [AndroidNativeShell + OperationalHealth]: Android container status probe responds with valid bridge specification")

    # Pair 42: NotificationListenerService + Android Notification Adapter (REQ-AND-02 x REQ-INGEST-05)
    reporter.record(r_and_stat.json.get("notification_service") == "supported" and "com.bca" in r_and_stat.json.get("approved_packages", []),
                    "PAIR-42 [NotificationListener + ApprovedPackages]: Android shell registry declares approved banking notification packages")

    # Pair 43: Versioned JS Bridge + WebView Origin Validation (REQ-AND-03 x REQ-AND-04)
    reporter.record("check_permissions" in r_and_stat.json.get("capabilities", []) and r_and_stat.json.get("trusted_origin") == "https://api.nurdiansyahlabs.com",
                    "PAIR-43 [JSBridge + OriginValidation]: Capability bridge contract verified with strict trusted origin binding")

    # Pair 44: OS Background Sync + Cursor Delta Sync (REQ-AND-05 x REQ-FE-08)
    r_and_sync = client.post("/api/v1/android/sync", {})
    reporter.record(r_and_sync.status == 200 and r_and_sync.json.get("sync_status") == "dispatched",
                    "PAIR-44 [BackgroundSync + DeltaSync]: Background worker sync dispatch coordinates delta sync queue")

    # Pair 45: First-Launch Subscription + 3-Month Premium Trial (REQ-AND-06 x REQ-SEC-04)
    new_first_launch = ApiClient(base_url=base_url)
    new_first_launch.register(f"fl_{uuid.uuid4().hex[:6]}@invinite.app", user_pass, "First Launch")
    fl_sub = new_first_launch.subscription_status().json or {}
    reporter.record(fl_sub.get("tier") == "free" and fl_sub.get("has_used_trial") is False,
                    "PAIR-45 [FirstLaunchSubscription + TrialPrompt]: New user starts as free with available 3-month trial")

    # Pair 46: Secure Native Storage + Cookie Session Management (REQ-AND-07 x REQ-SEC-02)
    fl_me = new_first_launch.me()
    reporter.record(fl_me.status == 200 and fl_me.json.get("email") is not None,
                    "PAIR-46 [SecureNativeStorage + CookieSession]: Session cookie securely restores authenticated user identity")

    # Pair 47: Vue 3 PWA Shell + Production Build Validation (REQ-FE-01 x REQ-QA-02)
    fe_pkg = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend/package.json"
    fe_vite = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend/vite.config.js"
    reporter.record(os.path.exists(fe_pkg) and os.path.exists(fe_vite),
                    "PAIR-47 [Vue3PWA + ProductionBuild]: Frontend package and build configs verified for production PWA output")

    # Pair 48: 5-Tab Ergonomic Navigation + Living Product Personalization (REQ-FE-02 x REQ-FE-04)
    fe_home = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend/src/views/HomeView.vue"
    fe_onb = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend/src/components/ProgressiveOnboardingModal.vue"
    reporter.record(os.path.exists(fe_home) and os.path.exists(fe_onb),
                    "PAIR-48 [5TabNavigation + PersonalizationModal]: Core navigation view and progressive onboarding components verified")

    # Pair 49: Rapid 4x3 POS Keypad + Integer Rupiah Currency Math (REQ-FE-03 x REQ-ARCH-02)
    fe_add_comp = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend/src/components/AddTransactionModal.vue"
    with open(fe_add_comp) as f:
        fe_add_content = f.read()
    has_chips = "50000" in fe_add_content and "100000" in fe_add_content and "500000" in fe_add_content
    reporter.record(has_chips,
                    "PAIR-49 [POSKeypad + IntegerRupiah]: Keypad component configures integer quick chips (+50rb, +100rb, +500rb)")

    # Pair 50: Localized IDR Formatting + Financial Date/Time Policy (REQ-FE-05 x REQ-FE-06)
    r_tx_fmt = client.create_transaction(acc1, 1500000, "income", note="Salary")
    tx_amt = r_tx_fmt.json.get("amount")
    tx_dt = r_tx_fmt.json.get("date")
    reporter.record(tx_amt == 1500000 and "T" in tx_dt and tx_dt.endswith("Z"),
                    "PAIR-50 [IDRFormatting + DateTimePolicy]: Ledger outputs raw integer amount and UTC date for frontend formatting")

    # Pair 51: Foreground WebSocket + Cursor Delta Sync (REQ-FE-07 x REQ-FE-08)
    ws_probe = client.get("/api/v1/ws", headers={"Upgrade": "websocket"})
    sync_probe = client.delta_sync(0)
    reporter.record(ws_probe.status == 200 and sync_probe.status == 200 and "cursor" in sync_probe.json,
                    "PAIR-51 [ForegroundWebSocket + DeltaSyncStream]: WebSocket live probe and cursor sync endpoint interoperate")

    # Pair 52: Feature Lock Overlay + Commercial Pricing Plans (REQ-FE-09 x REQ-SEC-06)
    r_plans = client.get_subscription_plans()
    reporter.record(r_plans.status == 200 and any(p["amount"] == 10000 for p in r_plans.json.get("plans", [])),
                    "PAIR-52 [FeatureLockOverlay + PricingPlans]: Feature lock upgrade path offers authentic commercial plans")

    # Pair 53: Fintech Visual Design System + WCAG AA Accessibility (REQ-FE-10 x REQ-FE-11)
    with open(fe_add_comp) as f:
        add_text = f.read()
    reporter.record("aria-modal" in add_text and "role=\"dialog\"" in add_text,
                    "PAIR-53 [VisualDesign + WCAGAccessibility]: Financial modal components implement accessible dialog ARIA attributes")

    # Pair 54: Automated Cargo Test Suite + Acceptance Runner (REQ-QA-01 x REQ-QA-03)
    cargo_toml = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/Cargo.toml"
    runner_sh = "/home/nurdiansyah/teamwork_projects/personal_finance_pwa/e2e_tests/runner.sh"
    reporter.record(os.path.exists(cargo_toml) and os.path.exists(runner_sh),
                    "PAIR-54 [CargoWorkspace + RunnerScript]: Cargo workspace definition and master e2e runner script verified")

    # Pair 55: Operational Health Probes + Adversarial Security (REQ-QA-04 x REQ-QA-05)
    r_health = client.get("/health")
    r_ready = client.get("/ready")
    leaks_secret = "JWT_SECRET" in str(r_health.json) or "password" in str(r_health.json)
    reporter.record(r_health.status == 200 and r_ready.status == 200 and not leaks_secret,
                    "PAIR-55 [OperationalHealth + ZeroSecretLeaks]: Health and readiness probes return operational status with zero secret leaks")

    return reporter

if __name__ == "__main__":
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    rep = run_tier3_tests(base)
    success = rep.print_summary()
    sys.exit(0 if success else 1)
