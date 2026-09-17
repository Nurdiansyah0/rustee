# Project: Invinite Personal Finance Intelligence Platform

**Authoritative Specification:** `Personal_Finance_Master_Specification_v3.1.0.md`  
**User Request Log:** `.agents/ORIGINAL_REQUEST.md`  
**Target Infrastructure:** `https://api.nurdiansyahlabs.com`  

---

## 1. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                             Native Android Kotlin Shell                          │
│  - WebView Container (Safe area, hardware acceleration, origin restriction)     │
│  - NotificationListenerService (Bank & e-wallet financial notifications)        │
│  - Versioned JavaScript Bridge (window.InviniteBridge)                           │
│  - OS Background Sync Coordination                                              │
└────────────────────────────────────────┬─────────────────────────────────────────┘
                                         │ Hosts
┌────────────────────────────────────────▼─────────────────────────────────────────┐
│                              Vue 3 Mobile-First PWA                              │
│  - Composition API (<script setup>), Vite, Tailwind CSS, Pinia, Workbox         │
│  - 5-Tab Navigation: Home, Transactions, Add (4x3 POS Keypad), Analytics, Profile│
│  - Progressive Personalization Onboarding (Custom vocabulary & workspace)        │
│  - Dual Communication: HTTPS (Cache-Control: private, no-store), Foreground WS, │
│    and Cursor Delta Synchronization (/api/v1/sync?cursor=...)                   │
│  - Localized IDR Formatting (Zero float loss) & Financial Date Policy (Asia/Jakarta)│
└────────────────────────────────────────┬─────────────────────────────────────────┘
                                         │ HTTPS / Cookie Auth (HttpOnly, SameSite=Lax)
                                         │ WebSocket (/api/v1/ws)
┌────────────────────────────────────────▼─────────────────────────────────────────┐
│                      Rust Axum Modular Monolith Backend                          │
│  - Layered Architecture: Presentation → DTO → Service → Domain → Repo → SQLite   │
│  - Invariants: Strict i64 integer Rupiah minor units, checked arithmetic        │
│  - Multi-wallet accounting & atomic balance mutations                            │
│  - Single shared Net Cash Flow engine: net_cash_flow = income - expenses         │
│  - Category soft-deletion (deleted_at) with historical ledger integrity          │
│  - Tenant-isolated custom financial vocabulary (stable SQLite categories schema) │
│  - Idempotency-Key deduplication engine with SHA-256 request payload hashing     │
│  - Argon2id password hashing + secure httpOnly cookie session management         │
│  - Authoritative Subscription Machine: Free, 3-Month Trial, Monthly Rp10k,      │
│    Annual Rp110k, Grace, Cancelled, Expired                                     │
│  - Direct DANA Open API integration (RSA-SHA256 asymmetric signature verification)│
│  - Server-side feature gating returning HTTP 403 FEATURE_LOCKED                  │
│  - Ingestion Pipeline: Source Adapter → Parser → Normalizer → Validator →        │
│    Confidence Engine (HIGH/MEDIUM/LOW) → Deduplication → Ledger                  │
│  - Embedded SQLite WAL mode, PRAGMA busy_timeout=5000, strictly UTC timestamps   │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Feature Inventory

Every requirement from Master Specification v3.1.0 is cataloged here with its assigned milestone:

