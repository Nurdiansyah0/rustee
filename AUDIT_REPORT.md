# Master Technical Audit Report: Invinite Personal Finance PWA SaaS Platform

**Target System**: Invinite Personal Finance PWA (`personal_finance_pwa`)  
**Target Ingress Endpoint**: `https://api.nurdiansyahlabs.com`  
**Edge Hardware Node**: Samsung Galaxy A20s (`SM-A207F`, Qualcomm Snapdragon 450, ARMv8-A `aarch64`, 3 GB RAM, Android 11 / Ubuntu RootFS chroot)  
**Target Monorepo Integration Path**: `/home/nurdiansyah/dev/Personal_project` (`nurdiansyahlabs-react`)  
**Audit Date**: 2026-09-13  
**Auditor**: Master Technical Audit Report Writer (Synthesized from Specialist Domain Auditors 1–8)  
**Audit Integrity Mode**: Strictly Read-Only (Non-Modification Constraint)  
**Verified Baselines**:
- Backend Rust Test Suite: `cargo test --workspace` (108 passed; 0 failed)
- E2E Integration Suite: `bash e2e_tests/runner.sh all` (344 passed; 0 failed, TAP-13 compliant)
- Frontend Production Build: `npm run build` (0 errors; exit code 0)
- Edge Server Health Probe: `https://api.nurdiansyahlabs.com/health` (HTTP 200 OK)

---

