# PERSONAL FINANCE MASTER SPECIFICATION

**Product:** Invinite Personal Finance  
**Version:** 3.1.0  
**Status:** Authoritative Master Specification  
**Date:** 2026-09-17  
**Working Directory:** `/home/nurdiansyah/teamwork_projects/personal_finance_pwa`  
**Target Production API:** `https://api.nurdiansyahlabs.com`

---

## 0. AUTHORITY

This document is the single source of truth for product, business, domain, architecture, security, platform, personalization, subscription, and acceptance requirements.

The Teamwork Project Prompt and all implementation agents MUST treat this specification as authoritative. Agents MUST NOT invent or silently change business rules.

The governing principle is:

```
BUSINESS PROBLEM
      ↓
PRODUCT VALUE
      ↓
USER EXPERIENCE
      ↓
BUSINESS MODEL
      ↓
DOMAIN
      ↓
ARCHITECTURE
      ↓
IMPLEMENTATION
      ↓
VERIFICATION
```

Technology is an implementation mechanism, not the product objective.

---

## 1. BUSINESS PROBLEM

Users conduct financial activity across banks, e-wallets, transfers, digital payments, cash, subscriptions, and other sources.

Financial information is therefore fragmented. Users commonly have to:
- remember transactions;
- manually record them;
- manually classify them;
- reconcile information from different sources;
- calculate cash flow;
- review spending patterns;
- determine what requires attention.

The central problem is:
> Reduce the distance between a financial transaction occurring and the user gaining reliable financial understanding of that transaction, while preserving correctness, privacy, transparency, and user control.

Invinite is therefore not merely an expense tracker. It is a:
**Personal Finance Intelligence Platform.**

Target transformation:
```
TRANSACTION
    ↓
CAPTURE
    ↓
STRUCTURE
    ↓
VALIDATE
    ↓
DEDUPLICATE
    ↓
LEDGER
    ↓
ANALYTICS
    ↓
FINANCIAL UNDERSTANDING
```

---

## 2. PRODUCT VALUE

The product value is reduction of financial-management friction.

Primary outcomes:
- less manual data entry;
- more complete transaction records;
- faster financial visibility;
- consistent financial calculations;
- personalized terminology;
- understandable analytics;
- transparent automation.

Automatic ingestion is an automation layer, not a prerequisite for using the core product.

```
Manual entry + Automatic ingestion + Financial analytics + Personalization = Financial intelligence
```

---

## 3. TARGET USER AND LIVING PRODUCT EXPERIENCE

The primary user wants to understand their financial condition without maintaining complicated records manually.

The desired outcome is:
> “I can see what happened to my money, where it went, how my cash flow is changing, and what requires my attention.”

The application must feel alive and personalized from the beginning.
After registration, the user must not be dropped into an anonymous generic dashboard.
The application should ask a small set of purposeful personalization questions and progressively construct the user's workspace.

Possible questions:
- preferred display name;
- financial tracking goals;
- preferred account/wallet names;
- common income types;
- common expense types;
- preferred terminology;
- recurring financial activities;
- other information that materially improves the experience.

Optional questions can be skipped and edited later. Do not turn onboarding into a long questionnaire.

---

## 4. USER-OWNED FINANCIAL VOCABULARY

A key product requirement is that users may define their own financial terminology.

Examples:
- **User A**: Income: `Gaji` | Expense: `Makan`, `Kopi`, `Transport`
- **User B**: Income: `Salary` | Expense: `Operasional`, `Ngopi`, `Bensin`
- **User C**: Income: `Project` | Expense: `Keluarga`, `Hobi`

The product MUST NOT force every user to use identical visible titles.
The user may:
- choose a suggestion;
- edit a suggestion;
- create a new title;
- type their own title from scratch.

Suggestions are assistive only. The user's explicit input is authoritative.

---

## 5. PERSONALIZATION DOES NOT MEAN DYNAMIC SQL SCHEMAS

Each user may have different logical labels, but the database schema MUST remain stable.

Do NOT create dynamic user tables like `table_user_1`, `table_user_2`.

Instead:
```
categories
------------
id
user_id
type
display_name
normalized_name
metadata
created_at
updated_at
```

The distinction is:
- **Technical schema**: Stable for the platform.
- **User vocabulary**: Customizable per authenticated user.

This provides personalization without destroying indexing, migrations, analytics, tenant isolation, or maintainability. The user owns visible terminology; the platform owns the technical schema.

---

## 6. PERSONALIZATION BEHAVIOR

The system should become more useful as the user supplies more information.

Requirements:
- remember user-defined labels;
- prioritize previous choices;
- provide contextual suggestions;
- never silently rename user-defined terms;
- preserve historical meaning;
- isolate personalization by authenticated user;
- allow editing/deletion of user-defined vocabulary;
- learn only from explicit user actions;
- do not require users to adopt platform terminology.

