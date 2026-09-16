# PERSONAL FINANCE MASTER SPECIFICATION

**Product:** Invinite Personal Finance  
**Specification Status:** Authoritative Master Specification  
**Version:** 3.0.0  
**Date:** 2026-09-16  
**Integrity Mode:** Development  
**Working Directory:** `/home/nurdiansyah/teamwork_projects/personal_finance_pwa`  
**Target Production API:** `https://api.nurdiansyahlabs.com`

---

## 0. PURPOSE AND AUTHORITY

This document is the authoritative product, architecture, business-logic, platform, security, performance, and acceptance specification for Invinite Personal Finance.

It consolidates the original Personal Finance requirements and the subsequent PWA + Android architecture decisions into one source of truth. Agents and engineers MUST use this document as the authoritative specification.

Do not silently alter architecture, business rules, financial calculations, subscription semantics, security requirements, or platform responsibilities. Any ambiguity or implementation conflict MUST be documented and resolved against this specification before changing the design.

The product is **PWA-first**, with an **Android-native Kotlin capability layer**. Android is not a second UI implementation.

---

# 1. PRODUCT DEFINITION

Invinite is a personal-finance SaaS application for recording, analyzing, and monitoring financial activity with precise Rupiah accounting.

Foundational principles:

1. Financial correctness before architectural complexity.
2. One authoritative financial domain model.
3. One authoritative subscription/entitlement model.
4. PWA-first presentation.
5. Native Android only where the OS provides capabilities the browser cannot reliably provide.
6. Automatic financial ingestion must be conservative, explainable, permission-aware, and deduplicated.
7. Battery efficiency comes from lifecycle-aware synchronization, not permanent realtime connections.
8. Backend is authoritative for authorization, entitlement, calculations, and persisted state.

---

# 2. TARGET RUNTIME MODEL

```text
                         INVINITE PERSONAL FINANCE
                                  │
                ┌─────────────────┴─────────────────┐
                │                                   │
          Web / iOS PWA                       Android APK
                │                                   │
                │                            Kotlin Native Shell
                │                                   │
                │                              Android WebView
                │                                   │
                └───────────────┬───────────────────┘
                                │
                         Vue 3 PWA UI
                                │
                              HTTPS
                                │
                         Rust + Axum
                       Modular Monolith
                                │
                         SQLite WAL
                                │
                    Authoritative Domain State
```

Platform roles:

- Web: Vue PWA in browser.
- iOS: PWA via Safari/Add to Home Screen as the baseline.
- Android: Kotlin application shell hosting the same PWA UI.
- Kotlin: native capability adapter, lifecycle/background coordinator, notification ingestion client, push integration, secure native storage where justified, and versioned JS bridge.
- Rust: all authoritative backend domain logic.
- SQLite: authoritative relational persistence.

---

# 3. TECHNOLOGY BASELINE

## 3.1 Backend

- Rust
- Axum
- Tokio
- SQLx
- SQLite
- SQLite WAL
- Modular Monolith

## 3.2 Frontend

- Vue 3
- Composition API
- Vite
- Tailwind CSS
- Pinia
- Workbox / PWA
- Lucide icons

## 3.3 Android

- Kotlin
- Native Android application shell
- Android WebView
- NotificationListenerService where explicitly authorized and technically available
- OS-compatible background work
- Push notification integration
- Secure native storage where justified
- Versioned, capability-based JavaScript bridge

## 3.4 Communication

- HTTPS/HTTP: primary client/API transport
- WebSocket: foreground realtime transport
- Push/background synchronization: background mechanism
- gRPC: NOT part of the current architecture

---

# 4. BACKEND ARCHITECTURE

The backend MUST remain a Modular Monolith.

```text
backend/
└── rust/
    └── src/
        ├── modules/
        │   ├── auth/
        │   ├── users/
        │   ├── accounts/
        │   ├── categories/
        │   ├── transactions/
        │   ├── subscriptions/
        │   ├── reports/
        │   ├── notifications/
        │   ├── ingestion/
        │   └── realtime/
        │
        ├── infrastructure/
        ├── config/
        └── main.rs
```

Layering:

