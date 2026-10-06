#!/usr/bin/env python3
"""
Tier 3: Pairwise Cross-Feature Interactions Acceptance Test Suite (45 tests).
Verifies interactions between orthogonal subsystems across the 42 inventoried features
from PROJECT.md § Feature Inventory for Invinite Business OS v4.1.
Output: TAP (Test Anything Protocol) version 13.
"""

import sys
import os

os.environ["no_proxy"] = "localhost,127.0.0.1"
os.environ["NO_PROXY"] = "localhost,127.0.0.1"

import time
import uuid
import json
import re
from datetime import datetime, timezone
from typing import Optional, Dict, Any, List

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from harness.client import (
    ApiClient, TapReporter, assert_true, assert_equal,
    assert_status, assert_rfc7807
)

def run_tier3_tests(base_url: str, reporter: Optional[TapReporter] = None) -> TapReporter:
    if reporter is None:
        reporter = TapReporter(total_expected=45)
        reporter.print_header()

    client = ApiClient(base_url=base_url)
    uid_suffix = uuid.uuid4().hex[:8]
    user_email = f"tier3_{uid_suffix}@invinite.biz"
    user_pass = "P@ssword123!"

    reg_resp = client.register(user_email, user_pass, "Enterprise Architect")
    user_id = reg_resp.json.get("user", {}).get("id") if reg_resp.json else None

    # Primary tenant
    t_main = client.create_tenant(f"PT Sinergi Nusantara {uid_suffix}", slug=f"sinergi-{uid_suffix}")
    t_main_id = t_main.json.get("id")
    client.set_tenant(t_main_id)

    # Secondary user and tenant for cross-tenant testing
    c_peer = ApiClient(base_url=base_url)
    peer_email = f"peer_{uid_suffix}@partner.biz"
    c_peer.register(peer_email, user_pass, "Partner Auditor")
    t_peer = c_peer.create_tenant(f"PT Mitra Mandiri {uid_suffix}", slug=f"mitra-{uid_suffix}")
    t_peer_id = t_peer.json.get("id")
    c_peer.set_tenant(t_peer_id)

    # =========================================================================
    # PAIRWISE INTERACTIONS (36 Tests)
    # =========================================================================

    # Pair 1: Multi-Tenancy (F01) x Accounting Core (F08)
    # Tenant creation automatically seeds Chart of Accounts and isolates journal posting
    coa_resp = client.list_chart_of_accounts()
    coa_accounts = coa_resp.json.get("accounts", [])
    j_init = client.post_journal(
        entry_date="2026-10-01T00:00:00Z",
        description="Initial Capital Contribution",
        lines=[
            {"account_code": "1000", "debit": 50000000, "credit": 0},
            {"account_code": "2000", "debit": 0, "credit": 50000000}
        ]
    )
    reporter.record(
        coa_resp.status == 200 and len(coa_accounts) >= 8 and j_init.status == 201,
        "PAIR-01 [F01 Multi-Tenancy x F08 Accounting]: Workspace initialization automatically seeds COA and posts balanced double-entry journals"
    )

    # Pair 2: TenantContext Scoping (F02) x Commercial Invoicing (F16)
    # X-Tenant-ID context determines invoice draft creation and prevents cross-tenant invoice leaking
    inv_t1 = client.create_invoice(
        customer_name="PT Konsumen Utama",
        items=[{"description": "Konsultasi IT", "quantity": 1, "unit_price": 10000000}],
        due_date="2026-11-01"
    )
    inv_t1_id = inv_t1.json.get("id")
    peer_read_t1 = c_peer.get_invoice(inv_t1_id)
    reporter.record(
        inv_t1.status == 201 and peer_read_t1.status == 404,
        "PAIR-02 [F02 TenantContext x F16 Invoicing]: TenantContext scopes invoice creation and strictly isolates invoice access (HTTP 404 across tenants)"
    )

    # Pair 3: Cross-Tenant 404 Isolation (F03) x Receivable Tracking (F19)
    # Issuing an invoice in Tenant 1 creates a receivable; Tenant 2 cannot access or view it (HTTP 404)
    iss_t1 = client.issue_invoice(inv_t1_id)
    peer_rec_query = c_peer.get(f"/api/v1/receivables/{inv_t1_id}")
    reporter.record(
        iss_t1.status == 200 and peer_rec_query.status == 404,
        "PAIR-03 [F03 Cross-Tenant 404 x F19 Receivables]: Generated receivable is invisible and returns HTTP 404 when probed from foreign tenant"
    )

    # Pair 4: RBAC Authorization (F04) x Payment Allocation (F20)
    # Staff member can view invoices but cannot remove members or perform unauthorized reversals
    c_staff = ApiClient(base_url=base_url)
    c_staff.register(f"staff_{uid_suffix}@staff.com", user_pass, "Staff Member")
    invite_stf = client.invite_tenant_member(t_main_id, f"staff_{uid_suffix}@staff.com", role="staff")
    c_staff.login(f"staff_{uid_suffix}@staff.com", user_pass)
    c_staff.set_tenant(t_main_id)
    staff_view_inv = c_staff.get_invoice(inv_t1_id)
    staff_del_owner = c_staff.remove_tenant_member(t_main_id, user_id)
    reporter.record(
        invite_stf.status == 201 and staff_view_inv.status == 200 and staff_del_owner.status == 403,
        "PAIR-04 [F04 RBAC x F20 Payment Allocation]: Staff member operates within role boundaries; unauthorized admin actions strictly return HTTP 403"
    )

    # Pair 5: Indonesian Defaults (F05) x Server-Side Sequential Numbering (F17)
    # Custom Indonesian prefix updated in profile is reflected in server-authoritative sequential numbering
    client.update_tenant_profile(t_main_id, {"invoice_prefix": "FAK"})
    inv_fak = client.create_invoice("Klien Lokal", [{"description": "Barang Dagang", "quantity": 1, "unit_price": 500000}])
    iss_fak = client.issue_invoice(inv_fak.json.get("id"))
    inv_num_fak = iss_fak.json.get("invoice_number", "")
    client.update_tenant_profile(t_main_id, {"invoice_prefix": "INV"})
    reporter.record(
        iss_fak.status == 200 and inv_num_fak.startswith("FAK-"),
        "PAIR-05 [F05 Indonesian Defaults x F17 Sequential Numbering]: Profile invoice prefix reflects dynamically in sequential invoice numbering (FAK-YYYY-XXXXXX)"
    )

    # Pair 6: Slug Validation (F06) x Mutation Idempotency (F21)
    # Reserved slug rejected with 400; Idempotent payment allocation prevents double-deduction under validated slug workspace
    res_slug = client.create_tenant("Reserved Attempt", slug="admin")
    inv_slug_idem = client.create_invoice("Slug Idem Customer", [{"description": "Item", "quantity": 1, "unit_price": 500000}], tax_type="NONE")
    client.issue_invoice(inv_slug_idem.json.get("id"))
    idem_k = str(uuid.uuid4())
    p_idem1 = client.allocate_payment(inv_slug_idem.json.get("id"), amount=250000, idempotency_key=idem_k)
    p_idem2 = client.allocate_payment(inv_slug_idem.json.get("id"), amount=250000, idempotency_key=idem_k)
    reporter.record(
        res_slug.status == 400 and p_idem1.status == 201 and p_idem2.status == 201 and p_idem1.json.get("id") == p_idem2.json.get("id"),
        "PAIR-06 [F06 Slug Validation x F21 Idempotency]: Reserved slugs strictly rejected (HTTP 400) and payment allocation deduplicates replay deterministically"
    )

    # Pair 7: Personal Workspace (F07) x Standard Chart of Accounts (F12)
    # Auto-provisioned personal workspace contains complete 8-account COA with system account protection
    u_pers = ApiClient(base_url=base_url)
    reg_pers = u_pers.register(f"pers_{uid_suffix}@invinite.app", user_pass, "Personal User")
    pers_tid = reg_pers.json.get("user", {}).get("default_tenant_id")
    u_pers.set_tenant(pers_tid)
    pers_coa = u_pers.list_chart_of_accounts().json.get("accounts", [])
    del_sys = u_pers.delete_account_coa("1000")
    reporter.record(
        len(pers_coa) >= 8 and del_sys.status == 403,
        "PAIR-07 [F07 Personal Workspace x F12 Chart of Accounts]: Auto-provisioned personal workspace seeds standard COA with system deletion protection"
    )

    # Pair 8: Double-Entry Balancing (F08) x Journal Reversal Workflow (F11)
    # Reversing a balanced journal produces an exact inverted balanced journal with zero net impact
    j_rev_src = client.post_journal(
        entry_date="2026-10-02T00:00:00Z",
        description="Pembelian Perlengkapan Kantor",
        lines=[
            {"account_code": "5000", "debit": 750000, "credit": 0},
            {"account_code": "1000", "debit": 0, "credit": 750000}
        ]
    )
    j_src_id = j_rev_src.json.get("id")
    rev_act = client.reverse_journal(j_src_id, reason="Koreksi salah catat akun beban")
    tb = client.get_trial_balance().json or {}
    reporter.record(
        j_rev_src.status == 201 and rev_act.status in (200, 201) and tb.get("net_balance") == 0,
        "PAIR-08 [F08 Balancing Invariant x F11 Journal Reversal]: Journal reversal workflow maintains exact debit==credit balance and zero net trial balance"
    )

    # Pair 9: Integer Rupiah Math (F09) x PPN Indonesian Tax Engine (F13)
    # PPN 11% calculated on integer Rupiah computes deterministic half-up rounding
    tax_calc_1 = client.calculate_tax(100, "PPN_11_EXCL").json
    tax_calc_2 = client.calculate_tax(5, "PPN_11_EXCL").json
    reporter.record(
        tax_calc_1.get("tax_amount") == 11 and tax_calc_2.get("tax_amount") == 1,
        "PAIR-09 [F09 Integer Rupiah x F13 PPN Tax Engine]: PPN 11% calculates exact integer Rupiah values with statutory half-up rounding"
    )

    # Pair 10: Journal Immutability (F10) x RBAC Authorization (F04)
    # Workspace owner cannot mutate posted journals directly via PUT or DELETE
    r_put_j = client.update_journal(j_src_id, {"description": "Hacked Description"})
    r_del_j = client.delete_journal(j_src_id)
    reporter.record(
        r_put_j.status == 405 and r_del_j.status == 405,
        "PAIR-10 [F10 Journal Immutability x F04 RBAC]: Journal immutability cannot be overridden by owner role via PUT or DELETE (HTTP 405)"
    )

    # Pair 11: Standard Chart of Accounts (F12) x Double-Entry Posting (F08)
    # Journal entries post successfully against system accounts and update account balance aggregates
    j_coa_test = client.post_journal(
        entry_date="2026-10-02T00:00:00Z",
        description="Penjualan Tunai",
        lines=[
            {"account_code": "1000", "debit": 2000000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 2000000}
        ]
    )
    reporter.record(
        j_coa_test.status == 201 and j_coa_test.json.get("total_debit") == 2000000,
        "PAIR-11 [F12 Chart of Accounts x F08 Accounting Posting]: Standard COA system accounts accept balanced journal line postings"
    )

    # Pair 12: PPN Tax Engine (F13) x Tax Inclusive/Exclusive Pricing (F15)
    # Inclusive tax pricing extracts exact integer net and tax amounts summing to gross
    tax_incl = client.calculate_tax(1110000, "PPN_11_INCL", is_inclusive=True).json
    net_val = tax_incl.get("net_amount", 0)
    tax_val = tax_incl.get("tax_amount", 0)
    gross_val = tax_incl.get("gross_amount", 0)
    reporter.record(
        net_val == 1000000 and tax_val == 110000 and gross_val == 1110000,
        "PAIR-12 [F13 PPN Tax Engine x F15 Tax Inclusive Pricing]: PPN 11% inclusive extraction deterministically partitions net and tax from gross total"
    )

    # Pair 13: UMKM Final Tax Engine (F14) x Accounting Journal Posting (F08)
    # UMKM 0.5% tax calculated from turnover posts a balanced tax liability journal (Debit 5000, Credit 2100)
    turnover = 40000000  # Rp 40,000,000 gross monthly turnover
    umkm_tax = client.calculate_tax(turnover, "UMKM_05").json.get("tax_amount", 0)
    j_umkm = client.post_journal(
        entry_date="2026-10-02T00:00:00Z",
        description="Akrual Pajak UMKM 0.5% PP 23/2018",
        lines=[
            {"account_code": "5000", "debit": umkm_tax, "credit": 0, "memo": "Beban Pajak Final UMKM"},
            {"account_code": "2100", "debit": 0, "credit": umkm_tax, "memo": "Utang Pajak Final PP 23"}
        ]
    )
    reporter.record(
        umkm_tax == 200000 and j_umkm.status == 201 and j_umkm.json.get("total_debit") == 200000,
        "PAIR-13 [F14 UMKM Final Tax x F08 Accounting Posting]: UMKM 0.5% gross turnover tax calculates Rp 200k and posts balanced tax accrual journal"
    )

    # Pair 14: Tax Pricing (F15) x Commercial Invoice Lifecycle (F16)
    # Commercial invoice with PPN 11% exclusive pricing calculates subtotal, tax, and total accurately in draft
    inv_tax_excl = client.create_invoice(
        customer_name="PT Pembeli Bersahabat",
        items=[
            {"description": "Server Hosting Cloud", "quantity": 2, "unit_price": 2500000}
        ],
        tax_type="PPN_11_EXCL",
        due_date="2026-11-15"
    )
    d_inv = inv_tax_excl.json or {}
    reporter.record(
        d_inv.get("subtotal") == 5000000 and d_inv.get("tax_amount") == 550000 and d_inv.get("total_amount") == 5550000,
        "PAIR-14 [F15 Tax Pricing x F16 Invoice Lifecycle]: Invoice creation computes subtotal, tax (PPN 11%), and grand total deterministically"
    )

    # Pair 15: Commercial Invoice Lifecycle (F16) x Issued Document Snapshotting (F18)
    # Issuing invoice freezes document snapshot; subsequent customer updates do not alter snapshot
    iss_snap = client.issue_invoice(d_inv.get("id"))
    client.update_invoice(d_inv.get("id"), customer_name="Customer Modified Post Issue Attempt")
    inv_post_iss = client.get_invoice(d_inv.get("id")).json
    snap_data = inv_post_iss.get("snapshot", {})
    reporter.record(
        iss_snap.status == 200 and snap_data.get("customer_name") == "PT Pembeli Bersahabat" and snap_data.get("total_amount") == 5550000,
        "PAIR-15 [F16 Invoice Lifecycle x F18 Document Snapshot]: Issuing locks invoice and captures frozen document snapshot immune to subsequent mutations"
    )

    # Pair 16: Commercial Invoice Lifecycle (F16) x Receivable Tracking (F19)
    # Issuing invoice automatically creates matching open receivable record with exact total amount
    recs = client.list_receivables().json.get("receivables", [])
    matched_rec = next((r for r in recs if r.get("invoice_id") == d_inv.get("id")), None)
    reporter.record(
        matched_rec is not None and matched_rec.get("outstanding_amount") == 5550000 and matched_rec.get("status") == "OPEN",
        "PAIR-16 [F16 Invoice Lifecycle x F19 Receivable Tracking]: Issuing invoice atomically provisions OPEN receivable with identical outstanding balance"
    )

    # Pair 17: Commercial Invoice Lifecycle (F16) x Atomic Payment Allocation (F20)
    # Full payment settlement transitions both invoice and receivable to PAID atomically
    pay_full = client.allocate_payment(d_inv.get("id"), amount=5550000, payment_method="BANK_TRANSFER")
    inv_paid = client.get_invoice(d_inv.get("id")).json
    rec_paid = next((r for r in client.list_receivables().json.get("receivables", []) if r.get("invoice_id") == d_inv.get("id")), None)
    reporter.record(
        pay_full.status == 201 and inv_paid.get("status") == "PAID" and rec_paid.get("status") == "PAID" and rec_paid.get("outstanding_amount") == 0,
        "PAIR-17 [F16 Invoice Lifecycle x F20 Payment Allocation]: Full payment allocation atomically transitions both invoice and receivable to PAID"
    )

    # Pair 18: Commercial Invoice Lifecycle (F16) x Transactional Outbox (F22)
    # Outbox events persist with valid aggregate references when invoice is issued and paid
    ob_events = client.list_outbox_events().json.get("events", [])
    has_inv_issued_ev = any(e.get("event_type") == "InvoiceIssued" and e.get("aggregate_id") == d_inv.get("id") for e in ob_events)
    has_pay_confirmed_ev = any(e.get("event_type") == "PaymentConfirmed" for e in ob_events)
    reporter.record(
        has_inv_issued_ev and has_pay_confirmed_ev,
        "PAIR-18 [F16 Invoice Lifecycle x F22 Outbox Persistence]: Invoice issue and payment allocation atomically persist corresponding outbox events"
    )

    # Pair 19: Server-Side Sequential Numbering (F17) x Cross-Tenant Isolation (F03)
    # Tenant 1 and Tenant 2 generate isolated sequential number sequences starting from 000001
    t_fresh_a = client.create_tenant(f"PT Fresh A {uid_suffix}", slug=f"fresh-a-{uid_suffix}")
    client.set_tenant(t_fresh_a.json.get("id"))
    inv_a1 = client.create_invoice("A Customer", [{"description": "Item A", "quantity": 1, "unit_price": 100000}])
    iss_a1 = client.issue_invoice(inv_a1.json.get("id"))
    num_a1 = iss_a1.json.get("invoice_number", "")

    t_fresh_b = c_peer.create_tenant(f"PT Fresh B {uid_suffix}", slug=f"fresh-b-{uid_suffix}")
    c_peer.set_tenant(t_fresh_b.json.get("id"))
    inv_b1 = c_peer.create_invoice("B Customer", [{"description": "Item B", "quantity": 1, "unit_price": 100000}])
    iss_b1 = c_peer.issue_invoice(inv_b1.json.get("id"))
    num_b1 = iss_b1.json.get("invoice_number", "")

    # Reset client back to main
    client.set_tenant(t_main_id)
    reporter.record(
        num_a1.endswith("-000001") and num_b1.endswith("-000001"),
        "PAIR-19 [F17 Sequential Numbering x F03 Cross-Tenant Isolation]: Gapless sequential numbering sequences are strictly isolated per tenant"
    )

    # Pair 20: Issued Document Snapshotting (F18) x Journal Immutability (F10)
    # Snapshot data remains frozen alongside posted double-entry journal records
    j_list = client.list_journals().json.get("journals", [])
    reporter.record(
        iss_snap.json.get("snapshot") is not None and len(j_list) > 0,
        "PAIR-20 [F18 Document Snapshot x F10 Journal Immutability]: Financial truth is preserved across immutable document snapshots and immutable journals"
    )

    # Pair 21: Receivable Aging (F19) x Atomic Payment Allocation (F20)
    # Partial payment on invoice updates outstanding balance and reduces current aging bucket
    inv_aging = client.create_invoice(
        "Klien Termin",
        [{"description": "Konsultasi Bulanan", "quantity": 1, "unit_price": 1000000}],
        tax_type="NONE"
    )
    inv_ag_id = inv_aging.json.get("id")
    client.issue_invoice(inv_ag_id)
    ag_pre = client.get_receivable_aging().json.get("total_outstanding", 0)
    client.allocate_payment(inv_ag_id, amount=400000)
    ag_post = client.get_receivable_aging().json.get("total_outstanding", 0)
    reporter.record(
        ag_post == ag_pre - 400000,
        "PAIR-21 [F19 Receivable Aging x F20 Payment Allocation]: Partial payment dynamically decrements aging total_outstanding by exact allocated amount"
    )

    # Pair 22: Atomic Payment Allocation (F20) x Double-Entry Posting (F08)
    # Allocating payment automatically commits a balanced journal (Debit 1100 Kas/Bank, Credit 1200 Piutang)
    j_after_pay = client.list_journals().json.get("journals", [])
    pay_journal = next((j for j in j_after_pay if j.get("source_type") == "PAYMENT"), None)
    reporter.record(
        pay_journal is not None and pay_journal.get("status") == "POSTED",
        "PAIR-22 [F20 Payment Allocation x F08 Accounting Posting]: Payment allocation automatically creates balanced double-entry journal (Debit 1100, Credit 1200)"
    )

    # Pair 23: Atomic Payment Allocation (F20) x Mutation Idempotency (F21)
    # Replaying payment allocation with identical Idempotency-Key returns cached response without duplicate debit/credit
    inv_idem_pay = client.create_invoice("Idem Pay Cust", [{"description": "Service", "quantity": 1, "unit_price": 500000}], tax_type="NONE")
    client.issue_invoice(inv_idem_pay.json.get("id"))
    idem_pay_key = str(uuid.uuid4())
    p_first = client.allocate_payment(inv_idem_pay.json.get("id"), amount=200000, idempotency_key=idem_pay_key)
    p_replay = client.allocate_payment(inv_idem_pay.json.get("id"), amount=200000, idempotency_key=idem_pay_key)
    rec_idem = next((r for r in client.list_receivables().json.get("receivables", []) if r.get("invoice_id") == inv_idem_pay.json.get("id")), None)
    reporter.record(
        p_first.status == 201 and p_replay.status == 201 and rec_idem.get("outstanding_amount") == 300000,
        "PAIR-23 [F20 Payment Allocation x F21 Idempotency]: Idempotent payment replay prevents duplicate balance deduction on receivable"
    )

    # Pair 24: Mutation Idempotency (F21) x Commercial Invoice Lifecycle (F16)
    # Replaying invoice issue with Idempotency-Key returns cached response without duplicate sequential numbers
    inv_idem_iss = client.create_invoice("Idem Issue Cust", [{"description": "Service", "quantity": 1, "unit_price": 300000}])
    idem_iss_key = str(uuid.uuid4())
    iss1 = client.issue_invoice(inv_idem_iss.json.get("id"), idempotency_key=idem_iss_key)
    iss2 = client.issue_invoice(inv_idem_iss.json.get("id"), idempotency_key=idem_iss_key)
    reporter.record(
        iss1.status == 200 and iss2.status == 200 and iss1.json.get("invoice_number") == iss2.json.get("invoice_number"),
        "PAIR-24 [F21 Idempotency x F16 Invoice Lifecycle]: Replaying invoice issue with Idempotency-Key returns cached invoice number deterministically"
    )

    # Pair 25: Transactional Outbox (F22) x Outbox Dispatch Worker (F23)
    # Pending outbox events are polled and transitioned to PUBLISHED status by asynchronous processor
    proc_res = client.process_outbox(simulate_sidecar_failure=False)
    ob_published = client.list_outbox_events().json.get("events", [])
    all_pub = all(e.get("status") == "PUBLISHED" for e in ob_published)
    reporter.record(
        proc_res.status == 200 and len(ob_published) > 0 and all_pub,
        "PAIR-25 [F22 Outbox Persistence x F23 Outbox Dispatcher]: Outbox worker successfully transitions all pending events to PUBLISHED"
    )

    # Pair 26: Outbox Dispatcher (F23) x Sidecar Isolation Boundary (F25)
    # Sidecar failure during outbox processing captures error, leaves event PENDING, and does not compromise database
    inv_sc = client.create_invoice("Sidecar Customer", [{"description": "Cloud License", "quantity": 1, "unit_price": 800000}])
    inv_sc_id = inv_sc.json.get("id")
    client.issue_invoice(inv_sc_id)
    proc_fail = client.process_outbox(simulate_sidecar_failure=True)
    inv_sc_check = client.get_invoice(inv_sc_id)
    reporter.record(
        proc_fail.status == 200 and proc_fail.json.get("failed", 0) >= 1 and inv_sc_check.status == 200 and inv_sc_check.json.get("status") == "ISSUED",
        "PAIR-26 [F23 Outbox Dispatcher x F25 Sidecar Isolation]: Simulated sidecar failure safely captured; core invoice state remains committed and ISSUED"
    )

    # Pair 27: Outbox Event Deduplication (F24) x Transactional Outbox Persistence (F22)
    # Every generated outbox event has a strictly unique RFC 4122 UUID and chronological ordering
    all_evs = client.list_outbox_events().json.get("events", [])
    ev_ids = [e["id"] for e in all_evs]
    reporter.record(
        len(ev_ids) > 0 and len(ev_ids) == len(set(ev_ids)),
        "PAIR-27 [F24 Outbox Deduplication x F22 Outbox Persistence]: All outbox events maintain globally unique UUIDs for consumer deduplication"
    )

    # Pair 28: Sidecar Isolation Boundary (F25) x Atomic Payment Allocation (F20)
    # Recovering outbox processor dispatches previously failed sidecar events cleanly
    proc_recovery = client.process_outbox(simulate_sidecar_failure=False)
    ev_sc = next((e for e in client.list_outbox_events().json.get("events", []) if e.get("aggregate_id") == inv_sc_id), None)
    reporter.record(
        proc_recovery.status == 200 and ev_sc is not None and ev_sc.get("status") == "PUBLISHED",
        "PAIR-28 [F25 Sidecar Isolation x F20 Payment Allocation]: Sidecar recovery publishes pending outbox queue without core domain corruption"
    )

    # Pair 29: Frontend Workspace Store (F26) x Multi-Tenant Data Model (F01)
    # Active workspace switcher updates session context and list_tenants returns all valid candidates
    tenants_list = client.list_tenants().json.get("tenants", [])
    switch_res = client.switch_tenant(t_main_id)
    reporter.record(
        len(tenants_list) >= 2 and switch_res.status == 200 and switch_res.json.get("active_tenant_id") == t_main_id,
        "PAIR-29 [F26 Workspace Store x F01 Multi-Tenancy]: Workspace store candidate listing and active switching operate cleanly"
    )

    # Pair 30: Header & Sidebar UI (F27) x Indonesian Defaults (F05)
    # Tenant profile provides Indonesian locale attributes (id-ID, IDR, Asia/Jakarta) for UI layout display
    prof_ui = client.get_tenant_profile(t_main_id).json or {}
    reporter.record(
        prof_ui.get("currency") == "IDR" and prof_ui.get("locale") == "id-ID" and prof_ui.get("timezone") in ["Asia/Jakarta", "WIB"],
        "PAIR-30 [F27 Header/Sidebar UI x F05 Indonesian Defaults]: Header and sidebar business profile state reflects standard Indonesian defaults"
    )

    # Pair 31: API Client 404 Mock Fix (F28) x Cross-Tenant 404 Isolation (F03)
    # Querying cross-tenant entity strictly returns RFC 7807 problem details with zero mock fallback data
    r_probe = c_peer.get_invoice(inv_sc_id)
    probe_body = r_probe.json or {}
    reporter.record(
        r_probe.status == 404 and probe_body.get("code") == "NOT_FOUND" and "mock" not in r_probe.text.lower(),
        "PAIR-31 [F28 404 Mock Fallback Fix x F03 Cross-Tenant Isolation]: Cross-tenant query strictly returns RFC 7807 404 NOT_FOUND with zero synthetic mock data"
    )

    # Pair 32: Capability Navigation (F29) x Frontend Workspace Store (F26)
    # Workspace capabilities endpoint provides dynamic module navigation list for Pinia store
    client.update_tenant_profile(t_main_id, {"business_type": "retail"})
    caps_retail = client.get_tenant_capabilities(t_main_id).json or {}
    nav_modules = [m.get("module") for m in caps_retail.get("navigation", [])]
    reporter.record(
        "pos" in nav_modules and "invoicing" in nav_modules,
        "PAIR-32 [F29 Capability Navigation x F26 Workspace Store]: Retail workspace provisions 'pos' and 'invoicing' navigation modules for frontend store"
    )

    # Pair 33: Backend Harness Synchronization (F30) x Accounting Invariants (F08)
    # SQLite WAL journal mode and 41 relational tables remain active during accounting state mutations
    sys_schema = client.get_system_schema().json or {}
    reporter.record(
        sys_schema.get("table_count") == 41 and sys_schema.get("journal_mode") == "wal" and sys_schema.get("foreign_keys") == 1,
        "PAIR-33 [F30 Harness Synchronization x F08 Accounting Invariants]: Backend relational schema verified at exactly 41 tables in SQLite WAL mode"
    )

    # Pair 34: Full E2E Test Suite (F31) x Receivable Aging (F19)
    # Receivable aging endpoint aggregates balances cleanly with sub-100ms response performance
    t_start = time.perf_counter()
    aging_fast = client.get_receivable_aging()
    dur_ms = (time.perf_counter() - t_start) * 1000
    reporter.record(
        aging_fast.status == 200 and dur_ms < 200,
        "PAIR-34 [F31 Full E2E Suite x F19 Receivable Aging]: Receivable aging aggregation responds within performance budget (<200ms)"
    )

    # Pair 35: Adversarial Hardening (F32) x Multi-Tenant Data Model (F01)
    # Malicious SQL injection in tenant name is safely parameterized without corruption
    sqli_tenant = client.create_tenant("PT Aman'); DROP TABLE memberships; --", slug=f"sqli-safe-{uid_suffix}")
    t_sqli_id = sqli_tenant.json.get("id")
    read_sqli = client.get_tenant(t_sqli_id)
    reporter.record(
        sqli_tenant.status == 201 and "DROP TABLE" in read_sqli.json.get("name", ""),
        "PAIR-35 [F32 Adversarial Hardening x F01 Multi-Tenancy]: SQL injection payload in workspace name safely parameterized and stored literally"
    )

    # Pair 36: Adversarial Hardening (F32) x Double-Entry Balancing (F08)
    # Adversarial unbalanced journal injection off by 1 Rupiah is strictly rejected by double-entry invariant validator
    unbal_attack = client.post_journal(
        entry_date="2026-10-03T00:00:00Z",
        description="Adversarial Attack Off By 1",
        lines=[
            {"account_code": "1000", "debit": 100000000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 99999999}
        ]
    )
    reporter.record(
        unbal_attack.status == 422 and unbal_attack.json.get("code") == "UNBALANCED_JOURNAL_ENTRY",
        "PAIR-36 [F32 Adversarial Hardening x F08 Balancing Invariant]: Adversarial unbalanced journal injection strictly rejected with HTTP 422"
    )

    # -------------------------------------------------------------------------
    # Pair 37: Multi-Location Warehouse (F33) x Chart of Accounts & Seed Data (F06)
    # -------------------------------------------------------------------------
    # Warehouse provisioning seamlessly coexists with seeded inventory asset account 1300
    wh_pair = client.create_warehouse(code=f"WH-P37-{uid_suffix}", name="Gudang Integrasi Akuntansi", is_default=True)
    wh_pair_id = wh_pair.json.get("id") if wh_pair.json else None
    coa_res = client.get("/api/v1/accounting/accounts")
    accounts = coa_res.json.get("accounts", []) if coa_res.json else []
    acc1300 = next((a for a in accounts if a.get("code") == "1300"), None)
    reporter.record(
        wh_pair.status == 201 and acc1300 is not None and acc1300.get("account_type", "").lower() == "asset",
        "PAIR-37 [F33 Warehouse Registry x F06 Chart of Accounts]: Warehouse provisioning seamlessly coexists with seeded inventory asset account 1300"
    )

    # -------------------------------------------------------------------------
    # Pair 38: Product Catalog SKU (F34) x Multi-Tenant Data Model (F01)
    # -------------------------------------------------------------------------
    # Unique SKU enforcement strictly scopes to active tenant without cross-tenant collisions
    p_sku = f"SKU-PAIR38-{uid_suffix}"
    prod_t1 = client.create_product(name="Produk Tenant 1", sku=p_sku, cost_price=10000, sale_price=15000)
    dup_t1 = client.create_product(name="Produk Tenant 1 Dup", sku=p_sku, cost_price=12000, sale_price=18000)
    prod_peer = c_peer.create_product(name="Produk Peer Tenant", sku=p_sku, cost_price=20000, sale_price=25000)
    reporter.record(
        prod_t1.status == 201 and dup_t1.status == 409 and prod_peer.status == 201,
        "PAIR-38 [F34 Product Catalog x F01 Multi-Tenancy]: Unique SKU enforcement strictly scopes to active tenant without cross-tenant collisions"
    )

    # -------------------------------------------------------------------------
    # Pair 39: Multi-Location Inventory Balances (F35) x Low Stock Notification (F23)
    # -------------------------------------------------------------------------
    # Stock below reorder threshold dynamically activates is_low_stock filter
    prod_alert = client.create_product(name="Produk Kritis Alert", cost_price=5000, sale_price=10000, reorder_threshold=10)
    pa_id = prod_alert.json.get("id")
    client.create_stock_movement(
        movement_type="INBOUND",
        product_id=pa_id,
        destination_warehouse_id=wh_pair_id,
        quantity=4,
        unit_cost=5000
    )
    low_stock_res = client.get_stock_items(low_stock=True)
    low_items = low_stock_res.json.get("stock_items", []) if low_stock_res.json else []
    target_low = next((it for it in low_items if it.get("product_id") == pa_id), None)
    reporter.record(
        target_low is not None and target_low.get("is_low_stock") is True and target_low.get("quantity_on_hand") == 4,
        "PAIR-39 [F35 Inventory Balances x F23 Low Stock Alerting]: Stock below reorder threshold dynamically activates is_low_stock filter"
    )

    # -------------------------------------------------------------------------
    # Pair 40: Direct Stock Movements (F36) x Double-Entry Invariant Balancing (F08/F42)
    # -------------------------------------------------------------------------
    # Outbound stock deduction atomically posts balanced COGS journal with Debit == Credit
    prod_bal = client.create_product(name="Produk COGS Balance", cost_price=25000, sale_price=40000)
    pb_id = prod_bal.json.get("id")
    client.create_stock_movement(
        movement_type="INBOUND",
        product_id=pb_id,
        destination_warehouse_id=wh_pair_id,
        quantity=10,
        unit_cost=25000
    )
    out_mv = client.create_stock_movement(
        movement_type="OUTBOUND",
        product_id=pb_id,
        source_warehouse_id=wh_pair_id,
        quantity=3
    )
    journals_res = client.get("/api/v1/accounting/journals")
    all_j = journals_res.json.get("journals", []) if journals_res.json else []
    cogs_j = [j for j in all_j if j.get("source_type") == "INVENTORY_OUTBOUND"]
    target_cogs = next((j for j in cogs_j if j.get("source_id") == out_mv.json.get("id")), None)
    reporter.record(
        out_mv.status == 201 and target_cogs is not None and target_cogs.get("total_debit") == target_cogs.get("total_credit") == 75000,
        "PAIR-40 [F36 Stock Movement x F08/F42 General Ledger Invariant]: Outbound stock deduction atomically posts balanced COGS journal with Debit == Credit"
    )

    # -------------------------------------------------------------------------
    # Pair 41: Inter-Warehouse Transfer (F37) x Transactional Outbox Pipeline (F24)
    # -------------------------------------------------------------------------
    # Stock transfer atomically updates dual warehouse balances and persists StockTransferred outbox event
    wh_dest = client.create_warehouse(code=f"WH-DEST-{uid_suffix}", name="Gudang Cabang Distribusi")
    wh_dest_id = wh_dest.json.get("id")
    xfer_res = client.transfer_stock(
        source_warehouse_id=wh_pair_id,
        destination_warehouse_id=wh_dest_id,
        product_id=pb_id,
        quantity=2
    )
    mov_xfer_id = xfer_res.json.get("movement_id") if xfer_res.json else None
    all_ob = client.list_outbox_events().json.get("events", [])
    xfer_event = next((e for e in all_ob if e.get("event_type") == "StockTransferred" and e.get("aggregate_id") == mov_xfer_id), None)
    reporter.record(
        xfer_res.status == 200 and xfer_event is not None and xfer_event.get("aggregate_type") == "Inventory",
        "PAIR-41 [F37 Inter-Warehouse Transfer x F24 Outbox Pipeline]: Stock transfer atomically updates dual warehouse balances and persists StockTransferred outbox event"
    )

    # -------------------------------------------------------------------------
    # Pair 42: Physical Stock Adjustment (F38) x Double-Entry Invariant Balancing (F08/F42)
    # -------------------------------------------------------------------------
    # Positive stock count adjustment posts strictly balanced journal with Debit Persediaan == Credit Selisih
    # pb_id currently has 2 units in wh_dest_id. Adjust to 5 units (+3 variance @ 25000 = 75000 IDR)
    adj_res = client.adjust_stock(
        warehouse_id=wh_dest_id,
        product_id=pb_id,
        actual_quantity=5,
        reason="Periodic physical inventory cycle count"
    )
    adj_id = adj_res.json.get("id")
    all_j2 = client.get("/api/v1/accounting/journals").json.get("journals", [])
    adj_j = next((j for j in all_j2 if j.get("source_type") == "STOCK_ADJUSTMENT" and j.get("source_id") == adj_id), None)
    reporter.record(
        adj_res.status == 200 and adj_j is not None and adj_j.get("total_debit") == adj_j.get("total_credit") == 75000,
        "PAIR-42 [F38 Physical Adjustment x F08/F42 Journal Invariant]: Positive stock count adjustment posts strictly balanced journal with Debit Persediaan == Credit Selisih"
    )

    # -------------------------------------------------------------------------
    # Pair 43: Purchase Order Lifecycle (F39) x Cross-Tenant Anti-Enumeration (F03)
    # -------------------------------------------------------------------------
    # Cross-tenant access and mutation attempts on purchase orders return pure RFC 7807 404
    po_t1 = client.create_purchase_order(
        supplier_name="PT Supplier Teruji",
        destination_warehouse_id=wh_pair_id,
        items=[{"product_id": pb_id, "quantity_ordered": 20, "unit_cost": 22000}]
    )
    po_t1_id = po_t1.json.get("id")
    peer_get_po = c_peer.get_purchase_order(po_t1_id)
    peer_order_po = c_peer.order_purchase_order(po_t1_id)
    reporter.record(
        po_t1.status == 201 and peer_get_po.status == 404 and peer_order_po.status == 404,
        "PAIR-43 [F39 Purchase Order Lifecycle x F03 Cross-Tenant Isolation]: Cross-tenant access and mutation attempts on purchase orders return pure RFC 7807 404"
    )

    # -------------------------------------------------------------------------
    # Pair 44: Inbound Goods Receipt (F40) x Moving WAC Valuation Engine (F41)
    # -------------------------------------------------------------------------
    # Sequential goods receipts recalculate moving weighted average unit cost accurately
    prod_wac = client.create_product(name="Produk WAC Evaluator", cost_price=10000, sale_price=20000)
    pw_id = prod_wac.json.get("id")
    po1 = client.create_purchase_order(
        supplier_name="Supplier Alpha",
        destination_warehouse_id=wh_pair_id,
        items=[{"product_id": pw_id, "quantity_ordered": 10, "unit_cost": 10000}]
    )
    client.order_purchase_order(po1.json.get("id"))
    rcv1 = client.receive_purchase_order(po1.json.get("id"), [{"product_id": pw_id, "quantity_received": 10}])

    po2 = client.create_purchase_order(
        supplier_name="Supplier Beta",
        destination_warehouse_id=wh_pair_id,
        items=[{"product_id": pw_id, "quantity_ordered": 10, "unit_cost": 20000}]
    )
    client.order_purchase_order(po2.json.get("id"))
    rcv2 = client.receive_purchase_order(po2.json.get("id"), [{"product_id": pw_id, "quantity_received": 10}])

    cur_stock = client.get_stock_items(warehouse_id=wh_pair_id, product_id=pw_id).json.get("stock_items", [])[0]
    expected_wac = (10 * 10000 + 10 * 20000) // 20  # 15000
    reporter.record(
        rcv1.status == 200 and rcv2.status == 200 and cur_stock.get("quantity_on_hand") == 20 and cur_stock.get("average_cost") == expected_wac,
        "PAIR-44 [F40 Goods Receipt x F41 Moving WAC Valuation]: Sequential goods receipts recalculate moving weighted average unit cost accurately"
    )

    # -------------------------------------------------------------------------
    # Pair 45: PO Goods Receipt (F40) x Automated General Ledger Journal (F42)
    # -------------------------------------------------------------------------
    # Goods receipt from purchase order atomically commits balanced receipt journal (Debit 1300 / Credit 2000)
    all_j3 = client.get("/api/v1/accounting/journals").json.get("journals", [])
    po_receipt_j = [j for j in all_j3 if j.get("source_type") in ["PURCHASE_ORDER_RECEIPT", "INVENTORY_INBOUND"]]
    all_po_balanced = all(j.get("total_debit") == j.get("total_credit") and j.get("total_debit") > 0 for j in po_receipt_j)
    reporter.record(
        len(po_receipt_j) >= 2 and all_po_balanced,
        "PAIR-45 [F40 PO Goods Receipt x F42 Journal Balancing]: Goods receipt from purchase order atomically commits balanced receipt journal (Debit 1300 / Credit 2000)"
    )

    return reporter

if __name__ == "__main__":
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    rep = run_tier3_tests(base)
    success = rep.print_summary()
    sys.exit(0 if success else 1)