| # | Req ID | Track | Module | Feature Name | Description | Milestone | Source |
|---|--------|-------|--------|--------------|-------------|-----------|--------|
| 1 | `REQ-ARCH-01` | ARCH-LEDGER | Architecture | Modular Monolith Layering | Layered architecture (Presentation → DTO → Service → Domain → Repository → SQLite WAL). | M1 | Spec §11-13 |
| 2 | `REQ-ARCH-02` | ARCH-LEDGER | Domain | Integer Rupiah Currency Math | Store and calculate all monetary amounts as `i64` minor units with zero floating-point arithmetic. | M1 | Spec §14-15 |
| 3 | `REQ-ARCH-03` | ARCH-LEDGER | Domain | Single Net Cash Flow Engine | Centralized calculation `net_cash_flow = income - expenses` shared uniformly across all modules. | M1 | Spec §15 |
| 4 | `REQ-ARCH-04` | ARCH-LEDGER | Domain | Multi-Wallet Accounting | Support multiple account types (checking, savings, credit, e-wallet, cash) with initial/current balance. | M1 | Spec §14-15 |
| 5 | `REQ-ARCH-05` | ARCH-LEDGER | Domain | Atomic Balance Mutations | Transactions and account balances mutate atomically inside a single database transaction. | M1 | Spec §15 |
| 6 | `REQ-ARCH-06` | ARCH-LEDGER | Domain | Category Soft-Deletion | Soft-delete categories via `deleted_at`, preserving historical transaction and ledger records. | M1 | Spec §14 |
| 7 | `REQ-ARCH-07` | ARCH-LEDGER | Persistence | SQLite WAL & Pragmas | Configure SQLite WAL mode, `busy_timeout = 5000`, `foreign_keys = ON`, `synchronous = NORMAL`. | M1 | Spec §16 |
| 8 | `REQ-ARCH-08` | ARCH-LEDGER | Persistence | Canonical Composite Indexes | Composite indexes on `(user_id, date)`, `(account_id)`, `(user_id, category_id)`, etc. | M1 | Spec §16 |
| 9 | `REQ-ARCH-09` | ARCH-LEDGER | Personalization | Stable Schema Vocabulary | Tenant-isolated vocabulary in `categories` (`display_name`, `normalized_name`, `metadata`) without dynamic tables. | M1 | Spec §4-6 |
| 10 | `REQ-ARCH-10` | ARCH-LEDGER | Persistence | Strict UTC Timestamps | All database records store timestamps in ISO 8601 UTC format. Client converts to local timezone. | M1 | Spec §10 |
| 11 | `REQ-ARCH-11` | ARCH-LEDGER | Core | Idempotency Engine | Deduplicate mutations via `Idempotency-Key` header with SHA-256 payload hashing and 24h cached replay. | M1 | Spec §18 |
| 12 | `REQ-ARCH-12` | ARCH-LEDGER | Core | Multi-Tenant Data Isolation | Enforce query-level tenant isolation `WHERE user_id = :auth_user_id` across all repositories. | M1 | Spec §17 |
| 13 | `REQ-SEC-01` | SEC-PAY | Auth | Argon2id Password Hashing | Secure credential storage using Argon2id algorithm with constant-time dummy verification on unknown emails. | M1 | Spec §17 |
| 14 | `REQ-SEC-02` | SEC-PAY | Auth | Cookie Session Management | Short-lived tokens stored in secure `httpOnly; SameSite=Lax; Path=/` cookies + Bearer fallback. | M1 | Spec §17 |
| 15 | `REQ-SEC-03` | SEC-PAY | Auth | Rate Limiting | Rate limit sensitive endpoints (login: max 5 attempts per 15-minute window per IP). | M1 | Spec §17 |
| 16 | `REQ-SEC-04` | SEC-PAY | Subscriptions | 3-Month Premium Trial | 3-month trial (90 days) with 0 upfront payment, 0 credit card, exactly 1 activation per account, auto-expiry. | M2 | Spec §8 |
| 17 | `REQ-SEC-05` | SEC-PAY | Subscriptions | Subscription Lifecycle Machine | Authoritative state machine: `FREE`, `TRIALING`, `ACTIVE`, `GRACE`, `CANCELLED`, `EXPIRED`, `PENDING`, `UNVERIFIED`. | M2 | Spec §8, §19 |
| 18 | `REQ-SEC-06` | SEC-PAY | Subscriptions | Commercial Pricing Plans | Baseline plans: Monthly Premium @ Rp10.000 / month, Annual Premium @ Rp110.000 / year. | M2 | Spec §7 |
| 19 | `REQ-SEC-07` | SEC-PAY | Payment | DANA Open API Integration | Direct integration with DANA supporting checkout order creation and QRIS/URL generation. | M2 | Spec §19 |
| 20 | `REQ-SEC-08` | SEC-PAY | Payment | RSA-SHA256 Signature Verification | Cryptographically verify DANA webhook callbacks using RSA-SHA256 / SNAP-compatible signature verification. | M2 | Spec §19 |
| 21 | `REQ-SEC-09` | SEC-PAY | Payment | Idempotent Webhook Processing | Webhooks deduplicated via unique provider `event_id`, returning standard acknowledgement (`2005600`). | M2 | Spec §19 |
| 22 | `REQ-SEC-10` | SEC-PAY | Entitlement | Server-Side Feature Gating | Enforce feature permissions; unauthorized access to locked features strictly returns HTTP 403 `FEATURE_LOCKED`. | M2 | Spec §9, §19 |
| 23 | `REQ-SEC-11` | SEC-PAY | Security | Financial Response Cache Control | Enforce HTTP header `Cache-Control: private, no-store` on all sensitive financial and ledger endpoints. | M1 | Spec §33 |
| 24 | `REQ-SEC-12` | SEC-PAY | Audit | Immutable Audit Log | Record immutable audit logs for trial activation, subscription changes, webhook processing, and admin events. | M2 | Spec §34 |
| 25 | `REQ-INGEST-01` | INGEST | Pipeline | Ingestion Pipeline Architecture | 9-stage pipeline: Adapter → Parser → Normalizer → Validator → Confidence → Deduplication → Candidate → Domain → Ledger. | M3 | Spec §20, §24 |
| 26 | `REQ-INGEST-02` | INGEST | Pipeline | Canonical Ingestion Representation | Canonical model: `transaction_id`, `amount`, `currency`, `direction`, `occurred_at`, `provider`, `merchant`, `source`, `confidence`. | M3 | Spec §24 |
| 27 | `REQ-INGEST-03` | INGEST | Pipeline | Confidence Threshold Engine | Confidence scoring: HIGH (auto-create), MEDIUM (user confirmation required), LOW (rejected/unresolved). | M3 | Spec §24 |
| 28 | `REQ-INGEST-04` | INGEST | Pipeline | Cross-Source Deduplication | Deduplicate candidates across Notification, SMS, and Gmail using amount, direction, time window (±300s), and merchant. | M3 | Spec §24 |
| 29 | `REQ-INGEST-05` | INGEST | Notification | Android Notification Adapter | Capture notifications via `NotificationListenerService` only with explicit user permission and target financial packages. | M3 | Spec §21 |
| 30 | `REQ-INGEST-06` | INGEST | SMS | SMS Capability Adapter | Capability-aware SMS parser for bank/e-wallet SMS, adhering to device permissions and distribution constraints. | M3 | Spec §22 |
| 31 | `REQ-INGEST-07` | INGEST | Gmail | Targeted Gmail Ingestion | Google OAuth integration with minimum practical scope querying financial receipts without mailbox mirroring. | M3 | Spec §23 |
| 32 | `REQ-INGEST-08` | INGEST | Privacy | Payload Minimization | Discard raw notification, SMS, and email body payloads once canonical fields are extracted. No hoarding of PII. | M3 | Spec §23, §33 |
| 33 | `REQ-AND-01` | ANDROID | Container | Kotlin Native Shell & WebView | Native Android Kotlin container hosting the Vue 3 PWA with hardware acceleration, safe-area, and lifecycle handling. | M4 | Spec §28 |
| 34 | `REQ-AND-02` | ANDROID | Service | NotificationListenerService Integration | Financial notification listener with runtime permission verification and background event forwarding. | M4 | Spec §21, §28 |
| 35 | `REQ-AND-03` | ANDROID | Bridge | Versioned JS Capability Bridge | Minimal, explicit JavaScript bridge exposing validated capabilities (`notification`, `sync`, `push`, `secure-storage`, `lifecycle`). | M4 | Spec §29 |
| 36 | `REQ-AND-04` | ANDROID | Bridge | WebView Origin Validation | Restrict JS bridge invocation strictly to trusted application origins (`https://api.nurdiansyahlabs.com` / app host). | M4 | Spec §29 |
| 37 | `REQ-AND-05` | ANDROID | Sync | OS Background Sync Coordination | Coordinate periodic background synchronization conforming to Android WorkManager / Battery Saver constraints. | M4 | Spec §25, §28 |
| 38 | `REQ-AND-06` | ANDROID | UX | First-Launch Subscription Layer | Android native first-launch activation flow checking backend entitlement before granting full access. | M4 | Spec §8, §28 |
| 39 | `REQ-AND-07` | ANDROID | Storage | Secure Native Storage | Encrypted storage for short-lived refresh tokens and local installation identifiers (Android Keystore / EncryptedSharedPreferences). | M4 | Spec §28, §33 |
| 40 | `REQ-FE-01` | FRONTEND | PWA | Vue 3 Mobile-First PWA Shell | Vue 3 Composition API (`<script setup>`), Vite, Tailwind CSS, Pinia stores, and Workbox offline caching. | M5 | Spec §12, §30 |
| 41 | `REQ-FE-02` | FRONTEND | Navigation | 5-Tab Ergonomic Navigation | Bottom navigation bar: Home, Transactions, Add, Analytics, Profile with 44x44px touch targets. | M5 | Spec §30, §31 |
| 42 | `REQ-FE-03` | FRONTEND | Transactions | Rapid 4x3 POS Keypad | Custom numeric keypad with quick increment chips (+50rb, +100rb, +500rb), reset button, and tactile feedback. | M5 | Spec §30 |
| 43 | `REQ-FE-04` | FRONTEND | Personalization | Progressive Onboarding Flow | Multi-step initial registration flow configuring user display name, financial goals, wallets, and custom category vocabulary. | M5 | Spec §3, §4, §6 |
| 44 | `REQ-FE-05` | FRONTEND | Currency | Localized IDR Formatting | Formatter utility (`id-ID`, IDR) ensuring integer precision presentation without floating-point conversion issues. | M5 | Spec §15, §30 |
| 45 | `REQ-FE-06` | FRONTEND | Timezone | Financial Date/Time Policy | Centralized `Asia/Jakarta` display conversion from backend UTC timestamps (`resolveFinancialDate`, `getFinancialPeriod`). | M5 | Spec §10 |
| 46 | `REQ-FE-07` | FRONTEND | Realtime | Foreground WebSocket Client | WebSocket connection for real-time events (`TransactionCreated`, `BalanceChanged`), active only when app is foregrounded. | M5 | Spec §25, §27 |
| 47 | `REQ-FE-08` | FRONTEND | Sync | Cursor Delta Sync | Incremental offline mutation sync and delta fetching via `/api/v1/sync?cursor=<cursor>` on network reconnect. | M5 | Spec §25, §26 |
| 48 | `REQ-FE-09` | FRONTEND | UI/UX | Feature Lock Overlay & Upgrade Modal | Display locked premium features with non-coercive CTAs, triggering checkout or trial activation modal. | M5 | Spec §9, §30 |
| 49 | `REQ-FE-10` | FRONTEND | Design | Fintech Visual Design System | Restrained palette, 1px borders, subtle elevation, tabular numerals, Lucide SVG icons, zero raw emojis. | M5 | Spec §31 |
| 50 | `REQ-FE-11` | FRONTEND | Accessibility | WCAG 2.1 AA Compliance | Semantic HTML, full keyboard navigation, aria-live status regions, visible focus rings, high contrast ratios. | M5 | Spec §32 |
| 51 | `REQ-QA-01` | QA | Backend | Automated Cargo Test Suite | Comprehensive unit and integration test coverage across all modules (`cargo test --workspace`). | M1-M6, E2E | Spec §36, §37 |
| 52 | `REQ-QA-02` | QA | Frontend | Production Build Validation | Clean compilation of Vue 3 PWA without warnings or bundle errors (`npm run build`). | M5, M6 | Spec §37 |
| 53 | `REQ-QA-03` | QA | E2E | E2E Acceptance Test Runner | Automated suite executing 334+ test assertions (`bash e2e_tests/runner.sh all`). | E2E, M6 | Spec §37 |
| 54 | `REQ-QA-04` | QA | Health | Operational Health & Readiness | Endpoints `GET /health` and `GET /ready` verifying database connectivity and operational status with zero secret leaks. | M1, M6 | Spec §37 |
| 55 | `REQ-QA-05` | QA | Security | Adversarial Security Testing | Verify multi-tenant isolation, SQL injection prevention, rate limiting enforcement, and webhook signature tampering. | M6 | Spec §36 |