```text
Presentation / API
        ↓
Transport / DTO
        ↓
Service Orchestration
        ↓
Domain Invariants / Calculations
        ↓
Repository
        ↓
SQLite
```

Rules:

- Modules MUST have explicit boundaries.
- A module MUST NOT directly access another module's repository implementation.
- Cross-module interaction MUST use service interfaces, commands, queries, or domain events.
- All modules run in the same process and deployment unit.
- Backend modules communicate in-process.
- Do not introduce microservices or gRPC merely for architectural appearance.

---

# 5. FINANCIAL DOMAIN AND LEDGER

## 5.1 Accounts / Wallets

Users may maintain multiple financial accounts/wallets.

Support:

- account creation;
- editing;
- archiving;
- account balance;
- account transaction history;
- ownership isolation.

## 5.2 Categories

Categories support:

- income categories;
- expense categories;
- user customization;
- soft deletion.

Deleted categories MUST NOT corrupt historical transactions.

## 5.3 Transactions

Transactions MUST support:

- income;
- expense;
- account association;
- category association;
- amount;
- occurred date/time;
- notes/merchant information where applicable;
- automatic-ingestion source metadata;
- external reference where available;
- idempotent creation.

## 5.4 Monetary Representation

All monetary values MUST use integer Rupiah / integer arithmetic.

- No floating-point arithmetic for financial values.
- No `f32`/`f64` for monetary calculations.
- Checked integer arithmetic MUST be used where overflow is possible.
- Currency formatting occurs only at the presentation boundary.
- Canonical currency is IDR unless another currency is explicitly introduced.

## 5.5 Net Cash Flow

There MUST be one shared calculation engine:

```text
net_cash_flow = income - expenses
```

Balances, analytics, dashboards, and reports MUST NOT implement competing versions of the same calculation.

## 5.6 Atomicity

Transaction creation and balance mutation MUST be atomic.

A failed transaction MUST NOT leave a transaction without its balance effect, a balance mutation without its transaction, or partially updated financial state.

---

# 6. IDEMPOTENCY AND DATA INTEGRITY

Financial mutations MUST be idempotent.

```text
Idempotency-Key
        ↓
Acquire / validate key
        ↓
Process exactly once
        ↓
Persist result
        ↓
Return stable result on retry
```

The system MUST prevent duplicates caused by:

- client retries;
- network retries;
- background synchronization;
- repeated native events;
- duplicate notifications;
- duplicate SMS;
- duplicate Gmail messages;
- repeated payment callbacks.

Webhook processing MUST use unique provider event IDs or equivalent provider references.

---

# 7. DATABASE — SQLITE WAL

The application MUST use embedded SQLite as the authoritative database.

Required baseline tables:

```text
users
accounts
categories
transactions
budgets
goals
subscriptions
audit_logs
```

Additional support tables MAY be introduced for correctness, including:

```text
idempotency_keys
ingestion_events
subscription_events
device_installations
sync_cursors
```

Required SQLite configuration:

```sql
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
```

`PRAGMA synchronous = NORMAL` may be used where appropriate.

Required indexes must support at minimum:

- `(user_id, date)`;
- `(account_id)`;
- source/external-reference lookup;
- idempotency/event deduplication;
- subscription lookup by user;
- synchronization cursor/change lookup.

SQLite WAL semantics MUST be respected. The application MUST NOT assume unlimited concurrent writers.

---

# 8. AUTHENTICATION, AUTHORIZATION AND TENANCY

Authentication MUST use:

- short-lived authenticated sessions/tokens;
- secure `httpOnly` cookies where cookie authentication is used;
- `SameSite` protection;
- `Secure` behavior in production;
- Argon2id or another approved password hashing mechanism;
- rate limiting.

Baseline login rate limit:

```text
5 attempts / 15 minutes
```

Every repository query involving user-owned data MUST enforce authenticated tenant isolation:

```sql
WHERE user_id = :auth_user_id
```

Client-supplied user IDs MUST NOT override the authenticated tenant identity.

---

# 9. FEATURE GATING AND ENTITLEMENTS

Backend is authoritative for feature access.

Baseline permissions:

