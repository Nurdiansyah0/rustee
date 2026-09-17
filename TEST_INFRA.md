# E2E Test Infrastructure: Invinite Personal Finance Intelligence Platform

**Authoritative Specification:** `Personal_Finance_Master_Specification_v3.1.0.md`  
**Feature Inventory Source:** `PROJECT.md` § Feature Inventory  
**Target Infrastructure:** `https://api.nurdiansyahlabs.com` / Local Port `8080` (or embedded reference harness on `8089`)  
**Status:** Authoritative Test Infrastructure Specification  
**Version:** 3.1.0  

---

## 1. Test Philosophy

The Invinite acceptance test suite is designed on the following foundational principles:

1. **Strictly Opaque-Box & Requirement-Driven**:
   - The test suite treats the application under test (AUT) as a black box.
   - Tests have **zero dependency on internal implementation details**, private Rust structs, internal crate layouts, or direct manipulation of the SQLite database file during execution.
   - All interactions and validations occur through public, observable contracts: HTTP REST APIs (`/api/v1/*`), HTTP status codes, RFC 7807 Problem Details responses, HTTP headers (`Idempotency-Key`, `Cache-Control`, `Cookie`, `X-Signature`), WebSocket endpoints (`/api/v1/ws`), and PWA web assets.

2. **Authoritative Expected Output Derivation**:
   - For every test assertion, the expected value is derived directly from the authoritative specifications in `Personal_Finance_Master_Specification_v3.1.0.md` and `PROJECT.md`.
   - When running against the embedded reference program (oracle in `e2e_tests/harness/server.py`), the test framework verifies that the live implementation matches the reference oracle's behavior across all happy-path, boundary, pairwise, and end-to-end user journeys.

3. **No Facade or Dummy Testing**:
   - Every test executes genuine network requests and asserts on exact HTTP status codes, response headers, and validated JSON payloads.
   - No mock shortcuts, hardcoded passes, or superficial tests that do not exercise actual server logic.

4. **Progressive Testability & System Isolation**:
   - Tests are independent and idempotent. Each test generates unique user credentials and UUIDs to prevent cross-test interference.
   - Tests cleanly tear down or isolate their state.

---

## 2. Methodology

The test suite employs four complementary testing methodologies:

### 2.1 Category-Partition Method (Tier 1)
Systematically partitions the input domain and application state for each of the 55 features into valid equivalence classes:
- **Partitioning Dimensions**: Authentication status (Unauthenticated, Free User, Trialing User, Active Premium User), Account Types (`checking`, `savings`, `credit`, `e_wallet`, `cash`), Transaction Directions (`income`, `expense`, `transfer`), Ingestion Channels (Notification, SMS, Gmail), Confidence Levels (`HIGH`, `MEDIUM`, `LOW`), Subscription Plans (`premium_monthly`, `premium_annual`).
- **Happy-Path Verification**: At least 5 distinct, well-defined test cases per feature covering primary functional requirements in isolation.

### 2.2 Boundary Value Analysis & Corner Cases (Tier 2)
Focuses on input boundaries, extreme conditions, and error-handling defenses:
- **Numerical Boundaries**: Zero currency amounts (`amount = 0`), negative values (`amount < 0`), maximum 64-bit integer values (`i64::MAX = 9,223,372,036,854,775,807`), fractional/floating amounts.
- **Security Boundaries**: Malformed JWT cookies, expired tokens, forged/tampered cryptographic signatures (RSA-SHA256), brute-force rate-limiting triggers (>5 attempts / 15 min), SQL injection payloads, cross-tenant resource tampering.
- **State Boundaries**: Activating 3-month trial more than once, double-spending or duplicate idempotency keys, replayed DANA webhook event IDs, expired trial feature access.

### 2.3 Pairwise Feature Combinations (Tier 3)
Tests interactions between orthogonal features that intersect in business workflows:
- Trial activation combined with real-time server-side feature gating.
- Multi-wallet transfers interacting with the centralized Net Cash Flow calculation engine and category aggregations.
- `Idempotency-Key` replay interacting with brute-force rate limiting and SQLite WAL transaction concurrency.
- DANA webhook callbacks interacting with subscription state transitions and audit logging.
- Ingestion pipeline deduplication interacting with delta sync cursor updates.

### 2.4 Real-World Workload Testing (Tier 4)
Executes end-to-end, multi-step customer journeys mirroring production user behavior:
- **Workflow A**: New user registration → Progressive personalization onboarding (vocabulary, wallets, goals) → 3-Month Premium Trial activation → POS keypad transaction entry → Multi-wallet transfer → Net cash flow verification → Historical category soft-deletion → Monthly report generation.
- **Workflow B**: Ingestion pipeline intake (Notification/SMS/Gmail) → Confidence classification → Duplicate suppression → One-tap candidate confirmation → Ledger settlement → WebSocket broadcast → Cursor delta sync reconciliation.
- **Workflow C**: Free tier feature lock discovery → Checkout order creation (Monthly/Annual) → DANA SNAP payment webhook delivery → RSA-SHA256 signature verification → Entitlement activation → Audit trail validation.

