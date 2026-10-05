#!/usr/bin/env python3
"""
Test Client & TAP v13 Framework for Invinite E2E Tests.
Independent opaque-box HTTP client interacting with live or mock server instance.
"""

import sys
import os
import json
import time
import uuid
import urllib.request
import urllib.error
import urllib.parse
import hmac
import hashlib
from typing import Dict, Any, Optional, Tuple, List

os.environ["no_proxy"] = "localhost,127.0.0.1"
os.environ["NO_PROXY"] = "localhost,127.0.0.1"

sys.path.insert(0, os.path.abspath(os.path.dirname(__file__)))
from crypto_keys import sign_dana_payload

class ApiResponse:
    def __init__(self, status: int, headers: Dict[str, str], body: bytes):
        self.status = status
        self.headers = {k.lower(): v for k, v in headers.items()}
        self.raw_body = body
        self._json = None

    @property
    def json(self) -> Any:
        if self._json is None and self.raw_body:
            try:
                self._json = json.loads(self.raw_body.decode("utf-8"))
            except Exception:
                self._json = None
        return self._json

    @property
    def text(self) -> str:
        return self.raw_body.decode("utf-8", errors="replace")

    def header(self, name: str) -> Optional[str]:
        return self.headers.get(name.lower())

class ApiClient:
    def __init__(self, base_url: str = "http://127.0.0.1:8089", client_ip: Optional[str] = None):
        self.base_url = base_url.rstrip("/")
        self.cookies: Dict[str, str] = {}
        self.tenant_id: Optional[str] = None
        if not client_ip:
            u = uuid.uuid4().hex
            client_ip = f"10.{(int(u[0:2], 16) % 250) + 1}.{(int(u[2:4], 16) % 250) + 1}.{(int(u[4:6], 16) % 250) + 1}"
        self.default_headers = {
            "User-Agent": "Invinite-E2E-Tester/4.1.0",
            "Accept": "application/json",
            "X-Forwarded-For": client_ip
        }

    def set_cookie(self, name: str, value: str):
        self.cookies[name] = value

    def clear_cookies(self):
        self.cookies.clear()

    def set_tenant(self, tenant_id: Optional[str]):
        self.tenant_id = tenant_id

    def _build_request(self, method: str, path: str, data: Optional[bytes] = None, headers: Optional[Dict[str, str]] = None) -> urllib.request.Request:
        url = f"{self.base_url}{path}" if path.startswith("/") else f"{self.base_url}/{path}"
        req = urllib.request.Request(url, data=data, method=method)

        for k, v in self.default_headers.items():
            req.add_header(k, v)

        if self.tenant_id:
            req.add_header("X-Tenant-ID", self.tenant_id)

        if headers:
            for k, v in headers.items():
                req.add_header(k, v)

        if self.cookies:
            cookie_str = "; ".join(f"{k}={v}" for k, v in self.cookies.items())
            req.add_header("Cookie", cookie_str)

        return req

    def request(self, method: str, path: str, json_data: Any = None, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        body = None
        req_headers = dict(headers or {})
        if json_data is not None:
            body = json.dumps(json_data).encode("utf-8")
            if "Content-Type" not in req_headers and "content-type" not in req_headers:
                req_headers["Content-Type"] = "application/json"

        req = self._build_request(method, path, data=body, headers=req_headers)
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                status = resp.getcode()
                resp_headers = dict(resp.headers)
                resp_body = resp.read()

                set_cookie = resp_headers.get("Set-Cookie") or resp_headers.get("set-cookie")
                if set_cookie:
                    parts = set_cookie.split(";")
                    if parts and "=" in parts[0]:
                        c_name, c_val = parts[0].split("=", 1)
                        self.cookies[c_name.strip()] = c_val.strip()

                return ApiResponse(status, resp_headers, resp_body)
        except urllib.error.HTTPError as e:
            resp_headers = dict(e.headers)
            set_cookie = resp_headers.get("Set-Cookie") or resp_headers.get("set-cookie")
            if set_cookie:
                parts = set_cookie.split(";")
                if parts and "=" in parts[0]:
                    c_name, c_val = parts[0].split("=", 1)
                    self.cookies[c_name.strip()] = c_val.strip()
            return ApiResponse(e.code, resp_headers, e.read())
        except Exception as e:
            return ApiResponse(599, {}, str(e).encode("utf-8"))

    def get(self, path: str, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("GET", path, headers=headers)

    def post(self, path: str, json_data: Any = None, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("POST", path, json_data=json_data, headers=headers)

    def put(self, path: str, json_data: Any = None, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("PUT", path, json_data=json_data, headers=headers)

    def delete(self, path: str, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("DELETE", path, headers=headers)

    def head(self, path: str, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("HEAD", path, headers=headers)

    # Auth domain helpers
    def register(self, email: str, password: str = "P@ssword123!", name: str = "Invinite User") -> ApiResponse:
        return self.post("/api/v1/auth/register", {"name": name, "email": email, "password": password})

    def login(self, email: str, password: str = "P@ssword123!") -> ApiResponse:
        return self.post("/api/v1/auth/login", {"email": email, "password": password})

    def logout(self) -> ApiResponse:
        return self.post("/api/v1/auth/logout")

    def me(self) -> ApiResponse:
        return self.get("/api/v1/auth/me")

    # Account helpers
    def list_accounts(self) -> ApiResponse:
        return self.get("/api/v1/accounts")

    def create_account(self, name: str, account_type: str = "checking", initial_balance: int = 0) -> ApiResponse:
        return self.post("/api/v1/accounts", {
            "name": name,
            "account_type": account_type,
            "initial_balance": initial_balance
        })

    def get_account(self, account_id: str) -> ApiResponse:
        return self.get(f"/api/v1/accounts/{account_id}")

    # Category helpers (Stable Schema Vocabulary)
    def list_categories(self) -> ApiResponse:
        return self.get("/api/v1/categories")

    def create_category(self, name: str, category_type: str = "expense", display_name: Optional[str] = None,
                        icon: str = "tag", color: str = "#10b981", metadata: Optional[Dict[str, Any]] = None) -> ApiResponse:
        payload = {
            "name": name,
            "display_name": display_name or name,
            "category_type": category_type,
            "icon": icon,
            "color": color
        }
        if metadata:
            payload["metadata"] = json.dumps(metadata) if isinstance(metadata, dict) else str(metadata)
        return self.post("/api/v1/categories", payload)

    def soft_delete_category(self, category_id: str) -> ApiResponse:
        return self.delete(f"/api/v1/categories/{category_id}")

    # Transaction helpers
    def list_transactions(self, params: Optional[Dict[str, Any]] = None) -> ApiResponse:
        path = "/api/v1/transactions"
        if params:
            query_str = urllib.parse.urlencode(params)
            path = f"{path}?{query_str}"
        return self.get(path)

    def create_transaction(self, account_id: str, amount: int, tx_type: str = "expense",
                           category_id: Optional[str] = None, destination_account_id: Optional[str] = None,
                           note: str = "", date: Optional[str] = None, idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key

        payload = {
            "account_id": account_id,
            "amount": amount,
            "transaction_type": tx_type,
            "note": note,
            "date": date or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        }
        if category_id:
            payload["category_id"] = category_id
        if destination_account_id:
            payload["destination_account_id"] = destination_account_id

        return self.post("/api/v1/transactions", payload, headers=headers)

    def delete_transaction(self, transaction_id: str) -> ApiResponse:
        return self.delete(f"/api/v1/transactions/{transaction_id}")

    # Analytics & reports
    def get_cash_flow(self) -> ApiResponse:
        return self.get("/api/v1/analytics/cash-flow")

    def get_category_analytics(self) -> ApiResponse:
        return self.get("/api/v1/analytics/categories")

    def get_advanced_analytics(self) -> ApiResponse:
        return self.get("/api/v1/analytics/advanced")

    def get_advanced_reports(self) -> ApiResponse:
        return self.get("/api/v1/reports/advanced")

    def list_budgets(self) -> ApiResponse:
        return self.get("/api/v1/budgets")

    # Subscription & Trial & Pricing (v3.1.0: 3-month trial, Rp10k monthly, Rp110k annual)
    def subscription_status(self) -> ApiResponse:
        return self.get("/api/v1/subscription")

    def get_subscription_plans(self) -> ApiResponse:
        return self.get("/api/v1/subscriptions/plans")

    def continue_free(self) -> ApiResponse:
        return self.post("/api/v1/subscriptions/free", {})

    def activate_trial(self) -> ApiResponse:
        return self.post("/api/v1/subscriptions/trial/activate", {})

    def checkout(self, plan: str = "premium_monthly", provider: str = "dana") -> ApiResponse:
        return self.post("/api/v1/subscriptions/checkout", {
            "plan": plan,
            "plan_id": plan,
            "provider": provider
        })

    def get_audit_logs(self) -> ApiResponse:
        return self.get("/api/v1/audit/logs")

    def send_dana_webhook(self, event_id: str, order_id: str, user_id: str, amount: int = 10000,
                           tampered: bool = False, custom_sig: Optional[str] = None) -> ApiResponse:
        timestamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        payload = {
            "event_id": event_id,
            "order_id": order_id,
            "user_id": user_id,
            "amount": amount,
            "status": "SUCCESS",
            "timestamp": timestamp
        }
        raw_body = json.dumps(payload)
        string_to_sign = f"POST:/api/v1/webhooks/dana:{timestamp}:{raw_body}"
        
        if custom_sig:
            sig = custom_sig
        elif tampered:
            sig = "tampered-invalid-rsa-signature-base64"
        else:
            sig = sign_dana_payload(string_to_sign)

        headers = {
            "X-SIGNATURE": sig,
            "X-TIMESTAMP": timestamp,
            "X-PARTNER-ID": "DANA_PARTNER_INVINITE",
            "X-EXTERNAL-ID": event_id
        }
        return self.post("/api/v1/webhooks/dana", payload, headers=headers)

    # Ingestion helpers
    def ingest_notification(self, package_name: str, title: str, text: str, posted_at: Optional[int] = None, idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload = {
            "package_name": package_name,
            "title": title,
            "text": text,
            "posted_at": posted_at or int(time.time() * 1000)
        }
        return self.post("/api/v1/ingestion/notification", payload, headers=headers)

    def ingest_sms(self, sender: str, text: str, timestamp: Optional[int] = None) -> ApiResponse:
        return self.post("/api/v1/ingestion/sms", {
            "sender": sender,
            "text": text,
            "timestamp": timestamp or int(time.time() * 1000)
        })

    def ingest_gmail(self, message_id: str, subject: str, snippet: str, date: Optional[str] = None) -> ApiResponse:
        return self.post("/api/v1/ingestion/gmail", {
            "message_id": message_id,
            "subject": subject,
            "snippet": snippet,
            "date": date or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        })

    def list_ingestion_candidates(self) -> ApiResponse:
        return self.get("/api/v1/ingestion/candidates")

    def confirm_candidate(self, candidate_id: str, account_id: str, category_id: Optional[str] = None) -> ApiResponse:
        return self.post(f"/api/v1/ingestion/candidates/{candidate_id}/confirm", {
            "account_id": account_id,
            "category_id": category_id
        })

    def reject_candidate(self, candidate_id: str) -> ApiResponse:
        return self.post(f"/api/v1/ingestion/candidates/{candidate_id}/reject", {})

    # Progressive Onboarding
    def submit_onboarding(self, display_name: str, financial_goals: List[str],
                          wallets: List[Dict[str, Any]], categories: List[Dict[str, Any]]) -> ApiResponse:
        return self.post("/api/v1/users/onboarding", {
            "display_name": display_name,
            "financial_goals": financial_goals,
            "wallets": wallets,
            "categories": categories
        })

    # Sync
    def delta_sync(self, cursor: int = 0) -> ApiResponse:
        return self.get(f"/api/v1/sync?cursor={cursor}")

    # Health & Ready
    def health(self) -> ApiResponse:
        return self.get("/health")

    def ready(self) -> ApiResponse:
        return self.get("/ready")

    # =========================================================================
    # v4.1 Multi-Tenant Workspace Helpers
    # =========================================================================
    def create_tenant(self, name: str, slug: Optional[str] = None, timezone: Optional[str] = None, currency: Optional[str] = None) -> ApiResponse:
        payload: Dict[str, Any] = {"name": name}
        if slug:
            payload["slug"] = slug
        if timezone:
            payload["timezone"] = timezone
        if currency:
            payload["currency"] = currency
        return self.post("/api/v1/tenants", payload)

    def list_tenants(self) -> ApiResponse:
        return self.get("/api/v1/tenants")

    def get_tenant(self, tenant_id: str) -> ApiResponse:
        return self.get(f"/api/v1/tenants/{tenant_id}")

    def get_tenant_profile(self, tenant_id: str) -> ApiResponse:
        return self.get(f"/api/v1/tenants/{tenant_id}/profile")

    def update_tenant_profile(self, tenant_id: str, profile_data: Dict[str, Any]) -> ApiResponse:
        return self.put(f"/api/v1/tenants/{tenant_id}/profile", profile_data)

    def list_tenant_members(self, tenant_id: str) -> ApiResponse:
        return self.get(f"/api/v1/tenants/{tenant_id}/members")

    def invite_tenant_member(self, tenant_id: str, email: str, role: str = "member") -> ApiResponse:
        return self.post(f"/api/v1/tenants/{tenant_id}/members", {"email": email, "role": role})

    def remove_tenant_member(self, tenant_id: str, user_id: str) -> ApiResponse:
        return self.delete(f"/api/v1/tenants/{tenant_id}/members/{user_id}")

    def switch_tenant(self, tenant_id: str) -> ApiResponse:
        return self.post(f"/api/v1/tenants/{tenant_id}/switch", {})

    def get_tenant_capabilities(self, tenant_id: str) -> ApiResponse:
        return self.get(f"/api/v1/tenants/{tenant_id}/capabilities")

    # =========================================================================
    # v4.1 Double-Entry Accounting Helpers
    # =========================================================================
    def list_chart_of_accounts(self) -> ApiResponse:
        return self.get("/api/v1/accounting/accounts")

    def create_account_coa(self, code: str, name: str, account_type: str) -> ApiResponse:
        return self.post("/api/v1/accounting/accounts", {
            "code": code,
            "name": name,
            "account_type": account_type
        })

    def delete_account_coa(self, code: str) -> ApiResponse:
        return self.delete(f"/api/v1/accounting/accounts/{code}")

    def post_journal(self, entry_date: str, description: str, lines: List[Dict[str, Any]],
                     source_type: str = "MANUAL", source_id: Optional[str] = None) -> ApiResponse:
        payload = {
            "entry_date": entry_date,
            "description": description,
            "lines": lines,
            "source_type": source_type,
            "source_id": source_id
        }
        return self.post("/api/v1/accounting/journals", payload)

    def list_journals(self) -> ApiResponse:
        return self.get("/api/v1/accounting/journals")

    def get_journal(self, journal_id: str) -> ApiResponse:
        return self.get(f"/api/v1/accounting/journals/{journal_id}")

    def update_journal(self, journal_id: str, data: Dict[str, Any]) -> ApiResponse:
        return self.put(f"/api/v1/accounting/journals/{journal_id}", data)

    def delete_journal(self, journal_id: str) -> ApiResponse:
        return self.delete(f"/api/v1/accounting/journals/{journal_id}")

    def reverse_journal(self, journal_id: str, reason: str = "Reversal") -> ApiResponse:
        return self.post(f"/api/v1/accounting/journals/{journal_id}/reverse", {"reason": reason})

    def get_trial_balance(self) -> ApiResponse:
        return self.get("/api/v1/accounting/trial-balance")

    def calculate_tax(self, amount: int, tax_type: str, is_inclusive: bool = False) -> ApiResponse:
        return self.post("/api/v1/accounting/tax/calculate", {
            "amount": amount,
            "tax_type": tax_type,
            "is_inclusive": is_inclusive
        })

    # =========================================================================
    # v4.1 Commercial Invoicing Helpers
    # =========================================================================
    def create_invoice(self, customer_name: str, items: List[Dict[str, Any]],
                       due_date: Optional[str] = None, currency: str = "IDR", tax_type: str = "PPN_11_EXCL",
                       customer_address: str = "", customer_email: str = "") -> ApiResponse:
        payload = {
            "customer_name": customer_name,
            "customer_address": customer_address,
            "customer_email": customer_email,
            "items": items,
            "due_date": due_date or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "currency": currency,
            "tax_type": tax_type
        }
        return self.post("/api/v1/invoices", payload)

    def list_invoices(self) -> ApiResponse:
        return self.get("/api/v1/invoices")

    def get_invoice(self, invoice_id: str) -> ApiResponse:
        return self.get(f"/api/v1/invoices/{invoice_id}")

    def update_invoice(self, invoice_id: str, data: Optional[Dict[str, Any]] = None, **kwargs) -> ApiResponse:
        payload = dict(data or {})
        payload.update(kwargs)
        return self.put(f"/api/v1/invoices/{invoice_id}", payload)

    def issue_invoice(self, invoice_id: str, idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        return self.post(f"/api/v1/invoices/{invoice_id}/issue", {}, headers=headers)

    def void_invoice(self, invoice_id: str, reason: str = "Voided") -> ApiResponse:
        return self.post(f"/api/v1/invoices/{invoice_id}/void", {"reason": reason})

    # =========================================================================
    # v4.1 Receivables & Payments Helpers
    # =========================================================================
    def list_receivables(self) -> ApiResponse:
        return self.get("/api/v1/receivables")

    def get_receivable_aging(self) -> ApiResponse:
        return self.get("/api/v1/receivables/aging")

    def allocate_payment(self, invoice_id: str, amount: int, payment_method: str = "BANK_TRANSFER",
                         payment_date: Optional[str] = None, reference: Optional[str] = None,
                         idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload: Dict[str, Any] = {
            "invoice_id": invoice_id,
            "amount": amount,
            "payment_method": payment_method,
        }
        if payment_date is not None:
            payload["payment_date"] = payment_date
        if reference is not None:
            payload["reference"] = reference
        return self.post("/api/v1/payments", payload, headers=headers)

    # =========================================================================
    # v4.1 Transactional Outbox Helpers
    # =========================================================================
    def list_outbox_events(self) -> ApiResponse:
        return self.get("/api/v1/outbox/events")

    def process_outbox(self, simulate_sidecar_failure: bool = False) -> ApiResponse:
        return self.post("/api/v1/outbox/process", {"simulate_sidecar_failure": simulate_sidecar_failure})

    # =========================================================================
    # v4.1 Phase 2: Inventory & Multi-Location Stock Management Helpers
    # =========================================================================
    def create_warehouse(self, code: str, name: str, address: Optional[str] = None, is_default: bool = False) -> ApiResponse:
        payload = {
            "code": code,
            "name": name,
            "is_default": is_default
        }
        if address is not None:
            payload["address"] = address
        return self.post("/api/v1/warehouses", payload)

    def list_warehouses(self) -> ApiResponse:
        return self.get("/api/v1/warehouses")

    def get_warehouse(self, warehouse_id: str) -> ApiResponse:
        return self.get(f"/api/v1/warehouses/{warehouse_id}")

    def create_product(self, name: str, sku: Optional[str] = None, unit: str = "pcs",
                       cost_price: int = 0, sale_price: int = 0, reorder_threshold: int = 0) -> ApiResponse:
        payload: Dict[str, Any] = {
            "name": name,
            "unit": unit,
            "cost_price": cost_price,
            "sale_price": sale_price,
            "reorder_threshold": reorder_threshold
        }
        if sku is not None:
            payload["sku"] = sku
        return self.post("/api/v1/products", payload)

    def list_products(self) -> ApiResponse:
        return self.get("/api/v1/products")

    def get_product(self, product_id: str) -> ApiResponse:
        return self.get(f"/api/v1/products/{product_id}")

    def get_stock_items(self, warehouse_id: Optional[str] = None, product_id: Optional[str] = None,
                        low_stock: Optional[bool] = None) -> ApiResponse:
        params = []
        if warehouse_id:
            params.append(f"warehouse_id={urllib.parse.quote(warehouse_id)}")
        if product_id:
            params.append(f"product_id={urllib.parse.quote(product_id)}")
        if low_stock is not None:
            params.append(f"low_stock={'true' if low_stock else 'false'}")
        query = f"?{'&'.join(params)}" if params else ""
        return self.get(f"/api/v1/inventory{query}")

    def create_stock_movement(self, movement_type: str, product_id: str, quantity: int,
                              source_warehouse_id: Optional[str] = None,
                              destination_warehouse_id: Optional[str] = None,
                              unit_cost: Optional[int] = None,
                              notes: Optional[str] = None,
                              idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload: Dict[str, Any] = {
            "movement_type": movement_type,
            "product_id": product_id,
            "quantity": quantity
        }
        if source_warehouse_id is not None:
            payload["source_warehouse_id"] = source_warehouse_id
        if destination_warehouse_id is not None:
            payload["destination_warehouse_id"] = destination_warehouse_id
        if unit_cost is not None:
            payload["unit_cost"] = unit_cost
        if notes is not None:
            payload["notes"] = notes
        return self.post("/api/v1/inventory/movements", payload, headers=headers)

    def adjust_stock(self, warehouse_id: str, product_id: str, actual_quantity: int,
                     reason: Optional[str] = None, idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload: Dict[str, Any] = {
            "warehouse_id": warehouse_id,
            "product_id": product_id,
            "actual_quantity": actual_quantity
        }
        if reason is not None:
            payload["reason"] = reason
        return self.post("/api/v1/inventory/adjust", payload, headers=headers)

    def transfer_stock(self, source_warehouse_id: str, destination_warehouse_id: str,
                       product_id: str, quantity: int, notes: Optional[str] = None,
                       idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload: Dict[str, Any] = {
            "source_warehouse_id": source_warehouse_id,
            "destination_warehouse_id": destination_warehouse_id,
            "product_id": product_id,
            "quantity": quantity
        }
        if notes is not None:
            payload["notes"] = notes
        return self.post("/api/v1/inventory/transfer", payload, headers=headers)

    def create_purchase_order(self, supplier_name: str, destination_warehouse_id: str,
                              items: List[Dict[str, Any]], notes: Optional[str] = None,
                              idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload: Dict[str, Any] = {
            "supplier_name": supplier_name,
            "destination_warehouse_id": destination_warehouse_id,
            "items": items
        }
        if notes is not None:
            payload["notes"] = notes
        return self.post("/api/v1/purchase-orders", payload, headers=headers)

    def list_purchase_orders(self) -> ApiResponse:
        return self.get("/api/v1/purchase-orders")

    def get_purchase_order(self, po_id: str) -> ApiResponse:
        return self.get(f"/api/v1/purchase-orders/{po_id}")

    def order_purchase_order(self, po_id: str) -> ApiResponse:
        return self.post(f"/api/v1/purchase-orders/{po_id}/order", {})

    def receive_purchase_order(self, po_id: str, items: List[Dict[str, Any]],
                               idempotency_key: Optional[str] = None) -> ApiResponse:
        headers = {}
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        payload = {"items": items}
        return self.post(f"/api/v1/purchase-orders/{po_id}/receive", payload, headers=headers)

    def cancel_purchase_order(self, po_id: str, reason: Optional[str] = None) -> ApiResponse:
        payload = {}
        if reason is not None:
            payload["reason"] = reason
        return self.post(f"/api/v1/purchase-orders/{po_id}/cancel", payload)


    # =========================================================================
    # v4.1 System & Schema Probes
    # =========================================================================
    def get_system_schema(self) -> ApiResponse:
        return self.get("/api/v1/system/schema")

    def check_client_isolation(self, non_existent_id: str) -> ApiResponse:
        return self.get(f"/api/v1/system/client-check?probe_id={non_existent_id}")


class TapReporter:
    def __init__(self, total_expected: Optional[int] = None):
        self.total_expected = total_expected
        self.results = []
        self.start_time = time.time()
        self.current_idx = 0

    def record(self, passed: bool, description: str, diagnostic: Optional[str] = None):
        self.current_idx += 1
        idx = self.current_idx
        status_str = "ok" if passed else "not ok"
        line = f"{status_str} {idx} - {description}"
        print(line, flush=True)
        if not passed and diagnostic:
            print(f"  ---", flush=True)
            print(f"  message: {diagnostic}", flush=True)
            print(f"  ...", flush=True)
        self.results.append({
            "index": idx,
            "passed": passed,
            "description": description,
            "diagnostic": diagnostic
        })

    def print_header(self):
        print("TAP version 13", flush=True)
        if self.total_expected is not None:
            print(f"1..{self.total_expected}", flush=True)

    def print_summary(self) -> bool:
        duration = time.time() - self.start_time
        total = len(self.results)
        passed = sum(1 for r in self.results if r["passed"])
        failed = sum(1 for r in self.results if not r["passed"])

        print(f"# Summary: {total} tests run, {passed} passed, {failed} failed in {duration:.3f}s", flush=True)
        return failed == 0


# Assertion helpers
def assert_true(cond: bool, msg: str = "Condition not true"):
    if not cond:
        raise AssertionError(msg)

def assert_equal(actual: Any, expected: Any, msg: str = ""):
    if actual != expected:
        raise AssertionError(f"{msg}: expected '{expected}', got '{actual}'")

def assert_status(resp: ApiResponse, expected_status: int, msg: str = ""):
    if resp.status != expected_status:
        body_preview = resp.text[:250]
        raise AssertionError(f"{msg} Expected HTTP {expected_status}, got {resp.status}. Body: {body_preview}")

def assert_rfc7807(resp: ApiResponse, expected_status: int, expected_code: Optional[str] = None):
    assert_status(resp, expected_status, "RFC 7807 status check:")
    data = resp.json
    assert_true(isinstance(data, dict), "Response must be a JSON object")
    assert_true("type" in data, "RFC 7807 response must contain 'type'")
    assert_true("title" in data, "RFC 7807 response must contain 'title'")
    assert_true("status" in data, "RFC 7807 response must contain 'status'")
    assert_equal(data["status"], expected_status, "Status field in RFC 7807 body must match HTTP status")
    if expected_code:
        assert_equal(data.get("code"), expected_code, f"Error code must match '{expected_code}'")