*Feature Inventory Cross-Check*: All 55 features have assigned milestones. 0 unassigned features.

---

## 3. Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| **E2E** | **E2E Testing Track** | Test harness, Tiers 1-4 opaque-box acceptance suite, `TEST_INFRA.md`, publishes `TEST_READY.md`. | none | **DONE** |
| **M1** | **Backend Monolith, SQLite WAL Ledger & Personalization (R1)** | Re-establish Rust Axum workspace, database migrations (`0001`, `0002`, `0003`), `Rupiah(i64)` integer currency, multi-wallet accounting, atomic balances, single shared Net Cash Flow engine, category soft-deletion, user vocabulary in stable schema, Argon2id auth, rate limiting, and tenant query isolation. | none | **DONE** |
| **M2** | **Subscription Engine & Cryptographic DANA Webhooks (R2)** | Authoritative subscription lifecycle (`FREE`, `TRIALING`, `ACTIVE`, `GRACE`, `CANCELLED`, `EXPIRED`), 3-month trial (90 days, 0 upfront, 1x/account, auto-expire), Monthly Rp10k / Annual Rp110k plans, DANA Open API RSA-SHA256 signature verification, idempotent webhook processing via `event_id`, and server-side feature gating returning HTTP 403 `FEATURE_LOCKED`. | M1 | **DONE** |
| **M3** | **Transaction Ingestion & Deduplication Pipeline (R3)** | 9-stage ingestion pipeline: Adapter → Parser → Normalizer → Validator → Confidence Engine (HIGH auto-create, MEDIUM confirm, LOW reject) → Cross-Source Deduplication (window ±300s) → Candidate Review API → Domain Validation → Ledger. | M1 | **DONE** |
| **M4** | **Android Native Kotlin Shell & JS Capability Bridge (R4)** | Native Kotlin Android shell hosting Vue 3 PWA in WebView, `NotificationListenerService` for financial notifications with permission checks, versioned JS capability bridge (`window.InviniteBridge`) with trusted origin validation, OS background sync coordination. | none | **DONE** |
| **M5** | **Frontend Vue 3 PWA Full-Stack Integration & Personalization (R5)** | Connect Vue 3 PWA to live backend, rapid 4x3 POS keypad, progressive personalization onboarding flow (vocabulary & workspace configuration), dual communication (HTTPS + foreground WebSocket + cursor sync `/sync?cursor=...`), `FeatureLockOverlay.vue`, and v3.1.0 pricing alignment. | M1, M2 | **DONE** |
| **M6** | **Final Acceptance Verification & Adversarial Hardening** | Phase 1: 100% E2E test pass across Tiers 1-4 (`TEST_READY.md`). Phase 2: Tier 5 adversarial coverage hardening with Challengers. Independent Forensic Integrity Audit verification. | M1-M5, E2E | **DONE** |

