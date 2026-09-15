# Project: Personal Finance PWA SaaS

Production-ready, full-stack Personal Finance PWA SaaS platform targeting deployment on `api.nurdiansyahlabs.com` infrastructure.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       Vue 3 Mobile-First PWA                            │
│  - Composition API (<script setup>), Vite, Tailwind CSS, Pinia          │
│  - Workbox Offline App Shell (NetworkOnly on /api/v1/*)                 │
│  - Views: Home, Transactions, Add (Rapid Keypad), Analytics, Profile    │
│  - Localized id-ID IDR formatting (Zero floating-point loss)            │
│  - FeatureLockOverlay & UpgradeModal on HTTP 403                        │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ HTTPS / Cookies (HttpOnly; SameSite=Lax)
┌────────────────────────────────────▼────────────────────────────────────┐
│                    Layer 1: Axum REST API & Routing                     │
│  - Extractors: AuthenticatedUser, FeatureGate, IdempotencyKey           │
│  - Middleware: RateLimiter (5/15m), CacheControl (private, no-store)    │
│  - Endpoints: /auth/*, /accounts/*, /categories/*, /transactions/*,      │
│               /budgets/*, /goals/*, /analytics/*, /subscriptions/*,     │
│               /webhooks/*, /health, /ready                              │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ Direct In-Memory Invocation (DTOs)
┌────────────────────────────────────▼────────────────────────────────────┐
│                  Layer 2: Service & Workflow Engine                     │
│  - AuthService, AccountService, TransactionService, SubscriptionService │
│  - Idempotency deduplication & result cache                             │
│  - Transaction boundaries (`sqlx::Transaction`)                         │
│  - PaymentGateway abstraction (Midtrans & Xendit)                       │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ Invokes pure business logic
┌────────────────────────────────────▼────────────────────────────────────┐
│                  Layer 3: Domain Entities & Pure Engine                 │
│  - Pure Rust (Zero I/O, Zero DB): Rupiah(i64) value object               │
│  - NetCashFlowEngine: Income - Expense with transfer net-0 invariance   │
│  - Domain Invariants: atomic balance delta, category soft-deletion      │
│  - Subscription State Machine (Free vs Premium @ Rp5k/mo)               │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ Implements repository traits
┌────────────────────────────────────▼────────────────────────────────────┐
│                  Layer 4: SQLx Repositories                             │
│  - Multi-tenant isolation enforced in every query: `WHERE user_id = ?`  │
│  - UserRepository, AccountRepository, CategoryRepository,               │
│    TransactionRepository, SubscriptionRepository, IdempotencyRepository │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ Connection Pool (WAL, busy_timeout=5000)
┌────────────────────────────────────▼────────────────────────────────────┐
│                  Layer 5: Embedded SQLite Database                      │
│  - PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;               │
│  - PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;               │
│  - 10 Tables, Composite Indexes on (user_id, date) and (account_id)     │
└─────────────────────────────────────────────────────────────────────────┘
```

## Feature Inventory

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | SQLite WAL & PRAGMAs | SQLite initialized with WAL mode, busy_timeout=5000, foreign keys ON, synchronous=NORMAL | M1 | ORIGINAL_REQUEST §R3 |
| 2 | Relational Schema Migrations | SQLx migrations for 10 tables (users, accounts, categories, transactions, budgets, goals, subscriptions, audit_logs, idempotency_keys, webhook_events) | M1 | ORIGINAL_REQUEST §R3 |
| 3 | Composite & Performance Indexes | Mandatory indexes on `transactions(user_id, date)`, `transactions(account_id)`, and secondary acceleration indexes | M1 | ORIGINAL_REQUEST §R3 |
| 4 | Integer Rupiah Value Object | `Rupiah(i64)` integer minor unit type with zero floating-point arithmetic loss | M1 | ORIGINAL_REQUEST §R3, AC |
| 5 | Multi-Tenant Data Isolation | Strict multi-tenant query scoping (`WHERE user_id = :auth_user_id`) returning 404 on cross-user access | M2 | ORIGINAL_REQUEST §R4, AC |
| 6 | Argon2id Password Hashing | Secure password hashing using Argon2id offloaded via `tokio::task::spawn_blocking` | M2 | ORIGINAL_REQUEST §R4, AC |
| 7 | JWT Token & HttpOnly Cookies | Short-lived tokens (15m) served via secure `HttpOnly`, `SameSite=Lax` cookies | M2 | ORIGINAL_REQUEST §R4 |
| 8 | Sliding Window Rate Limiting | Rate limiter enforcing maximum 5 auth attempts per 15 minutes per IP/client | M2 | ORIGINAL_REQUEST §R4 |
| 9 | Pure NetCashFlowEngine | Single shared calculation engine for net cash flow (`income - expenses`) and category aggregations | M3 | ORIGINAL_REQUEST §R1, AC |
| 10 | Multi-Wallet Accounts | Account entity supporting checking, savings, e-wallet, cash with atomic balance maintenance | M3 | ORIGINAL_REQUEST §R1 |
| 11 | Category Personalization & Soft-Delete | Custom income/expense categories with `deleted_at` soft-deletion preserving historical integrity | M3 | ORIGINAL_REQUEST §R1, AC |
| 12 | Strict Idempotency-Key Deduplication | Transaction submission deduplication preventing duplicate balance updates under sequential/concurrent replayed keys | M3 | ORIGINAL_REQUEST §R1, AC |
| 13 | Payment Gateway Trait Abstraction | Unified `PaymentGateway` trait abstracting Midtrans and Xendit providers | M4 | ORIGINAL_REQUEST §R2 |
| 14 | Cryptographic HMAC/SHA-512 Verification | Constant-time verification of Midtrans SHA-512 and Xendit HMAC-SHA256 signatures | M4 | ORIGINAL_REQUEST §R2, AC |
| 15 | Idempotent Webhook Processing | Webhook deduplication using SQLite `ON CONFLICT DO NOTHING` on `webhook_events` | M4 | ORIGINAL_REQUEST §R2, AC |
| 16 | Subscription Lifecycle State Machine | Free vs Premium @ Rp5,000/month state transitions (Active, Grace, Cancelled, Expired) | M4 | ORIGINAL_REQUEST §R2, AC |
| 17 | Server-Side Feature Gating | Permission strings (`transactions.basic`, `analytics.advanced`, `budgeting`, `reports.advanced`) returning HTTP 403 | M5 | ORIGINAL_REQUEST §R4, AC |
| 18 | Full Axum REST API Endpoints | REST handlers for accounts, categories, transactions, budgets, goals, analytics, subscriptions | M5 | ORIGINAL_REQUEST §R1 |
| 19 | Health & Readiness Probes | Public `/health` (liveness) and `/ready` (DB ping) probes with zero credential leaks | M5 | ORIGINAL_REQUEST AC |
| 20 | Environment & Secrets Config | Safe `.env.example` template with zero client-side credentials | M5 | ORIGINAL_REQUEST AC |
| 21 | Mobile-First Vue 3 PWA Shell | Vue 3 (Composition API) + Vite + Tailwind CSS + Workbox mobile container | M6 | ORIGINAL_REQUEST §R5 |
| 22 | 5-Tab Mobile Navigation | Bottom navigation bar for Home, Transactions, Add, Analytics, Profile | M6 | ORIGINAL_REQUEST §R5 |
| 23 | Rapid POS Numeric Keypad Entry | Custom 4x3 keypad with `000` shortcut, quick IDR increment chips, haptics, UUIDv4 Idempotency-Key | M6 | ORIGINAL_REQUEST §R5 |
| 24 | Localized id-ID Currency Formatting | `Intl.NumberFormat('id-ID', IDR)` formatting with zero floating-point display inaccuracies | M6 | ORIGINAL_REQUEST §R5, AC |
| 25 | Workbox Offline Caching & Cache-Control | Precached offline app shell, `NetworkOnly` on API, `Cache-Control: private, no-store` | M6 | ORIGINAL_REQUEST §R5, AC |
| 26 | Accessible Financial Charts | WCAG AA compliant Chart.js visualizations with screen-reader data tables | M6 | ORIGINAL_REQUEST §R5 |
| 27 | UI Feature Gating & Upgrade Modal | Visible Premium features with frosted lock overlays, upgrade CTAs, Midtrans/Xendit checkout modal | M6 | ORIGINAL_REQUEST §R4, R5 |
| 28 | 100% E2E Test Pass (Tiers 1-4) | Comprehensive opaque-box E2E test suite passing across all functional tiers | M7 | ORIGINAL_REQUEST AC |
| 29 | Adversarial Coverage Hardening (Tier 5) | White-box stress testing, race conditions, edge cases, and boundary mutations | M7 | Project Quality Gate |

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | SQLite Schema, Migrations & Domain Persistence | Embedded SQLite connection pool (WAL, busy_timeout=5000, foreign_keys=ON), SQLx migrations for 10 tables, composite indexes, integer Rupiah (`Rupiah(i64)`) value object, repository traits & implementations | none | DONE |
| M2 | Multi-Tenant Auth, Security & Rate Limiting Middleware | Argon2id password hashing, JWT manager, httpOnly cookie auth extractor, sliding-window rate limiter (5 req / 15m), auth API endpoints, multi-tenant extractor | M1 | IN_PROGRESS |
| M3 | Core Financial Service, Ledger Engine & Deduplication | Pure `NetCashFlowEngine` calculation engine, multi-wallet accounts, soft-deletable categories, atomic balance maintenance, strict `Idempotency-Key` deduplication workflow, deterministic unit tests | M1, M2 | PLANNED |
| M4 | Native Payment & Subscription Webhook Engine | `PaymentGateway` trait, Midtrans SHA-512 & Xendit HMAC-SHA256 signature verifiers, webhook deduplication engine (`webhook_events`), subscription lifecycle state machine (Free vs Premium @ Rp5k/mo: Active, Grace, Cancelled, Expired) | M1, M2 | PLANNED |
| M5 | Feature Gating, REST API Endpoints & Health Probes | Server-side feature gating middleware (`transactions.basic`, `analytics.advanced`, `budgeting`, `reports.advanced` -> HTTP 403), full Axum REST endpoints, `/health` and `/ready` probes, `.env.example` | M2, M3, M4 | PLANNED |
| M6 | Mobile-First Vue 3 PWA Frontend | Vue 3 + Vite + Tailwind CSS + Workbox setup, Pinia stores, 5 mobile navigation views, rapid POS numeric keypad entry, `id-ID` IDR formatting, accessible Chart.js visualizations, offline app shell, feature lock overlays and upgrade modals, `Cache-Control: private, no-store` | M5 | PLANNED |
| M7 | Final Milestone: 100% E2E Pass & Adversarial Hardening | Phase 1: Pass 100% of E2E test suite (Tiers 1-4) published by E2E Testing Track.<br>Phase 2: Adversarial coverage hardening (Tier 5) with Challenger -> Worker -> Reviewer loop. | M6, TEST_READY.md | PLANNED |

*Note: The **E2E Testing Track** runs in parallel with M1-M6, delivering test infrastructure and Tiers 1-4 test suites leading to `TEST_READY.md`.*

## Interface Contracts

### Backend Layer 1 (API) ↔ Layer 2 (Service)
- **Auth Service**:
  - `register(req: RegisterRequest) -> Result<(User, String), AppError>` (Returns user and JWT cookie value)
  - `login(req: LoginRequest) -> Result<(User, String), AppError>`
  - `get_me(user_id: &str) -> Result<UserProfileResponse, AppError>`
- **Transaction Service**:
  - `record_transaction(user_id: &str, idempotency_key: Option<&str>, req: CreateTransactionRequest) -> Result<TransactionResponse, AppError>`
  - `list_transactions(user_id: &str, filter: TransactionFilter) -> Result<Vec<TransactionResponse>, AppError>`
  - `get_cash_flow_summary(user_id: &str, period: DateRange) -> Result<CashFlowSummary, AppError>`
- **Account Service**:
  - `create_account(user_id: &str, req: CreateAccountRequest) -> Result<AccountResponse, AppError>`
  - `list_accounts(user_id: &str) -> Result<Vec<AccountResponse>, AppError>`
- **Category Service**:
  - `list_categories(user_id: &str) -> Result<Vec<CategoryResponse>, AppError>`
  - `create_category(user_id: &str, req: CreateCategoryRequest) -> Result<CategoryResponse, AppError>`
  - `soft_delete_category(user_id: &str, category_id: &str) -> Result<(), AppError>`
- **Subscription & Webhook Service**:
  - `process_midtrans_webhook(headers: &HeaderMap, body: &[u8]) -> Result<WebhookResult, AppError>`
  - `process_xendit_webhook(headers: &HeaderMap, body: &[u8]) -> Result<WebhookResult, AppError>`
  - `create_checkout_session(user_id: &str, provider: &str) -> Result<CheckoutResponse, AppError>`

### Layer 2 (Service) ↔ Layer 3 (Domain)
- **NetCashFlowEngine**:
  - `compute(transactions: &[TransactionRecord], categories: &HashMap<String, CategoryInfo>) -> Result<CashFlowSummary, DomainError>`
  - Invariants:
    - Transfers do not affect net cash flow ($income - expense$).
    - $amount > 0$ for all transactions; balances maintain exact delta equality.
- **Rupiah Value Object**:
  - `Rupiah::new(i64) -> Rupiah`
  - `checked_add(Rupiah) -> Option<Rupiah>`
  - `checked_sub(Rupiah) -> Option<Rupiah>`

### Layer 2 (Service) ↔ Layer 4 (Repository)
- All repository methods explicitly require `user_id: &str` parameter to enforce multi-tenant isolation at the query level:
  - `find_by_id(user_id: &str, entity_id: &str) -> Result<Option<Entity>, DbError>`
  - `list_by_user(user_id: &str, filter: ...) -> Result<Vec<Entity>, DbError>`
  - Cross-tenant queries return `Ok(None)`, mapped to HTTP 404 at the API layer.

### Frontend ↔ Backend REST API Contract
- Base URL: `/api/v1`
- Authentication: Cookie named `auth_token` with `HttpOnly; SameSite=Lax; Path=/`.
- Financial Endpoints Headers:
  - Request: `Cache-Control: private, no-store`, `Idempotency-Key: <UUIDv4>` (on mutations).
  - Response: `Cache-Control: private, no-store, must-revalidate`.
- Error Format (RFC 7807 compatible):
  ```json
  {
    "type": "https://api.nurdiansyahlabs.com/errors/forbidden",
    "title": "Forbidden",
    "status": 403,
    "detail": "Subscription feature 'analytics.advanced' required.",
    "code": "FEATURE_LOCKED"
  }
  ```

## Code Layout

```
personal_finance_pwa/
├── Cargo.toml                       # Consolidated Rust workspace
├── backend/                         # Consolidated Rust Backend (Axum / Tokio / SQLx)
│   ├── Cargo.toml
│   ├── migrations/                  # SQLx migrations (0001_initial_schema.sql)
│   ├── src/
│   │   ├── main.rs                  # Server entrypoint, listener on 0.0.0.0:8080
│   │   ├── config.rs                # AppConfig (.env reading, PRAGMAs, secrets)
│   │   ├── error.rs                 # RFC 7807 AppError, DomainError, Status codes
│   │   ├── api/                     # Layer 1: HTTP API Handlers & Routing
│   │   │   ├── mod.rs
│   │   │   ├── routes.rs            # Axum Router assembly
│   │   │   ├── auth.rs              # /api/v1/auth/* handlers
│   │   │   ├── accounts.rs          # /api/v1/accounts/* handlers
│   │   │   ├── categories.rs        # /api/v1/categories/* handlers
│   │   │   ├── transactions.rs      # /api/v1/transactions/* handlers
│   │   │   ├── analytics.rs         # /api/v1/analytics/* handlers
│   │   │   ├── subscriptions.rs     # /api/v1/subscriptions/* handlers
│   │   │   ├── webhooks.rs          # /api/v1/webhooks/:provider handlers
│   │   │   ├── health.rs            # /health & /ready probes
│   │   │   └── middleware/
│   │   │       ├── auth_extractor.rs # AuthenticatedUser cookie/header extractor
│   │   │       ├── rate_limiter.rs   # 5 attempts / 15m sliding window
│   │   │       └── feature_gate.rs   # Server-side permission checker (403)
│   │   ├── service/                 # Layer 2: Workflows & Orchestration
│   │   │   ├── mod.rs
│   │   │   ├── auth_service.rs
│   │   │   ├── account_service.rs
│   │   │   ├── transaction_service.rs # Atomic balance maintenance & Idempotency
│   │   │   ├── category_service.rs
│   │   │   ├── subscription_service.rs # State machine & lifecycle
│   │   │   └── payment/
│   │   │       ├── mod.rs           # PaymentGateway trait
│   │   │       ├── midtrans.rs      # SHA-512 verification & payload parser
│   │   │       └── xendit.rs        # HMAC-SHA256 verification & parser
│   │   ├── domain/                  # Layer 3: Pure Domain Business Logic
│   │   │   ├── mod.rs
│   │   │   ├── money.rs             # Rupiah(i64) integer minor unit value object
│   │   │   ├── engine.rs            # NetCashFlowEngine pure calculation
│   │   │   ├── entities.rs          # User, Account, Category, Transaction, etc.
│   │   │   └── subscription.rs      # SubscriptionTier, Lifecycle states & rules
│   │   └── repository/              # Layer 4 & 5: SQLx Repositories & SQLite WAL
│   │       ├── mod.rs
│   │       ├── db.rs                # SqlitePool factory with WAL & PRAGMAs
│   │       ├── user_repo.rs         # WHERE user_id = ?
│   │       ├── account_repo.rs
│   │       ├── category_repo.rs
│   │       ├── transaction_repo.rs
│   │       ├── subscription_repo.rs
│   │       └── idempotency_repo.rs
│   └── tests/                       # Rust Integration & Unit Tests
│       ├── ledger_tests.rs          # Deterministic financial math & net cash flow
│       ├── auth_isolation_tests.rs  # Multi-tenant isolation & rate limiting
│       └── webhook_tests.rs         # Payment signatures & idempotent replay
├── frontend/                        # Mobile-First Vue 3 PWA (Vite + Tailwind + Workbox)
│   ├── package.json
│   ├── vite.config.ts               # Vite + VitePWA + proxy to backend
│   ├── tailwind.config.js
│   ├── index.html
│   ├── src/
│   │   ├── main.ts
│   │   ├── App.vue
│   │   ├── router/index.ts
│   │   ├── stores/                  # 7 Pinia stores
│   │   │   ├── auth.ts
│   │   │   ├── wallets.ts
│   │   │   ├── transactions.ts
│   │   │   ├── categories.ts
│   │   │   ├── analytics.ts
│   │   │   ├── subscription.ts
│   │   │   └── pwa.ts
│   │   ├── views/                   # 5 Primary Views
│   │   │   ├── HomeView.vue
│   │   │   ├── TransactionsView.vue
│   │   │   ├── AddTransactionView.vue
│   │   │   ├── AnalyticsView.vue
│   │   │   └── ProfileView.vue
│   │   ├── components/
│   │   │   ├── QuickKeypadEntry.vue # Custom 4x3 POS numeric keypad
│   │   │   ├── BottomNavBar.vue     # Mobile 5-tab thumb-friendly nav
│   │   │   ├── FeatureLockOverlay.vue # Frosted overlay for locked Premium
│   │   │   ├── UpgradeModal.vue     # Midtrans/Xendit checkout modal
│   │   │   └── charts/
│   │   │       ├── CashFlowBarChart.vue
│   │   │       └── CategoryDonutChart.vue
│   │   └── utils/
│   │       ├── currency.ts          # id-ID localized integer IDR formatting
│   │       └── api.ts               # Axios / Fetch client with Cache-Control
│   └── tests/
│       └── unit/
├── e2e_tests/                       # Independent Opaque-Box E2E Test Suite (Tiers 1-4)
│   ├── runner.sh
│   ├── fixtures/
│   ├── tier1_feature_coverage/
│   ├── tier2_boundary_corner/
│   ├── tier3_cross_feature/
│   └── tier4_real_world/
├── .env.example
├── ORIGINAL_REQUEST.md
├── PROJECT.md
├── TEST_INFRA.md                    # Published by E2E Testing Track
└── TEST_READY.md                    # Published by E2E Testing Track upon completion
```