Personalization should affect: labels, suggestions, account names, category choices, onboarding, dashboard presentation, and relevant insights. It must not alter authoritative financial arithmetic.

---

## 7. BUSINESS MODEL

Current commercial baseline:
- **Free**
- **Premium Monthly:** Rp10.000 / month
- **Premium Annual:** Rp110.000 / year

The annual plan is now explicitly defined.
Premium value should come primarily from: automation, transaction ingestion, advanced analytics, advanced reports, budgeting, personalization capabilities, and financial intelligence. Premium must not be merely a collection of arbitrary locked screens.

---

## 8. PREMIUM TRIAL

Trial duration: **3 months**

Requirements:
- no upfront payment;
- no credit-card requirement;
- one trial activation per account;
- full Premium capability during trial;
- accurate remaining duration;
- automatic expiration;
- return to Free without data loss.

Subscription lifecycle: `FREE`, `TRIALING`, `ACTIVE`, `GRACE`, `CANCELLED`, `EXPIRED`, `PENDING`, `UNVERIFIED`.
Trial and subscription timestamps are stored in UTC.

---

## 9. PRO EXPERIENCE FROM REGISTRATION

The Pro account experience must be introduced from the beginning of the user's journey:
```
Register → Personalization → Introduce Pro value → Trial / Subscription state → Personalized workspace
```
The system should explain the value of automation and advanced financial analysis early. The user must not be required to build a large manual dataset before discovering the Pro workflow.
However: Free user ≠ Blocked from basic financial tracking. Core manual financial tracking remains usable.

---

## 10. TIME AND DATE STANDARD

All authoritative application date/time values MUST remain UTC.
Canonical rule:
```
Backend (UTC) → API (UTC) → Client (Local presentation only)
```
Persist in UTC: transaction timestamps, subscription start/end, trial start/end, audit timestamps, ingestion timestamps, synchronization timestamps, server-generated events.
A user in WIB, WITA, or WIT must share the same authoritative UTC event timeline. The backend must not depend on Indonesian timezone selection for correctness. Clients convert UTC to local time for display.

---

## 11. TARGET PLATFORM MODEL

```text
                         INVINITE
                            │
             ┌──────────────┴──────────────┐
             │                             │
          Web / iOS                    Android APK
             │                             │
         Vue PWA                     Kotlin Shell
             │                             │
             │                         WebView
             │                             │
             └──────────────┬──────────────┘
                            │
                       HTTPS / WS
                            │
                     Rust + Axum
                   Modular Monolith
                            │
                       SQLite WAL
```

Platform roles:
- **Web**: PWA.
- **iOS**: PWA through Safari/Add to Home Screen as baseline.
- **Android**: Kotlin native shell hosting the same PWA.
- **Kotlin**: native capability and lifecycle layer.
- **Rust**: authoritative backend/domain.
- **SQLite**: authoritative persistence.

---

## 12. TECHNOLOGY BASELINE

- **Backend**: Rust, Axum, Tokio, SQLx, SQLite, SQLite WAL, Modular Monolith.
- **Frontend**: Vue 3, Composition API, Vite, Tailwind CSS, Pinia, Workbox/PWA, Lucide icons.
- **Android**: Kotlin, Android WebView, NotificationListenerService where authorized and available, OS-compatible background work, push, secure native storage where justified, versioned JavaScript bridge.
- **Communication**: Client ↔ Backend (HTTPS), Foreground realtime (WebSocket), Background (Push + OS work + delta sync), Backend modules (In-process).

---

## 13. BACKEND ARCHITECTURE

Modular Monolith: `API → DTO / Transport → Service → Domain → Repository → SQLite`.
Modules: `auth`, `users`, `accounts`, `categories`, `transactions`, `subscriptions`, `reports`, `notifications`, `ingestion`, `realtime`.
Rules: explicit module boundaries, no direct cross-module repository access, service interfaces/commands/queries/domain events for cross-module interaction, same process and deployment unit.

---

## 14. FINANCIAL DOMAIN

- **Accounts**: create, edit, archive, balance, transaction history, user ownership isolation.
- **Categories**: income, expense, user-defined labels, soft deletion.
- **Transactions**: income/expense, account, category, integer amount, occurred_at, merchant/notes, source metadata, external reference, idempotency.

---

## 15. MONETARY AND LEDGER RULES

- All monetary values use integer Rupiah (`i64`). Never use floating point for financial calculations.
- Checked arithmetic where overflow is possible.
- Canonical cash flow: `net_cash_flow = income - expenses`.
- One authoritative financial calculation engine reused by dashboard, reports, analytics, and summaries.
- Transaction persistence and balance mutation must be atomic. No partial financial state is acceptable.