---

## 4. Interface Contracts

### 4.1 REST API & Transport Standards
- **Base URL**: `/api/v1`
- **Session Header**: `Cookie: auth_token=<JWT>` or `Authorization: Bearer <JWT>`
- **Mutation Header**: `Idempotency-Key: <UUIDv4>`
- **Cache-Control**: `Cache-Control: private, no-store` on all financial endpoints.
- **Error Format (RFC 7807 compatible)**:
  ```json
  {
    "type": "https://api.nurdiansyahlabs.com/errors/feature-locked",
    "title": "Forbidden",
    "status": 403,
    "detail": "Subscription feature 'analytics.advanced' required.",
    "code": "FEATURE_LOCKED"
  }
  ```

### 4.2 Subscription & Payment Contract (DANA SNAP)
- **Checkout Order Endpoint**: `POST /api/v1/subscriptions/checkout`
  - Request: `{"plan": "premium_monthly" | "premium_annual", "provider": "dana"}`
  - Response: `{"order_id": "...", "checkout_url": "...", "amount": 10000 | 110000, "currency": "IDR"}`
- **Trial Activation Endpoint**: `POST /api/v1/subscriptions/trial/activate`
  - Request: `{}` (Zero upfront payment, no card)
  - Response: `{"status": "trialing", "trial_ends_at": "...", "message": "3-month trial activated"}`