---

## 3. Feature Inventory & Coverage Targets (All 55 Features)

Every feature from `PROJECT.md` § Feature Inventory is mapped with exact coverage targets:

| # | Req ID | Track | Module | Feature Name | Spec Ref | Tier 1 (Coverage ≥5) | Tier 2 (Boundary ≥5) | Tier 3 (Pairwise) | Tier 4 (Scenario) |
|---|--------|-------|--------|--------------|----------|:--------------------:|:--------------------:|:-----------------:|:-----------------:|
| 1 | `REQ-ARCH-01` | ARCH-LEDGER | Architecture | Modular Monolith Layering | §11-13 | 5 | 5 | ✓ | ✓ |
| 2 | `REQ-ARCH-02` | ARCH-LEDGER | Domain | Integer Rupiah Currency Math | §14-15 | 5 | 5 | ✓ | ✓ |
| 3 | `REQ-ARCH-03` | ARCH-LEDGER | Domain | Single Net Cash Flow Engine | §15 | 5 | 5 | ✓ | ✓ |
| 4 | `REQ-ARCH-04` | ARCH-LEDGER | Domain | Multi-Wallet Accounting | §14-15 | 5 | 5 | ✓ | ✓ |
| 5 | `REQ-ARCH-05` | ARCH-LEDGER | Domain | Atomic Balance Mutations | §15 | 5 | 5 | ✓ | ✓ |
| 6 | `REQ-ARCH-06` | ARCH-LEDGER | Domain | Category Soft-Deletion | §14 | 5 | 5 | ✓ | ✓ |
| 7 | `REQ-ARCH-07` | ARCH-LEDGER | Persistence | SQLite WAL & Pragmas | §16 | 5 | 5 | ✓ | ✓ |
| 8 | `REQ-ARCH-08` | ARCH-LEDGER | Persistence | Canonical Composite Indexes | §16 | 5 | 5 | ✓ | ✓ |
| 9 | `REQ-ARCH-09` | ARCH-LEDGER | Personalization | Stable Schema Vocabulary | §4-6 | 5 | 5 | ✓ | ✓ |
| 10 | `REQ-ARCH-10` | ARCH-LEDGER | Persistence | Strict UTC Timestamps | §10 | 5 | 5 | ✓ | ✓ |
| 11 | `REQ-ARCH-11` | ARCH-LEDGER | Core | Idempotency Engine | §18 | 5 | 5 | ✓ | ✓ |
| 12 | `REQ-ARCH-12` | ARCH-LEDGER | Core | Multi-Tenant Data Isolation | §17 | 5 | 5 | ✓ | ✓ |
| 13 | `REQ-SEC-01` | SEC-PAY | Auth | Argon2id Password Hashing | §17 | 5 | 5 | ✓ | ✓ |
| 14 | `REQ-SEC-02` | SEC-PAY | Auth | Cookie Session Management | §17 | 5 | 5 | ✓ | ✓ |
| 15 | `REQ-SEC-03` | SEC-PAY | Auth | Rate Limiting | §17 | 5 | 5 | ✓ | ✓ |
| 16 | `REQ-SEC-04` | SEC-PAY | Subscriptions | 3-Month Premium Trial | §8 | 5 | 5 | ✓ | ✓ |
| 17 | `REQ-SEC-05` | SEC-PAY | Subscriptions | Subscription Lifecycle Machine | §8, §19 | 5 | 5 | ✓ | ✓ |
| 18 | `REQ-SEC-06` | SEC-PAY | Subscriptions | Commercial Pricing Plans | §7 | 5 | 5 | ✓ | ✓ |
| 19 | `REQ-SEC-07` | SEC-PAY | Payment | DANA Open API Integration | §19 | 5 | 5 | ✓ | ✓ |
| 20 | `REQ-SEC-08` | SEC-PAY | Payment | RSA-SHA256 Signature Verification | §19 | 5 | 5 | ✓ | ✓ |
| 21 | `REQ-SEC-09` | SEC-PAY | Payment | Idempotent Webhook Processing | §19 | 5 | 5 | ✓ | ✓ |
| 22 | `REQ-SEC-10` | SEC-PAY | Entitlement | Server-Side Feature Gating | §9, §19 | 5 | 5 | ✓ | ✓ |
| 23 | `REQ-SEC-11` | SEC-PAY | Security | Financial Response Cache Control | §33 | 5 | 5 | ✓ | ✓ |
| 24 | `REQ-SEC-12` | SEC-PAY | Audit | Immutable Audit Log | §34 | 5 | 5 | ✓ | ✓ |
| 25 | `REQ-INGEST-01` | INGEST | Pipeline | Ingestion Pipeline Architecture | §20, §24 | 5 | 5 | ✓ | ✓ |
| 26 | `REQ-INGEST-02` | INGEST | Pipeline | Canonical Ingestion Representation | §24 | 5 | 5 | ✓ | ✓ |
| 27 | `REQ-INGEST-03` | INGEST | Pipeline | Confidence Threshold Engine | §24 | 5 | 5 | ✓ | ✓ |
| 28 | `REQ-INGEST-04` | INGEST | Pipeline | Cross-Source Deduplication | §24 | 5 | 5 | ✓ | ✓ |
| 29 | `REQ-INGEST-05` | INGEST | Notification | Android Notification Adapter | §21 | 5 | 5 | ✓ | ✓ |
| 30 | `REQ-INGEST-06` | INGEST | SMS | SMS Capability Adapter | §22 | 5 | 5 | ✓ | ✓ |
| 31 | `REQ-INGEST-07` | INGEST | Gmail | Targeted Gmail Ingestion | §23 | 5 | 5 | ✓ | ✓ |
| 32 | `REQ-INGEST-08` | INGEST | Privacy | Payload Minimization | §23, §33 | 5 | 5 | ✓ | ✓ |
| 33 | `REQ-AND-01` | ANDROID | Container | Kotlin Native Shell & WebView | §28 | 5 | 5 | ✓ | ✓ |
| 34 | `REQ-AND-02` | ANDROID | Service | NotificationListenerService Integration | §21, §28 | 5 | 5 | ✓ | ✓ |
| 35 | `REQ-AND-03` | ANDROID | Bridge | Versioned JS Capability Bridge | §29 | 5 | 5 | ✓ | ✓ |
| 36 | `REQ-AND-04` | ANDROID | Bridge | WebView Origin Validation | §29 | 5 | 5 | ✓ | ✓ |
| 37 | `REQ-AND-05` | ANDROID | Sync | OS Background Sync Coordination | §25, §28 | 5 | 5 | ✓ | ✓ |
| 38 | `REQ-AND-06` | ANDROID | UX | First-Launch Subscription Layer | §8, §28 | 5 | 5 | ✓ | ✓ |
| 39 | `REQ-AND-07` | ANDROID | Storage | Secure Native Storage | §28, §33 | 5 | 5 | ✓ | ✓ |
| 40 | `REQ-FE-01` | FRONTEND | PWA | Vue 3 Mobile-First PWA Shell | §12, §30 | 5 | 5 | ✓ | ✓ |
| 41 | `REQ-FE-02` | FRONTEND | Navigation | 5-Tab Ergonomic Navigation | §30, §31 | 5 | 5 | ✓ | ✓ |
| 42 | `REQ-FE-03` | FRONTEND | Transactions | Rapid 4x3 POS Keypad | §30 | 5 | 5 | ✓ | ✓ |
| 43 | `REQ-FE-04` | FRONTEND | Personalization | Progressive Onboarding Flow | §3-6 | 5 | 5 | ✓ | ✓ |
| 44 | `REQ-FE-05` | FRONTEND | Currency | Localized IDR Formatting | §15, §30 | 5 | 5 | ✓ | ✓ |
| 45 | `REQ-FE-06` | FRONTEND | Timezone | Financial Date/Time Policy | §10 | 5 | 5 | ✓ | ✓ |
| 46 | `REQ-FE-07` | FRONTEND | Realtime | Foreground WebSocket Client | §25, §27 | 5 | 5 | ✓ | ✓ |
| 47 | `REQ-FE-08` | FRONTEND | Sync | Cursor Delta Sync | §25, §26 | 5 | 5 | ✓ | ✓ |
| 48 | `REQ-FE-09` | FRONTEND | UI/UX | Feature Lock Overlay & Upgrade Modal | §9, §30 | 5 | 5 | ✓ | ✓ |
| 49 | `REQ-FE-10` | FRONTEND | Design | Fintech Visual Design System | §31 | 5 | 5 | ✓ | ✓ |
| 50 | `REQ-FE-11` | FRONTEND | Accessibility | WCAG 2.1 AA Compliance | §32 | 5 | 5 | ✓ | ✓ |
| 51 | `REQ-QA-01` | QA | Backend | Automated Cargo Test Suite | §36, §37 | 5 | 5 | ✓ | ✓ |
| 52 | `REQ-QA-02` | QA | Frontend | Production Build Validation | §37 | 5 | 5 | ✓ | ✓ |
| 53 | `REQ-QA-03` | QA | E2E | E2E Acceptance Test Runner | §37 | 5 | 5 | ✓ | ✓ |
| 54 | `REQ-QA-04` | QA | Health | Operational Health & Readiness | §37 | 5 | 5 | ✓ | ✓ |
| 55 | `REQ-QA-05` | QA | Security | Adversarial Security Testing | §36 | 5 | 5 | ✓ | ✓ |