## Table of Contents
- [A. Executive Summary](#a-executive-summary)
- [B. Current Architecture & System Topology](#b-current-architecture--system-topology)
- [C. Technology Stack Inventory](#c-technology-stack-inventory)
- [D. Feature Status Matrix](#d-feature-status-matrix)
- [E. Security Findings & Vulnerability Matrix](#e-security-findings--vulnerability-matrix)
- [F. Database Findings & Data Integrity](#f-database-findings--data-integrity)
- [G. Backend Findings & API Surface Audit (34 Endpoints)](#g-backend-findings--api-surface-audit-34-endpoints)
- [H. Frontend Findings, State Architecture & UX Gaps](#h-frontend-findings-state-architecture--ux-gaps)
- [I. DevOps, Infrastructure & Edge Deployment Findings](#i-devops-infrastructure--edge-deployment-findings)
- [J. Testing Infrastructure, Maturity & Coverage Gaps](#j-testing-infrastructure-maturity--coverage-gaps)
- [K. Technical Debt Register](#k-technical-debt-register)
- [L. Architectural Risk Register](#l-architectural-risk-register)
- [M. What Should NOT Be Changed (The PRESERVE List)](#m-what-should-not-be-changed-the-preserve-list)
- [N. What Should Be Changed & Target Architecture](#n-what-should-be-changed--target-architecture)
- [O. Implementation Plan (Phased Execution)](#o-implementation-plan-phased-execution)
- [Section 25: Final Audit Decision & System Posture](#section-25-final-audit-decision--system-posture)

---

## A. Executive Summary

A comprehensive, evidence-based technical audit was conducted across the Personal Finance PWA repository (`Invinite`), synthesizing findings from eight specialist domain audits: Repository & Tooling, Frontend Architecture, Backend Architecture, Database & Data Integrity, Security, DevOps & Deployment, Product Requirements, and Systems Architecture.

The application is a mobile-first personal finance SaaS designed for Indonesian individual users and micro-merchants. The core domain layer demonstrates **exceptional mathematical and domain rigor**: monetary values strictly enforce zero floating-point arithmetic via a signed 64-bit integer Rupiah value object (`Rupiah(i64)`), account balances are updated atomically via SQL `RETURNING` expressions inside database transactions, and the frontend presents a modern fintech user experience with authentic branding, a 50/50 split-screen authentication screen, a 4-tier dashboard, a POS numeric keypad with haptic vibration, and 100% Lucide SVG iconography.

However, the audit uncovered **critical security vulnerabilities, architectural disconnects, and single points of failure** in deployment and production wiring that currently prohibit production release.

### Top 15 Synthesized Architectural & Systemic Findings

1. **[CRITICAL RISK / FACT] Production Router Bypasses Rate Limiting**: While `SlidingWindowRateLimiter` (5 attempts / 900s) is fully implemented and tested in test harnesses, `backend/src/api/mod.rs:83` mounts unthrottled `auth::auth_routes` rather than `auth::auth_routes_with_rate_limiter`. The production binary (`main.rs`) exposes `/api/v1/auth/login` and `/api/v1/auth/register` to automated credential stuffing attacks.
2. **[CRITICAL RISK / FACT] Hardcoded Plaintext Secrets & RSA Private Key in Git**: `start.sh:7-16` hardcodes live production credentials, including a 2048-bit DANA PKCS#1 RSA private key (`DANA_PRIVATE_KEY`), DANA Client Secret, DANA Merchant ID, and production `JWT_SECRET`, with `COOKIE_SECURE="false"`.
3. **[CRITICAL RISK / FACT] Financial Idempotency Payload Verification Bypassed**: In `backend/src/service/ledger_service.rs:154`, the call to `idempotency_repo.acquire_lock` hardcodes an empty string `""` as the `request_hash`, permanently disabling payload mismatch detection (`IdempotencyLockResult::MismatchedPayload`) and violating financial deduplication standards.
4. **[CRITICAL RISK / FACT] Edge Single Point of Failure (SPOF) with Zero Database Backups**: The live database resides exclusively on the internal eMMC flash storage of a Samsung Galaxy A20s smartphone (`data/personal_finance.db`). There is zero automated database replication (Litestream) or offsite backup cron. A device hardware failure or flash wear-out causes irreversible, permanent data loss.
5. **[CRITICAL RISK / FACT] Unfiltered Target Monorepo CI/CD Deployment**: The target integration repo (`/home/nurdiansyah/dev/Personal_project/.github/workflows/deploy.yml:3-8`) triggers on every push to `main` without `paths:` filters, executing React builds and uploading via FTP directly to production cPanel shared hosting. Merging code without path filters will trigger accidental production cPanel FTP overwrites.
6. **[HIGH RISK / FACT] PWA Offline Startup Lockout**: While Workbox caches static assets, `frontend/src/App.vue:173` unconditionally executes `authStore.checkAuth()` on application mount. When launched offline, `/api/v1/auth/me` fails, resetting `user = null` and locking authenticated users out to the login screen, breaking core PWA offline functionality.
7. **[HIGH RISK / FACT] Permanent Premium Privilege Retention via Lazy Trial Expiry**: `activate_trial_with_pool` permanently updates `users.subscription_tier = 'premium'` (`backend/src/service/payment_service.rs:1091`). Expiry is evaluated lazily only within `GET /api/v1/subscription` (`webhooks.rs:125-163`), while `AuthService::login` loads the tier directly from `users` without checking `trial_ends_at`. Trial users who skip the subscription screen retain permanent Pro access across logins.
8. **[HIGH RISK / FACT] Non-Transactional Webhook Settlement**: `handle_dana_notification` (`backend/src/service/payment_service.rs:751-786`) updates subscriptions, user tiers, and marks webhooks as processed across multiple uncoordinated SQL queries without an enclosing database transaction (`pool.begin()`), while silencing audit log errors (`let _ =`).
9. **[HIGH RISK / FACT] Absence of Vue Router & Orphan Pinia Store**: Navigation relies on a primitive `ref('home')` tab-switching state in `App.vue:115`, breaking browser history, URL deep linking, and hardware back buttons. Concurrently, a complete 213-line Pinia store (`src/stores/transactions.js`) is completely bypassed by views that duplicate fetch logic and local state.
10. **[HIGH RISK / FACT] 15-Minute Short JWT Without Refresh Token Endpoint**: Tokens expire after 900 seconds (`JWT_TTL_SECONDS`), but no refresh token endpoint or sliding session mechanism exists. Active mobile users are abruptly logged out mid-session every 15 minutes.
11. **[HIGH RISK / FACT] Unmanaged Process Execution via `nohup` & Hard `pkill -9`**: The backend is spawned via `nohup ./backend > /tmp/personal_finance_pwa.log 2>&1 &` without a process supervisor (`supervisord`/`systemd`). Deployments execute `pkill -9 backend`, abruptly aborting active connections, bypassing graceful shutdown hooks, and risking WAL database corruption.
12. **[HIGH RISK / FACT] E2E Mock Server Fallback Masks Backend Outages**: `e2e_tests/runner.sh:61-63` automatically starts an embedded 1,323-line Python mock server (`harness/server.py`) if the Rust backend is unreachable, reporting 100% test pass even when the compiled Rust binary fails to build or boot.
13. **[MEDIUM RISK / FACT] False-Positive Readiness Probe Bug**: In `backend/src/api/health.rs:44-63`, `ready_handler` swallows database query errors via `.unwrap_or(false)` and returns `StatusCode::OK` (HTTP 200), deceiving external monitors when the database is locked or dead.
14. **[MEDIUM RISK / FACT] Missing Advertised SaaS Features**: `UpgradeModal.vue` and `ProfileView.vue` advertise category budgeting, financial goals, and encrypted PDF/CSV export. In reality, schema tables `budgets` and `goals` are ghost tables with zero backend repositories, services, or endpoints, and export functionality does not exist.
15. **[RECOMMENDATION] Immediate Phase 1 Remediation**: Prioritize wiring the rate limiter, computing SHA-256 idempotency request hashes, purging secrets from `start.sh`, implementing target CI/CD path filters, and setting up Litestream replication before permitting production traffic.

---

## B. Current Architecture & System Topology

### B.1 Actual System Topology & Ingress Data Path

The application runs in a hybrid edge architecture, connecting mobile/desktop web clients through Cloudflare's global network to an edge server hosted on physical smartphone hardware:

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                       CLIENT ENVIRONMENT                                         │
│  Mobile / Desktop Browser (Vue 3 PWA, Pinia, Tailwind CSS, Workbox Service Worker)             │
└────────────────────────────────────────────────┬─────────────────────────────────────────────────┘
                                                 │ HTTPS (TLS 1.3 / HTTP/2 / HTTP/3)
                                                 ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                  CLOUDFLARE GLOBAL EDGE NETWORK                                  │
│  • Anycast DNS: api.nurdiansyahlabs.com                                                          │
│  • Edge TLS Termination & Automated Certificate Rotation                                        │
│  • DDoS Shield & Cloudflare Tunnel Proxy                                                         │
│  • Honors Cache-Control: private, no-store, must-revalidate                                      │
└────────────────────────────────────────────────┬─────────────────────────────────────────────────┘
                                                 │ Persistent Outbound HTTPS Tunnel (cloudflared)
                                                 ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│                       EDGE HARDWARE NODE: SAMSUNG GALAXY A20s (SM-A207F)                         │
│  • SoC: Qualcomm Snapdragon 450 (8x ARM Cortex-A53 @ 1.80 GHz)                                   │
│  • Memory: 3 GB LPDDR3 RAM | Storage: 32 GB eMMC 5.1 Flash                                      │
│  • Host OS: Rooted Android 11 (SELinux: Enforcing)                                               │
│  • Local Tunnel Daemon: /system/bin/cloudflared (PID 9105) ───► Ingress to 127.0.0.1:8080        │
│                                                                                                  │
│  ┌────────────────────────────────────────────────────────────────────────────────────────────┐  │
│  │                    CHROOT RUNTIME CONTAINER: /data/local/ubuntu-rootfs                     │  │
│  │                                                                                            │  │
│  │  ┌──────────────────────────────────────────────────────────────────────────────────────┐  │  │
│  │  │                     CONSOLIDATED RUST AXUM APPLICATION DAEMON                        │  │  │
│  │  │                     (Cross-compiled via cargo zigbuild for aarch64)                  │  │  │
│  │  │                                                                                      │  │  │
│  │  │  [Layer 1: Transport & Static Assets]                                                │  │  │
│  │  │  • Axum 0.8 Router listening on 127.0.0.1:8080                                       │  │  │
│  │  │  • Fallback Static File Service: ServeDir("./dist") -> index.html (SPA Fallback)    │  │  │
│  │  │                                                                                      │  │  │
│  │  │  [Layer 2: Middleware & Security]                                                    │  │  │
│  │  │  • FromRequestParts<AuthenticatedUser> (Extracts JWT from auth_token cookie/Bearer)   │  │  │
│  │  │  • FeatureGate (analytics.advanced, budgeting, reports.advanced)                     │  │  │
│  │  │  • CacheControlMiddleware (private, no-store, must-revalidate)                       │  │  │
│  │  │  • (RateLimiter: Implemented but UNWIRED in production router)                       │  │  │
│  │  │                                                                                      │  │  │
│  │  │  [Layer 3: Service Orchestration]                                                    │  │  │
│  │  │  • AuthService (Argon2id password verification, JWT generation)                      │  │  │
│  │  │  • LedgerService (Idempotency acquisition, balance deltas, net cash flow)            │  │  │
│  │  │  • PaymentService (RSA-SHA256 DANA verify, Midtrans HMAC, Trial lifecycle)           │  │  │
│  │  │                                                                                      │  │  │
│  │  │  [Layer 4: Pure Domain Invariants]                                                   │  │  │
│  │  │  • Rupiah(i64) Pure Integer Value Object (Checked math, zero floating point)         │  │  │
│  │  │                                                                                      │  │  │
│  │  │  [Layer 5: Persistence Abstraction]                                                 │  │  │
│  │  │  • SQLx 0.8 SqlitePool (max_connections: 10, busy_timeout: 5000ms)                  │  │  │
│  │  │  • Repositories: User, Account, Category, Transaction, Subscription, Idempotency     │  │  │
│  │  │  • AuditRepository: audit_logs (partially integrated)                                │  │  │
│  │  └──────────────────────────────────────┬───────────────────────────────────────────────┘  │  │
│  │                                         │ Embedded SQLite Connection (C ABI)               │  │
│  │                                         ▼                                                  │  │
│  │  ┌──────────────────────────────────────────────────────────────────────────────────────┐  │  │
│  │  │                        EMBEDDED SQLITE 3 DATABASE ENGINE                             │  │  │
│  │  │  Location: /root/personal_finance_pwa/data/personal_finance.db                       │  │  │
│  │  │  Mode: Write-Ahead Logging (WAL) | PRAGMA synchronous = NORMAL                       │  │  │
│  │  │  Concurrency: Multi-reader (up to 10 pool handles) + SINGLE EXCLUSIVE WRITER         │  │  │
│  │  │  Files: personal_finance.db, personal_finance.db-wal, personal_finance.db-shm         │  │  │
│  │  └──────────────────────────────────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### B.2 6-Tier Component Boundaries & Layer Isolation Analysis

The architecture adheres to a 6-tier layered model:

```text
┌────────────────────────────────────────────────────────────────────────┐
│  Tier 1: Presentation & State Layer (Vue 3, Pinia, Workbox)            │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ JSON over HTTP/2 (Cookies / REST)
┌───────────────────────────────────▼────────────────────────────────────┐
│  Tier 2: API Transport & Protocol (Axum 0.8, Extractors, DTOs)         │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Typed Invocations (Send + Sync)
┌───────────────────────────────────▼────────────────────────────────────┐
│  Tier 3: Service Orchestration (Ledger, Auth, Payment)                 │
└───────────────────┬────────────────────────────────┬───────────────────┘
                    │ Uses Invariants                │ Interacts with Repos
┌───────────────────▼────────────────────┐ ┌─────────▼───────────────────┐
│  Tier 4: Pure Domain Mathematics       │ │ Tier 5: Persistence         │
│          (Rupiah, Checked Math)        │ │         Repositories (SQLx) │
└────────────────────────────────────────┘ └─────────┬───────────────────┘
                                                     │ SQL Statements / Bindings
                                           ┌─────────▼───────────────────┐
                                           │ Tier 6: Embedded Storage    │
                                           │         (SQLite WAL Engine) │
                                           └─────────────────────────────┘
```

#### Layer Isolation Evaluation:
- **Tier 1 (Presentation)**: Vue 3 Composition API with Pinia. *Isolation Defect*: `TransactionsView.vue` and `AddTransactionModal.vue` bypass `src/stores/transactions.js` and directly execute `api.getTransactions()` and `api.createTransaction()`, duplicating local reactive state.
- **Tier 2 (API Transport)**: Axum handlers cleanly extract DTOs and convert domain errors to RFC 7807 problem details JSON. *Anomaly*: `backend/src/api/webhooks.rs:128-142` issues raw SQL `UPDATE` statements directly inside the handler for lazy trial expiry, violating repository encapsulation.
- **Tier 3 (Service Orchestration)**: Manages cross-entity boundaries. `LedgerService` correctly scopes transaction creation within `sqlx::Transaction` blocks. *Defect*: `PaymentService` settles payment webhooks via multiple un-transacted queries.
- **Tier 4 (Domain Mathematics)**: `Rupiah(pub i64)` has zero dependencies on web or database crates. Implements checked arithmetic, prevents integer overflow, and Serde rejects floats. *Status: Exemplary*.
- **Tier 5 (Repositories)**: Trait-driven SQLx repositories. Parameterized queries enforce multi-tenant isolation (`WHERE user_id = ?`).
- **Tier 6 (Storage)**: Embedded SQLite 3 in WAL mode with `busy_timeout = 5000ms`. Enforces single-writer serial write locks across all Tokio connections.

### B.3 End-to-End Data Flows

1. **Authentication Flow**: Client `POST /api/v1/auth/login` ──► Axum Router (currently unthrottled) ──► `AuthService::login()` ──► `SqlxUserRepository::find_by_email()` (table scan on `LOWER(email)`) ──► `tokio::task::spawn_blocking(argon2id_verify)` (~120ms CPU) ──► `JwtEngine::generate_token()` ──► Returns `Set-Cookie: auth_token=<jwt>; HttpOnly; SameSite=Lax`.
2. **Transaction Recording Flow**: Client `POST /api/v1/transactions` (`Idempotency-Key` header) ──► `AuthenticatedUser` extractor ──► `LedgerService::create_transaction()` ──► `IdempotencyRepo::acquire_lock(user_id, key, hash="")` (empty hash bug) ──► `pool.begin()` (acquires SQLite write lock) ──► `TransactionRepo::create_in_tx()` ──► `AccountRepo::adjust_balance_atomic_tx()` (`UPDATE accounts SET current_balance = current_balance + ?1 RETURNING current_balance`) ──► `IdempotencyRepo::save_response_tx()` ──► `tx.commit()` ──► HTTP 201 Created.
3. **DANA Webhook Settlement Flow**: DANA Gateway `POST /api/v1/webhooks/dana` (`X-TIMESTAMP`, `X-SIGNATURE`) ──► `PaymentService::verify_dana_signature()` (SNAP RSA-SHA256 PKCS#1 v1.5; missing timestamp drift check) ──► `SubscriptionRepo::record_webhook_event()` (deduplicates on `UNIQUE(provider, event_id)`) ──► `handle_dana_notification()` (non-transactional: separate queries to `upsert_subscription`, `update_tier`, and `log_event`) ──► Returns `2005600 Successful`.
4. **Analytics Flow**: Client `GET /api/v1/analytics/basic` or `/advanced` ──► `AuthenticatedUser` extractor ──► Feature gate check (`user_tier == "premium"`) ──► `LedgerService::cash_flow_summary()` ──► SQL `SELECT SUM(income), SUM(expense) WHERE user_id = ?` ──► `CashFlowSummary::compute()` (pure integer `income - expense`) ──► HTTP 200 OK.

---

## C. Technology Stack Inventory

Comprehensive technology stack catalog synthesized across all audit inspection tracks:

| Layer | Technology | Specified Version | Resolved / Installed Version | Status | Repository Evidence | Classification |
|---|---|---|---|---|---|---|
| **Backend Language** | Rust | Edition 2021 | 1.93.1 (`rustc`/`cargo`) | Operational | `backend/Cargo.toml:4` | `FACT` |
| **Async Runtime** | Tokio | `1.43` | `1.53.1` (`features = ["full"]`) | Operational | `backend/Cargo.toml:7`, `Cargo.lock:1924` | `FACT` |
| **Web Framework** | Axum | `0.8` | `0.8.9` | Operational | `backend/Cargo.toml:17`, `Cargo.lock:65` | `FACT` |
| **Cookie Middleware** | axum-extra | `0.10` | `0.10.1` (`features = ["cookie"]`) | Operational | `backend/Cargo.toml:18`, `Cargo.lock:95` | `FACT` |
| **Database Driver** | SQLx | `0.8` | `0.8.6` (`sqlite`, `tokio`, `migrate`) | Operational | `backend/Cargo.toml:8`, `Cargo.lock:1578` | `FACT` |
| **Database Engine** | SQLite 3 (WAL mode) | Embedded | 3.x (via SQLx C-bindings) | Operational | `backend/src/repository/db.rs:60` | `FACT` |
| **Password Hashing** | Argon2id | `0.6` | `0.6.0` (OWASP parameters) | Operational | `backend/Cargo.toml:15`, `Cargo.lock:21` | `FACT` |
| **Token Authentication** | jsonwebtoken | `9.3` | `9.3.1` (HMAC-SHA256) | Operational | `backend/Cargo.toml:16`, `Cargo.lock:930` | `FACT` |
| **Public-Key Crypto** | RSA | `0.9` | `0.9.10` (`sha2`, `pem`) | Operational | `backend/Cargo.toml:24`, `Cargo.lock:1356` | `FACT` |
| **Constant-Time Crypto** | subtle | `2.6` | `2.6.1` | Operational | `backend/Cargo.toml:22`, `backend/src/service/payment_service.rs:21` | `FACT` |
| **HTTP Client** | Reqwest | `0.12` | `0.12.28` (`rustls-tls`) | Operational | `backend/Cargo.toml:25`, `Cargo.lock:1301` | `FACT` |
| **Static File Server** | Tower-HTTP | `0.7.1` | `0.7.1` (`features = ["fs"]`) | Operational | `backend/Cargo.toml:26`, `Cargo.lock:1980` | `FACT` |
| **Validation Framework** | Manual Imperative | N/A | Handlers / Services | Incomplete | `backend/src/api/*.rs` (No `validator` crate) | `FACT` |
| **Structured Tracing** | Standard Out (`println!`) | N/A | Missing | Deficient | `backend/Cargo.toml` (No `tracing` crate) | `FACT` |
| **Frontend Framework** | Vue.js 3 | `^3.5.6` | `3.5.42` (Composition API) | Operational | `frontend/package.json:15` | `FACT` |
| **Frontend Bundler** | Vite | `^5.4.6` | `5.4.21` | Operational | `frontend/package.json:23`, `frontend/vite.config.js:2` | `FACT` |
| **Frontend State Layer** | Pinia | `^2.2.2` | `2.3.1` | Partial (Orphan store) | `frontend/package.json:14`, `frontend/src/main.js:7` | `FACT` |
| **CSS Framework** | Tailwind CSS | `^3.4.11` | `3.4.19` | Operational | `frontend/package.json:22`, `frontend/tailwind.config.js:2` | `FACT` |
| **CSS Postprocessing** | PostCSS / Autoprefixer | `^8.4.47` / `^10.4.20` | `8.4.49` / `10.4.20` | Operational | `frontend/package.json:20-21` | `FACT` |
| **PWA Service Worker** | vite-plugin-pwa | `^0.20.5` | `0.20.5` (Workbox) | Partial (Dual manifest) | `frontend/package.json:24`, `frontend/vite.config.js:4` | `FACT` |
| **Iconography** | lucide-vue-next | `^0.454.0` | `0.454.0` | Deprecated | `frontend/package.json:13`, `package-lock.json:4862` | `FACT` |
| **Chart Visualization** | Chart.js / vue-chartjs | `^4.4.4` / `^5.3.1` | `4.5.1` / `5.3.4` | Dead (0 imports in src) | `frontend/package.json:12,16` | `FACT` |
| **Client Router** | None (Tab state) | None | Missing (`currentTab = ref('home')`) | Deficient | `frontend/src/App.vue:115` | `FACT` |
| **TypeScript Support** | None (`ts` blocks in esbuild) | None | Incomplete (No `tsconfig.json`) | Risk | `frontend/src/components/ui/index.ts:1-16` | `RISK` |
| **DOM Testing Harness** | happy-dom | None | `15.11.7` (installed) | Undeclared in package.json | `frontend/tests/adversarial_challenge.mjs:6` | `RISK` |
| **E2E Test Runner** | Python / TAP v13 | 3.11.2 | 3.11.2 | Operational | `e2e_tests/run_all.py:1`, `e2e_tests/runner.sh:5` | `FACT` |
| **Edge Ingress Proxy** | Cloudflare Tunnel (`cloudflared`) | Release | PID 9105 (`SM-A207F`) | Operational | Edge OS `/system/bin/cloudflared` | `FACT` |

---

## D. Feature Status Matrix

Exhaustive audit covering all 15 capability areas specified in project requirements:

| Feature / Capability | Implemented | Partial | Missing | Broken | Repository Evidence & Location | Classification | Detailed Audit Notes |
| :--- | :---: | :---: | :---: | :---: | :--- | :--- | :--- |
| **1. Authentication & Session** | | | | | | | |
| Email/Password Registration | [x] | [ ] | [ ] | [ ] | `backend/src/api/auth.rs:60-84` | `FACT` | Argon2id hashing, starter cash account & 10 categories seeded in transaction. |
| Email/Password Login | [x] | [ ] | [ ] | [ ] | `backend/src/api/auth.rs:86-110` | `FACT` | Case-insensitive email lookup; dummy verify defeats timing attacks. |
| Split-Screen Auth UI | [x] | [ ] | [ ] | [ ] | `frontend/src/components/auth/SplitScreenAuth.vue:1-363` | `FACT` | 50/50 desktop split, brand logo, quick demo buttons, error sanitization. |
| Auth Rate Limiter | [ ] | [ ] | [ ] | [x] | `backend/src/api/mod.rs:83` | `CRITICAL RISK` | `SlidingWindowRateLimiter` implemented but **unwired in production router**. |
| Session Renewal / Refresh | [ ] | [ ] | [x] | [ ] | `backend/src/service/jwt.rs:50-144` | `FACT` | 15-minute JWT expires abruptly; no refresh token endpoint exists. |
| **2. Multi-Wallet Accounts** | | | | | | | |
| Multi-Wallet Account Engine | [x] | [ ] | [ ] | [ ] | `backend/src/repository/account_repo.rs:1-260` | `FACT` | Supports checking, savings, credit, e-wallet, cash, investment with integer balances. |
| Account REST Endpoints | [x] | [ ] | [ ] | [ ] | `backend/src/api/accounts.rs:1-120` | `FACT` | List, get by ID, create account, and soft-archive (`POST /accounts/{id}/archive`). |
| Multi-Wallet Dashboard Deck | [x] | [ ] | [ ] | [ ] | `frontend/src/views/HomeView.vue:85-116` | `FACT` | Responsive cards for each wallet with balance privacy toggle (`••••`). |
| Wallet Management UI | [ ] | [ ] | [x] | [ ] | `frontend/src/views/ProfileView.vue:1-224` | `FACT` | Backend endpoints and store methods exist, but **zero UI exists** to create/archive wallets. |
| **3. Categories & Personalization** | | | | | | | |
| System Starter Categories | [x] | [ ] | [ ] | [ ] | `backend/src/service/auth_service.rs:182-214` | `FACT` | 10 default categories seeded automatically during user registration. |
| Category Soft-Deletion | [x] | [ ] | [ ] | [ ] | `backend/src/repository/category_repo.rs:122-154` | `FACT` | User categories soft-deleted (`deleted_at = now`); system categories protected. |
| Category Ownership Check in Tx | [ ] | [ ] | [ ] | [x] | `backend/src/service/ledger_service.rs:186-215` | `RISK` | IDOR vulnerability: Transaction creation omits `category.user_id == user_id` check. |
| Category Management UI | [ ] | [ ] | [x] | [ ] | `frontend/src/views/ProfileView.vue:1-224` | `FACT` | Users cannot create custom categories or delete existing ones from the UI. |
| **4. Transaction Ledger & Entry** | | | | | | | |
| POS Numeric Keypad Modal | [x] | [ ] | [ ] | [ ] | `frontend/src/views/AddTransactionModal.vue:120-138` | `FACT` | 4x3 keypad with `navigator.vibrate(10)` haptic feedback and instant IDR formatting. |
| Transaction Recording & Transfer | [x] | [ ] | [ ] | [ ] | `backend/src/service/ledger_service.rs:136-245` | `FACT` | Atomic balance debit/credit across source and destination accounts for transfers. |
| Transaction Deletion & Reversal | [x] | [ ] | [ ] | [ ] | `backend/src/service/ledger_service.rs:250-320` | `FACT` | Deleting transaction atomically reverses balance delta on affected accounts. |
| Transaction Ledger Audit Trail | [ ] | [ ] | [ ] | [x] | `backend/src/service/ledger_service.rs:250-320` | `RISK` | Transactions are hard-deleted (`DELETE FROM transactions`) without writing to `audit_logs`. |
| Transaction Search & Date Filters | [ ] | [ ] | [x] | [ ] | `frontend/src/views/TransactionsView.vue:21-43` | `FACT` | Store supports search and dates, but view lacks search bar and date range inputs. |
| Transaction Backdating | [ ] | [ ] | [x] | [ ] | `frontend/src/views/AddTransactionModal.vue:266` | `FACT` | Forces `date: new Date().toISOString()`; users cannot record past expenses. |
| Transaction Editing | [ ] | [ ] | [x] | [ ] | `frontend/src/views/TransactionsView.vue:83-91` | `FACT` | No edit action or modal exists; transactions can only be deleted. |
| Pinia Transaction Store Integration | [ ] | [ ] | [ ] | [x] | `frontend/src/views/TransactionsView.vue:132-168` | `FACT` | `useTransactionStore` is completely orphaned; views invoke raw `api.js` directly. |
| **5. Idempotency & Concurrency** | | | | | | | |
| Idempotency Key 24h Lock | [x] | [ ] | [ ] | [ ] | `backend/src/repository/idempotency_repo.rs:83-145` | `FACT` | 24-hour TTL locking via SQLite unique constraint on `(user_id, idempotency_key)`. |
| Payload Hash Mismatch Detection | [ ] | [ ] | [ ] | [x] | `backend/src/service/ledger_service.rs:154` | `CRITICAL RISK` | Passes `request_hash = ""`; payload tampering detection is completely bypassed. |
| Concurrency Write Handling | [x] | [ ] | [ ] | [ ] | `backend/src/repository/db.rs:61` | `FACT` | SQLite WAL mode with `busy_timeout = 5000ms` queues contending writers safely. |
| **6. Calculation Engine & Currency** | | | | | | | |
| Integer Rupiah Representation | [x] | [ ] | [ ] | [ ] | `backend/src/domain/money.rs:15-35` | `FACT` | Signed 64-bit integer (`Rupiah(i64)`), zero floats, Serde rejects decimal values. |
| Checked Arithmetic | [x] | [ ] | [ ] | [ ] | `backend/src/domain/money.rs:60-125` | `FACT` | Safe math with `checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_mul_bps`. |
| Shared Net Cash Flow Engine | [x] | [ ] | [ ] | [ ] | `backend/src/repository/transaction_repo.rs:41-60` | `FACT` | Deterministic computation of `net = total_income - total_expenses` across endpoints. |
| Localized Currency Formatting | [x] | [ ] | [ ] | [ ] | `frontend/src/utils/currency.js:1-25` | `FACT` | Formats as `Rp 1.000.000` using `Intl.NumberFormat('id-ID')` with integer rounding guards. |
| **7. Dashboard & Visualization** | | | | | | | |
| 4-Tier Visual Hierarchy | [x] | [ ] | [ ] | [ ] | `frontend/src/views/HomeView.vue:1-274` | `FACT` | Tier 1: Pulse/Net Worth; Tier 2: Cash Flow/Wallets; Tier 3: Recent Transactions. |
| **8. Analytics & Financial Health** | | | | | | | |
| Basic Cash Flow & Savings Rate | [x] | [ ] | [ ] | [ ] | `backend/src/api/analytics.rs:55-80` | `FACT` | Dynamic savings rate calculation `(net * 100) / income` available to Free tier. |
| Pro Financial Health Scoring | [x] | [ ] | [ ] | [ ] | `backend/src/api/analytics.rs:82-135` | `FACT` | Clamped 0-100 score `(savings_rate * 8 / 10) + 20` gated behind Premium tier. |
| Dynamic Runway Projection | [ ] | [ ] | [ ] | [x] | `frontend/src/views/AnalyticsView.vue:89-101` | `FACT` | Hardcoded static HTML (`6.2 Bulan`, `w-[65%]`) disconnecting from reactive store math. |
| Pro Frosted Locked Overlay | [x] | [ ] | [ ] | [ ] | `frontend/src/views/AnalyticsView.vue:104-125` | `FACT` | Frosted backdrop blur with lock badge and upgrade trigger for Free Tier users. |
| Interactive Charts (`chart.js`) | [ ] | [ ] | [x] | [ ] | `frontend/package.json:12,16` | `FACT` | `chart.js` and `vue-chartjs` installed but **never imported or rendered** anywhere. |
| **9. Budgeting Controls** | | | | | | | |
| Budget Database Schema | [x] | [ ] | [ ] | [ ] | `backend/migrations/0001_initial_schema.sql:68-78` | `FACT` | `budgets` table exists with `amount_limit`, `period`, `start_date`, `end_date`. |
| Budget Backend API & Services | [ ] | [ ] | [x] | [ ] | `backend/src/` | `FACT` | **Zero repositories, services, or API endpoints exist** for budgets. |
| Budget UI Controls | [ ] | [ ] | [x] | [ ] | `frontend/src/` | `FACT` | **Zero frontend views or components exist** for budgets, despite being advertised. |
| **10. Financial Goals** | | | | | | | |
| Goals Database Schema | [x] | [ ] | [ ] | [ ] | `backend/migrations/0001_initial_schema.sql:81-93` | `FACT` | `goals` table exists with `target_amount`, `current_amount`, `target_date`. |
| Goals Backend API & Services | [ ] | [ ] | [x] | [ ] | `backend/src/` | `FACT` | **Zero repositories, services, or API endpoints exist** for goals. |
| Goals Tracking UI | [ ] | [ ] | [x] | [ ] | `frontend/src/` | `FACT` | **Zero frontend views or components exist** for goals. |
| **11. Reports & Data Export** | | | | | | | |
| Encrypted PDF/CSV Export API | [ ] | [ ] | [x] | [ ] | `backend/src/` | `FACT` | **Zero export endpoints exist** in backend. |
| Data Export UI | [ ] | [ ] | [x] | [ ] | `frontend/src/` | `FACT` | Advertised in `UpgradeModal.vue:96`, but **no UI trigger exists**. |
| **12. Payments & Subscriptions** | | | | | | | |
| Direct DANA Checkout Session | [x] | [ ] | [ ] | [ ] | `backend/src/service/payment_service.rs:440-540` | `FACT` | Generates RSA-signed payment order to live DANA sandbox host-to-host API. |
| DANA Webhook Handler (SNAP) | [x] | [ ] | [ ] | [ ] | `backend/src/api/webhooks.rs:266-335` | `FACT` | RSA-SHA256 signature verification over SNAP string-to-sign; returns `2005600`. |
| DANA Timestamp Drift Check | [ ] | [ ] | [ ] | [x] | `backend/src/service/payment_service.rs:365-375` | `RISK` | Extracts `X-TIMESTAMP` but omits freshness assertion (`|now - ts| <= 300s`). |
| Midtrans / Xendit Webhooks | [x] | [ ] | [ ] | [ ] | `backend/src/service/payment_service.rs:797-930` | `FACT` | Constant-time HMAC & callback token verification; deduplicates and settles. |
| Atomic Webhook Settlement | [ ] | [ ] | [ ] | [x] | `backend/src/service/payment_service.rs:751-770` | `RISK` | Updates subscription, user tier, and audit log without `pool.begin()` transaction. |
| Plaintext Credentials in `start.sh` | [ ] | [ ] | [ ] | [x] | `start.sh:7-16` | `CRITICAL RISK` | Plaintext RSA-2048 private key, client secrets, and JWT secret tracked in script. |
| **13. 7-Day Free Trial Lifecycle** | | | | | | | |
| On-Demand Trial Activation | [x] | [ ] | [ ] | [ ] | `backend/src/api/webhooks.rs:188-235` | `FACT` | Instant activation without credit card; re-issues Pro JWT session cookie. |
| Single Trial Enforcement | [x] | [ ] | [ ] | [ ] | `backend/src/service/payment_service.rs:1095` | `FACT` | Atomic conditional update `WHERE has_used_trial = 0` prevents repeated activations. |
| Trial Expiry Evaluation | [ ] | [ ] | [ ] | [x] | `backend/src/service/auth_service.rs:269` | `RISK` | Expiry evaluated lazily in `GET /subscription`; login skips expiry check. |
| **14. PWA & Mobile UX** | | | | | | | |
| Workbox App Shell Caching | [x] | [ ] | [ ] | [ ] | `frontend/vite.config.js:53-69` | `FACT` | StaleWhileRevalidate for scripts, styles, images, and HTML. |
| PWA Offline Startup | [ ] | [ ] | [ ] | [x] | `frontend/src/App.vue:173-177` | `RISK` | Offline boot fails network auth check, ejecting authenticated user to login screen. |
| Client Routing (Vue Router) | [ ] | [ ] | [ ] | [x] | `frontend/src/App.vue:115` | `HIGH RISK` | Uses `ref('home')` tab switching; deep linking and back button do not work. |
| Dark Mode Toggle | [ ] | [ ] | [ ] | [x] | `frontend/src/index.css:62-115` | `FACT` | CSS variables defined, but no UI toggle or class persistence exists. |
| Dead UI Primitives | [ ] | [ ] | [x] | [ ] | `frontend/src/components/ui/Card.vue` | `FACT` | `Card.vue` and `ModalSheet.vue` exported in barrel file but 100% unused in views. |
| Undeclared Test Dependency | [ ] | [ ] | [ ] | [x] | `frontend/tests/adversarial_challenge.mjs:6` | `RISK` | Imports `happy-dom` which is omitted from `package.json`; breaks `npm ci`. |
| **15. DevOps & CI/CD** | | | | | | | |
| Target CI/CD Path Filtering | [ ] | [ ] | [ ] | [x] | Target `deploy.yml:3-8` | `CRITICAL RISK` | Target `deploy.yml` lacks `paths:` filter; any push triggers cPanel FTP deployment. |
| Repository `.gitignore` | [ ] | [ ] | [x] | [ ] | Repository Root | `HIGH RISK` | Complete absence of `.gitignore`; 9.6GB target and live DBs untracked. |
| Process Supervision | [ ] | [ ] | [ ] | [x] | `start.sh:18` | `HIGH RISK` | Executed via unmanaged `nohup ... &` with `pkill -9` termination. |
| Automated Database Backups | [ ] | [ ] | [x] | [ ] | Repository & Edge System | `CRITICAL RISK` | Zero automated SQLite backups or replication on edge phone storage. |

---

## E. Security Findings & Vulnerability Matrix

Prioritized security audit matrix synthesized from the defensive security review:

| Severity | ID | Vulnerability / Issue | File Location | Classification | Impact | Remediation Recommendation |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **CRITICAL** | SEC-01 | Plaintext RSA Private Key & Secrets Hardcoded in Deployment Script | `start.sh:7-16` | **FACT** | Immediate compromise of payment webhook signing, merchant credentials, and user JWT signatures. | Remove all credentials from `start.sh`. Load from an external secrets manager or deployment `.env`. Rotate DANA keys and client secrets immediately. |
| **CRITICAL** | SEC-02 | Production Axum Router Bypasses Rate Limiter | `backend/src/api/mod.rs:83`<br>`backend/src/main.rs:141` | **FACT** | `/login` and `/register` are completely unthrottled in production, enabling automated brute-force attacks and account enumeration. | Update `create_app` in `api/mod.rs` to accept `Arc<SlidingWindowRateLimiter>` and mount `auth_routes_with_rate_limiter`. Wire and spawn cleanup in `main.rs`. |
| **CRITICAL** | SEC-03 | Insecure Default Fallback for `JWT_SECRET` | `backend/src/main.rs:63-65` | **FACT** | If `JWT_SECRET` is unset, backend boots with a publicly known string, allowing arbitrary token forgery and administrative privilege escalation. | Replace `unwrap_or_else` with strict startup validation: abort process (`expect(...)`) if `JWT_SECRET` is missing or shorter than 32 bytes. |
| **HIGH** | SEC-04 | Hardcoded Default DANA Test RSA Private Key in Source Code | `backend/src/service/payment_service.rs:174-201, 316` | **FACT** | If `DANA_PUBLIC_KEY` is not provided in environment, verification falls back to default key pair. Attackers can sign webhooks with hardcoded private key. | Require `dana_public_key_pem` in production; reject webhooks with HTTP 500/401 rather than falling back to embedded test keys. |
| **HIGH** | SEC-05 | Permanent Premium Privilege Retention via Lazy Trial Expiry | `backend/src/service/payment_service.rs:1091`<br>`backend/src/api/webhooks.rs:125-163` | **FACT** | Users can retain active Pro status indefinitely by skipping `GET /api/v1/subscription`, because `users.subscription_tier` remains `'premium'` and login issues Pro tokens. | Re-evaluate trial expiry during `AuthService::login` and `AuthService::get_me`, and implement a periodic background task to transition expired trials in SQLite. |
| **HIGH** | SEC-06 | Missing Replay Protection Timestamp Validation in DANA Webhooks | `backend/src/service/payment_service.rs:365-376` | **FACT** | Webhook handler concatenates `X-TIMESTAMP` without asserting freshness against `Utc::now()`, allowing captured valid payloads to be replayed. | Parse `X-TIMESTAMP` (ISO 8601), compare with server time, and reject if `\|now - timestamp\| > 300` seconds (5 minutes). |
| **HIGH** | SEC-07 | Client IP Spoofing in Rate Limiter via Unvalidated Headers | `backend/src/api/middleware/rate_limiter.rs:180-207` | **FACT** | Blindly trusts `cf-connecting-ip`, `x-real-ip`, and `x-forwarded-for` without verifying if incoming socket IP is a trusted reverse proxy. | Only parse proxy forwarding headers if `connect_info` matches an explicit trusted proxy whitelist (e.g. 127.0.0.1 or Cloudflare CIDR). |
| **HIGH** | SEC-08 | Absence of Root `.gitignore` in Repository | Repository Root | **FACT** | Production secrets, `.env`, SQLite databases (`data/*.db`), logs, and binaries risk being committed to git tracking. | Add standard `.gitignore` covering `.env*`, `data/*.db*`, `target/`, `node_modules/`, `dist/`, and credentials. |
| **MEDIUM** | SEC-09 | IDOR: Missing Category Ownership Check on Transaction Creation | `backend/src/service/ledger_service.rs:186-215` | **FACT** | User A can supply User B's custom `category_id` when creating transactions; SQLite FK passes because ID exists in `categories` table. | Validate that `category_id` belongs to `user_id` or has `is_system = 1` before persisting the transaction. |
| **MEDIUM** | SEC-10 | Missing Security Headers (CSP, HSTS, X-Frame-Options, X-Content-Type-Options) | `backend/src/api/mod.rs:82-99` | **FACT** | Absence of security headers leaves frontend vulnerable to Clickjacking (lack of `X-Frame-Options`), MIME-sniffing, and lacks CSP defense-in-depth. | Add global response header middleware setting `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Strict-Transport-Security`, and CSP. |
| **MEDIUM** | SEC-11 | `COOKIE_SECURE="false"` Configured in Production Start Script | `start.sh:8` | **FACT** | Auth session cookies omit the `; Secure` flag, allowing transmission over plaintext HTTP and exposing cookies to network snooping / SSL stripping. | Enforce `COOKIE_SECURE="true"` in production environments. |
| **MEDIUM** | SEC-12 | Server-Side Feature Gating Incomplete for `budgeting` & `reports.advanced` | `backend/src/api/middleware/feature_gate.rs:18` | **FACT** | Permission strings exist in `feature_gate.rs`, but there are no backend routes for budgeting or reports; client cannot interact with these features. | Implement backend budget and report endpoints, and protect them with `require_permission(&user, "budgeting")` and `require_permission(&user, "reports.advanced")`. |
| **LOW** | SEC-13 | Incomplete `.env.example` Omits DANA Open API Variables | `.env.example:1-30` | **FACT** | `.env.example` documents Midtrans and Xendit, but provides no guidance or placeholders for DANA credentials. | Update `.env.example` with documented placeholders for all DANA Open API settings. |
| **LOW** | SEC-14 | Unauthenticated Disburse Webhook Stub Endpoint | `backend/src/api/webhooks.rs:339-353` | **FACT** | `POST /api/v1/webhooks/dana/disburse` accepts unauthenticated requests with no signature check and returns HTTP 200. | Apply DANA RSA signature verification middleware or reject unauthorized calls on this route. |

---

## F. Database Findings & Data Integrity

### F.1 SQLite WAL Configuration & PRAGMA Enforcement

Connection pool architecture is configured via `SqliteConnectOptions` in `backend/src/repository/db.rs:55-75` and initialized on boot in `backend/src/main.rs:45-57`:

| PRAGMA Parameter | Configured Value | Target Invariant | Operational Status | Verification Evidence | Classification |
|---|---|---|---|---|---|
| `journal_mode` | `WAL` | `WAL` | Active & Enforced | `db.rs:60`, `tests/m1_persistence_tests.rs:40` | `FACT` |
| `busy_timeout` | `5000` ms | `5000` ms | Active & Enforced | `db.rs:61`, `tests/m1_persistence_tests.rs:41` | `FACT` |
| `foreign_keys` | `ON` (true) | `ON` | Active & Enforced | `db.rs:62`, `tests/m1_persistence_tests.rs:42` | `FACT` |
| `synchronous` | `NORMAL` (1) | `NORMAL` | Active & Enforced | `db.rs:63`, `tests/m1_persistence_tests.rs:43` | `FACT` |
| `max_connections` | `10` | $\le 10$ | Active | `db.rs:20`, `main.rs:32` | `FACT` |
| `acquire_timeout` | `5` seconds | `5` seconds | Active | `db.rs:23`, `main.rs:40` | `FACT` |

### F.2 Schema Specification & Table Inventory

The schema is managed by embedded SQLx migrations (`0001_initial_schema.sql` and `0002_trial_and_dana_support.sql`):

| Table Name | Primary Key | Foreign Key Constraints | Check Constraints | Indexed Columns | Entity Purpose |
|---|---|---|---|---|---|
| `users` | `id TEXT` | None | `role IN ('user', 'admin')`, `subscription_tier IN ('free', 'premium')`, `has_used_trial IN (0, 1)` | `UNIQUE(email)` | User identity, credentials, currency, trial flags |
| `accounts` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE` | `account_type IN ('checking','savings','credit','e_wallet','cash','investment')`, `is_archived IN (0, 1)` | `idx_accounts_user(user_id)` | Multi-wallet financial accounts & integer balances |
| `categories` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE` | `category_type IN ('income', 'expense')`, `is_system IN (0, 1)` | `idx_categories_user(user_id)`, `idx_categories_system(is_system)` | Transaction categorization with soft-delete (`deleted_at`) |
| `transactions` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE`, `account_id REFERENCES accounts(id) ON DELETE RESTRICT`, `to_account_id REFERENCES accounts(id) ON DELETE RESTRICT`, `category_id REFERENCES categories(id) ON DELETE RESTRICT` | `transaction_type IN ('income', 'expense', 'transfer')`, `amount > 0`, `is_recurring IN (0, 1)` | `idx_transactions_user_date(user_id, date)`, `idx_transactions_account(account_id)`, `idx_transactions_to_account(to_account_id)`, `idx_transactions_user_category(user_id, category_id)` | Financial transaction records & transfer links |
| `budgets` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE`, `category_id REFERENCES categories(id) ON DELETE RESTRICT` | `amount_limit > 0`, `period IN ('weekly', 'monthly', 'yearly')` | `idx_budgets_user(user_id)`, `idx_budgets_user_category(user_id, category_id)` | **Ghost Table**: Schema exists; zero backend repositories or endpoints |
| `goals` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE` | `target_amount > 0`, `current_amount >= 0`, `is_completed IN (0, 1)` | `idx_goals_user(user_id)` | **Ghost Table**: Schema exists; zero backend repositories or endpoints |
| `subscriptions` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE` | `provider IN ('midtrans', 'xendit', 'dana', 'trial')`, `status IN ('active', 'grace', 'cancelled', 'expired', 'trialing')` | `idx_subscriptions_user(user_id)`, `idx_subscriptions_provider_id(provider, provider_subscription_id)` | Subscription state, gateway tokens, and billing periods |
| `audit_logs` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE SET NULL` | None | `idx_audit_logs_user_date(user_id, created_at)` | Security and administrative audit trail |
| `idempotency_keys` | `id TEXT` | `user_id REFERENCES users(id) ON DELETE CASCADE` | `status IN ('in_progress', 'completed', 'failed')`, `UNIQUE(user_id, idempotency_key)` | `idx_idempotency_expires(expires_at)` | Transaction deduplication and response replay cache |
| `webhook_events` | `id TEXT` | None | `provider IN ('midtrans', 'xendit', 'dana')`, `status IN ('received', 'processed', 'ignored', 'failed')`, `UNIQUE(provider, event_id)` | `idx_webhook_events_status(status)` | Inbound payment webhook deduplication log |

### F.3 Financial Precision & Pure Integer Rupiah Invariants

1. **Monetary Representation (`domain/money.rs:15-32`)**:
   - `pub struct Rupiah(pub i64);`
   - Invariant: 1 unit = Rp 1 (Indonesian Rupiah circulating currency has no fractional sen subunits).
   - Invariant: Absolute prohibition of floating-point representations (`f32`, `f64`, `REAL`, `DOUBLE`).
   - Serialization: `#[serde(transparent)]` serializes strictly as a JSON integer. Deserialization rejects floating-point values (e.g. `50000.50` returns a Serde parsing error).
   - Database Binding: `#[sqlx(transparent)]` binds directly to SQLite `INTEGER`.
2. **Checked Arithmetic Operations**:
   - `checked_add`, `checked_sub`, `checked_mul`, `checked_div`, and `checked_mul_bps` prevent integer overflow/underflow.
3. **Single Calculation Engine (`CashFlowSummary::compute`)**:
   - Location: `backend/src/repository/transaction_repo.rs:41-60` and `backend/src/domain/money.rs:180-215`.
   - Formula: `net_cash_flow = total_income - total_expenses`.
   - Verified: All financial endpoints (`/dashboard`, `/analytics/basic`, `/analytics/advanced`) query the identical SQL aggregation expression and execute `CashFlowSummary::compute`. Transfers are excluded from income and expenses, ensuring transfer neutrality on total wealth.

### F.4 Atomic Balance Deltas & Concurrency Maintenance

Account balance maintenance is implemented via atomic SQL expressions in `backend/src/repository/account_repo.rs:69-96`:
```sql
UPDATE accounts
SET current_balance = current_balance + ?1, updated_at = ?2
WHERE id = ?3 AND user_id = ?4
RETURNING current_balance;
```
- In `LedgerService::create_transaction` (`ledger_service.rs:183-298`), the entire lifecycle (inserting the transaction, adjusting source account balance, adjusting destination account balance for transfers, and caching the idempotency response) executes inside an atomic `sqlx::Transaction` (`let mut db_tx = self.pool.begin().await?`). If any sub-operation fails, the transaction is rolled back completely.
- Reversal on deletion (`ledger_service.rs:318-353`) applies exact inverse deltas within a database transaction block.

### F.5 Multi-Tenancy Query Isolation Matrix

Every SQL query in the repository layer was audited for proper tenant boundary scoping (`user_id`):

| Repository | Method | SQL Operation | Scoped by `user_id`? | Line Number | Isolation Status |
| :--- | :--- | :--- | :---: | :--- | :--- |
| `SqlxAccountRepository` | `create` | `INSERT INTO accounts ... VALUES (?, ?, ...)` | Yes (`?2 = user_id`) | `account_repo.rs:107-112` | Verified Isolated |
| `SqlxAccountRepository` | `find_by_id` | `SELECT ... WHERE id = ?1 AND user_id = ?2` | Yes (`?2 = user_id`) | `account_repo.rs:147-151` | Verified Isolated |
| `SqlxAccountRepository` | `list_by_user` | `SELECT ... WHERE user_id = ?1 AND ...` | Yes (`?1 = user_id`) | `account_repo.rs:168-172` | Verified Isolated |
| `SqlxAccountRepository` | `update_balance` | `UPDATE accounts SET ... WHERE id = ?3 AND user_id = ?4` | Yes (`?4 = user_id`) | `account_repo.rs:192-198` | Verified Isolated |
| `SqlxAccountRepository` | `adjust_balance_atomic` | `UPDATE accounts SET ... WHERE id = ?3 AND user_id = ?4 RETURNING ...` | Yes (`?4 = user_id`) | `account_repo.rs:220-227` | Verified Isolated |
| `SqlxAccountRepository` | `adjust_balance_atomic_tx` | `UPDATE accounts SET ... WHERE id = ?3 AND user_id = ?4 RETURNING ...` | Yes (`?4 = user_id`) | `account_repo.rs:80-87` | Verified Isolated |
| `SqlxAccountRepository` | `archive` | `UPDATE accounts SET ... WHERE id = ?2 AND user_id = ?3` | Yes (`?3 = user_id`) | `account_repo.rs:244-249` | Verified Isolated |
| `SqlxCategoryRepository` | `create` | `INSERT INTO categories ...` | Yes (`?2 = user_id`) | `category_repo.rs:56-61` | Verified Isolated |
| `SqlxCategoryRepository` | `find_by_id` | `SELECT ... WHERE id = ?1 AND (user_id = ?2 OR is_system = 1)` | Yes (`?2 = user_id`) | `category_repo.rs:91-96` | Verified Isolated |
| `SqlxCategoryRepository` | `list_by_user` | `SELECT ... WHERE (user_id = ?1 OR is_system = 1) AND deleted_at IS NULL` | Yes (`?1 = user_id`) | `category_repo.rs:107-112` | Verified Isolated |
| `SqlxCategoryRepository` | `soft_delete` | `UPDATE categories SET ... WHERE id = ?3 AND user_id = ?4 AND is_system = 0` | Yes (`?4 = user_id`) | `category_repo.rs:126-133` | Verified Isolated |
| `SqlxTransactionRepository` | `create_in_tx` | `INSERT INTO transactions ...` | Yes (`?2 = user_id`) | `transaction_repo.rs:147-153` | Verified Isolated |
| `SqlxTransactionRepository` | `find_by_id` | `SELECT ... WHERE id = ?1 AND user_id = ?2` | Yes (`?2 = user_id`) | `transaction_repo.rs:195-199` | Verified Isolated |
| `SqlxTransactionRepository` | `list` (count) | `SELECT COUNT(*) FROM transactions WHERE user_id = ?1 AND ...` | Yes (`?1 = user_id`) | `transaction_repo.rs:216-224` | Verified Isolated |
| `SqlxTransactionRepository` | `list` (fetch) | `SELECT ... FROM transactions WHERE user_id = ?1 AND ...` | Yes (`?1 = user_id`) | `transaction_repo.rs:238-248` | Verified Isolated |
| `SqlxTransactionRepository` | `delete_in_tx` | `SELECT ... WHERE id = ?1 AND user_id = ?2` then `DELETE ...` | Yes (`?2 = user_id`) | `transaction_repo.rs:275-288` | Verified Isolated |
| `SqlxTransactionRepository` | `cash_flow_summary` | `SELECT SUM(...) FROM transactions WHERE user_id = ?1 AND ...` | Yes (`?1 = user_id`) | `transaction_repo.rs:307-312` | Verified Isolated |
| `SqlxSubscriptionRepository` | `upsert_subscription` | `INSERT INTO subscriptions ... VALUES (?1, ?2, ...) ON CONFLICT DO UPDATE` | Yes (`?2 = user_id`) | `subscription_repo.rs:98-114` | Verified Isolated |
| `SqlxSubscriptionRepository` | `find_by_user_id` | `SELECT ... WHERE user_id = ?1 ORDER BY updated_at DESC LIMIT 1` | Yes (`?1 = user_id`) | `subscription_repo.rs:151-156` | Verified Isolated |
| `SqlxSubscriptionRepository` | `update_status` | `UPDATE subscriptions SET ... WHERE user_id = ?3` | Yes (`?3 = user_id`) | `subscription_repo.rs:168-173` | Verified Isolated |
| `SqlxIdempotencyRepository` | `acquire_lock` | `INSERT INTO idempotency_keys (id, user_id, ...)` / `WHERE user_id = ?1` | Yes (`user_id`) | `idempotency_repo.rs:98, 122` | Verified Isolated |
| `SqlxIdempotencyRepository` | `get_cached_response` | `SELECT ... WHERE user_id = ?1 AND idempotency_key = ?2` | Yes (`user_id`) | `idempotency_repo.rs:214-218` | Verified Isolated |
| `SqlxIdempotencyRepository` | `save_response` | `UPDATE idempotency_keys SET ... WHERE user_id = ?3` | Yes (`user_id`) | `idempotency_repo.rs:236-242` | Verified Isolated |
| `SqlxIdempotencyRepository` | `save_response_tx` | `UPDATE idempotency_keys SET ... WHERE user_id = ?3` | Yes (`user_id`) | `idempotency_repo.rs:265-271` | Verified Isolated |
| `SqlxIdempotencyRepository` | `release_lock_on_failure` | `DELETE FROM idempotency_keys WHERE user_id = ?1` | Yes (`user_id`) | `idempotency_repo.rs:290-294` | Verified Isolated |
| `SqlxAuditRepository` | `list_by_user` | `SELECT ... WHERE user_id = ?1 ORDER BY created_at DESC` | Yes (`?1 = user_id`) | `audit_repo.rs:90-95` | Verified Isolated |

### F.6 Key Database Deficiencies & Bottlenecks

1. **Email Lookup Table Scan Bottleneck (`user_repo.rs:110`)**:
   - `SELECT ... FROM users WHERE LOWER(email) = LOWER(?1)` triggers a full table scan because the index on `email` is binary collation. As the user table grows, login latency will degrade. Remediation: Add `CREATE INDEX idx_users_email_lower ON users(LOWER(email));` or declare `email TEXT NOT NULL UNIQUE COLLATE NOCASE`.
2. **Hard Deletion & Missing Ledger Audit Trail (`transaction_repo.rs:285`, `ledger_service.rs:200, 302`)**:
   - Financial transactions are physically destroyed via `DELETE FROM transactions` without writing to `audit_logs`. Remediation: Add `deleted_at TEXT` to `transactions` and inject `AuditRepository` into `LedgerService`.
3. **Foreign Key Cascade Diamond Hazard (`0001_initial_schema.sql:23, 54`)**:
   - `users` cascades to `accounts` and `transactions`, but `transactions.account_id` has `ON DELETE RESTRICT`. A cascading user deletion can fail depending on SQLite constraint evaluation order. Remediation: Defer foreign keys (`PRAGMA defer_foreign_keys = ON`) or delete child transactions explicitly.
4. **Ghost Tables (`budgets`, `goals`)**:
   - Database tables exist with composite indexes but have zero backend code. They incur cascade maintenance overhead without providing value.

---

## G. Backend Findings & API Surface Audit (34 Endpoints)

Complete audit of all 34 REST API endpoints exposed by the Axum backend (`backend/src/api/`):

| # | Method | Path | Auth Requirements | Request Payload | Response Payload | Core Business Logic | Error Handling (Status & Codes) |
|---|--------|------|-------------------|-----------------|------------------|---------------------|--------------------------------|
| 1 | `GET` | `/health` | None | None | Status: 200<br>`HealthResponse { status: "pass", service, version }` | Service liveness probe returning static status | Zero error states |
| 2 | `GET` | `/ready` | None | None | Status: 200<br>`ReadyResponse { status: "pass", database: "connected", wal_mode: bool }` | Verifies SQLite connectivity and checks `PRAGMA journal_mode == wal` | *Bug*: Returns 200 OK even if DB query fails (`.unwrap_or(false)`) |
| 3 | `GET` | `/manifest.json` | None | None | Status: 200<br>`application/manifest+json` | Reads `dist/manifest.json` from filesystem or returns built-in fallback JSON | Fallback JSON returned if file not found |
| 4 | `GET` | `/service-worker.js` | None | None | Status: 200<br>`application/javascript` | Reads `dist/sw.js` or returns embedded Workbox PWA service worker script | Fallback script returned if file not found |
| 5 | `GET` | `/` | None | None | Status: 200<br>`text/html; charset=utf-8` | Serves SPA entry HTML `index.html` from `WEB_DIST` or returns embedded shell | Fallback HTML returned if file not found |
| 6 | `GET` | `/index.html` | None | None | Status: 200<br>`text/html; charset=utf-8` | Explicit alias for `/` serving SPA entry HTML | Fallback HTML returned if file not found |
| 7 | `GET` | `/payment/success` | None | None | Status: 200<br>`text/html; charset=utf-8` | Static HTML redirect page showing success checkmark and redirecting to `/` in 4s | Zero error states |
| 8 | `POST` | `/api/v1/auth/register` | None (Unthrottled in prod) | JSON: `RegisterRequest { email, password, display_name }` | Status: 201<br>Headers: `Set-Cookie: auth_token=...; HttpOnly; SameSite=Lax`<br>JSON: `AuthResponse { user, token, permissions }` | Normalizes email, validates password strength (min 8 chars, 1 letter, 1 number), hashes via Argon2id, creates User, seeds Cash account and 10 categories in transaction, issues JWT | 400 `INVALID_EMAIL_FORMAT`<br>400 `PASSWORD_TOO_SHORT`<br>400 `PASSWORD_TOO_WEAK`<br>409 `EMAIL_ALREADY_EXISTS`<br>500 `INTERNAL_ERROR` |
| 9 | `POST` | `/api/v1/auth/login` | None (Unthrottled in prod) | JSON: `LoginRequest { email, password }` | Status: 200<br>Headers: `Set-Cookie: auth_token=...; HttpOnly; SameSite=Lax`<br>JSON: `AuthResponse { user, token, permissions }` | Case-insensitive email lookup. Verifies Argon2id hash. If not found, runs dummy Argon2 verification to defeat user enumeration. Issues JWT | 400 `INVALID_EMAIL_FORMAT`<br>401 `INVALID_CREDENTIALS`<br>500 `INTERNAL_ERROR` |
| 10 | `POST` | `/api/v1/auth/logout` | None | None | Status: 200<br>Headers: `Set-Cookie: auth_token=; Max-Age=0`<br>JSON: `LogoutResponse { status: "success", message }` | Clears `auth_token` cookie by setting expiration in 1970 | 500 `INTERNAL_ERROR` |
| 11 | `GET` | `/api/v1/auth/me` | Valid JWT (Cookie or Bearer) | None | Status: 200<br>JSON: `UserProfileResponse { user, permissions }` | Extracts user ID from JWT, fetches current user record, calculates active permissions based on `subscription_tier` | 401 `AUTH_TOKEN_MISSING`<br>401 `AUTH_TOKEN_EXPIRED`<br>404 `USER_NOT_FOUND` |
| 12 | `GET` | `/api/v1/accounts` | Valid JWT | None | Status: 200<br>JSON: `Vec<Account>` | Lists all non-archived multi-wallet accounts owned by authenticated user | 401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 13 | `POST` | `/api/v1/accounts` | Valid JWT | JSON: `CreateAccountRequest { name, account_type, currency, initial_balance, color, icon }` | Status: 201<br>JSON: `Account` | Validates non-empty name, sets initial and current balances to `initial_balance` (default 0), inserts into `accounts` | 400 `INVALID_NAME`<br>401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 14 | `GET` | `/api/v1/accounts/{id}` | Valid JWT | Path: `id` | Status: 200<br>JSON: `Account` | Fetches account by ID scoped by `WHERE id = ?1 AND user_id = ?2` | 401 `AUTH_TOKEN_MISSING`<br>404 `NOT_FOUND` |
| 15 | `POST` | `/api/v1/accounts/{id}/archive` | Valid JWT | Path: `id` | Status: 204 No Content | Soft-archives account (`SET is_archived = 1`) enforcing user ownership | 401 `AUTH_TOKEN_MISSING`<br>404 `NOT_FOUND` |
| 16 | `GET` | `/api/v1/categories` | Valid JWT | None | Status: 200<br>JSON: `Vec<Category>` | Lists active categories (`deleted_at IS NULL`) for user (`user_id = ? OR is_system = 1`) sorted by system first then name | 401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 17 | `POST` | `/api/v1/categories` | Valid JWT | JSON: `CreateCategoryRequest { name, category_type, icon, color }` | Status: 201<br>JSON: `Category` | Validates non-empty name and `category_type` in ('income', 'expense'). Creates user category (`is_system = 0`) | 400 `INVALID_NAME`<br>400 `INVALID_TYPE`<br>401 `AUTH_TOKEN_MISSING` |
| 18 | `DELETE` | `/api/v1/categories/{id}` | Valid JWT | Path: `id` | Status: 204 No Content | Soft-deletes user category (`SET deleted_at = now`). Rejects deletion of system categories | 401 `AUTH_TOKEN_MISSING`<br>403 `CANNOT_DELETE_SYSTEM_ENTITY`<br>404 `NOT_FOUND` |
| 19 | `GET` | `/api/v1/transactions` | Valid JWT | Query: `TransactionFilter { account_id, category_id, transaction_type, date_from, date_to, page, per_page }` | Status: 200<br>JSON: `TransactionListResponse { data: Vec<TransactionRecord>, meta: PaginationMeta }` | Queries paginated transactions scoped by `user_id` with dynamic filters. Computes total count and total pages | 401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 20 | `POST` | `/api/v1/transactions` | Valid JWT | Header: `Idempotency-Key`<br>JSON: `CreateTxPayload { account_id, to_account_id, category_id, transaction_type, amount, date, description, notes, is_recurring }` | Status: 201<br>JSON: `TransactionResponse { transaction, account_balance }` | Acquires idempotency lock. If cached, returns replay. In transaction: inserts record, applies atomic balance adjustment via `UPDATE ... RETURNING`, caches response, commits | 400 `INVALID_TRANSFER`<br>400 `VALIDATION_FAILED`<br>401 `AUTH_TOKEN_MISSING`<br>404 `NOT_FOUND`<br>409 `IDEMPOTENCY_DUPLICATE`<br>409 `IDEMPOTENCY_IN_PROGRESS` |
| 21 | `GET` | `/api/v1/transactions/{id}` | Valid JWT | Path: `id` | Status: 200<br>JSON: `TransactionRecord` | Retrieves transaction by ID enforcing user isolation | 401 `AUTH_TOKEN_MISSING`<br>404 `NOT_FOUND` |
| 22 | `DELETE` | `/api/v1/transactions/{id}` | Valid JWT | Path: `id` | Status: 204 No Content | Deletes transaction in transaction block and atomically reverses balance delta on account (and destination account if transfer) | 401 `AUTH_TOKEN_MISSING`<br>404 `NOT_FOUND` |
| 23 | `GET` | `/api/v1/dashboard` | Valid JWT | None | Status: 200<br>JSON: `DashboardResponse { total_balance, cash_flow, accounts, recent_transactions, tier }` | Aggregates all accounts balance sum, queries `cash_flow_summary()`, and fetches top 5 recent transactions | 401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 24 | `GET` | `/api/v1/analytics/basic` | Valid JWT | None | Status: 200<br>JSON: `BasicAnalyticsResponse { cash_flow, savings_rate_percent, tier }` | Computes cash flow summary and savings rate percentage: `(net_cash_flow * 100) / total_income` | 401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 25 | `GET` | `/api/v1/analytics/advanced` | Valid JWT + Premium/Trial Tier | None | Status: 200<br>JSON: `AdvancedAnalyticsResponse { cash_flow, savings_rate_percent, financial_health_score, monthly_trend, tier, message }` | Feature gate: verifies `analytics.advanced`. Computes health score clamped `(savings_rate * 8 / 10) + 20` | 401 `AUTH_TOKEN_MISSING`<br>403 `FEATURE_LOCKED`<br>500 `DATABASE_ERROR` |
| 26 | `POST` | `/api/v1/webhooks/midtrans` | Cryptographic Signature (SHA-512) | JSON: `MidtransNotification { order_id, status_code, gross_amount, signature_key, transaction_status, fraud_status, custom_field1 }` | Status: 200<br>JSON: `WebhookProcessingResult` | Verifies `SHA512(order_id + status_code + gross_amount + server_key)` in constant time. Deduplicates in `webhook_events`. Transitions user to Premium if `settlement`/`capture` | 400 `INVALID_PAYLOAD`<br>401 `INVALID_SIGNATURE`<br>404 `USER_NOT_FOUND` |
| 27 | `POST` | `/api/v1/webhooks/xendit` | Callback Token (`x-callback-token`) | Header: `x-callback-token`<br>JSON: `XenditNotification { id, event, external_id, user_id, status, amount }` | Status: 200<br>JSON: `WebhookProcessingResult` | Verifies callback token in constant time. Deduplicates event. Transitions user to Premium if `SUCCEEDED`/`ACTIVE`/`PAID` | 400 `INVALID_PAYLOAD`<br>401 `INVALID_SIGNATURE`<br>404 `USER_NOT_FOUND` |
| 28 | `POST` | `/api/v1/webhooks/dana` | Cryptographic Signature (RSA-SHA256 SNAP) | Headers: `X-TIMESTAMP`, `X-SIGNATURE`<br>JSON: `DanaNotification` | Status: 200 / 401 / 400<br>JSON: `DanaAckResponse { responseCode, responseMessage }` | Verifies RSA-SHA256 signature against String-to-Sign: `{method}:{path}:{sha256_hex}:{timestamp}`. Deduplicates in `webhook_events`. Transitions user to Premium (+30 days) | 401 `4015600` (bad signature)<br>400 `4005600` (bad JSON)<br>200 `2005600` (success) |
| 29 | `POST` | `/api/v1/webhooks/dana/disburse` | None | None | Status: 200<br>JSON: `DanaAckResponse { responseCode: "2005600", responseMessage: "Successful" }` | Stub endpoint required by DANA developer dashboard for disbursement callback acknowledgement | Zero error states |
| 30 | `GET` | `/api/v1/subscription` | Valid JWT | None | Status: 200<br>JSON: `{ tier, status, is_premium, days_remaining, current_period_end, plan_id, amount, price_monthly, subscription, features }` | Fetches active subscription. Lazily evaluates trial expiration: if expired, transitions user to `free` and subscription to `expired` without data loss | 401 `AUTH_TOKEN_MISSING`<br>500 `DATABASE_ERROR` |
| 31 | `GET` | `/api/v1/subscriptions/status` | Valid JWT | None | Same as `/api/v1/subscription` | Plural alias for subscription status | Same as `/api/v1/subscription` |
| 32 | `POST` | `/api/v1/subscriptions/checkout` | Valid JWT | JSON: `CheckoutRequest { provider: "dana" \| "midtrans" \| "xendit", plan_id }` | Status: 200<br>JSON: `CheckoutSession { order_id, provider, amount: 5000, currency: "IDR", checkout_url, reference_no }` | Dispatches provider checkout. For DANA: calls live DANA Open API sandbox endpoint `POST /v1.0/debit/payment-host-to-host.htm` signed with RSA private key to get live payment URL | 400 `UNSUPPORTED_PROVIDER`<br>400 `INVALID_PAYLOAD`<br>401 `AUTH_TOKEN_MISSING` |
| 33 | `POST` | `/api/v1/subscriptions/trial` | Valid JWT | None | Status: 200<br>Headers: `Set-Cookie: auth_token=...; HttpOnly; SameSite=Lax`<br>JSON: `TrialActivationResult { status: "trialing", tier: "premium", is_premium: true, trial_started_at, trial_ends_at, days_remaining: 7, message, subscription }` | Validates `has_used_trial == 0`. In transaction: sets `has_used_trial = 1`, `subscription_tier = 'premium'`, inserts trial subscription row. Re-issues JWT with `tier = "premium"` in `Set-Cookie` | 400 `TRIAL_ALREADY_USED`<br>400 `ALREADY_PREMIUM`<br>401 `AUTH_TOKEN_MISSING` |
| 34 | `POST` | `/api/v1/subscription/trial` | Valid JWT | None | Same as `/api/v1/subscriptions/trial` | Singular alias for trial activation | Same as `/api/v1/subscriptions/trial` |

---

## H. Frontend Findings, State Architecture & UX Gaps

### H.1 Routing Architecture (Tab-Based Switching vs. Vue Router)
- **Status (FACT)**: The application does not use `vue-router`. Navigation between views (Home, Transactions, Analytics, Profile) is implemented via reactive tab state in `App.vue:115` (`const currentTab = ref('home')`).
- **Impact & Risk (HIGH RISK)**:
  1. Deep linking is impossible: users cannot bookmark or directly share URLs like `/transactions` or `/analytics`.
  2. Browser refresh always resets the view to the Home dashboard (`currentTab.value = 'home'`).
  3. Browser history navigation (`window.history.back()`) does not function. On mobile devices, triggering hardware back or gesture back navigates away from the PWA rather than to the previous tab.
  4. Standard navigation guards (`beforeEach`) are absent; auth protection relies on conditional component mounting (`v-if="!authStore.isAuthenticated"`).

### H.2 State Architecture: The Orphan Transaction Store
- **Status (FACT)**: A complete 213-line Pinia store exists at `frontend/src/stores/transactions.js` with reactive state (`transactions`, `recentTransactions`, `meta`, `filters`, `loading`, `submitting`) and action methods (`fetchTransactions`, `createTransaction`, `deleteTransaction`, `setFilter`).
- **Audit Finding**: `TransactionsView.vue` and `AddTransactionModal.vue` **never import or use this store**. Instead:
  - `TransactionsView.vue:132-168` creates its own local `ref([])` state and calls `api.getTransactions()`.
  - `AddTransactionModal.vue:160, 270` directly calls `api.createTransaction()`.
  - `App.vue:148-151` uses an imperative template ref (`transactionsViewRef.value?.reload()`) to refresh the transaction list when a transaction is added.
- **Impact (HIGH RISK)**: Dual sources of truth, dead code in production bundle, and fragile cross-component event chains.

### H.3 Dead Component & Dependency Inventory
1. **`Card.vue` (`src/components/ui/Card.vue`, 127 lines)**: Exported in `ui/index.ts:8`, but grep across `frontend/src` confirms **0 usages** in views. All screens construct cards using raw `<div>` with `.fintech-card` classes.
2. **`ModalSheet.vue` (`src/components/ui/ModalSheet.vue`, 287 lines)**: Exported in `ui/index.ts:14`, but grep confirms **0 usages**. Implements accessible swipe-down dismiss, body scroll locking, and focus trapping. Yet `AddTransactionModal.vue` and `UpgradeModal.vue` create ad-hoc fixed overlay divs from scratch, losing accessibility guarantees.
3. **`chart.js` and `vue-chartjs` (`package.json:12,16`)**: Installed in dependencies, but grep across `frontend/src` returns **0 matches**. Unused dependencies bloating `node_modules`.
4. **Hardcoded Runway Projection (`AnalyticsView.vue:89-101`)**: Displays static text `6.2 Bulan` and width `w-[65%]` instead of computing runway dynamically from `analyticsStore`.

### H.4 PWA Offline Strategy & Manifest Conflicts
1. **Offline Launch Auth Lockout (`App.vue:173-177`)**: On PWA launch while offline, `App.vue` runs `authStore.checkAuth()`. Because Workbox routes `/api/v1/*` as `NetworkOnly`, the fetch fails. `checkAuth()` catches the error, sets `user.value = null`, and redirects the user to `SplitScreenAuth.vue`. The user cannot view cached balances or transactions offline.
2. **Dual Manifest Conflict**: `index.html:10` manually links `/manifest.json`, while VitePWA automatically generates and injects `/manifest.webmanifest`. Different browsers resolve conflicting metadata.
3. **Missing Assets**: `vite.config.js:16` references `favicon.ico`, but the file is missing from `frontend/public/`. Icons lack `"purpose": "maskable any"`.

### H.5 Styling, Design Tokens & Theming
1. **Dormant Dark Mode**: `src/index.css:62-115` contains 54 lines of `.dark` CSS tokens, and `tailwind.config.js:7` sets `darkMode: "class"`. However, **zero UI controls, zero class toggles, and zero local storage persistence exist**. The application runs strictly in light mode.
2. **Missing Web Fonts**: `tailwind.config.js:118-127` specifies `"Plus Jakarta Sans"` as primary font, but `index.html` loads no Google Fonts link or local font stylesheets, falling back to system fonts.

---

## I. DevOps, Infrastructure & Edge Deployment Findings

### I.1 Monorepo CI/CD Collision Risk
- **Repository Location**: `/home/nurdiansyah/dev/Personal_project/.github/workflows/deploy.yml`
- **Hazard (CRITICAL RISK / FACT)**: The existing production workflow for `nurdiansyahlabs-react` triggers on every push to `main` with **no path filters**:
  ```yaml
  on:
    push:
      branches: [ main ]
    workflow_dispatch:
  ```
  It executes `npm run build` in root (expecting React), copies `api/*` to `dist/api`, and uploads `dist/` via FTP to production cPanel shared hosting.
- If Personal Finance PWA code is pushed or merged without path filters, it will immediately trigger this cPanel deployment workflow. Because cPanel shared hosting runs an Apache/PHP stack, it cannot execute compiled Tokio/Axum binary daemons.

### I.2 Edge Hardware Profile: Samsung Galaxy A20s (`SM-A207F`)
The backend daemon runs on physical smartphone hardware:
- **Processor**: Qualcomm Snapdragon 450 (8x ARM Cortex-A53 @ 1.80 GHz). Argon2id password hashing consumes ~400–800ms of CPU per verification. Four concurrent logins saturate 50% of CPU capacity.
- **Memory & Storage**: 3 GB LPDDR3 RAM, 32 GB eMMC 5.1 storage. Chroot environment operates at `/data/local/ubuntu-rootfs`. Headroom is ~700 MB.
- **Compilation**: Cross-compiled on workstation via `cargo zigbuild --target aarch64-unknown-linux-gnu.2.31 --release`.
- **Ingress**: Managed via persistent outbound HTTPS connection by `/system/bin/cloudflared` (PID 9105) mapping to `127.0.0.1:8080` under domain `api.nurdiansyahlabs.com`.
- **SELinux**: `Enforcing` on Android 11 host. Asset pushes must be staged through `/data/local/tmp` using root shell (`su -c`).

### I.3 Process Supervision & Volatile Logging
- In `start.sh:18`, the backend process is started via:
  ```bash
  nohup ./backend > /tmp/personal_finance_pwa.log 2>&1 &
  ```
- **Defects (HIGH RISK)**:
  1. No supervisor (`systemd`, `supervisord`, `runit`) exists. A kernel panic, thermal reboot, or Low Memory Killer (`lmkd`) termination leaves the service offline.
  2. Deployment commands use `pkill -9 backend`, abruptly terminating active connections without flushing SQLite WAL caches or releasing locks.
  3. Output logs pipe to `/tmp` (mounted on RAM-based `tmpfs` on Android/Linux), erasing logs on every device reboot.

### I.4 Disaster Recovery & Database Durability (Single Point of Failure)
- **Defect (CRITICAL RISK / FACT)**: All production ledger data resides exclusively in `data/personal_finance.db` on mobile eMMC flash storage.
- **Zero Backups**: There is no Litestream continuous streaming replication, no AWS S3 / Cloudflare R2 backup cron, and no `VACUUM INTO` snapshot script. A mobile hardware failure causes **100% permanent data loss**.

### I.5 False-Positive Readiness Probe Bug (`api/health.rs:44-63`)
```rust
let wal = sqlx::query_scalar::<_, String>("PRAGMA journal_mode;")
    .fetch_one(&pool)
    .await
    .map(|s| s.to_lowercase() == "wal")
    .unwrap_or(false);

(StatusCode::OK, headers, Json(ReadyResponse { status: "pass", database: "connected", wal_mode: wal }))
```
- If the SQLite query fails (due to database lock timeout or corrupted file), `wal` evaluates to `false`, but the handler returns `StatusCode::OK` (HTTP 200). Uptime monitors and load balancers receive false-positive healthy status when the database is completely broken.

---

## J. Testing Infrastructure, Maturity & Coverage Gaps

### J.1 Testing Hierarchy & Test Inventory

| Layer / Track | Framework / Harness | Test Count | Pass Rate | Execution Speed | Maturity Level |
|---|---|:---:|:---:|:---:|:---:|
| **Backend Unit & Integration** | Rust `cargo test --workspace` | 108 | 100% (108/108) | ~1.8s | **Advanced / High** |
| **Opaque-Box E2E Suite** | Python TAP v13 (`runner.sh`) | 344 | 100% (344/344) | ~2.1s | **Advanced / High** |
| **Frontend Component & Unit** | None (Vitest/Jest absent) | 0 | N/A | N/A | **Deficient / Low** |
| **Frontend Adversarial Harness** | Node.js (`adversarial_challenge.mjs`) | Script | 100% (115 assertions) | ~0.5s | **Moderate** |
| **Frontend Production Build** | Vite Build (`npm run build`) | 1 | PASS (0 errors) | ~1.2s | **Production Grade** |

### J.2 Rust Backend Test Suite (108/108 Tests Passing)
Covers PRAGMA verifications, integer overflow boundaries, negative money amounts, transaction atomicity, cross-tenant isolation, 7-day trial lifecycle, Argon2id password hashing, JWT token lifecycle, DANA RSA-SHA256 signature verification, Midtrans HMAC-SHA512 verification, and feature gating.

### J.3 E2E Test Suite & Test Runner Masking Defect
- Master runner `e2e_tests/run_all.py` executes 344 tests in TAP-13 format across 4 tiers:
  - Tier 1 (Feature Coverage): 155 test cases
  - Tier 2 (Boundary & Corner): 145 boundary test cases
  - Tier 3 (Cross-Feature Pairwise): 29 integration test cases
  - Tier 4 (Real-World Scenarios): 15 realistic user workflows
- **Critical Finding (HIGH RISK)**: `e2e_tests/runner.sh:61-63` checks whether `http://127.0.0.1:8080/health` is responsive. If unreachable, it boots `python3 e2e_tests/harness/server.py 8089 &` (an embedded 1,323-line Python reference mock server) and runs the entire suite against the mock server. If the Rust backend is down or fails to compile, `runner.sh` reports 100% pass, masking real outages.

### J.4 Frontend Testing Deficit
- `frontend/package.json` contains no `"test"` script.
- Zero unit tests exist for Pinia stores (`auth.js`, `wallets.js`, `subscription.js`, `analytics.js`).
- `frontend/tests/adversarial_challenge.mjs:6` imports `happy-dom`, but `happy-dom` is omitted from `package.json` dependencies, causing clean CI builds (`npm ci`) to fail.

---

## K. Technical Debt Register

Prioritized technical debt register categorized by severity:

| ID | Severity | Category | Location | Issue Description | Impact | Recommended Action |
|:---:|:---:|---|---|---|---|---|
| **TD-01** | `CRITICAL` | Security / Transport | `backend/src/api/mod.rs:83` | `create_app` mounts unthrottled `auth::auth_routes`. | Unprotected login and registration endpoints open to credential stuffing. | Pass `Arc<SlidingWindowRateLimiter>` into `create_app` and mount `auth_routes_with_rate_limiter`. |
| **TD-02** | `CRITICAL` | Security / Credentials | `start.sh:7-16` | Plaintext RSA-2048 private key, client secrets, and JWT secret tracked in script. | Immediate credential compromise upon repository exposure. | Purge credentials from `start.sh`, source from untracked `.env`, rotate DANA keys. |
| **TD-03** | `CRITICAL` | Data Integrity | `backend/src/service/ledger_service.rs:154` | `acquire_lock` passes `request_hash = ""`. | Idempotency payload mismatch detection is completely disabled. | Compute SHA-256 of request payload and pass hex digest to `acquire_lock`. |
| **TD-04** | `CRITICAL` | Infrastructure / DR | Galaxy A20s Edge Storage | Zero automated SQLite backups or replication. | Hardware fault or flash wear causes 100% catastrophic data loss. | Install Litestream replicating WAL stream to Cloudflare R2 / S3. |
| **TD-05** | `CRITICAL` | CI/CD Safety | Target `deploy.yml:3-8` | Target cPanel deployment workflow lacks path filtering. | Any push touching finance code triggers accidental production cPanel FTP upload. | Add `paths:` filter to target `deploy.yml`. |
| **TD-06** | `HIGH` | Client Architecture | `frontend/src/App.vue:115` | Tab-switching state (`currentTab = ref('home')`) instead of Vue Router. | Deep linking impossible; browser back button broken; page refresh resets to Home. | Install `vue-router@4` and establish standard SPA routes and guards. |
| **TD-07** | `HIGH` | State Architecture | `frontend/src/views/TransactionsView.vue` | Bypasses `src/stores/transactions.js` and duplicates fetch logic. | Dual source of truth, 213 lines of dead store code, fragile event chains. | Refactor `TransactionsView` and `AddTransactionModal` to consume `useTransactionStore`. |
| **TD-08** | `HIGH` | PWA / Offline | `frontend/src/App.vue:173-177` | Offline boot wipes user session on failed `/auth/me` network fetch. | App shell cannot be opened offline; locks user out to login screen. | Cache auth session in localStorage/IndexedDB for offline read access. |
| **TD-09** | `HIGH` | Security / Session | `backend/src/service/jwt.rs` | 15-minute JWT expires abruptly with no refresh token endpoint. | Users abruptly logged out mid-session every 15 minutes. | Implement sliding session renewal or `POST /api/v1/auth/refresh`. |
| **TD-10** | `HIGH` | Security / Lifecycle | `backend/src/service/payment_service.rs:1091` | Lazy trial expiration evaluated only on subscription screen. | Trial users who skip subscription screen retain permanent Pro access. | Re-evaluate trial expiry during login and token extraction. |
| **TD-11** | `HIGH` | Data Integrity / Webhook | `backend/src/service/payment_service.rs:751-786` | Webhook settlement updates executed as uncoordinated queries without `pool.begin()`. | Crash between queries desynchronizes subscription tier from payment state. | Enclose webhook settlement operations in a database transaction block. |
| **TD-12** | `HIGH` | Data Integrity / Audit | `backend/src/repository/transaction_repo.rs:285` | `delete_in_tx` executes physical `DELETE` without writing to `audit_logs`. | Irreversible destruction of financial records; zero audit trail. | Add `deleted_at TEXT` to `transactions` and record deletion in `audit_logs`. |
| **TD-13** | `MEDIUM` | Performance | `backend/src/repository/user_repo.rs:110` | `WHERE LOWER(email) = LOWER(?1)` triggers full table scan on `users`. | Auth query latency degrades linearly with user table growth. | Add functional index `CREATE INDEX idx_users_email_lower ON users(LOWER(email));`. |
| **TD-14** | `MEDIUM` | Observability | `backend/src/api/health.rs:44-63` | `/ready` probe returns HTTP 200 even on database query failure. | Uptime monitors receive false-positive healthy status during DB outages. | Return HTTP 503 Service Unavailable when SQLite query fails. |
| **TD-15** | `MEDIUM` | Security / Headers | `backend/src/api/mod.rs:82-99` | Missing security headers (`X-Frame-Options`, `X-Content-Type-Options`, HSTS). | Leaves frontend vulnerable to Clickjacking and MIME-confusion attacks. | Inject standard security headers via Axum response middleware. |
| **TD-16** | `MEDIUM` | Dead Code / Components | `frontend/src/components/ui/Card.vue` | `Card.vue` (127 lines) and `ModalSheet.vue` (287 lines) are 100% unused in views. | Code bloat; loss of accessible focus trapping in modals. | Adopt `ModalSheet.vue` in modals and `Card.vue` in views. |
| **TD-17** | `MEDIUM` | Missing Features | `UpgradeModal.vue:90,96` | Budgets, goals, and encrypted CSV/PDF export advertised but unbuilt. | User friction and broken SaaS promises. | Build backend modules and UI for budgeting, goals, and export. |
| **TD-18** | `LOW` | Dependencies | `frontend/package.json:12,16` | `chart.js` and `vue-chartjs` installed but have 0 imports in `src/`. | Unnecessary bundle and node_modules footprint. | Either render charts in `AnalyticsView` or remove dependencies. |
| **TD-19** | `LOW` | Asset Hygiene | `frontend/vite.config.js:16` | Missing `favicon.ico` in `public/` despite reference in config. | Browser requests to `/favicon.ico` return 404 Not Found. | Provide 32x32 `favicon.ico` in `frontend/public/`. |
| **TD-20** | `LOW` | Testing Dependency | `frontend/tests/adversarial_challenge.mjs:6` | `happy-dom` imported in test script but omitted from `package.json`. | Clean CI installs (`npm ci`) fail with module not found. | Add `"happy-dom": "^15.11.7"` to `devDependencies`. |

---

## L. Architectural Risk Register

Prioritized Architectural Risk Register evaluating severity, probability, impact, and mitigation:

| Risk ID | Title / Architectural Risk | Severity | Probability | Impact | Architectural Mitigation Strategy |
|:---:|---|:---:|:---:|:---:|---|
| **ARCH-01** | **Single Point of Failure on Edge Hardware Without Backup** | **CRITICAL** | **HIGH** | **CATASTROPHIC** | Implement **Litestream** real-time continuous replication of `personal_finance.db` to Cloudflare R2 or AWS S3. Deploy an automated cron executing `VACUUM INTO` daily snapshots. |
| **ARCH-02** | **Production Axum Router Bypasses Rate Limiting** | **CRITICAL** | **HIGH** | **HIGH** | Modify `backend/src/api/mod.rs:83` and `main.rs:141` to pass `Arc<SlidingWindowRateLimiter>` into `create_app` and mount `auth_routes_with_rate_limiter` on `/login` and `/register`. |
| **ARCH-03** | **Plaintext Private Key & Production Secrets in Git (`start.sh`)** | **CRITICAL** | **HIGH** | **HIGH** | Remove all credentials from `start.sh`. Load from an uncommitted `.env` file. Add `.gitignore` to repository root. Immediately rotate DANA RSA key pairs, merchant secrets, and `JWT_SECRET`. |
| **ARCH-04** | **Idempotency Payload Hash Bypassed with Empty String** | **CRITICAL** | **HIGH** | **HIGH** | In `backend/src/service/ledger_service.rs:154`, compute `sha256(serde_json::to_string(&request))` and pass the resulting hexadecimal digest into `acquire_lock` instead of `""`. |
| **ARCH-05** | **Permanent Premium Privilege Retention via Lazy Trial Expiry** | **HIGH** | **HIGH** | **MEDIUM** | In `AuthService::login` and `AuthService::get_me`, evaluate `trial_ends_at` against `Utc::now()`. If expired, atomically downgrade user to `free` before issuing tokens. Implement background periodic cleanup task. |
| **ARCH-06** | **Unmanaged Process Execution via `nohup` & Hard `pkill -9`** | **HIGH** | **HIGH** | **HIGH** | Implement a lightweight supervisor inside chroot (`supervisord` or POSIX watchdog daemon with auto-respawn). Replace `pkill -9` with `SIGTERM` allowing Tokio and SQLx graceful shutdown. |
| **ARCH-07** | **15-Minute Short JWT Expiration Without Refresh Mechanism** | **HIGH** | **HIGH** | **MEDIUM** | Introduce a dedicated refresh token endpoint (`POST /api/v1/auth/refresh`) or sliding session mechanism in `auth_extractor`, and configure frontend `api.js` with an automated 401 token refresh interceptor. |
| **ARCH-08** | **PWA Offline Lockout on App Launch** | **HIGH** | **HIGH** | **MEDIUM** | In `frontend/src/stores/auth.js:36-49`, inspect error types in `checkAuth()`. If the error is a network/offline error, preserve existing cached session in `localStorage` / `Pinia` and allow offline read access. |
| **ARCH-09** | **SQLite Single Writer Lock Contention Under Concurrent Traffic** | **MEDIUM** | **MEDIUM** | **HIGH** | Separate SQLite reader pool (`max_connections = 10`) from dedicated single-connection writer pool (`max_connections = 1`). Configure `PRAGMA busy_timeout = 10000;`, `PRAGMA temp_store = MEMORY;`, and `PRAGMA cache_size = -64000`. |
| **ARCH-10** | **Non-Transactional Payment Webhook Settlement** | **MEDIUM** | **MEDIUM** | **HIGH** | Wrap `PaymentService::handle_dana_notification` in a single `sqlx::Transaction` (`pool.begin()`). Ensure subscription upsert, tier update, webhook status, and audit log succeed or fail atomically. |
| **ARCH-11** | **Absence of Vue Router (Tab-Based State Navigation)** | **MEDIUM** | **HIGH** | **LOW** | Install `vue-router` (`npm i vue-router@4`). Implement standard SPA routes (`/`, `/transactions`, `/analytics`, `/profile`, `/auth`) with navigation guards, browser history, and deep-link support. |
| **ARCH-12** | **Target Monorepo Deployment Pipeline Collision** | **CRITICAL** | **HIGH** | **HIGH** | In `/home/nurdiansyah/dev/Personal_project/.github/workflows/deploy.yml`, add strict `paths:` filters restricting triggers to React application files. Never trigger cPanel FTP deploys on Rust or Finance files. |

---

## M. What Should NOT Be Changed (The PRESERVE List)

To protect system stability, the following existing, verified components must be **strictly preserved**:

1. **Rust Domain Money Value Object (`backend/src/domain/money.rs`)**:
   - `pub struct Rupiah(pub i64)` signed 64-bit integer minor unit representation.
   - Strict prohibition of floating-point conversions (`f32`/`f64`).
   - Checked arithmetic methods: `checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_mul_bps`.
   - Serde deserializer that strictly rejects fractional floating-point numbers.
   - Formatter `format_idr()` safely handling `i64::MIN` via absolute math.
2. **Net Cash Flow Engine (`backend/src/repository/transaction_repo.rs:41-60`)**:
   - Deterministic formula: `net_cash_flow = total_income - total_expenses`.
   - Transfer net-zero wealth invariance (transfers excluded from income and expenses).
3. **Atomic Account Balance Maintenance (`backend/src/repository/account_repo.rs:69-96`)**:
   - Atomic SQL delta: `UPDATE accounts SET current_balance = current_balance + ?1 WHERE id = ?2 AND user_id = ?3 RETURNING current_balance;`.
   - Strict encapsulation within `sqlx::Transaction` blocks in `ledger_service.rs`.
4. **Database Engine & PRAGMA Settings (`backend/src/repository/db.rs:50-65`)**:
   - `PRAGMA journal_mode = WAL;`
   - `PRAGMA busy_timeout = 5000;`
   - `PRAGMA foreign_keys = ON;`
   - `PRAGMA synchronous = NORMAL;`
   - Composite indexing strategy on `(user_id, date)` and `(account_id)`.
5. **Multi-Tenant Query Scoping (`backend/src/repository/*_repo.rs`)**:
   - Consistent `WHERE user_id = ?` scoping on all SQLx repository queries.
6. **Authentication Cryptographic Security (`backend/src/service/crypto.rs`)**:
   - Argon2id password hashing parameters: 64 MiB memory (`m_cost = 65536`), 3 iterations (`t_cost = 3`), 1 lane (`p_cost = 1`).
   - Offloading hashing and verification to Tokio worker threads via `tokio::task::spawn_blocking`.
   - Constant-time dummy verification (`verify_or_dummy`) to defeat username enumeration timing attacks.
   - Password strength validator (minimum 8 chars, 1 letter, 1 number).
7. **Payment Webhook Cryptography (`backend/src/service/payment_service.rs`)**:
   - DANA SNAP RSA-SHA256 PKCS#1 v1.5 signature verification logic.
   - Midtrans SHA-512 constant-time HMAC validation.
   - Xendit callback token constant-time validation.
   - Webhook event deduplication via `webhook_events` table enforcing `UNIQUE(provider, event_id)`.
8. **Frontend Brand Identity & SplitScreenAuth (`frontend/src/components/auth/SplitScreenAuth.vue`)**:
   - Authentic brand logo asset `/icons/Invinite_Logo.png`.
   - 50/50 responsive desktop split-screen layout with fintech dark showcase and demo logins.
9. **Dual-Mode Layout Shell (`frontend/src/components/layout/`)**:
   - `DesktopSidebar.vue` (for `md:` screens $\ge 768$px) and `MobileBottomNav.vue` (for mobile screens $< 768$px).
   - Safe-area inset handling (`pb-safe`) for edge-to-edge mobile devices.
10. **POS Numeric Keypad Interaction (`frontend/src/views/AddTransactionModal.vue:120-138`)**:
    - 4x3 rapid numeric keypad with `navigator.vibrate(10)` tactile feedback.
11. **Iconography & Styling Tokens**:
    - 100% Lucide SVG icons (`lucide-vue-next`); zero raw emojis.
    - Localized currency formatter `frontend/src/utils/currency.js` utilizing `Intl.NumberFormat('id-ID')`.
12. **Target Repository Production cPanel Baseline**:
    - Existing deployment behavior of `nurdiansyahlabs-react` via `deploy.yml` for the React website must remain fully intact.

---

## N. What Should Be Changed & Target Architecture

### N.1 Target High-Level Architecture (Production Standard)

```text
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                              TARGET CLIENT PWA ARCHITECTURE                            │
│  • Vue 3 Composition API + Vue Router 4 (Deep linking, history, navigation guards)     │
│  • Pinia Store Layer (Consolidated, zero orphan stores, reactive state binding)        │
│  • Workbox Offline Mode with IndexedDB / LocalStorage Auth Session Persistence         │
│  • Shared UI Primitives: ModalSheet.vue for all sheets, Card.vue for all cards         │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            │ HTTPS (TLS 1.3 / HTTP/3)
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                         INGRESS & API GATEWAY / REVERSE PROXY                          │
│  • Cloudflare Edge (WAF, Rate Limiting, DDoS, Strict Cache-Control, TLS Termination)   │
│  • Cloudflare Tunnel (cloudflared daemon) ──► 127.0.0.1:8080                           │
│  • Reverse Proxy Header Validation (Trust only loopback & Cloudflare CIDRs)            │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            │ Local Socket (127.0.0.1:8080)
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                       CONSOLIDATED RUST AXUM BACKEND SERVICE                           │
│                                                                                        │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │ Middlewares: Tower Tracing + SlidingWindowRateLimiter + SecurityHeaders + CORS  │  │
│  └──────────────────────────────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │ API Layer: Slim handlers, DTO validation, RFC 7807 Problem Details               │  │
│  │ Endpoints: Auth (with Refresh), Accounts, Categories, Tx, Analytics, Subscriptions│  │
│  │ NEW: Budgets (/api/v1/budgets), Goals (/api/v1/goals), Export (/api/v1/export)   │  │
│  └──────────────────────────────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │ Service Layer: Strict Transactional Boundaries (pool.begin() in Webhooks & Ledger)│  │
│  │ SHA-256 Request Hash in Idempotency, Active Trial Expiration on Login             │  │
│  └──────────────────────────────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │ Pure Domain Layer: Rupiah(i64) checked math, unified Net Cash Flow Engine        │  │
│  └──────────────────────────────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │ Database Layer: Dual-Pool Architecture (Reader Pool: 10 conns, Writer Pool: 1)   │  │
│  │ Functional index on LOWER(email), Soft-delete on transactions                    │  │
│  └────────────────────────────────────────┬─────────────────────────────────────────┘  │
└───────────────────────────────────────────┼────────────────────────────────────────────┘
                                            │ Local IPC
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        DURABLE PERSISTENCE & DISASTER RECOVERY                         │
│  • SQLite 3 WAL Mode (local low-latency reads/writes on eMMC)                          │
│  • Litestream Replication Daemon ──► Real-time WAL stream to Cloudflare R2 / AWS S3   │
│  • Automated Daily Snapshot Cron (`VACUUM INTO '/backups/db-$(date +%Y%m%d).sqlite'`)  │
│  • Process Supervision: supervisord managing backend daemon with auto-restart          │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### N.2 Prioritized Changes & Change Impact Analysis

#### Phase P0: Production Blockers
1. **Wire SlidingWindowRateLimiter in Production**:
   - *Target*: `backend/src/main.rs`, `backend/src/api/mod.rs:83`.
   - *Action*: Instantiate `Arc<SlidingWindowRateLimiter>`, spawn cleanup task, pass to `create_app`, and mount `auth::auth_routes_with_rate_limiter`.
   - *Impact*: Low blast radius; protects `/login` and `/register` against brute force.
2. **Purge Plaintext Secrets from `start.sh`**:
   - *Target*: `start.sh`, `.env.example`, `.gitignore`.
   - *Action*: Remove hardcoded RSA keys, client secrets, and JWT secrets. Source from untracked `.env`. Set `COOKIE_SECURE="true"`. Rotate DANA sandbox keys.
   - *Impact*: Eliminates critical secret exposure.
3. **Fix Idempotency Request Hash**:
   - *Target*: `backend/src/service/ledger_service.rs:154`.
   - *Action*: Compute SHA-256 hash of serialized transaction payload and pass hex digest to `acquire_lock`.
   - *Impact*: Restores cryptographic payload mismatch detection (`IdempotencyLockResult::MismatchedPayload`).
4. **Guard Target Monorepo CI/CD Pipeline**:
   - *Target*: `/home/nurdiansyah/dev/Personal_project/.github/workflows/deploy.yml:3-8`.
   - *Action*: Add `paths:` filter restricting push triggers to React frontend and PHP API files.
   - *Impact*: Prevents accidental production cPanel FTP overwrites from finance PWA commits.
5. **Add Root `.gitignore`**:
   - *Target*: `personal_finance_pwa/.gitignore`.
   - *Action*: Ignore `target/` (9.6GB), `node_modules/`, `data/*.db*`, `.env*`, and logs.

#### Phase P1: Critical Architecture & UX
1. **Adopt Vue Router 4**: Install `vue-router@4`, replace `currentTab = ref('home')`, establish routes (`/`, `/transactions`, `/analytics`, `/profile`, `/auth`) with navigation guards.
2. **Harmonize Pinia Transaction Store**: Refactor `TransactionsView.vue` and `AddTransactionModal.vue` to consume `useTransactionStore()`, eliminating orphaned store state.
3. **Fix PWA Offline Boot Lockout**: Modify `authStore.checkAuth()` to preserve cached user profile and allow offline read access.
4. **Fix Lazy Trial Expiry**: Re-evaluate `trial_ends_at` in `AuthService::login` and `AuthService::get_me`, atomically downgrading expired trials to `free`.
5. **Transactional Webhook Settlement**: Enclose `PaymentService::handle_dana_notification` in `pool.begin()` database transaction.
6. **Fix Readiness Probe Bug**: Update `/ready` in `backend/src/api/health.rs` to return HTTP 503 on database query failure.
7. **Transaction UI Enhancements**: Add search keyword input, date range filters, and transaction backdating date picker.

#### Phase P2: Important Features & Observability
1. **Build Budgeting & Goals Modules**: Implement repositories, services, and endpoints for `/api/v1/budgets` and `/api/v1/goals`.
2. **Implement Encrypted PDF/CSV Export**: Build `/api/v1/export` endpoint protected by `reports.advanced` feature gate.
3. **Add Structured Logging**: Add `tracing` and `tracing-subscriber` with JSON format and `TraceLayer`.
4. **Deploy Litestream Replication**: Stream SQLite WAL to Cloudflare R2 / AWS S3 for continuous disaster recovery.
5. **Deploy Process Supervisor**: Configure `supervisord` in edge chroot to manage backend lifecycle.

#### Phase P3: Optimization & Polish
1. **Index Optimization**: Add index on `LOWER(email)` and composite `(user_id, date DESC, created_at DESC)`.
2. **Dark Mode UI Switcher**: Implement theme store with toggle control and `localStorage` persistence.
3. **Clean Dead Code**: Remove `chart.js`, `vue-chartjs`, and adopt `ModalSheet.vue` across modals.

---

## O. Implementation Plan (Phased Execution)

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│                        PHASED IMPLEMENTATION TIMELINE                        │
├──────────────────┬──────────────────┬──────────────────┬─────────────────────┤
│     PHASE 1      │     PHASE 2      │     PHASE 3      │       PHASE 4       │
│  P0 Blockers &   │  P1 Core UX &    │  P2 Pro Features │   P3 Parity &       │
│ Security Hotfix  │ State Alignment  │ & Observability  │ Disaster Recovery   │
└──────────────────┴──────────────────┴──────────────────┴─────────────────────┘
```

### Phase 1: Security & Ledger Integrity Hardening (Immediate P0)
- **Objective**: Neutralize critical vulnerabilities and restore financial idempotency.
- **Tasks**:
  1. Create root `.gitignore` in `personal_finance_pwa` ignoring `target/`, `node_modules/`, `data/*.db*`, `.env*`, and logs.
  2. Strip plaintext credentials from `start.sh`, source from untracked `.env`, set `COOKIE_SECURE="true"`, and rotate DANA keys.
  3. Wire `SlidingWindowRateLimiter` in `backend/src/main.rs` and `backend/src/api/mod.rs`.
  4. Fix `backend/src/service/ledger_service.rs:154` to compute and verify SHA-256 payload request hash.
  5. Add `paths:` filter to `/home/nurdiansyah/dev/Personal_project/.github/workflows/deploy.yml`.
  6. Enforce fail-fast startup check on `JWT_SECRET` (abort if unset or $< 32$ bytes).
- **Exit Criteria**:
  - `git status` cleanly ignores build artifacts and live databases.
  - Rate limiting tests pass on live server binary.
  - Idempotency test asserts HTTP 409 `MismatchedPayload` on modified payloads.
  - Target `deploy.yml` ignores finance commits.

### Phase 2: Core UX Completeness & State Alignment (P1)
- **Objective**: Eliminate frontend architectural divergence and complete transaction and wallet management UX.
- **Tasks**:
  1. Install `vue-router@4` and configure routes (`/`, `/transactions`, `/analytics`, `/profile`, `/auth`) with auth guards.
  2. Refactor `TransactionsView.vue` and `AddTransactionModal.vue` to use `useTransactionStore`.
  3. Add transaction search bar and date range filters in `TransactionsView.vue`.
  4. Add date picker input to `AddTransactionModal.vue` for transaction backdating.
  5. Build wallet and category management modals in `ProfileView.vue` and `HomeView.vue`.
  6. Connect `AnalyticsView.vue` runway card to `analyticsStore.runwayMonths`.
  7. Fix PWA offline boot in `App.vue` to preserve cached session on network fetch error.
  8. Fix `/ready` health probe in `health.rs` to return HTTP 503 on database failure.
  9. Enforce database transaction `pool.begin()` in `PaymentService` webhook settlement.
  10. Re-evaluate trial expiration in `AuthService::login` and `AuthService::get_me`.
- **Exit Criteria**:
  - Browser back button navigates views cleanly without page reloads.
  - Transactions can be searched, date-filtered, and created with custom past dates.
  - Wallets and categories can be created and archived from UI.
  - PWA opens offline without kicking users to login screen.
  - `/ready` returns HTTP 503 when SQLite is inaccessible.

### Phase 3: Missing Pro Tier Capabilities & Observability (P2)
- **Objective**: Deliver advertised Pro SaaS features and establish structured telemetry.
- **Tasks**:
  1. Implement Backend Repositories, Services, and REST endpoints for `budgets` (`GET /budgets`, `POST /budgets`, `DELETE /budgets/{id}`) protected by `budgeting` feature gate.
  2. Implement Backend Repositories, Services, and REST endpoints for `goals` (`GET /goals`, `POST /goals`, `PUT /goals/{id}`).
  3. Implement CSV and PDF export endpoints (`GET /api/v1/export?format=csv|pdf`) protected by `reports.advanced` feature gate.
  4. Build Budgeting and Goals dashboard cards in `AnalyticsView.vue`.
  5. Add timestamp freshness validation (`|now - ts| <= 300s`) to DANA SNAP webhook verification.
  6. Add category IDOR ownership verification in `ledger_service.rs`.
  7. Add `"happy-dom": "^15.11.7"` to `frontend/package.json` devDependencies.
  8. Add `tracing`, `tracing-subscriber`, and `TraceLayer` with request correlation IDs.
- **Exit Criteria**:
  - Pro users can set monthly category budgets and track spending limits.
  - Pro users can export encrypted transaction history to CSV and PDF.
  - Webhook replays older than 5 minutes are rejected.
  - Requests emit structured JSON logs with correlation IDs.

### Phase 4: Production Edge Hardening & Disaster Recovery (P3)
- **Objective**: Establish automated backup, process supervision, and performance optimization on edge hardware.
- **Tasks**:
  1. Deploy Litestream continuous replication daemon streaming SQLite WAL to Cloudflare R2 / AWS S3.
  2. Configure automated daily snapshot cron executing `VACUUM INTO '/backups/db-$(date +%Y%m%d).sqlite'`.
  3. Replace unmanaged `nohup` in `start.sh` with `supervisord` ensuring auto-restart on device reboot.
  4. Add database functional index `CREATE INDEX idx_users_email_lower ON users(LOWER(email));`.
  5. Implement dark mode UI switcher with `localStorage` persistence.
  6. Adopt `ModalSheet.vue` across modals and `Card.vue` across views; remove unused `chart.js` packages.
  7. Add multi-stage `Dockerfile` and `docker-compose.yml` for developer staging.
- **Exit Criteria**:
  - Automated SQLite replication streams continuously with zero data loss on crash.
  - Process supervisor restarts backend daemon automatically upon panic or reboot.
  - Login email query latency remains sub-millisecond under scale.

---

## Section 25: Final Audit Decision & System Posture

```text
PROJECT STATUS: FUNCTIONALLY COMPLETE WITH CRITICAL ARCHITECTURAL GAPS
ARCHITECTURAL HEALTH: DEGRADED (STRONG DOMAIN RIGOR, SEVERE DEPLOYMENT & RELIABILITY RISKS)
SECURITY POSTURE: AT RISK (RATE LIMITING UNWIRED, CREDENTIALS TRACKED IN START.SH, LAZY TRIAL ESCALATION)
PRODUCTION READINESS: NOT READY (STAGING READY ONLY)
PRIMARY BLOCKERS:
1. Hardcoded plaintext production secrets and 2048-bit RSA private key tracked in Git within start.sh.
2. Unwired rate limiter in production binary leaving authentication endpoints exposed to credential stuffing.
3. Zero database backup or replication strategy on edge hardware (Galaxy A20s eMMC flash storage single point of failure).
4. Unfiltered target monorepo GitHub Actions deployment workflow threatening production cPanel FTP overwrites.
5. Inoperative financial idempotency payload verification caused by hardcoded empty string hash in LedgerService.
RECOMMENDED NEXT ACTION:
Execute Phase 1 Remediation Plan immediately: Wire the sliding-window rate limiter in api/mod.rs, fix the idempotency payload hash in ledger_service.rs, purge all plaintext secrets from start.sh, commit a strict root .gitignore, add path filters to deploy.yml in the target monorepo, and install Litestream for continuous SQLite replication.
```