- **DANA Webhook Endpoint**: `POST /api/v1/webhooks/dana`
  - Headers: `X-SIGNATURE: <base64(rsa_sha256(string_to_sign))>`, `X-TIMESTAMP: <ISO8601>`, `X-PARTNER-ID: <id>`, `X-EXTERNAL-ID: <event_id>`
  - Success Response: `{"responseCode": "2005600", "responseMessage": "Successful"}`

### 4.3 Ingestion Pipeline Contracts
- **Ingest Notification Payload**: `POST /api/v1/ingestion/notification`
  - Headers: `Idempotency-Key: <UUIDv4>`
  - Request: `{"package_name": "com.bca", "title": "m-Transfer Berhasil", "text": "Transfer Rp 50.000 ke ...", "posted_at": 1773715200000}`
  - Response: `{"event_id": "...", "status": "auto_created" | "requires_confirmation" | "duplicate" | "rejected", "confidence": "HIGH" | "MEDIUM" | "LOW", "transaction_id": "..." | null}`
- **List Ingestion Candidates**: `GET /api/v1/ingestion/candidates`
  - Returns pending `requires_confirmation` candidates for user review.
- **Confirm Candidate**: `POST /api/v1/ingestion/candidates/:id/confirm`
  - Request: `{"account_id": "...", "category_id": "..."}`
  - Commits transaction atomically to ledger.