### Summary Test Targets
- **Tier 1 (Feature Coverage)**: $55 \times 5 = \mathbf{275\text{ tests}}$
- **Tier 2 (Boundary & Corner Cases)**: $55 \times 5 = \mathbf{275\text{ tests}}$
- **Tier 3 (Cross-Feature Combinations)**: $\mathbf{55\text{ tests}}$
- **Tier 4 (Real-World Scenarios)**: $\mathbf{20\text{ realistic scenarios}}$
- **Total Suite Target**: $\mathbf{625\text{ test assertions}}$

---

## 4. Test Architecture & Directory Layout

The acceptance testing harness is located entirely under `e2e_tests/`:

```
/home/nurdiansyah/teamwork_projects/personal_finance_pwa/
├── TEST_INFRA.md                       # This infrastructure & methodology specification
├── TEST_READY.md                       # Test readiness report & execution commands
└── e2e_tests/
    ├── runner.sh                       # Master bash runner supporting all, tier1, tier2, tier3, tier4
    ├── tier1_feature_coverage.sh       # Tier 1 runner (≥275 tests covering all 55 features)
    ├── tier2_boundary_corner.sh        # Tier 2 runner (≥275 boundary tests covering all 55 features)
    ├── tier3_cross_feature.sh          # Tier 3 runner (Pairwise interactions)
    ├── tier4_real_world.sh             # Tier 4 runner (Realistic complete user journeys)
    ├── harness/
    │   ├── client.py                   # Opaque-box HTTP client & TAP v13 test framework
    │   ├── server.py                   # High-fidelity reference mock server (oracle) for v3.1.0
    │   └── crypto_keys.py              # DANA SNAP test RSA keys & cryptographic signatures
    ├── tier1/                          # Feature coverage test implementation
    │   └── test_tier1.py
    ├── tier2/                          # Boundary value test implementation
    │   └── test_tier2.py
    ├── tier3/                          # Cross-feature pairwise test implementation
    │   └── test_tier3.py
    └── tier4/                          # Real-world scenario test implementation
        └── test_tier4.py
```

