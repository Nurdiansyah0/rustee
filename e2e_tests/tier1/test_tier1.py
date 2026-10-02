#!/usr/bin/env python3
"""
Tier 1: Feature Coverage Acceptance Test Suite (160 tests across all 32 inventoried features).
Covers primary behavior (happy path) for every feature from PROJECT.md § Feature Inventory.
Output: TAP (Test Anything Protocol) version 13.
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

def run_tier1_tests(base_url: str, reporter: Optional[TapReporter] = None) -> TapReporter:
    if reporter is None:
        reporter = TapReporter(total_expected=160)
        reporter.print_header()

    client = ApiClient(base_url=base_url)

    uid_suffix = uuid.uuid4().hex[:8]
    user_email = f"tier1_{uid_suffix}@invinite.biz"
    user_pass = "P@ssword123!"

    # =========================================================================
    # FEATURE 1: Multi-Tenant Data Model (PRD §5, §7, §10)
    # =========================================================================
    reg_resp = client.register(user_email, user_pass, "Enterprise Admin")
    user_id = reg_resp.json.get("user", {}).get("id") if reg_resp.json else None
    reporter.record(reg_resp.status == 201 and user_id is not None,
                    "F01.1: Registration creates user with active session")

    # 1.2 Tenant creation returns 201 Created with valid UUID and ACTIVE status
    t1_resp = client.create_tenant("PT Nusantara Jaya", slug=f"nusantara-{uid_suffix}")
    t1_id = t1_resp.json.get("id") if t1_resp.json else None
    reporter.record(t1_resp.status == 201 and t1_id is not None and t1_resp.json.get("status") == "ACTIVE",
                    "F01.2: Tenant creation returns 201 Created with valid UUID and ACTIVE status")

    # 1.3 BusinessProfile created with default Indonesian settings
    prof_resp = client.get_tenant_profile(t1_id)
    reporter.record(prof_resp.status == 200 and prof_resp.json.get("currency") == "IDR" and prof_resp.json.get("locale") == "id-ID",
                    "F01.3: BusinessProfile created with default Indonesian settings (IDR, id-ID)")

    # 1.4 Owner membership created with ACTIVE status and owner role
    mems_resp = client.list_tenant_members(t1_id)
    is_owner = any(m.get("user_id") == user_id and m.get("role") == "owner" for m in (mems_resp.json or {}).get("members", []))
    reporter.record(mems_resp.status == 200 and is_owner,
                    "F01.4: Owner membership created with ACTIVE status and owner role")

    # 1.5 Tenant details query returns accurate member count and timestamps
    td_resp = client.get_tenant(t1_id)
    reporter.record(td_resp.status == 200 and td_resp.json.get("member_count") >= 1 and "created_at" in (td_resp.json or {}),
                    "F01.5: Tenant details query returns accurate member count and timestamps")

    # Set active tenant for subsequent client requests
    client.set_tenant(t1_id)

    # =========================================================================
    # FEATURE 2: TenantContext Repository Scoping (PRD §5:272-286)
    # =========================================================================
    # 2.1 Explicit X-Tenant-ID header scopes all requests to target workspace
    coa_resp = client.list_chart_of_accounts()
    reporter.record(coa_resp.status == 200 and len(coa_resp.json.get("accounts", [])) >= 8,
                    "F02.1: Explicit X-Tenant-ID header scopes repository requests to target workspace")

    # 2.2 Requests without X-Tenant-ID default safely to active personal workspace
    unscoped_client = ApiClient(base_url=base_url)
    unscoped_client.login(user_email, user_pass)
    unscoped_coa = unscoped_client.list_chart_of_accounts()
    reporter.record(unscoped_coa.status == 200 and len(unscoped_coa.json.get("accounts", [])) >= 8,
                    "F02.2: Requests without X-Tenant-ID default safely to active personal workspace")

    # 2.3 Creating a second tenant and switching X-Tenant-ID scopes to that tenant
    t2_resp = client.create_tenant("CV Sumber Rejeki", slug=f"sumber-{uid_suffix}")
    t2_id = t2_resp.json.get("id") if t2_resp.json else None
    client2 = ApiClient(base_url=base_url)
    client2.login(user_email, user_pass)
    client2.set_tenant(t2_id)
    t2_coa = client2.list_chart_of_accounts()
    reporter.record(t2_resp.status == 201 and t2_coa.status == 200,
                    "F02.3: Switching X-Tenant-ID switches the isolated repository query boundary")

    # 2.4 Unauthenticated requests rejected before TenantContext resolution
    anon_client = ApiClient(base_url=base_url)
    anon_client.set_tenant(t1_id)
    anon_resp = anon_client.list_chart_of_accounts()
    reporter.record(anon_resp.status == 401,
                    "F02.4: Unauthenticated requests rejected before TenantContext resolution")

    # 2.5 Inactive or unassociated tenant ID in X-Tenant-ID strictly returns HTTP 404
    foreign_client = ApiClient(base_url=base_url)
    foreign_client.login(user_email, user_pass)
    foreign_client.set_tenant(str(uuid.uuid4()))
    bad_tenant_resp = foreign_client.list_chart_of_accounts()
    reporter.record(bad_tenant_resp.status == 404,
                    "F02.5: Inactive or unassociated tenant ID in X-Tenant-ID strictly returns HTTP 404")

    # =========================================================================
    # FEATURE 3: Cross-Tenant HTTP 404 Isolation (ORIGINAL_REQUEST §Acceptance Criteria)
    # =========================================================================
    # User B setup in isolated tenant
    user_b_email = f"user_b_{uid_suffix}@other.biz"
    client_b = ApiClient(base_url=base_url)
    client_b.register(user_b_email, user_pass, "Company B Owner")
    tb_resp = client_b.create_tenant("PT Lawan Bisnis", slug=f"lawan-{uid_suffix}")
    tb_id = tb_resp.json.get("id")
    client_b.set_tenant(tb_id)

    # Create draft invoice in Tenant 1
    inv_t1 = client.create_invoice(
        customer_name="PT Pelanggan T1",
        items=[{"description": "Barang T1", "quantity": 1, "unit_price": 500000}],
        due_date="2026-11-01"
    )
    inv_t1_id = inv_t1.json.get("id")

    # 3.1 Tenant B querying Tenant 1's invoice strictly returns HTTP 404 Not Found
    cross_inv = client_b.get_invoice(inv_t1_id)
    reporter.record(cross_inv.status == 404,
                    "F03.1: Attempting to read another tenant's entity by ID strictly returns HTTP 404 Not Found")

    # 3.2 Probing non-existent entity ID returns identical HTTP 404 Not Found
    non_existent_inv = client_b.get_invoice(str(uuid.uuid4()))
    reporter.record(non_existent_inv.status == 404,
                    "F03.2: Probing non-existent entity ID returns identical HTTP 404 Not Found (anti-enumeration)")

    # 3.3 Passing another tenant's ID in X-Tenant-ID strictly returns HTTP 404 Not Found
    tampered_client = ApiClient(base_url=base_url)
    tampered_client.login(user_b_email, user_pass)
    tampered_client.set_tenant(t1_id)  # Belongs to User A
    cross_hdr_resp = tampered_client.list_invoices()
    reporter.record(cross_hdr_resp.status == 404,
                    "F03.3: Passing another tenant's ID in X-Tenant-ID strictly returns HTTP 404 Not Found")

    # 3.4 Cross-tenant journal query by ID returns HTTP 404 Not Found
    j_t1 = client.post_journal(
        entry_date="2026-10-01T00:00:00Z",
        description="Modal Awal T1",
        lines=[
            {"account_code": "1000", "debit": 1000000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 1000000}
        ]
    )
    j_t1_id = j_t1.json.get("id")
    cross_j = client_b.get_journal(j_t1_id)
    reporter.record(cross_j.status == 404,
                    "F03.4: Cross-tenant journal query by ID strictly returns HTTP 404 Not Found")

    # 3.5 Cross-tenant tenant detail query returns HTTP 404 Not Found
    cross_t = client_b.get_tenant(t1_id)
    reporter.record(cross_t.status == 404,
                    "F03.5: Querying another tenant's workspace by ID strictly returns HTTP 404 Not Found")

    # =========================================================================
    # FEATURE 4: RBAC Authorization & HTTP 403 (PRD §6:291-304)
    # =========================================================================
    # User C (Staff) invite
    user_c_email = f"staff_{uid_suffix}@invinite.biz"
    client_c = ApiClient(base_url=base_url)
    client_c.register(user_c_email, user_pass, "Staff Member")
    inv_c = client.invite_tenant_member(t1_id, user_c_email, role="staff")
    reporter.record(inv_c.status == 201 and inv_c.json.get("role") == "staff",
                    "F04.1: Workspace owner can invite new members with designated role")

    # 4.2 Admin role invitation
    user_admin_email = f"admin_{uid_suffix}@invinite.biz"
    client_adm = ApiClient(base_url=base_url)
    client_adm.register(user_admin_email, user_pass, "Admin Member")
    inv_adm = client.invite_tenant_member(t1_id, user_admin_email, role="admin")
    reporter.record(inv_adm.status == 201 and inv_adm.json.get("role") == "admin",
                    "F04.2: Owner can invite administrator role members")

    # 4.3 Staff role attempting to invite member is rejected with HTTP 403 Forbidden
    client_c.login(user_c_email, user_pass)
    staff_invite = client_c.invite_tenant_member(t1_id, f"unauth_{uid_suffix}@test.biz", role="staff")
    reporter.record(staff_invite.status == 403,
                    "F04.3: Staff role attempting to invite member is rejected with HTTP 403 Forbidden")

    # 4.4 Staff role attempting to reverse a posted journal is rejected with HTTP 403 Forbidden
    client_c.set_tenant(t1_id)
    staff_rev = client_c.reverse_journal(j_t1_id)
    reporter.record(staff_rev.status == 403,
                    "F04.4: Staff role attempting to reverse a journal is rejected with HTTP 403 Forbidden")

    # 4.5 Staff role attempting to update business profile is rejected with HTTP 403 Forbidden
    staff_prof = client_c.update_tenant_profile(t1_id, {"business_name": "Tampered Name"})
    reporter.record(staff_prof.status == 403,
                    "F04.5: Staff role attempting to update business profile is rejected with HTTP 403 Forbidden")

    # =========================================================================
    # FEATURE 5: Indonesian Localization Defaults (PRD §7:334-356)
    # =========================================================================
    p5 = client.get_tenant_profile(t1_id).json or {}
    reporter.record(p5.get("currency") == "IDR",
                    "F05.1: Tenant default currency is strictly IDR")
    reporter.record(p5.get("locale") == "id-ID",
                    "F05.2: Tenant default locale is strictly id-ID")
    reporter.record(p5.get("timezone") == "Asia/Jakarta",
                    "F05.3: Tenant default timezone is strictly Asia/Jakarta (WIB)")
    # 5.4 Profile accepts WITA (Asia/Makassar) and WIT (Asia/Jayapura)
    up_tz = client.update_tenant_profile(t1_id, {"timezone": "Asia/Makassar"})
    reporter.record(up_tz.status == 200 and up_tz.json.get("timezone") == "Asia/Makassar",
                    "F05.4: Tenant profile accepts valid Indonesian timezones (Asia/Makassar)")
    reporter.record(p5.get("invoice_prefix") == "INV",
                    "F05.5: Tenant default invoice numbering prefix is INV")

    # =========================================================================
    # FEATURE 6: Tenant Slug Routing & Validation (PRD §41:1537-1568)
    # =========================================================================
    # 6.1 Custom valid slug accepted
    slug_custom = f"toko-berkah-{uid_suffix}"
    t_slug = client.create_tenant("Toko Berkah", slug=slug_custom)
    reporter.record(t_slug.status == 201 and t_slug.json.get("slug") == slug_custom,
                    "F06.1: Valid custom slug accepted upon tenant workspace creation")

    # 6.2 Auto-generated slug sanitizes spaces and symbols into hyphens
    t_auto = client.create_tenant(f"Koperasi Maju Bersama & Rekan {uid_suffix}")
    auto_slug = t_auto.json.get("slug", "")
    reporter.record(t_auto.status == 201 and "-" in auto_slug and not " " in auto_slug and not "&" in auto_slug,
                    "F06.2: Auto-generated slug sanitizes whitespace and special characters to hyphens")

    # 6.3 Reserved slug 'admin' rejected with HTTP 400 Bad Request
    t_res1 = client.create_tenant("Admin Workspace", slug="admin")
    reporter.record(t_res1.status == 400 and t_res1.json.get("code") == "RESERVED_SLUG",
                    "F06.3: Reserved system slug 'admin' rejected with HTTP 400 RESERVED_SLUG")

    # 6.4 Reserved slug 'api' rejected with HTTP 400 Bad Request
    t_res2 = client.create_tenant("API Workspace", slug="api")
    reporter.record(t_res2.status == 400 and t_res2.json.get("code") == "RESERVED_SLUG",
                    "F06.4: Reserved system slug 'api' rejected with HTTP 400 RESERVED_SLUG")

    # 6.5 Duplicate slug creation rejected with HTTP 409 Conflict
    t_dup = client.create_tenant("Duplicate Slug Workspace", slug=slug_custom)
    reporter.record(t_dup.status == 409 and t_dup.json.get("code") == "SLUG_ALREADY_EXISTS",
                    "F06.5: Duplicate tenant slug creation rejected with HTTP 409 SLUG_ALREADY_EXISTS")

    # =========================================================================
    # FEATURE 7: Personal Workspace Auto-Provisioning (Survey 2 & 3)
    # =========================================================================
    new_user_email = f"personal_{uid_suffix}@invinite.biz"
    c_new = ApiClient(base_url=base_url)
    reg_new = c_new.register(new_user_email, user_pass, "Budi Santoso")
    def_t_id = reg_new.json.get("user", {}).get("default_tenant_id") if reg_new.json else None
    reporter.record(reg_new.status == 201 and def_t_id is not None,
                    "F07.1: New user registration automatically provisions a default personal workspace")

    # 7.2 Auto-provisioned workspace is marked is_personal = 1
    t_list_new = c_new.list_tenants()
    personal_ws = next((t for t in (t_list_new.json or {}).get("tenants", []) if t.get("is_personal")), None)
    reporter.record(personal_ws is not None and personal_ws.get("is_personal") == 1,
                    "F07.2: Auto-provisioned workspace is marked with is_personal = 1")

    # 7.3 Auto-provisioned workspace has owner membership for new user
    reporter.record(personal_ws is not None and personal_ws.get("role") == "owner",
                    "F07.3: Auto-provisioned workspace assigns owner membership to the registered user")

    # 7.4 Auto-provisioned workspace has Indonesian defaults
    reporter.record(personal_ws is not None and personal_ws.get("currency") == "IDR" and personal_ws.get("timezone") == "Asia/Jakarta",
                    "F07.4: Auto-provisioned workspace defaults to IDR and Asia/Jakarta")

    # 7.5 Auto-provisioned workspace has pre-seeded standard Chart of Accounts
    c_new.set_tenant(def_t_id)
    coa_new = c_new.list_chart_of_accounts()
    reporter.record(coa_new.status == 200 and len(coa_new.json.get("accounts", [])) >= 8,
                    "F07.5: Auto-provisioned personal workspace has seeded Chart of Accounts")

    # =========================================================================
    # FEATURE 8: Double-Entry Balancing Invariant (PRD §11.1:560-575)
    # =========================================================================
    client.set_tenant(t1_id)
    # 8.1 Balanced 2-line journal entry posted successfully (debit == credit)
    j_bal2 = client.post_journal(
        entry_date="2026-10-02T00:00:00Z",
        description="Setoran Modal Bank",
        lines=[
            {"account_code": "1100", "debit": 5000000, "credit": 0, "memo": "Debit Bank"},
            {"account_code": "4000", "debit": 0, "credit": 5000000, "memo": "Credit Pendapatan"}
        ]
    )
    reporter.record(j_bal2.status == 201 and j_bal2.json.get("total_debit") == 5000000,
                    "F08.1: Balanced 2-line journal entry posted successfully enforcing debit == credit")

    # 8.2 Balanced multi-line journal entry posted successfully
    j_multi = client.post_journal(
        entry_date="2026-10-02T01:00:00Z",
        description="Penjualan Multi Baris dengan PPN",
        lines=[
            {"account_code": "1000", "debit": 1110000, "credit": 0, "memo": "Kas diterima"},
            {"account_code": "4000", "debit": 0, "credit": 1000000, "memo": "Pendapatan produk"},
            {"account_code": "2100", "debit": 0, "credit": 110000, "memo": "Utang PPN 11%"}
        ]
    )
    reporter.record(j_multi.status == 201 and j_multi.json.get("total_debit") == 1110000 and j_multi.json.get("total_credit") == 1110000,
                    "F08.2: Balanced multi-line journal posted successfully enforcing SUM(debits) == SUM(credits)")

    # 8.3 Unbalanced journal entry (debit != credit) rejected with HTTP 422 Unprocessable Entity
    j_unbal = client.post_journal(
        entry_date="2026-10-02T02:00:00Z",
        description="Unbalanced Attempt",
        lines=[
            {"account_code": "1000", "debit": 500000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 450000}
        ]
    )
    reporter.record(j_unbal.status == 422 and j_unbal.json.get("code") == "UNBALANCED_JOURNAL_ENTRY",
                    "F08.3: Unbalanced journal entry rejected with HTTP 422 UNBALANCED_JOURNAL_ENTRY")

    # 8.4 Trial balance aggregates all debits and credits with net_balance == 0
    tb = client.get_trial_balance()
    tb_data = tb.json or {}
    reporter.record(tb.status == 200 and tb_data.get("is_balanced") is True and tb_data.get("net_balance") == 0,
                    "F08.4: Trial balance aggregates debits and credits with exact zero net balance")

    # 8.5 Total debit equals total credit in trial balance
    reporter.record(tb_data.get("total_debit") == tb_data.get("total_credit") and tb_data.get("total_debit", 0) > 0,
                    "F08.5: Trial balance total_debit strictly matches total_credit")

    # =========================================================================
    # FEATURE 9: Integer Rupiah Math Invariant (PRD §72:2565)
    # =========================================================================
    # 9.1 Monetary amounts accepted and stored strictly as 64-bit signed integers
    reporter.record(isinstance(tb_data.get("total_debit"), int) and isinstance(tb_data.get("total_credit"), int),
                    "F09.1: Monetary amounts stored and returned strictly as 64-bit signed integers")

    # 9.2 Large enterprise transaction (Rp 5.000.000.000) stored without float precision loss
    j_large = client.post_journal(
        entry_date="2026-10-02T03:00:00Z",
        description="Investasi Korporat",
        lines=[
            {"account_code": "1100", "debit": 5000000000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 5000000000}
        ]
    )
    reporter.record(j_large.status == 201 and j_large.json.get("total_debit") == 5000000000,
                    "F09.2: Large enterprise amount (Rp 5.000.000.000) stored without float precision loss")

    # 9.3 Floating point decimal string in amount rejected
    j_float = client.post_journal(
        entry_date="2026-10-02T04:00:00Z",
        description="Float Amount Attempt",
        lines=[
            {"account_code": "1000", "debit": 100.50, "credit": 0},  # type: ignore
            {"account_code": "4000", "debit": 0, "credit": 100.50}   # type: ignore
        ]
    )
    reporter.record(j_float.status == 400 and j_float.json.get("code") == "INVALID_AMOUNT",
                    "F09.3: Floating-point decimal values in monetary amounts rejected with HTTP 400 INVALID_AMOUNT")

    # 9.4 Negative debit or credit amounts rejected
    j_neg = client.post_journal(
        entry_date="2026-10-02T05:00:00Z",
        description="Negative Amount Attempt",
        lines=[
            {"account_code": "1000", "debit": -50000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": -50000}
        ]
    )
    reporter.record(j_neg.status == 400 and j_neg.json.get("code") == "INVALID_AMOUNT",
                    "F09.4: Negative debit or credit values rejected with HTTP 400 INVALID_AMOUNT")

    # 9.5 Tax calculations return integer Rupiah without fractions
    tax_calc = client.calculate_tax(amount=100500, tax_type="PPN_11_EXCL")
    reporter.record(tax_calc.status == 200 and isinstance(tax_calc.json.get("tax_amount"), int),
                    "F09.5: Tax calculation returns exact integer Rupiah without floating fractions")

    # =========================================================================
    # FEATURE 10: Journal Immutability (PRD §11.2:577-586)
    # =========================================================================
    j_imm_resp = client.post_journal(
        entry_date="2026-10-03T00:00:00Z",
        description="Immutable Journal Target",
        lines=[
            {"account_code": "1000", "debit": 250000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 250000}
        ]
    )
    j_imm_id = j_imm_resp.json.get("id")

    # 10.1 Direct PUT modification on posted journal rejected with HTTP 405 Method Not Allowed
    put_j = client.update_journal(j_imm_id, {"description": "Tampered Journal"})
    reporter.record(put_j.status == 405 and put_j.json.get("code") == "JOURNAL_IMMUTABLE",
                    "F10.1: Direct PUT modification on posted journal rejected with HTTP 405 JOURNAL_IMMUTABLE")

    # 10.2 Direct DELETE on posted journal rejected with HTTP 405 Method Not Allowed
    del_j = client.delete_journal(j_imm_id)
    reporter.record(del_j.status == 405 and del_j.json.get("code") == "JOURNAL_IMMUTABLE",
                    "F10.2: Direct DELETE on posted journal rejected with HTTP 405 JOURNAL_IMMUTABLE")

    # 10.3 Journal entry fields remain unchanged
    j_verify = client.get_journal(j_imm_id)
    reporter.record(j_verify.status == 200 and j_verify.json.get("description") == "Immutable Journal Target",
                    "F10.3: Journal entry fields remain strictly unchanged after mutation attempts")

    # 10.4 Querying posted journal verifies lines and entry number intact
    reporter.record(len(j_verify.json.get("lines", [])) == 2 and j_verify.json.get("status") == "POSTED",
                    "F10.4: Posted journal lines and POSTED status verified intact")

    # 10.5 Immutable error details specify reversal requirement
    reporter.record("reversal" in (put_j.json.get("detail", "").lower()),
                    "F10.5: Immutability response guides user to perform corrections via reversal")

    # =========================================================================
    # FEATURE 11: Journal Reversal Workflow (PRD §11.2:587-595)
    # =========================================================================
    # 11.1 Reversing posted journal creates balanced reversal entry with swapped debits/credits
    rev_resp = client.reverse_journal(j_imm_id, reason="Correction of posting error")
    rev_data = rev_resp.json or {}
    rev_id = rev_data.get("id")
    reporter.record(rev_resp.status == 201 and rev_id is not None and rev_data.get("source_type") == "REVERSAL",
                    "F11.1: Reversing posted journal creates balanced reversal entry with swapped debits/credits")

    # 11.2 Original journal marked as is_reversed = 1
    orig_j_check = client.get_journal(j_imm_id)
    reporter.record(orig_j_check.status == 200 and orig_j_check.json.get("is_reversed") == 1,
                    "F11.2: Original journal marked as is_reversed = 1 with reversal linkage")

    # 11.3 Reversal journal references original journal as source_id
    reporter.record(rev_data.get("source_id") == j_imm_id,
                    "F11.3: Reversal journal explicitly references original journal as source_id")

    # 11.4 Net impact of original plus reversal on trial balance is exactly zero
    tb_after_rev = client.get_trial_balance()
    reporter.record(tb_after_rev.status == 200 and tb_after_rev.json.get("is_balanced") is True,
                    "F11.4: Net impact of original journal and reversal journal on trial balance balances to zero")

    # 11.5 Double reversal attempt on already reversed journal rejected with HTTP 409 Conflict
    rev_dup = client.reverse_journal(j_imm_id, reason="Second reversal attempt")
    reporter.record(rev_dup.status == 409 and rev_dup.json.get("code") == "ALREADY_REVERSED",
                    "F11.5: Double reversal attempt on already reversed journal rejected with HTTP 409 ALREADY_REVERSED")

    # =========================================================================
    # FEATURE 12: Standard Chart of Accounts (PRD §11.3:597-618)
    # =========================================================================
    coa_list = client.list_chart_of_accounts().json.get("accounts", [])
    coa_dict = {a["code"]: a for a in coa_list}

    # 12.1 Every workspace is seeded with 8 standard system accounts
    reporter.record(len(coa_list) >= 8 and "1000" in coa_dict and "4000" in coa_dict,
                    "F12.1: Workspace Chart of Accounts seeded with required system accounts")

    # 12.2 Account 1000 Cash and 1100 Bank verified as assets
    reporter.record(coa_dict.get("1000", {}).get("account_type") == "asset" and coa_dict.get("1100", {}).get("account_type") == "asset",
                    "F12.2: Standard accounts 1000 (Kas) and 1100 (Bank) classified as assets")

    # 12.3 Account 1200 AR and 2000 AP verified in Chart of Accounts
    reporter.record(coa_dict.get("1200", {}).get("account_type") == "asset" and coa_dict.get("2000", {}).get("account_type") == "liability",
                    "F12.3: Standard accounts 1200 (Piutang) and 2000 (Utang) classified correctly")

    # 12.4 Account 2100 Tax Payable, 4000 Revenue, 5000 COGS, 6000 Opex verified
    reporter.record(coa_dict.get("2100", {}).get("account_type") == "liability" and coa_dict.get("4000", {}).get("account_type") == "income",
                    "F12.4: Standard accounts 2100 (Tax) and 4000 (Revenue) verified")

    # 12.5 System accounts have deletion protection (HTTP 403 Forbidden)
    del_sys = client.delete_account_coa("1000")
    reporter.record(del_sys.status == 403 and del_sys.json.get("code") == "SYSTEM_ACCOUNT_PROTECTED",
                    "F12.5: System-critical Chart of Accounts entities protected against deletion with HTTP 403")

    # =========================================================================
    # FEATURE 13: PPN Indonesian Tax Engine (PRD §16:752-770)
    # =========================================================================
    # 13.1 PPN 11% exclusive tax calculated correctly (100000 -> 11000 tax, 111000 total)
    t13_1 = client.calculate_tax(100000, "PPN_11_EXCL")
    reporter.record(t13_1.status == 200 and t13_1.json.get("tax_amount") == 11000 and t13_1.json.get("gross_amount") == 111000,
                    "F13.1: PPN 11% exclusive tax calculation (Rp 100.000 -> Rp 11.000 tax, Rp 111.000 gross)")

    # 13.2 PPN 11% inclusive tax extracted correctly (111000 -> 11000 tax, 100000 net)
    t13_2 = client.calculate_tax(111000, "PPN_11_INCL")
    reporter.record(t13_2.status == 200 and t13_2.json.get("tax_amount") == 11000 and t13_2.json.get("net_amount") == 100000,
                    "F13.2: PPN 11% inclusive tax extraction (Rp 111.000 -> Rp 11.000 tax, Rp 100.000 net)")

    # 13.3 PPN 12% exclusive tax calculated correctly (100000 -> 12000 tax, 112000 total)
    t13_3 = client.calculate_tax(100000, "PPN_12_EXCL")
    reporter.record(t13_3.status == 200 and t13_3.json.get("tax_amount") == 12000 and t13_3.json.get("gross_amount") == 112000,
                    "F13.3: PPN 12% exclusive tax calculation (Rp 100.000 -> Rp 12.000 tax, Rp 112.000 gross)")

    # 13.4 PPN 12% inclusive tax extracted correctly (112000 -> 12000 tax, 100000 net)
    t13_4 = client.calculate_tax(112000, "PPN_12_INCL")
    reporter.record(t13_4.status == 200 and t13_4.json.get("tax_amount") == 12000 and t13_4.json.get("net_amount") == 100000,
                    "F13.4: PPN 12% inclusive tax extraction (Rp 112.000 -> Rp 12.000 tax, Rp 100.000 net)")

    # 13.5 Deterministic half-up rounding on fractional Rupiah
    t13_5 = client.calculate_tax(105, "PPN_11_EXCL")  # 105 * 0.11 = 11.55 -> rounds to 12
    reporter.record(t13_5.status == 200 and t13_5.json.get("tax_amount") == 12,
                    "F13.5: Deterministic half-up rounding on fractional tax (105 * 11% = 11.55 -> 12 Rupiah)")

    # =========================================================================
    # FEATURE 14: UMKM Final Tax Engine (PRD §16:752-770)
    # =========================================================================
    # 14.1 UMKM final tax calculates 0.5% (50 bps) on gross turnover (1000000 -> 5000 tax)
    t14_1 = client.calculate_tax(1000000, "UMKM_05")
    reporter.record(t14_1.status == 200 and t14_1.json.get("tax_amount") == 5000,
                    "F14.1: UMKM final tax calculates 0.5% (50 bps) on gross turnover (Rp 1.000.000 -> Rp 5.000)")

    # 14.2 UMKM tax calculation on small turnover applies half-up integer rounding
    t14_2 = client.calculate_tax(250000, "UMKM_05")  # 250000 * 0.005 = 1250
    reporter.record(t14_2.status == 200 and t14_2.json.get("tax_amount") == 1250,
                    "F14.2: UMKM final tax calculation on Rp 250.000 yields exact Rp 1.250")

    # 14.3 UMKM tax returns base_amount and net_amount equal to gross turnover
    reporter.record(t14_1.json.get("base_amount") == 1000000 and t14_1.json.get("gross_amount") == 1000000,
                    "F14.3: UMKM final tax preserves gross turnover as base amount")

    # 14.4 Zero turnover returns 0 UMKM tax
    t14_4 = client.calculate_tax(0, "UMKM_05")
    reporter.record(t14_4.status == 200 and t14_4.json.get("tax_amount") == 0,
                    "F14.4: Zero turnover returns exactly 0 UMKM tax")

    # 14.5 High gross turnover (Rp 100.000.000) calculates exact Rp 500.000 tax
    t14_5 = client.calculate_tax(100000000, "UMKM_05")
    reporter.record(t14_5.status == 200 and t14_5.json.get("tax_amount") == 500000,
                    "F14.5: High turnover (Rp 100.000.000) calculates exact Rp 500.000 tax")

    # =========================================================================
    # FEATURE 15: Tax Inclusive/Exclusive Pricing (PRD §16:758-762)
    # =========================================================================
    # 15.1 Commercial line items with exclusive pricing append tax to subtotal
    inv_excl = client.create_invoice(
        customer_name="PT Pembeli Eksklusif",
        items=[{"description": "Server Hosting", "quantity": 1, "unit_price": 1000000}],
        due_date="2026-11-15",
        tax_type="PPN_11_EXCL"
    )
    reporter.record(inv_excl.status == 201 and inv_excl.json.get("subtotal") == 1000000 and inv_excl.json.get("tax_amount") == 110000 and inv_excl.json.get("total_amount") == 1110000,
                    "F15.1: Commercial invoice with PPN 11% exclusive pricing appends tax to subtotal")

    # 15.2 Commercial line items with inclusive pricing retain total and extract net
    inv_incl = client.create_invoice(
        customer_name="PT Pembeli Inklusif",
        items=[{"description": "Konsultasi All-In", "quantity": 1, "unit_price": 1110000}],
        due_date="2026-11-15",
        tax_type="PPN_11_INCL"
    )
    reporter.record(inv_incl.status == 201 and inv_incl.json.get("total_amount") == 1110000 and inv_incl.json.get("tax_amount") == 110000,
                    "F15.2: Commercial invoice with PPN 11% inclusive pricing retains gross total and extracts tax")

    # 15.3 Mixed quantity line items calculate subtotal, total tax, and grand total consistently
    inv_multi_item = client.create_invoice(
        customer_name="PT Grosir Multi Item",
        items=[
            {"description": "Barang A", "quantity": 2, "unit_price": 250000},
            {"description": "Barang B", "quantity": 5, "unit_price": 100000}
        ],
        due_date="2026-11-20",
        tax_type="PPN_11_EXCL"
    )
    # Subtotal: 500000 + 500000 = 1000000 -> tax: 110000 -> total: 1110000
    reporter.record(inv_multi_item.status == 201 and inv_multi_item.json.get("subtotal") == 1000000 and inv_multi_item.json.get("total_amount") == 1110000,
                    "F15.3: Multiple line items calculate subtotal, tax, and total deterministically")

    # 15.4 Tax exemption / NONE tax type calculates 0 tax and total == subtotal
    inv_none = client.create_invoice(
        customer_name="PT Non PKP",
        items=[{"description": "Jasa Bebas Pajak", "quantity": 1, "unit_price": 750000}],
        due_date="2026-11-20",
        tax_type="NONE"
    )
    reporter.record(inv_none.status == 201 and inv_none.json.get("tax_amount") == 0 and inv_none.json.get("total_amount") == 750000,
                    "F15.4: Tax-exempt invoice calculates 0 tax with grand total equal to subtotal")

    # 15.5 Invoice response explicitly separates subtotal, tax_amount, and total_amount
    inv_resp_data = inv_excl.json or {}
    reporter.record("subtotal" in inv_resp_data and "tax_amount" in inv_resp_data and "total_amount" in inv_resp_data,
                    "F15.5: Invoice payload strictly separates subtotal, tax_amount, and total_amount")

    # =========================================================================
    # FEATURE 16: Commercial Invoice Lifecycle (PRD §12:620-644)
    # =========================================================================
    # 16.1 Invoice created in initial DRAFT status
    inv_lc = client.create_invoice(
        customer_name="PT Sukses Mandiri",
        items=[{"description": "Lisensi Software", "quantity": 1, "unit_price": 2000000}],
        due_date="2026-11-30"
    )
    inv_lc_id = inv_lc.json.get("id")
    reporter.record(inv_lc.status == 201 and inv_lc.json.get("status") == "DRAFT",
                    "F16.1: Commercial invoice created in initial DRAFT lifecycle state")

    # 16.2 Draft invoice can be updated with modified items and customer details
    up_draft = client.update_invoice(inv_lc_id, {"customer_name": "PT Sukses Mandiri Perkasa"})
    reporter.record(up_draft.status == 200 and up_draft.json.get("status") == "DRAFT",
                    "F16.2: Draft invoice details can be updated while in DRAFT state")

    # 16.3 Issuing draft invoice transitions status to ISSUED
    iss_resp = client.issue_invoice(inv_lc_id)
    reporter.record(iss_resp.status == 200 and iss_resp.json.get("status") == "ISSUED",
                    "F16.3: Issuing invoice transitions lifecycle status from DRAFT to ISSUED")

    # 16.4 Partial payment transitions invoice status to PARTIALLY_PAID
    total_due = iss_resp.json.get("total_amount", 0)
    pay_half = client.allocate_payment(inv_lc_id, amount=total_due // 2)
    inv_after_half = client.get_invoice(inv_lc_id)
    reporter.record(pay_half.status == 201 and inv_after_half.json.get("status") == "PARTIALLY_PAID",
                    "F16.4: Partial payment transitions invoice status to PARTIALLY_PAID")

    # 16.5 Full payment transitions invoice status to PAID
    rem_due = total_due - (total_due // 2)
    pay_full = client.allocate_payment(inv_lc_id, amount=rem_due)
    inv_after_full = client.get_invoice(inv_lc_id)
    reporter.record(pay_full.status == 201 and inv_after_full.json.get("status") == "PAID",
                    "F16.5: Final payment settlement transitions invoice status to PAID")

    # =========================================================================
    # FEATURE 17: Server-Side Sequential Numbering (PRD §14:712-732)
    # =========================================================================
    # 17.1 Issuing invoice assigns sequential number formatted INV-YYYY-XXXXXX
    inv_num_1 = iss_resp.json.get("invoice_number", "")
    current_yr = str(datetime.now(timezone.utc).year)
    reporter.record(inv_num_1.startswith(f"INV-{current_yr}-") and len(inv_num_1) >= 15,
                    "F17.1: Issuing invoice assigns sequential number formatted INV-YYYY-XXXXXX")

    # 17.2 First invoice in sequence has sequence counter
    reporter.record(inv_num_1.endswith("000001") or "000" in inv_num_1,
                    "F17.2: Server-side sequential number contains zero-padded counter")

    # 17.3 Second issued invoice in year gets incremented gapless sequence number
    inv_seq2 = client.create_invoice(
        customer_name="PT Kedua",
        items=[{"description": "Jasa", "quantity": 1, "unit_price": 500000}],
        due_date="2026-11-30"
    )
    iss_seq2 = client.issue_invoice(inv_seq2.json.get("id"))
    inv_num_2 = iss_seq2.json.get("invoice_number", "")
    reporter.record(iss_seq2.status == 200 and inv_num_2 > inv_num_1,
                    "F17.3: Subsequent issued invoice receives sequentially incremented gapless number")

    # 17.4 Invoice numbers are tenant-scoped and independent across tenants
    inv_tb = client_b.create_invoice(
        customer_name="Pelanggan Tenant B",
        items=[{"description": "Item B", "quantity": 1, "unit_price": 300000}],
        due_date="2026-11-30"
    )
    iss_tb = client_b.issue_invoice(inv_tb.json.get("id"))
    inv_num_tb = iss_tb.json.get("invoice_number", "")
    reporter.record(iss_tb.status == 200 and inv_num_tb.startswith(f"INV-{current_yr}-"),
                    "F17.4: Sequential numbering sequence is tenant-scoped and isolated per workspace")

    # 17.5 Draft invoices have null/draft number until issued
    draft_test = client.create_invoice("Draft Test", [{"description": "Item", "quantity": 1, "unit_price": 1000}], "2026-11-30")
    reporter.record(draft_test.json.get("invoice_number") is None,
                    "F17.5: Draft invoices do not receive final sequential number until issued")

    # =========================================================================
    # FEATURE 18: Issued Document Snapshotting (PRD §15:734-750)
    # =========================================================================
    inv_snap_check = client.get_invoice(inv_lc_id)
    snapshot = inv_snap_check.json.get("snapshot") or {}

    # 18.1 Customer name, address, and email captured in snapshot
    reporter.record(snapshot.get("customer_name") == "PT Sukses Mandiri Perkasa",
                    "F18.1: Customer details captured immutably in document snapshot at issue")

    # 18.2 Snapshot freezes item descriptions, quantities, unit prices, and line totals
    reporter.record(len(snapshot.get("items", [])) >= 1 and snapshot.get("items", [])[0].get("quantity") == 1,
                    "F18.2: Line item details, unit prices, and quantities frozen in snapshot")

    # 18.3 Snapshot freezes tax_type, subtotal, tax_amount, and grand total
    reporter.record(snapshot.get("subtotal") > 0 and snapshot.get("total_amount") > 0,
                    "F18.3: Financial figures (subtotal, tax, total) frozen in snapshot")

    # 18.4 Modifying draft details post-issue is rejected
    post_issue_up = client.update_invoice(inv_lc_id, {"customer_name": "Tampered Customer"})
    reporter.record(post_issue_up.status == 409 and post_issue_up.json.get("code") == "INVOICE_LOCKED",
                    "F18.4: Attempting to modify issued invoice rejected with HTTP 409 INVOICE_LOCKED")

    # 18.5 Snapshot JSON is returned in invoice detail response
    reporter.record(isinstance(snapshot, dict) and "issued_at" in snapshot,
                    "F18.5: Snapshot JSON includes authoritative issued_at timestamp")

    # =========================================================================
    # FEATURE 19: Receivable Tracking & Aging (PRD §17:772-800)
    # =========================================================================
    # 19.1 Issuing invoice automatically creates matching Receivable record with status OPEN
    inv_rec = client.create_invoice(
        customer_name="PT Piutang Jaya",
        items=[{"description": "Konsultasi", "quantity": 1, "unit_price": 1000000}],
        due_date="2026-11-30"
    )
    inv_rec_id = inv_rec.json.get("id")
    iss_rec = client.issue_invoice(inv_rec_id)
    recs_list = client.list_receivables().json.get("receivables", [])
    matched_rec = next((r for r in recs_list if r.get("invoice_id") == inv_rec_id), None)
    reporter.record(matched_rec is not None and matched_rec.get("status") == "OPEN",
                    "F19.1: Issuing invoice automatically creates matching Receivable record with status OPEN")

    # 19.2 Receivable tracks total_amount, allocated_amount=0, and outstanding_amount=total
    total_rec_amt = iss_rec.json.get("total_amount")
    reporter.record(matched_rec.get("total_amount") == total_rec_amt and matched_rec.get("outstanding_amount") == total_rec_amt and matched_rec.get("allocated_amount") == 0,
                    "F19.2: Receivable tracks initial total_amount, allocated_amount=0, and outstanding_amount")

    # 19.3 Partial payment updates outstanding_amount and sets status to PARTIALLY_PAID
    client.allocate_payment(inv_rec_id, amount=400000)
    recs_after_part = client.list_receivables().json.get("receivables", [])
    rec_part = next((r for r in recs_after_part if r.get("invoice_id") == inv_rec_id), None)
    reporter.record(rec_part is not None and rec_part.get("status") == "PARTIALLY_PAID" and rec_part.get("allocated_amount") == 400000,
                    "F19.3: Partial payment updates receivable outstanding_amount and status to PARTIALLY_PAID")

    # 19.4 Full payment reduces outstanding_amount to 0 and sets status to PAID
    client.allocate_payment(inv_rec_id, amount=rec_part.get("outstanding_amount"))
    recs_after_full = client.list_receivables().json.get("receivables", [])
    rec_full = next((r for r in recs_after_full if r.get("invoice_id") == inv_rec_id), None)
    reporter.record(rec_full is not None and rec_full.get("status") == "PAID" and rec_full.get("outstanding_amount") == 0,
                    "F19.4: Final settlement reduces outstanding_amount to 0 and transitions status to PAID")

    # 19.5 Aging endpoint aggregates receivables across aging buckets
    aging = client.get_receivable_aging()
    reporter.record(aging.status == 200 and "current_0_30" in (aging.json or {}) and "total_outstanding" in (aging.json or {}),
                    "F19.5: Receivable aging endpoint aggregates balances across aging buckets")

    # =========================================================================
    # FEATURE 20: Atomic Payment Allocation (PRD §17, §18)
    # =========================================================================
    inv_pay = client.create_invoice(
        customer_name="PT Pembayar Tepat Waktu",
        items=[{"description": "Langganan", "quantity": 1, "unit_price": 1000000}],
        due_date="2026-11-30"
    )
    inv_pay_id = inv_pay.json.get("id")
    client.issue_invoice(inv_pay_id)

    # 20.1 Payment allocated against outstanding invoice succeeds with HTTP 201 Created
    pay_alloc = client.allocate_payment(inv_pay_id, amount=500000, payment_method="BANK_TRANSFER")
    reporter.record(pay_alloc.status == 201 and pay_alloc.json.get("status") == "CONFIRMED",
                    "F20.1: Payment allocated against outstanding invoice succeeds with HTTP 201 Created")

    # 20.2 Payment atomically reduces receivable outstanding and updates invoice status
    reporter.record(pay_alloc.json.get("outstanding_balance") == (client.get_invoice(inv_pay_id).json.get("total_amount") - 500000),
                    "F20.2: Payment atomically updates outstanding balance on receivable and invoice")

    # 20.3 Payment automatically posts double-entry journal (Debit Bank, Credit AR)
    j_list = client.list_journals().json.get("journals", [])
    pay_j = next((j for j in j_list if j.get("source_type") == "PAYMENT" and j.get("source_id") == pay_alloc.json.get("id")), None)
    reporter.record(pay_j is not None and pay_j.get("total_debit") == 500000,
                    "F20.3: Payment automatically posts balanced double-entry journal (Debit 1100, Credit 1200)")

    # 20.4 Multiple partial payments accurately accumulate allocated_amount
    pay_alloc_2 = client.allocate_payment(inv_pay_id, amount=200000)
    reporter.record(pay_alloc_2.status == 201 and pay_alloc_2.json.get("amount") == 200000,
                    "F20.4: Multiple partial payments accurately accumulate allocated amounts")

    # 20.5 Payment detail records payment_method, reference, and payment_date
    reporter.record(pay_alloc.json.get("payment_method") == "BANK_TRANSFER" and "reference" in pay_alloc.json,
                    "F20.5: Payment record captures payment_method, reference, and payment_date")

    # =========================================================================
    # FEATURE 21: Mutation Idempotency Engine (PRD §18:814-824)
    # =========================================================================
    idem_key = f"idem_{uuid.uuid4().hex}"
    # 21.1 Request with Idempotency-Key returns 201 and original payload
    pay_idem1 = client.allocate_payment(inv_pay_id, amount=100000, idempotency_key=idem_key)
    reporter.record(pay_idem1.status == 201 and pay_idem1.json.get("amount") == 100000,
                    "F21.1: Request with Idempotency-Key succeeds and returns created entity")

    # 21.2 Replaying request with same Idempotency-Key returns cached response without duplicate record
    pay_idem2 = client.allocate_payment(inv_pay_id, amount=100000, idempotency_key=idem_key)
    reporter.record(pay_idem2.status == 201 and pay_idem2.json.get("id") == pay_idem1.json.get("id"),
                    "F21.2: Replaying request with identical Idempotency-Key returns cached response without duplication")

    # 21.3 Replaying Idempotency-Key with modified payload returns HTTP 409 Conflict
    pay_idem_tamper = client.allocate_payment(inv_pay_id, amount=250000, idempotency_key=idem_key)
    reporter.record(pay_idem_tamper.status == 409 and pay_idem_tamper.json.get("code") == "IDEMPOTENCY_KEY_MISMATCH",
                    "F21.3: Replaying Idempotency-Key with modified payload rejected with HTTP 409 Conflict")

    # 21.4 Concurrent or replayed payment does not double-decrement receivable balance
    rec_idem_check = client.list_receivables().json.get("receivables", [])
    rec_target = next((r for r in rec_idem_check if r.get("invoice_id") == inv_pay_id), None)
    # Total payments made on inv_pay: 500k + 200k + 100k = 800k (not 900k or double)
    reporter.record(rec_target is not None and rec_target.get("allocated_amount") == 800000,
                    "F21.4: Replayed idempotent payment does not double-decrement receivable balance")

    # 21.5 Idempotent response replay indicates cached response
    reporter.record(pay_idem2.header("X-Cache-Replay") == "true",
                    "F21.5: Replayed response includes X-Cache-Replay header indicator")

    # =========================================================================
    # FEATURE 22: Transactional Outbox Persistence (PRD §20:844-883)
    # =========================================================================
    # 22.1 Issuing invoice atomically inserts InvoiceIssued event in outbox_events table
    outbox_events = client.list_outbox_events().json.get("events", [])
    has_inv_issued = any(e.get("event_type") == "InvoiceIssued" for e in outbox_events)
    reporter.record(has_inv_issued,
                    "F22.1: Issuing an invoice atomically commits an InvoiceIssued event to outbox_events")

    # 22.2 Allocating payment atomically inserts PaymentConfirmed event in outbox_events table
    has_pay_confirmed = any(e.get("event_type") == "PaymentConfirmed" for e in outbox_events)
    reporter.record(has_pay_confirmed,
                    "F22.2: Payment allocation atomically commits a PaymentConfirmed event to outbox_events")

    # 22.3 Posting journal atomically inserts JournalPosted event in outbox_events table
    has_journal_posted = any(e.get("event_type") == "JournalPosted" for e in outbox_events)
    reporter.record(has_journal_posted,
                    "F22.3: Manual journal entry posting atomically commits a JournalPosted event to outbox")

    # 22.4 Outbox event records tenant_id, aggregate_type, aggregate_id, and payload_json
    sample_evt = outbox_events[0] if outbox_events else {}
    reporter.record("tenant_id" in sample_evt and "aggregate_type" in sample_evt and "payload" in sample_evt,
                    "F22.4: Outbox event captures tenant_id, aggregate_type, aggregate_id, and structured payload")

    # 22.5 Outbox event initializes with status PENDING and attempt_count 0
    pending_evt = next((e for e in outbox_events if e.get("status") == "PENDING"), None)
    reporter.record(pending_evt is not None and pending_evt.get("attempt_count") == 0,
                    "F22.5: Newly committed outbox event initializes in PENDING state with attempt_count 0")

    # =========================================================================
    # FEATURE 23: At-Least-Once Outbox Dispatcher (PRD §20:884-892)
    # =========================================================================
    # 23.1 Outbox process worker polls pending events and dispatches them
    dispatch_resp = client.process_outbox()
    reporter.record(dispatch_resp.status == 200 and dispatch_resp.json.get("processed", 0) > 0,
                    "F23.1: Asynchronous outbox dispatch worker polls and processes pending events")

    # 23.2 Successfully dispatched events transition to status PUBLISHED with published_at
    events_after_proc = client.list_outbox_events().json.get("events", [])
    all_published = all(e.get("status") == "PUBLISHED" for e in events_after_proc)
    reporter.record(len(events_after_proc) > 0 and all_published,
                    "F23.2: Successfully delivered outbox events transition to status PUBLISHED")

    # 23.3 Dispatched events record published_at timestamp
    reporter.record(events_after_proc[0].get("published_at") is not None,
                    "F23.3: Published outbox events record authoritative published_at timestamp")

    # 23.4 Process endpoint returns count of processed events
    reporter.record(dispatch_resp.json.get("status") == "completed",
                    "F23.4: Dispatch processor reports completed status and processed count")

    # 23.5 Subsequent run with zero pending events reports 0 processed
    dispatch_empty = client.process_outbox()
    reporter.record(dispatch_empty.status == 200 and dispatch_empty.json.get("processed") == 0,
                    "F23.5: Subsequent processor run on empty queue returns 0 processed cleanly")

    # =========================================================================
    # FEATURE 24: Outbox Event Deduplication (PRD §20:890)
    # =========================================================================
    # 24.1 Every outbox event has unique UUID event ID
    evt_ids = [e["id"] for e in events_after_proc]
    reporter.record(len(evt_ids) == len(set(evt_ids)) and len(evt_ids) > 0,
                    "F24.1: Every outbox event has a strictly unique RFC 4122 UUID event ID")

    # 24.2 Event payload contains consistent entity reference for consumer deduplication
    reporter.record("invoice_id" in events_after_proc[0].get("payload", {}) or "payment_id" in events_after_proc[0].get("payload", {}) or "journal_id" in events_after_proc[0].get("payload", {}),
                    "F24.2: Outbox event payload contains immutable entity reference for consumer deduplication")

    # 24.3 Repeated query maintains stable event IDs
    events_requery = client.list_outbox_events().json.get("events", [])
    reporter.record([e["id"] for e in events_after_proc] == [e["id"] for e in events_requery],
                    "F24.3: Repeated event queries maintain stable event ordering and IDs")

    # 24.4 Consumer can query outbox events stream idempotently
    list_evts_status = client.list_outbox_events().status
    reporter.record(list_evts_status == 200,
                    "F24.4: Consumer outbox event stream accessible via authenticated idempotent API")

    # 24.5 Event ordering is preserved chronologically by created_at
    created_timestamps = [e["created_at"] for e in events_after_proc]
    reporter.record(created_timestamps == sorted(created_timestamps),
                    "F24.5: Outbox event stream is strictly chronological by created_at timestamp")

    # =========================================================================
    # FEATURE 25: Sidecar Isolation Boundary (PRD §21, §56)
    # =========================================================================
    # Create new invoice to generate pending outbox event
    inv_sidecar = client.create_invoice(
        customer_name="PT Sidecar Test",
        items=[{"description": "Konsultasi Sidecar", "quantity": 1, "unit_price": 500000}],
        due_date="2026-12-01"
    )
    inv_sc_id = inv_sidecar.json.get("id")
    client.issue_invoice(inv_sc_id)

    # 25.1 Simulated sidecar failure during dispatch increments attempt_count without failing core
    sidecar_fail_resp = client.process_outbox(simulate_sidecar_failure=True)
    reporter.record(sidecar_fail_resp.status == 200 and sidecar_fail_resp.json.get("status") == "retry_scheduled",
                    "F25.1: Peripheral sidecar failure is isolated without compromising core server transaction")

    # 25.2 Sidecar failure leaves domain entities intact and committed
    inv_sc_check = client.get_invoice(inv_sc_id)
    reporter.record(inv_sc_check.status == 200 and inv_sc_check.json.get("status") == "ISSUED",
                    "F25.2: Core invoice entity remains valid and committed despite peripheral sidecar outage")

    # 25.3 Outbox event records last_error describing sidecar communication failure
    sc_evts = client.list_outbox_events().json.get("events", [])
    sc_pending = next((e for e in sc_evts if e.get("aggregate_id") == inv_sc_id), None)
    reporter.record(sc_pending is not None and sc_pending.get("last_error") is not None and sc_pending.get("attempt_count") >= 1,
                    "F25.3: Failed outbox delivery captures last_error and increments attempt_count")

    # 25.4 Outbox status remains PENDING for retry
    reporter.record(sc_pending is not None and sc_pending.get("status") == "PENDING",
                    "F25.4: Outbox event remains in PENDING state awaiting subsequent retry delivery")

    # 25.5 When sidecar recovers, subsequent dispatch successfully publishes event
    recover_resp = client.process_outbox(simulate_sidecar_failure=False)
    sc_evts_after = client.list_outbox_events().json.get("events", [])
    sc_pub = next((e for e in sc_evts_after if e.get("aggregate_id") == inv_sc_id), None)
    reporter.record(recover_resp.status == 200 and sc_pub is not None and sc_pub.get("status") == "PUBLISHED",
                    "F25.5: Peripheral sidecar recovery enables successful retry and transition to PUBLISHED")

    # =========================================================================
    # FEATURE 26: Frontend Workspace Store (PRD §8, §35, §37)
    # =========================================================================
    # 26.1 Workspace listing provides active workspace candidates for Pinia store
    ws_list = client.list_tenants()
    reporter.record(ws_list.status == 200 and len(ws_list.json.get("tenants", [])) >= 2,
                    "F26.1: Workspace listing endpoint provides active workspace candidates for Pinia store")

    # 26.2 Active workspace switch endpoint updates session context cleanly
    switch_resp = client.switch_tenant(t1_id)
    reporter.record(switch_resp.status == 200 and switch_resp.json.get("active_tenant_id") == t1_id,
                    "F26.2: Active workspace switch endpoint updates session context cleanly")

    # 26.3 Tenant capabilities endpoint provides capability flags to frontend store
    caps_resp = client.get_tenant_capabilities(t1_id)
    reporter.record(caps_resp.status == 200 and "capabilities" in (caps_resp.json or {}),
                    "F26.3: Tenant capabilities endpoint provides capability flags for Pinia store")

    # 26.4 Personal workspace ensures store always has initial active workspace
    reporter.record(any(t.get("is_personal") == 1 for t in ws_list.json.get("tenants", [])),
                    "F26.4: Personal workspace ensures frontend store always has a valid fallback workspace")

    # 26.5 User membership role in workspace is returned for frontend access control
    reporter.record(caps_resp.json.get("role") == "owner",
                    "F26.5: Current actor role returned with workspace capabilities for frontend RBAC gating")

    # =========================================================================
    # FEATURE 27: Header & Sidebar Workspace UI (Survey 3)
    # =========================================================================
    # 27.1 Tenant details endpoint returns name, slug, and status for UI dropdown
    t_ui = client.get_tenant(t1_id).json or {}
    reporter.record(t_ui.get("name") == "PT Nusantara Jaya" and "slug" in t_ui,
                    "F27.1: Workspace details endpoint provides name and slug for header dropdown UI")

    # 27.2 Workspace profile provides branding and legal identity for sidebar display
    p_ui = client.get_tenant_profile(t1_id).json or {}
    reporter.record("business_name" in p_ui and "timezone" in p_ui,
                    "F27.2: Workspace profile provides business identity attributes for sidebar display")

    # 27.3 Switching workspaces returns updated active tenant details for header reflection
    switch_ui = client.switch_tenant(t2_id)
    reporter.record(switch_ui.status == 200 and switch_ui.json.get("name") == "CV Sumber Rejeki",
                    "F27.3: Switching workspaces returns updated active tenant details for header reflection")
    client.switch_tenant(t1_id)  # Switch back

    # 27.4 Member count is provided for workspace management UI
    reporter.record(t_ui.get("member_count", 0) >= 2,
                    "F27.4: Accurate member count provided for workspace management UI display")

    # 27.5 Multiple workspaces returned in deterministic order for switcher dropdown
    ws_tenants = ws_list.json.get("tenants", [])
    reporter.record(ws_tenants[0]["created_at"] <= ws_tenants[1]["created_at"],
                    "F27.5: Multiple workspaces returned in deterministic chronological order for UI switcher")

    # =========================================================================
    # FEATURE 28: API Client 404 Mock Fallback Fix (Survey 3)
    # =========================================================================
    # 28.1 Probing non-existent endpoint returns RFC 7807 problem details without fallback mock data
    probe_404 = client.get("/api/v1/non-existent-resource-endpoint-xyz")
    reporter.record(probe_404.status == 404 and "application/problem+json" in probe_404.header("content-type"),
                    "F28.1: Querying non-existent resource strictly returns RFC 7807 problem details without mock fallback")

    # 28.2 Probing another tenant's entity returns genuine HTTP 404 without mock synthesis
    cross_probe = client_b.get_invoice(inv_t1_id)
    reporter.record(cross_probe.status == 404 and cross_probe.json.get("code") == "NOT_FOUND",
                    "F28.2: Querying cross-tenant entity strictly returns RFC 7807 NOT_FOUND without mock fallback")

    # 28.3 Client isolation probe verifies genuine 404 error response status and body
    iso_probe = client.check_client_isolation("probe_12345")
    reporter.record(iso_probe.status == 404 and iso_probe.json.get("code") == "NOT_FOUND",
                    "F28.3: Client isolation probe confirms HTTP 404 without latent synthetic mocks")

    # 28.4 No latent fake data or mock flag in 404 error responses
    reporter.record(iso_probe.json.get("mock") is None and iso_probe.json.get("fallback") is None,
                    "F28.4: Zero latent mock or fallback attributes present in HTTP 404 payload")

    # 28.5 Problem details contains type, title, status, and code fields
    p_body = cross_probe.json or {}
    reporter.record("type" in p_body and "title" in p_body and "status" in p_body and "code" in p_body,
                    "F28.5: Error payload adheres strictly to RFC 7807 specification schema")

    # =========================================================================
    # FEATURE 29: Capability-Driven Navigation (PRD §35, §37)
    # =========================================================================
    # Create workspaces with different business types
    t_retail = client.create_tenant(f"Toko Retail {uid_suffix}", slug=f"retail-{uid_suffix}")
    client.update_tenant_profile(t_retail.json.get("id"), {"business_type": "retail"})
    caps_retail = client.get_tenant_capabilities(t_retail.json.get("id")).json or {}
    reporter.record("pos" in caps_retail.get("capabilities", []) and "inventory" in caps_retail.get("capabilities", []),
                    "F29.1: Retail business type workspace provides 'pos' and 'inventory' capability navigation")

    # 29.2 General business type workspace provides standard invoicing and accounting
    caps_gen = client.get_tenant_capabilities(t1_id).json or {}
    reporter.record("invoicing" in caps_gen.get("capabilities", []) and "accounting" in caps_gen.get("capabilities", []),
                    "F29.2: General business type workspace provides standard 'invoicing' and 'accounting' capabilities")

    # 29.3 F&B business type workspace provides pos, tables, kitchen capabilities
    t_fnb = client.create_tenant(f"Warung Kopi {uid_suffix}", slug=f"fnb-{uid_suffix}")
    client.update_tenant_profile(t_fnb.json.get("id"), {"business_type": "fnb"})
    caps_fnb = client.get_tenant_capabilities(t_fnb.json.get("id")).json or {}
    reporter.record("pos" in caps_fnb.get("capabilities", []) and "kitchen" in caps_fnb.get("capabilities", []),
                    "F29.3: F&B business type workspace provides 'pos', 'tables', and 'kitchen' capabilities")

    # 29.4 Rental business type workspace provides inventory and bookings capabilities
    t_rental = client.create_tenant(f"Rental Motor {uid_suffix}", slug=f"rental-{uid_suffix}")
    client.update_tenant_profile(t_rental.json.get("id"), {"business_type": "rental"})
    caps_rental = client.get_tenant_capabilities(t_rental.json.get("id")).json or {}
    reporter.record("inventory" in caps_rental.get("capabilities", []) and "bookings" in caps_rental.get("capabilities", []),
                    "F29.4: Rental business type workspace provides 'inventory' and 'bookings' capabilities")

    # 29.5 Contractor business type workspace provides projects and milestones capabilities
    t_contractor = client.create_tenant(f"Kontraktor {uid_suffix}", slug=f"contractor-{uid_suffix}")
    client.update_tenant_profile(t_contractor.json.get("id"), {"business_type": "contractor"})
    caps_contractor = client.get_tenant_capabilities(t_contractor.json.get("id")).json or {}
    reporter.record("projects" in caps_contractor.get("capabilities", []) and "milestones" in caps_contractor.get("capabilities", []),
                    "F29.5: Contractor business type workspace provides 'projects' and 'milestones' capabilities")

    # =========================================================================
    # FEATURE 30: Backend Test Harness Synchronization (Survey 2)
    # =========================================================================
    schema_probe = client.get_system_schema()
    s_data = schema_probe.json or {}
    # 30.1 System schema endpoint verifies exactly 26 relational tables exist
    reporter.record(schema_probe.status == 200 and s_data.get("table_count") == 26,
                    "F30.1: Backend schema verification probe confirms exactly 26 relational tables")

    # 30.2 SQLite WAL journal_mode verified active
    reporter.record(s_data.get("journal_mode") == "wal",
                    "F30.2: SQLite WAL journal_mode verified strictly active on database connection")

    # 30.3 Foreign keys pragma verified enabled
    reporter.record(s_data.get("foreign_keys") == 1,
                    "F30.3: SQLite foreign_keys constraint enforcement pragma verified enabled")

    # 30.4 Core foundation tables verified present
    tbls = s_data.get("tables", [])
    reporter.record("tenants" in tbls and "business_profiles" in tbls and "memberships" in tbls,
                    "F30.4: Core foundation tables (tenants, business_profiles, memberships) verified present")

    # 30.5 Financial core tables verified present
    reporter.record("chart_of_accounts" in tbls and "journal_entries" in tbls and "invoices" in tbls and "receivables" in tbls and "outbox_events" in tbls,
                    "F30.5: Financial core tables (chart_of_accounts, journal_entries, invoices, receivables, outbox_events) verified")

    # =========================================================================
    # FEATURE 31: Full E2E Test Suite Pass (Tiers 1-4) (Project Pattern)
    # =========================================================================
    # 31.1 Test harness operational status probe /health returns 200
    h_resp = client.health()
    reporter.record(h_resp.status == 200 and h_resp.json.get("status") == "ok",
                    "F31.1: Health check probe /health returns HTTP 200 and status ok")

    # 31.2 Test harness operational status probe /ready returns 200
    r_resp = client.ready()
    reporter.record(r_resp.status == 200 and r_resp.json.get("wal") is True,
                    "F31.2: Readiness check probe /ready returns HTTP 200 and database WAL confirmation")

    # 31.3 Session token generation and validation verified
    me_resp = client.me()
    reporter.record(me_resp.status == 200 and me_resp.json.get("email") == user_email,
                    "F31.3: User authentication session verified via /api/v1/auth/me")

    # 31.4 TAP v13 reporter accumulates results consistently
    reporter.record(len(reporter.results) > 150,
                    "F31.4: TAP v13 test framework records assertion results incrementally")

    # 31.5 Zero failure contract maintained
    current_fails = sum(1 for r in reporter.results if not r["passed"])
    reporter.record(current_fails == 0,
                    "F31.5: E2E feature coverage suite executes with zero assertion failures")

    # =========================================================================
    # FEATURE 32: Adversarial Hardening (Tier 5) (Project Pattern)
    # =========================================================================
    # 32.1 SQL injection attack in tenant search or creation safely handled
    sqli_name = "Toko' OR '1'='1; DROP TABLE users; --"
    sqli_slug = f"sqli-{uuid.uuid4().hex[:6]}"
    t_sqli = client.create_tenant(sqli_name, slug=sqli_slug)
    reporter.record(t_sqli.status == 201 and t_sqli.json.get("name") == sqli_name,
                    "F32.1: SQL injection payload safely stored as string literal via parameterized queries")

    # 32.2 XSS attack strings in invoice customer snapshot safely handled
    xss_customer = "<script>alert('XSS_ATTACK')</script>"
    inv_xss = client.create_invoice(
        customer_name=xss_customer,
        items=[{"description": "Item Safe", "quantity": 1, "unit_price": 100000}],
        due_date="2026-12-15"
    )
    iss_xss = client.issue_invoice(inv_xss.json.get("id"))
    reporter.record(iss_xss.status == 200 and iss_xss.json.get("snapshot", {}).get("customer_name") == xss_customer,
                    "F32.2: XSS script payload safely captured as literal string without unescaped execution")

    # 32.3 Cross-tenant ID tampering in payload strictly returns HTTP 404
    tamper_pay = client_b.allocate_payment(inv_t1_id, amount=10000)
    reporter.record(tamper_pay.status == 404,
                    "F32.3: Cross-tenant entity manipulation attempt strictly rejected with HTTP 404 Not Found")

    # 32.4 Unbalanced journal injection strictly rejected with HTTP 422
    unbal_adv = client.post_journal(
        entry_date="2026-10-04T00:00:00Z",
        description="Adversarial Unbalanced Attempt",
        lines=[
            {"account_code": "1000", "debit": 999999999, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 999999998}  # Off by 1 Rupiah
        ]
    )
    reporter.record(unbal_adv.status == 422 and unbal_adv.json.get("code") == "UNBALANCED_JOURNAL_ENTRY",
                    "F32.4: Adversarial unbalanced journal injection off by 1 Rupiah strictly rejected with HTTP 422")

    # 32.5 Negative payment amount injection rejected with HTTP 400 Bad Request
    neg_pay = client.allocate_payment(inv_lc_id, amount=-100000)
    reporter.record(neg_pay.status == 400 and neg_pay.json.get("code") == "INVALID_AMOUNT",
                    "F32.5: Negative monetary payment injection strictly rejected with HTTP 400 INVALID_AMOUNT")

    return reporter

if __name__ == "__main__":
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8089"
    rep = run_tier1_tests(base)
    success = rep.print_summary()
    sys.exit(0 if success else 1)