---

## 16. DATABASE — SQLITE WAL

Authoritative database: SQLite.
Baseline tables: `users`, `accounts`, `categories`, `transactions`, `budgets`, `goals`, `subscriptions`, `audit_logs`.
Support tables: `idempotency_keys`, `ingestion_events`, `subscription_events`, `device_installations`, `sync_cursors`.
Required configuration:
```sql
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
```

---

## 17. AUTHENTICATION, AUTHORIZATION AND TENANCY

- Short-lived authenticated sessions/tokens, httpOnly cookies, SameSite, Secure production cookies.
- Argon2id password hashing, rate limiting (5 login attempts / 15 minutes).
- Every user-owned query must enforce authenticated tenant isolation (`WHERE user_id = :auth_user_id`).

---

## 18. IDEMPOTENCY

All financial mutations must be idempotent. Required against API retries, offline retries, repeated notifications, repeated SMS, repeated Gmail ingestion, payment webhook replay, reconnect/replay. Repeated observations of the same financial event must converge to one authoritative transaction.

---

## 19. SUBSCRIPTION AND PAYMENT

- Backend is authoritative for entitlement.
- Current plans: Monthly Premium = Rp10.000, Annual Premium = Rp110.000, Trial = 3 months.
- Required baseline endpoints:
  - `POST /api/v1/subscriptions/checkout`
  - `POST /api/v1/webhooks/dana`
  - `GET /api/v1/subscription`
- Current direct payment target: DANA Open API (RSA-SHA256/SNAP-compatible signature verification).
- Webhooks must validate fields, verify cryptographic signature, reject forged callbacks, deduplicate, atomically settle subscription, and audit.

---

## 20. AUTOMATIC FINANCIAL TRANSACTION INGESTION

Architecture: `Adapter → Parser → Normalizer → Validator → Confidence Engine → Deduplication → Transaction Candidate → Domain Validation → Ledger`.
Raw source format must never become the financial domain model.

---

## 21. ANDROID NOTIFICATION INGESTION

Use `NotificationListenerService` only when explicitly granted by user and permitted by device/platform. False positives require validation and confidence handling.

---

## 22. SMS INGESTION

SMS is optional and capability-dependent. Must account for Android restrictions, permissions, and policies.

---

## 23. GMAIL INGESTION

Requires explicit OAuth with minimum practical scope, clear consent, targeted financial-message query, no mailbox mirroring, minimal raw content retention, and revocation support.

---

## 24. TRANSACTION INTELLIGENCE

Canonical fields: `transaction_id`, `amount`, `currency`, `direction`, `occurred_at`, `provider`, `merchant`, `account`, `source`, `external_reference`, `confidence`, `ingestion_id`.
Confidence levels:
- `HIGH` → auto-create after domain validation
- `MEDIUM` → user confirmation
- `LOW` → reject / unresolved

Confidence never bypasses domain validation. Deduplication signals: provider, external reference, amount, direction, timestamp/window, merchant, account, normalized fingerprint, ingestion ID.

---

## 25. REALTIME AND SYNCHRONIZATION

- HTTPS is default API communication.
- WebSocket is foreground realtime channel (`TransactionCreated`, `TransactionUpdated`, `BalanceChanged`, `NotificationEvent`, `SyncHint`, `OperationProgress`).
- Background: Push + OS-compatible background work + Cursor/delta synchronization (`/sync?cursor=<last_cursor>`).

---

## 26. OFFLINE-FIRST

- PWA offline shell.
- Offline financial mutations use stable client IDs / idempotency keys.
- Prefer delta/cursor synchronization over full-history downloads upon reconnect.

---

## 27. PERFORMANCE AND BATTERY

Pagination, server-side filtering, compact DTOs, delta synchronization, lifecycle-aware WebSocket, bounded retries, exponential backoff, no unbounded historical fetches, no permanent background WebSocket.

---

## 28. ANDROID NATIVE SHELL

Kotlin Native Shell + Android WebView + Native Capability Layer.
Kotlin responsibilities: WebView hosting, lifecycle, notification access, background coordination, push, secure native storage, native activation/subscription UX, JS bridge.
Kotlin MUST NOT own ledger calculations, subscription authority, authorization, backend persistence, or financial reconciliation.

---

## 29. JAVASCRIPT ↔ KOTLIN BRIDGE

Explicit, versioned, minimal, capability-based, validated (`notification`, `sync`, `push`, `secure-storage`, `lifecycle`). Never expose arbitrary native execution or unrestricted networking. Trusted WebView origin only.

---

## 30. PWA AND UX

Vue 3 + Vite + PWA.
Navigation: Home, Transactions, Add, Analytics, Profile.
Features: rapid numeric keypad input, mobile-first layout, accessible charts, IDR formatting, responsive desktop, offline shell, loading/error/empty states, Premium states, pagination/filtering.

