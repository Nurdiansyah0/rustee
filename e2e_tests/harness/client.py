#!/usr/bin/env python3
"""
Test Client & TAP Test Framework for E2E Tests.
Independent opaque-box HTTP client interacting with any live or mock server instance.
"""

import sys
import os
import json
import time
import urllib.request
import urllib.error
import urllib.parse
import http.cookiejar
import hmac
import hashlib
from typing import Dict, Any, Optional, Tuple

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
            "User-Agent": "PWA-E2E-Tester/1.0",
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
            with urllib.request.urlopen(req, timeout=10.0) as resp:
                status = resp.status
                resp_headers = dict(resp.headers)
                resp_body = resp.read()

                # Handle set-cookie
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

    def delete(self, path: str, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("DELETE", path, headers=headers)

    def head(self, path: str, headers: Optional[Dict[str, str]] = None) -> ApiResponse:
        return self.request("HEAD", path, headers=headers)

    # Convenience domain helpers
    def register(self, email: str, password: str = "P@ssword123!", name: str = "Test User") -> ApiResponse:
        return self.post("/api/v1/auth/register", {"name": name, "email": email, "password": password})

    def login(self, email: str, password: str = "P@ssword123!") -> ApiResponse:
        return self.post("/api/v1/auth/login", {"email": email, "password": password})

    def logout(self) -> ApiResponse:
        return self.post("/api/v1/auth/logout")

    def me(self) -> ApiResponse:
        return self.get("/api/v1/auth/me")

    def list_accounts(self) -> ApiResponse:
        return self.get("/api/v1/accounts")

    def create_account(self, name: str, account_type: str = "checking", initial_balance: int = 0) -> ApiResponse:
        return self.post("/api/v1/accounts", {
            "name": name,
            "account_type": account_type,
            "initial_balance": initial_balance
        })

    def list_categories(self) -> ApiResponse:
        return self.get("/api/v1/categories")

    def create_category(self, name: str, category_type: str = "expense", icon: str = "tag", color: str = "#000") -> ApiResponse:
        return self.post("/api/v1/categories", {
            "name": name,
            "category_type": category_type,
            "icon": icon,
            "color": color
        })

    def soft_delete_category(self, category_id: str) -> ApiResponse:
        return self.delete(f"/api/v1/categories/{category_id}")

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
            "date": date or time.strftime("%Y-%m-%d")
        }
        if category_id:
            payload["category_id"] = category_id
        if destination_account_id:
            payload["destination_account_id"] = destination_account_id

        return self.post("/api/v1/transactions", payload, headers=headers)

    def get_cash_flow(self) -> ApiResponse:
        return self.get("/api/v1/analytics/cash-flow")

    def get_advanced_analytics(self) -> ApiResponse:
        return self.get("/api/v1/analytics/advanced")

    def list_budgets(self) -> ApiResponse:
        return self.get("/api/v1/budgets")

    def subscription_status(self) -> ApiResponse:
        return self.get("/api/v1/subscriptions/status")

    def checkout(self, provider: str = "midtrans") -> ApiResponse:
        return self.post("/api/v1/subscriptions/checkout", {"provider": provider})


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

# Test assertion helpers
def assert_true(cond: bool, msg: str = "Condition not true"):
    if not cond:
        raise AssertionError(msg)

def assert_equal(actual: Any, expected: Any, msg: str = ""):
    if actual != expected:
        raise AssertionError(f"{msg}: expected '{expected}', got '{actual}'")

def assert_status(resp: ApiResponse, expected_status: int, msg: str = ""):
    if resp.status != expected_status:
        body_preview = resp.text[:200]
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
        assert_equal(data.get("code"), expected_code, "Error code must match")

def compute_midtrans_signature(order_id: str, status_code: str, gross_amount: str, server_key: str) -> str:
    raw = f"{order_id}{status_code}{gross_amount}{server_key}".encode("utf-8")
    return hashlib.sha512(raw).hexdigest()

def compute_xendit_signature(body: bytes, secret: str) -> str:
    return hmac.new(secret.encode("utf-8"), body, hashlib.sha256).hexdigest()