```text
transactions.basic
analytics.advanced
budgeting
reports.advanced
```

Additional Pro/native capabilities may include:

```text
PRO_ACCESS
AUTO_TRANSACTION_INGESTION
NOTIFICATION_INGESTION
SMS_INGESTION
EMAIL_INGESTION
ADVANCED_ANALYTICS
ADVANCED_REPORTING
ENCRYPTED_EXPORTS
```

Premium features MAY remain visible in the UI when locked, but unauthorized backend access MUST return HTTP 403.

The client MUST NOT treat a locally stored subscription flag as authoritative.

---

# 10. SUBSCRIPTION MODEL

## 10.1 Commercial Baseline

Current commercial requirement:

```text
Free
Premium: Rp 5.000 / month
```

This remains authoritative unless a future commercial specification explicitly changes it.

## 10.2 On-Demand 7-Day Trial

Provide:

- no upfront payment;
- no credit-card requirement;
- one trial activation per account;
- full Pro access during trial;
- accurate remaining-duration display;
- automatic expiration;
- graceful return to Free without data loss.

Lifecycle:

```text
FREE
  │
  └── start trial
          ↓
      TRIALING
          │
          ├── payment
          ↓
       ACTIVE
          │
          └── expiry
               ↓
            EXPIRED
```

Additional states MAY include:

```text
GRACE
CANCELLED
PENDING
UNVERIFIED
```

## 10.3 Android First-Launch Subscription Layer

The Android APK MUST provide a first-launch activation/subscription experience.

```text
APK installed
      ↓
Kotlin shell starts
      ↓
Identify/authenticate account
      ↓
Request authoritative subscription status
      │
      ├── ACTIVE/TRIALING → open PWA
      ├── FREE/EXPIRED    → show subscription layer
      └── UNKNOWN         → verify backend state
```

Responsibilities:

- Kotlin owns native first-launch/activation UX.
- Rust owns subscription truth and entitlement.
- Local state is only a cache.
- Reinstall MUST NOT permanently destroy account entitlement.
- Entitlement MUST be restorable after account authentication.

### Annual Billing Clarification

The phrase “annual subscription layer” refers to the Android first-launch subscription experience. The authoritative commercial requirement currently defines **Rp5.000/month** and does not define an annual price.

Implementation agents MUST NOT invent an annual price.

If annual billing is intended as a separate plan, its amount, term, renewal policy, and provider flow must be specified separately.

---

# 11. PAYMENT AND WEBHOOK ENGINE

Payment logic remains inside Rust.

Provider-specific details MUST remain behind a payment abstraction.

Current direct provider target:

```text
DANA Open API
```

Required endpoints:

```text
POST /api/v1/subscriptions/checkout
POST /api/v1/webhooks/dana
GET  /api/v1/subscription
```

DANA webhook processing MUST:

1. validate required fields;
2. verify the cryptographic signature;
3. reject invalid/forged callbacks;
4. identify the payment/subscription;
5. deduplicate repeated/replayed callbacks;
6. atomically settle subscription state;
7. write audit information;
8. return the correct provider acknowledgement.

Existing requirement specifies RSA-SHA256 / SNAP-compatible verification and a successful acknowledgement such as `2005600 Successful`.

Frontend payment success MUST NOT by itself activate Premium.

---

# 12. AUTOMATIC FINANCIAL TRANSACTION INGESTION

Automatic transaction detection is a Pro capability.

Architecture:

```text
Android Notification ─┐
SMS (if available) ───┼──→ Source Adapter
Gmail OAuth ──────────┘          ↓
                            Provider Parser
                                  ↓
                              Normalizer
                                  ↓
                              Validator
                                  ↓
                          Confidence Engine
                                  ↓
                           Deduplication
                                  ↓
                        Transaction Candidate
                                  ↓
                         Domain Validation
                                  ↓
                              Persist
```

The financial domain MUST NOT depend on provider-specific raw message formats.

---

# 13. ANDROID NOTIFICATION INGESTION

Android may ingest financial notifications through `NotificationListenerService` when the user explicitly grants access and the device/platform permits it.

Example:

```text
"BCA — Debit Rp125.000"
```

Canonical candidate:

```json
{
  "amount": 125000,
  "direction": "expense",
  "currency": "IDR",
  "source": "notification",
  "provider": "bca"
}
```

The application MUST NOT assume every notification is a valid transaction.

Extract and retain only information required to construct/process a transaction candidate.

---

# 14. SMS INGESTION

SMS ingestion is optional and capability-dependent.

The implementation MUST account for:

- Android platform restrictions;
- permission state;
- device configuration;
- distribution requirements;
- application policy constraints.

The product MUST NOT depend on unrestricted SMS access.

When available:

```text
SMS
 ↓
Provider Parser
 ↓
Normalize
 ↓
Validate
 ↓
Confidence
 ↓
Deduplicate
 ↓
Candidate
```

---

# 15. GMAIL INGESTION

Gmail is a separately authorized source.

```text
User
 ↓
Google OAuth
 ↓
Minimum practical Gmail permission
 ↓
Targeted financial-message query
 ↓
Parser
 ↓
Canonical candidate
```

Rules:

- request minimum practical scope;
- explain why access is required;
- do not mirror the entire mailbox;
- avoid retaining complete email bodies when transaction fields are sufficient;
- prefer targeted synchronization;
- allow connection revocation.

---

# 16. SOURCE PRIORITY

Typical priority:

```text
1. Android financial notification
2. SMS, when legitimately available
3. Gmail connected account
4. Manual confirmation / manual entry
```

This is a fallback strategy, not a guarantee that every source exists on every device.

---

# 17. TRANSACTION INTELLIGENCE

Automatic ingestion MUST NOT blindly insert external text as a financial transaction.

## 17.1 Canonical Representation

All sources converge into:

```text
transaction_id
amount
currency
direction
occurred_at
provider
merchant
account
source
external_reference
confidence
ingestion_id
```

Provider-specific parsing details MUST stay within the ingestion boundary.

## 17.2 Confidence

```text
HIGH
  → auto-create if domain validation succeeds

MEDIUM
  → request user confirmation

LOW
  → reject or retain as unresolved
```

Confidence MUST NOT bypass domain validation.

## 17.3 Deduplication

```text
Notification ─┐
SMS ──────────┼──→ Deduplication Engine ─→ ONE transaction
Gmail ────────┘
```

Signals may include:

- provider;
- external reference;
- amount;
- direction;
- occurred-at time/window;
- merchant;
- source account;
- normalized text fingerprint;
- ingestion identifier.

A duplicate event MUST NOT create a second financial transaction.

---

# 18. REALTIME COMMUNICATION

## 18.1 HTTP / HTTPS

HTTP is the default transport for:

- authentication;
- CRUD;
- transaction operations;
- dashboards;
- reports;
- subscriptions;
- synchronization;
- settings.

Sensitive financial responses MUST use:

```http
Cache-Control: private, no-store
```

## 18.2 WebSocket

WebSocket is the foreground realtime channel.

Potential events:

```text
TransactionCreated
TransactionUpdated
BalanceChanged
NotificationEvent
SyncHint
OperationProgress
```

WebSocket is not required for every screen.

## 18.3 Background

Background mode MUST NOT maintain a permanent WebSocket solely to appear realtime.

Use:

```text
Push
+
OS-compatible scheduled/background work
+
Cursor/delta synchronization
```

On resume:

```text
Resume
 ↓
GET /sync?cursor=<last_cursor>
 ↓
Apply delta
 ↓
Update local state
 ↓
Open/reopen WebSocket when needed
```

---

# 19. OFFLINE AND SYNCHRONIZATION

The PWA MUST support an offline app shell.

Offline financial mutations MUST use stable client-generated IDs and/or idempotency keys.

```text
Offline mutation
      ↓
Local pending state
      ↓
Connectivity restored
      ↓
Idempotent synchronization
      ↓
Backend validation
      ↓
Server confirmation
      ↓
Local reconciliation
```

Delta synchronization SHOULD use a cursor:

```text
GET /sync?cursor=1820

→ cursor: 1827
→ changes: [...]
```

The client MUST NOT repeatedly fetch complete historical datasets.

---

# 20. PERFORMANCE AND BATTERY