---

## 31. DESIGN SYSTEM

Modern fintech, restrained semantic color, 1px border hierarchy, subtle elevation, tabular numerics, light/dark support, safe-area support, reduced motion, visible focus, ≥44×44px touch targets, Lucide icons.

---

## 32. ACCESSIBILITY

Target WCAG 2.1 AA: semantic HTML, keyboard accessibility, visible focus, appropriate ARIA, labeled icon controls, reduced motion, ≥44×44px touch targets, textual/accessible chart equivalents.

---

## 33. SECURITY AND PRIVACY

HTTPS, secure httpOnly cookies, Argon2id, rate limiting, tenant isolation, server-side entitlement, cryptographic payment verification, idempotent webhooks, audit logging, data minimization, `Cache-Control: private, no-store` on sensitive financial responses.

---

## 34. AUDIT LOGGING

Audit trial activation, subscription activation, renewal, payment settlement, webhook processing, entitlement changes, security events, and automatic transaction decisions. Audit logs are evidence, not financial source of truth.

---

## 35. API CONTRACTS

Stable OpenAPI / JSON Schema contracts defining requests/responses, auth, errors, idempotency, pagination, sync, and WebSocket events. Vue and Kotlin consume identical backend contracts.

---

## 36. TESTING

Deterministic automated testing covering: financial math, idempotency, subscription lifecycle (Free, 3-month trial, monthly, annual, renewal, replay), ingestion (notification, SMS, Gmail, confidence, deduplication), platform (PWA, Android WebView, JS bridge, sync), and personalization (custom labels, vocabulary persistence, tenant isolation).

---

## 37. EXISTING QUALITY BASELINES

- Frontend build: `npm run build` → 0 errors
- Rust tests: `cargo test --workspace` → 108 passed
- E2E: `bash e2e_tests/runner.sh all` → 334 passed
- Live health: `https://api.nurdiansyahlabs.com/health` → HTTP 200

---

## 38. DEPLOYMENT AND BOUNDARIES

Target API: `https://api.nurdiansyahlabs.com`.
System boundaries: NurdiansyahLabs React ≠ Invinite Vue PWA ≠ Invinite Rust Backend ≠ Android Kotlin Shell.
Do not merge React and Vue runtimes, move domain logic into Kotlin, or expose SQLite directly.

---

## 39. NON-GOALS

No microservices, no gRPC between current modules, no Redis solely for realtime, no permanent background WebSocket, no unrestricted SMS access, no Gmail mailbox mirroring, no floating-point financial arithmetic, no autonomous AI financial mutations.

---

## 40. IMPLEMENTATION PRIORITY

Sequence: Domain & DB → Auth & Tenancy → Accounts/Categories/Transactions → Ledger/Idempotency → Personalization → Analytics/Reports → Subscription/Trial/Payment → PWA → Offline Sync → WebSocket/Push → Android Shell/Bridge → Ingestion (Notification/SMS/Gmail) → Confidence/Deduplication → Verification.

---

## 41. DEFINITION OF DONE

Production readiness checklist:
- Business problem demonstrably addressed.
- Registration produces a personalized starting experience with purposeful onboarding questions.
- Users define own income/expense terminology; suggestions are assistive without forcing platform terminology.
- Personalization is isolated per user; underlying database schema remains stable.
- Manual financial tracking works independently.
- Ledger is deterministic; integer Rupiah calculations are correct (`net_cash_flow = income - expenses`).
- Timestamps are authoritative UTC.
- Transactions are atomic and deduplicated.
- Tenant isolation is verified.
- Pricing verified: Monthly Premium Rp10.000, Annual Premium Rp110.000, Trial 3 months.
- Entitlement is backend-authoritative.
- Payment callbacks (DANA Open API / SNAP) cryptographically verified.
- Automatic ingestion is permission-aware with confidence thresholds.
- PWA works independently; Android Kotlin shell hosts PWA with restricted JS bridge.
- Foreground realtime WebSocket and background delta sync work without battery drain.
- Offline mutations reconcile cleanly.
- Security and privacy controls verified.
- Quality baselines maintained (cargo test, npm run build, E2E runner).

---

## 42. FINAL ARCHITECTURAL DECISION

Consolidated Rust + Axum Modular Monolith owning SQLite WAL, serving Vue 3 PWA (Web/iOS) and Android Kotlin Native Shell (WebView + Capabilities) via HTTPS, WebSocket, and Delta Sync.

---

## 43. MASTER PRODUCT DECISION

Invinite exists to reduce the friction between financial activity and reliable financial understanding. The Teamwork Project Prompt MUST use this specification as its source of truth.
