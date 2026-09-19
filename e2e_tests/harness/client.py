#!/usr/bin/env python3
"""
Test Client & TAP v13 Framework for Invinite E2E Tests.
Independent opaque-box HTTP client interacting with live or mock server instance.
"""

import sys
import os
import json
import time
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
        self.default_headers = {
            "User-Agent": "Invinite-E2E-Tester/3.1.0",
            "Accept": "application/json"
        }
        if client_ip:
            self.default_headers["X-Forwarded-For"] = client_ip

    def set_cookie(self, name: str, value: str):
        self.cookies[name] = value

    def clear_cookies(self):
        self.cookies.clear()

    def _build_request(self, method: str, path: str, data: Optional[bytes] = None, headers: Optional[Dict[str, str]] = None) -> urllib.request.Request:
        url = f"{self.base_url}{path}" if path.startswith("/") else f"{self.base_url}/{path}"
        req = urllib.request.Request(url, data=data, method=method)

        for k, v in self.default_headers.items():
            req.add_header(k, v)

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