Required strategies:

- server-side pagination;
- server-side filtering;
- compact DTOs;
- delta synchronization;
- lifecycle-aware WebSocket;
- conservative heartbeat;
- bounded retries;
- exponential/backoff reconnect;
- push/background work where appropriate;
- no unbounded historical fetches;
- no continuous Gmail mailbox mirroring;
- no unnecessary permanent background WebSocket.

Examples:

```text
GET /transactions?page=1&limit=20
GET /transactions?from=2026-09-01&to=2026-09-30
GET /transactions?cursor=...
```

Battery efficiency is a lifecycle and synchronization property, not merely a protocol choice.

---

# 21. ANDROID NATIVE SHELL

Kotlin responsibilities:

- host PWA through WebView;
- manage lifecycle;
- receive supported notifications;
- coordinate background work;
- receive push;
- manage secure native storage where justified;
- expose approved native capabilities;
- report connectivity/sync state;
- provide first-launch native activation/subscription UX.

Kotlin MUST NOT duplicate:

- ledger calculations;
- subscription authority;
- authorization rules;
- financial domain rules;
- backend persistence;
- reconciliation logic.

The native shell is a capability adapter, not a second application.

---

# 22. JAVASCRIPT ↔ KOTLIN BRIDGE

The bridge MUST be:

- explicit;
- versioned;
- capability-based;
- validated;
- minimal.

```text
PWA
 │
 │ NativeBridge.request(...)
 ▼
Kotlin
 │
 ├── notification
 ├── sync
 ├── push
 ├── secure-storage
 └── lifecycle
 │
 ▼
Validated native result
 │
 ▼
PWA state
```

Forbidden:

- arbitrary JavaScript execution;
- unrestricted native method exposure;
- generic reflection-based command execution;
- exposing filesystem/database/network primitives to arbitrary web content.

WebView MUST load only trusted application content.

---

# 23. PWA USER EXPERIENCE

Primary UI remains Vue 3.

Baseline navigation:

```text
Home
Transactions
Add
Analytics
Profile
```

Required:

- rapid numeric keypad entry;
- mobile-first interaction;
- accessible charts;
- IDR / `id-ID` formatting;
- responsive desktop layout;
- installable PWA;
- offline shell;
- clear loading/error/empty states;
- premium feature visibility with clear upgrade states.

Large transaction collections MUST use pagination, filtering, search, grouping, or progressive disclosure.

Mobile layouts MUST avoid excessive vertical whitespace and redundant UI.

---

# 24. DESIGN SYSTEM

The application MUST follow the Personal Finance design-system direction:

- restrained fintech visual language;
- semantic color only;
- 1px border hierarchy;
- micro-elevation;
- tabular numerics;
- light/dark theme support;
- mobile-first ergonomics;
- minimum 44×44px touch targets;
- safe-area support;
- reduced-motion support;
- accessible focus states;
- Lucide SVG icons instead of raw emoji;
- no generic CRUD-template appearance.

The interface must remain information-dense without becoming visually cluttered.

Pagination/filtering should be used to keep important content and footer/layout spacing compact on mobile.

---

# 25. ACCESSIBILITY

Target:

- WCAG 2.1 AA;
- semantic HTML;
- keyboard accessibility;
- visible focus;
- appropriate ARIA;
- accessible labels;
- `aria-live` for dynamic errors/status;
- icon-only controls with labels;
- reduced-motion support;
- touch targets ≥44×44px.

Charts MUST provide textual interpretation or accessible data equivalents.

---

# 26. SECURITY AND PRIVACY

Required:

- HTTPS in production;
- secure cookies;
- Argon2id or approved password hashing;
- rate limiting;
- tenant isolation;
- server-side feature gating;
- cryptographic webhook verification;
- idempotent webhook handling;
- audit logging;
- minimum OAuth scopes;
- data minimization;
- no unnecessary raw notification/SMS/email archives;
- secure native storage where appropriate;
- trusted-origin WebView restrictions.

Sensitive financial endpoints MUST use:

```http
Cache-Control: private, no-store
```

Native ingestion MUST require explicit user authorization.

---