### 4.4 Realtime WebSocket & Sync Contracts
- **WebSocket Endpoint**: `GET /api/v1/ws` (Upgrade: websocket)
  - Events broadcast:
    - `{"type": "TransactionCreated", "data": {"id": "...", "amount": 50000, "account_id": "..."}}`
    - `{"type": "BalanceChanged", "data": {"account_id": "...", "new_balance": 1500000}}`
    - `{"type": "SyncHint", "data": {"new_cursor": 142}}`
- **Cursor Delta Sync Endpoint**: `GET /api/v1/sync?cursor=<number>`
  - Response: `{"cursor": 145, "transactions": [...], "accounts": [...], "categories": [...], "has_more": false}`

### 4.5 Android JavaScript Capability Bridge Contract
- Injected Global: `window.InviniteBridge`
- Interface:
  ```typescript
  interface InviniteBridge {
    postMessage(message: string): void;
    // Protocol messages:
    // {"action": "check_permissions"}
    // {"action": "request_notification_permission"}
    // {"action": "get_device_id"}
    // {"action": "haptic_feedback", "style": "light" | "medium" | "heavy"}
  }
  ```
- Origin Validation: The native shell strictly validates `WebView.url` against `https://api.nurdiansyahlabs.com` and authorized local dev URLs before processing bridge messages.

---

## 5. Code Layout