### 4.1 Server Detection & Reference Oracle Lifecycle
1. The test runner checks if a server is reachable on `http://127.0.0.1:8080/health` (live Rust Axum backend).
2. If unreachable, it checks `http://127.0.0.1:8089/health` (existing reference server).
3. If neither is reachable and `BASE_URL` is not specified, `runner.sh` automatically launches the embedded reference server (`e2e_tests/harness/server.py`) on port 8089 in the background, waits for readiness, runs the test suite, and traps signals to cleanly terminate the server on exit.

---

## 5. Execution Instructions

All test scripts are fully executable (`chmod +x`).

### 5.1 Run Full Acceptance Suite (All 4 Tiers)
```bash
bash e2e_tests/runner.sh all
```
or simply:
```bash
bash e2e_tests/runner.sh
```

### 5.2 Run Individual Tiers
```bash
# Tier 1: Feature Coverage (275 tests across 55 features)
bash e2e_tests/tier1_feature_coverage.sh
# or:
bash e2e_tests/runner.sh tier1

# Tier 2: Boundary & Corner Cases (275 tests across 55 features)
bash e2e_tests/tier2_boundary_corner.sh
# or:
bash e2e_tests/runner.sh tier2

# Tier 3: Pairwise Cross-Feature Interactions (55 tests)
bash e2e_tests/tier3_cross_feature.sh
# or:
bash e2e_tests/runner.sh tier3

# Tier 4: Real-World Application Scenarios (20 scenarios)
bash e2e_tests/tier4_real_world.sh
# or:
bash e2e_tests/runner.sh tier4
```

### 5.3 Target Live Server or Custom Environment
```bash
BASE_URL="https://api.nurdiansyahlabs.com" bash e2e_tests/runner.sh all
# or
bash e2e_tests/runner.sh all http://127.0.0.1:8080
```

---

## 6. Pass/Fail Semantics & Output Reporting

1. **Protocol Standard**: The suite outputs continuous **TAP (Test Anything Protocol) version 13**:
   - `TAP version 13`
   - Plan line: `1..625`
   - Individual test results: `ok <N> - <Description>` or `not ok <N> - <Description>`
   - Diagnostic output prefixed by `#`
2. **Exit Code**:
   - `0`: Exactly 100% of test assertions passed with zero failures.
   - `1` (or non-zero): One or more assertions failed, server was unreachable, or runtime error occurred.
3. **Audit Log & Traceability**: Each test logs its HTTP method, request path, status code, and assertion detail to stdout/stderr.