# 27. AUDIT LOGGING

Security-sensitive and financially consequential operations SHOULD produce immutable audit records for:

- subscription activation;
- subscription renewal;
- trial activation;
- payment settlement;
- webhook processing;
- entitlement changes;
- significant account/security events;
- automatic transaction ingestion decisions where traceability is required.

Audit logs MUST NOT replace authoritative domain state.

---

# 28. API CONTRACTS

Use stable language-neutral contracts.

Preferred:

- OpenAPI;
- JSON Schema where appropriate.

Contracts MUST define:

- request/response schemas;
- authentication requirements;
- authorization requirements;
- error responses;
- idempotency behavior;
- pagination;
- cursor synchronization;
- WebSocket event schemas.

Vue and Kotlin MUST consume the same backend contract rather than implementing divergent business semantics.

---

# 29. TESTING REQUIREMENTS

Tests MUST cover:

## Financial correctness

- balance calculations;
- income;
- expenses;
- net cash flow;
- category aggregation;
- multi-account behavior;
- checked arithmetic;
- transaction atomicity.

## Idempotency

- repeated transaction submission;
- duplicate ingestion;
- duplicate webhook callbacks;
- retry behavior.

## Subscription

- Free;
- trial activation;
- one-time trial enforcement;
- expiration;
- Premium activation;
- renewal;
- invalid webhook;
- replayed webhook;
- entitlement restoration after reinstall/authentication.

## Ingestion

- notification parser;
- SMS parser where available;
- Gmail parser;
- normalization;
- confidence;
- deduplication;
- malformed input;
- false-positive prevention.

## Platform

- browser PWA;
- Android WebView;
- native bridge;
- lifecycle transitions;
- foreground/background synchronization;
- offline/online reconciliation.

---

# 30. EXISTING QUALITY BASELINES

Existing project materials record:

```text
Frontend build: npm run build → 0 errors
Rust tests: cargo test --workspace → 108 passed
E2E: bash e2e_tests/runner.sh all → 334 passed
Live health: https://api.nurdiansyahlabs.com/health → HTTP 200
```

These are regression baselines, not a ceiling on future test coverage.

New architecture must preserve or supersede them with explicit evidence.

---

# 31. DEPLOYMENT

Target API:

```text
https://api.nurdiansyahlabs.com
```

Deployment must preserve existing infrastructure.

If integrated with the NurdiansyahLabs monorepo:

- React remains independent;
- Vue PWA remains independent;
- Rust backend remains independent;
- SQLite remains independent;
- existing React/cPanel deployment remains protected;
- Finance changes MUST NOT trigger unrelated cPanel FTP deployment;
- React changes MUST NOT trigger Finance builds.

---

# 32. APPLICATION BOUNDARIES

```text
NurdiansyahLabs React
        ≠
Invinite Vue PWA
        ≠
Invinite Rust Backend
        ≠
Android Kotlin Shell
```

They may share contracts/assets/tooling where appropriate, but MUST NOT be collapsed into incompatible runtimes.

Do NOT:

- make Rust depend on TypeScript packages;
- merge React and Vue runtimes;
- merge SQLite with an unrelated database;
- duplicate financial domain logic in Kotlin;
- create a second Android financial UI solely for packaging.

---

# 33. NON-GOALS

Current architecture does NOT require:

- microservices;
- gRPC between internal modules;
- Redis solely for realtime;
- permanent WebSocket connections;
- unrestricted SMS collection;
- complete Gmail mailbox mirroring;
- complete notification archives;
- duplicate Android UI;
- floating-point financial arithmetic.

Architecture may evolve only when measurable requirements justify added complexity.

Evolution:

```text
Modular Monolith
      ↓
Measure real constraints
      ↓
Extract only a real independent boundary
      ↓
Independent service
      ↓
gRPC/event-driven communication if justified
```

---

# 34. IMPLEMENTATION ORDER

Agents MUST implement in dependency order.

### Phase 1 — Domain and persistence

1. Database schema/migrations.
2. Domain value objects.
3. Ledger invariants.
4. Repositories.
5. Idempotency.

### Phase 2 — Backend services