```
/home/nurdiansyah/teamwork_projects/personal_finance_pwa
├── Cargo.toml                              # Root Cargo workspace manifest
├── backend/                                # Rust Axum modular monolith crate
│   ├── Cargo.toml
│   ├── migrations/                         # SQLx database migrations
│   │   ├── 0001_initial_schema.sql
│   │   ├── 0002_trial_and_dana_support.sql
│   │   └── 0003_v3_1_0_schema_upgrade.sql  # Ingestion, support tables, vocabulary columns
│   ├── src/
│   │   ├── main.rs                         # Application entrypoint & CLI
│   │   ├── config.rs                       # Environment & server config
│   │   ├── domain/                         # Pure domain logic & financial invariants
│   │   │   ├── money.rs                    # Rupiah(i64) checked arithmetic value object
│   │   │   ├── user.rs
│   │   │   ├── account.rs
│   │   │   ├── category.rs                 # User custom vocabulary models
│   │   │   ├── transaction.rs              # Direction, status, canonical models
│   │   │   └── subscription.rs             # Lifecycle state machine & plans
│   │   ├── repository/                     # SQLite WAL persistence layer
│   │   │   ├── db.rs                       # Connection pool, PRAGMAs (WAL, busy_timeout)
│   │   │   ├── user_repo.rs
│   │   │   ├── account_repo.rs
│   │   │   ├── category_repo.rs
│   │   │   ├── transaction_repo.rs         # Shared Net Cash Flow calculation engine
│   │   │   ├── subscription_repo.rs
│   │   │   ├── ingestion_repo.rs           # Ingestion events & candidate review
│   │   │   └── sync_repo.rs                # Sync cursors & delta tracking
│   │   ├── service/                        # Service orchestration
│   │   │   ├── auth_service.rs             # Argon2id, JWT, rate limiting
│   │   │   ├── ledger_service.rs           # Atomic balance updates, idempotency locks
│   │   │   ├── payment_service.rs          # 3-month trial, DANA RSA-SHA256, plans
│   │   │   ├── ingestion_service.rs        # 9-stage pipeline & deduplication
│   │   │   └── sync_service.rs             # Delta sync engine
│   │   ├── api/                            # Axum HTTP presentation layer
│   │   │   ├── router.rs                   # Route registration
│   │   │   ├── middleware/                 # Rate limiting, feature gate, cache-control
│   │   │   ├── handlers/                   # Controllers: auth, accounts, categories, tx, etc.
│   │   │   └── ws.rs                       # Axum WebSocket realtime handler
│   │   └── lib.rs
│   └── tests/                              # Automated Cargo test suites (108+ tests)
├── frontend/                               # Vue 3 Mobile-First PWA
│   ├── package.json
│   ├── vite.config.js
│   ├── src/
│   │   ├── App.vue                         # 5-Tab mobile navigation & state
│   │   ├── views/                          # Home, Transactions, Add, Analytics, Profile
│   │   ├── components/
│   │   │   ├── AddTransactionModal.vue     # Rapid 4x3 POS keypad
│   │   │   ├── FeatureLockOverlay.vue      # Reusable HTTP 403 lock overlay
│   │   │   ├── ProgressiveOnboardingModal.vue # Vocabulary & workspace onboarding
│   │   │   └── UpgradeModal.vue            # 3-Month Trial & Premium checkout
│   │   ├── stores/                         # Pinia stores (auth, transactions, subscription, realtime)
│   │   ├── services/                       # API client, WebSocket client, Delta sync service
│   │   └── utils/                          # Localized currency (IDR), financial date policy
├── android/                                # Android Native Kotlin Shell
│   ├── app/
│   │   ├── build.gradle.kts
│   │   └── src/main/
│   │       ├── AndroidManifest.xml         # NotificationListenerService declaration & permissions
│   │       └── kotlin/com/invinite/pwa/
│   │           ├── MainActivity.kt         # WebView host & safe area
│   │           ├── InviniteBridge.kt       # Versioned JS capability bridge & origin validation
│   │           ├── NotificationListener.kt # Financial notification capture service
│   │           └── BackgroundSyncWorker.kt # WorkManager background sync coordinator
└── e2e_tests/                              # Opaque-box E2E Acceptance Test Suite
    ├── runner.sh                           # Deterministic test runner
    ├── tier1_feature_coverage.sh           # Tier 1 (≥5 tests per feature)
    ├── tier2_boundary_corner.sh            # Tier 2 (Boundary value analysis)
    ├── tier3_cross_feature.sh              # Tier 3 (Pairwise feature combinations)
    └── tier4_real_world.sh                 # Tier 4 (Realistic end-to-end user scenarios)
```