6. Authentication.
7. Accounts.
8. Categories.
9. Transactions.
10. Analytics/reports.
11. Subscription/entitlement.
12. Payment/webhook processing.

### Phase 3 — PWA

13. API contract.
14. Pinia state.
15. Core navigation.
16. Dashboard.
17. Transactions.
18. Add transaction.
19. Analytics.
20. Profile/subscription.
21. Offline shell.
22. Synchronization.

### Phase 4 — Realtime

23. WebSocket event model.
24. Foreground lifecycle.
25. Push/background synchronization.
26. Cursor/delta sync.

### Phase 5 — Android

27. Kotlin shell.
28. WebView security.
29. JS bridge.
30. First-launch activation.
31. Notification ingestion.
32. Background synchronization.
33. Push.
34. Secure storage.

### Phase 6 — Financial intelligence

35. Source adapters.
36. Parsers.
37. Normalizer.
38. Validator.
39. Confidence engine.
40. Deduplication.
41. User confirmation workflow.

### Phase 7 — Verification

42. Unit tests.
43. Integration tests.
44. E2E tests.
45. Android lifecycle tests.
46. Ingestion fixtures.
47. Security tests.
48. Production edge verification.

No later phase may bypass unresolved correctness failures in an earlier phase.

---

# 35. DEFINITION OF DONE

Production-ready means:

- backend builds cleanly;
- frontend builds cleanly;
- required tests pass;
- financial arithmetic is deterministic;
- transactions are atomic;
- idempotency is enforced;
- tenant isolation is verified;
- Premium gating is backend-authoritative;
- trial lifecycle is correct;
- payment webhooks are cryptographically verified;
- duplicate callbacks are harmless;
- notification ingestion is permission-aware;
- SMS behavior is capability-aware;
- Gmail access is minimum-scope and targeted;
- automatic transactions are validated and deduplicated;
- WebSocket is lifecycle-aware;
- background synchronization is battery-conscious;
- offline mutations reconcile idempotently;
- Android bridge is restricted and versioned;
- PWA remains independently usable;
- iOS remains functional as PWA;
- production deployment is verified;
- unrelated deployment pipelines remain intact;
- security and privacy controls are verified;
- architecture documentation matches implementation.

---

# 36. FINAL ARCHITECTURAL DECISION

```text
                    ┌──────────────────────────┐
                    │       Web / iOS PWA      │
                    │      Vue 3 + Vite        │
                    └────────────┬─────────────┘
                                 │
                    ┌────────────▼─────────────┐
                    │ Android Kotlin Shell     │
                    │ WebView + Native         │
                    │ Capabilities             │
                    └────────────┬─────────────┘
                                 │
                         HTTPS / WebSocket
                                 │
                    ┌────────────▼─────────────┐
                    │ Rust Axum Modular        │
                    │ Monolith                 │
                    │                          │
                    │ Auth                     │
                    │ Ledger                   │
                    │ Transactions             │
                    │ Subscription             │
                    │ Ingestion                │
                    │ Realtime                 │
                    └────────────┬─────────────┘
                                 │
                         SQLx / SQLite
                                 │
                    ┌────────────▼─────────────┐
                    │ SQLite WAL               │
                    │ Authoritative State      │
                    └──────────────────────────┘
```

Communication:

```text
Client ↔ Backend        HTTPS
Foreground realtime     WebSocket
Background              Push + OS work + delta sync
Backend module ↔ module In-process
Future extracted svc    gRPC only if justified
```

Financial ingestion:

```text
Notification
     ↓
SMS if legitimately available
     ↓
Gmail if explicitly connected
     ↓
Manual confirmation
```

All sources converge through:

```text
Adapter
 → Parser
 → Normalizer
 → Validator
 → Confidence
 → Deduplication
 → Domain Validation
 → Transaction
```

Subscription:

```text
Backend = authority
Kotlin = first-launch/native activation layer
PWA = presentation
Provider = payment evidence
Audit log = traceability
```

This is the authoritative Personal Finance specification. The subsequent Teamwork Project Prompt MUST reference this specification as its source of truth and focus on orchestration, delegation, implementation discipline, verification, and Definition of Done rather than redefining product architecture.
