# Project: Invinite Business OS v4.1 Phase 1 Core Foundation

## Architecture
Invinite Business OS v4.1 evolves the Invinite v3.1 personal finance platform (Rust Axum 0.8 + SQLite WAL + Vue 3 PWA) into a multi-tenant business operating system governed by the maxim:
**"One Core, Many Businesses, One Financial Truth."**

### Core Principles
1. **Strict Multi-Tenancy**: The tenancy root boundary is `Tenant`. All domain and repository operations require an explicit `TenantContext { tenant_id, actor_id, role }`. Cross-tenant resource queries strictly return HTTP 404 Not Found to prevent entity existence enumeration; unauthorized tenant-internal actions return HTTP 403 Forbidden.
2. **Double-Entry Financial Truth**: Every posted business transaction is captured as balanced double-entry journal entries enforcing `SUM(debit) == SUM(credit)`. Posted journals are strictly immutable; corrections are achieved exclusively via balanced reversal journals. All monetary math operates on checked integer Rupiah (`i64`) without floating point.
3. **Commercial Invoicing & Receivables**: Invoices progress through a deterministic lifecycle (`DRAFT` -> `ISSUED` -> `PARTIALLY_PAID` -> `PAID` / `VOIDED`). Issuing locks a frozen document snapshot and assigns a server-side sequential gapless number. Payments allocate atomically against invoice receivables with automatic ledger posting and idempotency deduplication.
4. **Transactional Outbox & Sidecar Boundary**: Domain mutations and `outbox_events` records are committed within the same atomic SQLite transaction. Outbox events are dispatched asynchronously with at-least-once delivery, event ID deduplication, and exponential backoff. Peripheral sidecars (PDF, WhatsApp) never write directly to core tables, and third-party failures never compromise core transactions.
5. **Universal PWA & Backward Compatibility**: A single Vue 3 Composition API frontend dynamically adapts navigation, terminology, and modules according to workspace capability configurations without destructive rewrites. Legacy personal finance endpoints and test suites remain 100% operational via auto-provisioned personal workspaces.

---

## Feature Inventory

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Multi-Tenant Data Model | `tenants`, `business_profiles`, `memberships`, `users` schema & domain entities | M1 | PRD §5, §7, §10 |
| 2 | TenantContext Repository Scoping | Mandatory `TenantContext` parameter across all business repositories | M1 | PRD §5:272-286, ORIGINAL_REQUEST §R1 |
| 3 | Cross-Tenant HTTP 404 Isolation | Attempting to access another tenant's entity by ID strictly returns HTTP 404 | M1 | ORIGINAL_REQUEST §Acceptance Criteria |
| 4 | RBAC Authorization & HTTP 403 | Role-based permission checks; unauthorized tenant-internal actions return HTTP 403 | M1 | PRD §6:291-304 |
| 5 | Indonesian Localization Defaults | Currency `IDR`, locale `id-ID`, timezones `Asia/Jakarta` (WIB), WITA, WIT | M1 | PRD §7:334-356 |
| 6 | Tenant Slug Routing & Validation | Tenant URL slug validation rejecting reserved system slugs (`admin`, `api`, etc.) | M1 | PRD §41:1537-1568 |
| 7 | Personal Workspace Auto-Provisioning | Auto-provision default personal workspace on user registration for backward compatibility | M1 | Survey 2 & 3 |
| 8 | Double-Entry Balancing Invariant | Enforcement of `SUM(debit) == SUM(credit)` and `trial_balance == 0` | M2 | PRD §11.1:560-575 |
| 9 | Integer Rupiah Math Invariant | Strict `Rupiah(i64)` checked arithmetic, zero float in calculation paths | M2 | PRD §72:2565, money.rs |
| 10 | Journal Immutability | Posted journals cannot be edited or deleted directly | M2 | PRD §11.2:577-586 |
| 11 | Journal Reversal Workflow | Corrections performed exclusively via balanced reversal journals | M2 | PRD §11.2:587-595 |
| 12 | Standard Chart of Accounts | Seeded system accounts (1000, 1100, 1200, 2000, 2100, 4000, 5000, 6000) with deletion protection | M2 | PRD §11.3:597-618 |
| 13 | PPN Indonesian Tax Engine | 11% and 12% PPN calculation with deterministic half-up rounding | M2 | PRD §16:752-770 |
| 14 | UMKM Final Tax Engine | 0.5% (50 bps) final tax calculation on gross turnover | M2 | PRD §16:752-770 |
| 15 | Tax Inclusive/Exclusive Pricing | Deterministic net and tax extraction from gross or net unit prices | M2 | PRD §16:758-762 |
| 16 | Commercial Invoice Lifecycle | State machine: `DRAFT` -> `ISSUED` -> `PARTIALLY_PAID` -> `PAID` / `VOIDED` | M3 | PRD §12:620-644 |
| 17 | Server-Side Sequential Numbering | Server-authoritative sequential gapless numbering (`INV-YYYY-XXXXXX`) | M3 | PRD §14:712-732 |
| 18 | Issued Document Snapshotting | Immutable JSON snapshot of customer, line items, and taxes captured at issue | M3 | PRD §15:734-750 |
| 19 | Receivable Tracking & Aging | Outstanding balance tracking and aging buckets (0-30d, 31-60d, 61-90d, >90d) | M3 | PRD §17:772-800 |
| 20 | Atomic Payment Allocation | Atomic allocation of payments against invoices with automatic journal posting | M3 | PRD §17, §18 |
| 21 | Mutation Idempotency Engine | `Idempotency-Key` header deduplication with SHA-256 caching and HTTP 409 conflict handling | M3 | PRD §18:814-824 |
| 22 | Transactional Outbox Persistence | Atomic commit of domain state, journal lines, and `outbox_events` in single DB transaction | M4 | PRD §20:844-883 |
| 23 | At-Least-Once Outbox Dispatcher | Asynchronous outbox polling and delivery worker with exponential backoff retry | M4 | PRD §20:884-892 |
| 24 | Outbox Event Deduplication | Unique event ID deduplication for consumer idempotency | M4 | PRD §20:890 |
| 25 | Sidecar Isolation Boundary | Decoupled sidecar execution; external provider failures never fail core transactions | M4 | PRD §21, §56 |
| 26 | Frontend Workspace Store | Pinia `useWorkspaceStore` managing active workspace, switching, and capability state | M5 | PRD §8, §35, §37 |
| 27 | Header & Sidebar Workspace UI | Workspace dropdown switcher in `AppHeader.vue` and `DesktopSidebar.vue` | M5 | Survey 3 |
| 28 | API Client 404 Mock Fallback Fix | Remove latent HTTP 404 mock fallback in `api.js` to preserve cross-tenant 404 isolation | M5 | Survey 3 |
| 29 | Capability-Driven Navigation | Dynamic navigation tabs based on tenant capabilities while preserving v3.1 POS keypad | M5 | PRD §35, §37 |
| 30 | Backend Test Harness Synchronization | Update `m1_persistence_tests.rs` table count (26 tables) & verify all 14 test suites pass | M5 | Survey 2 |
| 31 | Full E2E Test Suite Pass (Tiers 1-4) | 100% pass across all 4 tiers of requirement-driven E2E tests | Final | Project Pattern |
| 32 | Adversarial Hardening (Tier 5) | Comprehensive adversarial coverage testing and clean forensic audit | Final | Project Pattern |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| Test | E2E Testing Track | Requirement-driven opaque-box test suite (Tiers 1-4) covering all 32 inventoried features; publishes `TEST_READY.md` | none | PLANNED |
| M1 | Multi-Tenant Architecture & Identity Boundaries | R1: `Tenant`, `User`, `Membership`, `Role`, `BusinessProfile`, `TenantContext`, cross-tenant 404, internal 403, Indonesian defaults, auto-provision personal workspace | none | DONE |
| M2 | Double-Entry Accounting Core & Financial State Machine | R2: `SUM(debit) == SUM(credit)`, integer `Rupiah(i64)`, immutable journals, reversal mechanics, Chart of Accounts, PPN/UMKM tax engine | M1 | DONE |
| M3 | Commercial Invoicing, Receivables, & Payment Allocation | R3: Invoicing state machine, sequential numbering, snapshots, receivable aging, atomic payment allocation, Idempotency-Key | M1, M2 | DONE |
| M4 | Transactional Outbox Pattern & Sidecar Boundary | R4: `outbox_events` table, atomic commit with domain mutations, asynchronous delivery, retry backoff, sidecar failure isolation | M1, M2, M3 | DONE |
| M5 | Universal PWA Alignment & Test Suite Synchronization | R5: Frontend `useWorkspaceStore`, header/sidebar switcher, fix 404 mock fallback in `api.js`, capability navigation, backend `m1_persistence_tests.rs` table count sync | M1, M2, M3, M4 | DONE |
| Final | Final Milestone & Adversarial Hardening | Phase 1: 100% E2E test suite pass (Tiers 1-4). Phase 2: Adversarial coverage hardening (Tier 5) with Challengers and Forensic Auditor | Test, M1, M2, M3, M4, M5 | DONE |

---

## Interface Contracts

### M1 (Multi-Tenancy) ↔ Repositories & Services
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub actor_id: Uuid,
    pub role: Role,
}

// All tenant-scoped repositories must take &TenantContext
pub trait TenantScopedRepository {
    async fn find_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Entity>, RepositoryError>;
}
// Cross-tenant lookup rule: If entity exists in DB under different tenant_id, repository returns Ok(None), and HTTP handler maps to HTTP 404 Not Found.
```

### M2 (Double-Entry Accounting) ↔ M3 (Invoicing & Payments)
```rust
pub struct PostJournalEntryCommand {
    pub tenant_id: Uuid,
    pub entry_date: DateTime<Utc>,
    pub description: String,
    pub source_type: String, // e.g., "INVOICE", "PAYMENT", "MANUAL"
    pub source_id: Option<Uuid>,
    pub lines: Vec<PostJournalLineCommand>,
}

pub struct PostJournalLineCommand {
    pub account_code: String, // e.g., "1200", "4000", "2100"
    pub debit: Rupiah,
    pub credit: Rupiah,
    pub memo: Option<String>,
}

// Invariant: lines.iter().map(|l| l.debit.0).sum::<i64>() == lines.iter().map(|l| l.credit.0).sum::<i64>()
```

### M3 (Invoicing & Payments) ↔ M4 (Transactional Outbox)
```rust
// Atomic transaction contract:
// Invoices or payments must be created inside an explicit database transaction
// that simultaneously inserts the corresponding OutboxEvent.
pub struct OutboxEventDraft {
    pub tenant_id: Uuid,
    pub event_type: String,     // "InvoiceIssued", "PaymentConfirmed", etc.
    pub aggregate_type: String, // "Invoice", "Payment"
    pub aggregate_id: String,
    pub payload_json: Value,
}
```

---

## Code Layout

```text
backend/
├── migrations/
│   ├── 0001_initial_schema.sql                # (v3.1 preserved)
│   ├── 0002_trial_and_dana_support.sql        # (v3.1 preserved)
│   ├── 0003_v3_1_0_schema_upgrade.sql         # (v3.1 preserved)
│   ├── 0004_subscription_lifecycle_v3_1_0.sql # (v3.1 preserved)
│   ├── 0005_user_preferences.sql              # (v3.1 preserved)
│   └── 0006_v4_1_core_foundation.sql          # [v4.1] Core foundation schema
├── src/
│   ├── domain/
│   │   ├── money.rs                           # (Preserved Rupiah(i64))
│   │   ├── tenant.rs                          # [M1] Tenant, Membership, Role, BusinessProfile, TenantContext
│   │   ├── accounting.rs                      # [M2] ChartOfAccounts, JournalEntry, JournalLine, TaxRule
│   │   ├── invoice.rs                         # [M3] Invoice, InvoiceItem, InvoiceSnapshot, InvoiceStatus
│   │   ├── receivable.rs                      # [M3] Receivable, Payment, PaymentAllocation
│   │   └── outbox.rs                          # [M4] OutboxEvent, OutboxStatus
│   ├── repository/
│   │   ├── tenant_repo.rs                     # [M1] TenantRepository & SqlxTenantRepository
│   │   ├── accounting_repo.rs                 # [M2] JournalRepository & AccountRepository
│   │   ├── invoice_repo.rs                    # [M3] InvoiceRepository
│   │   ├── receivable_repo.rs                 # [M3] ReceivableRepository
│   │   └── outbox_repo.rs                     # [M4] OutboxRepository
│   ├── service/
│   │   ├── tenant_service.rs                  # [M1] Workspace provisioning & validation
│   │   ├── accounting_service.rs              # [M2] Posting, trial balance, tax calculations
│   │   ├── invoice_service.rs                 # [M3] Sequential numbering, snapshots, lifecycle
│   │   ├── payment_service.rs                 # [M3] Atomic payment allocation
│   │   └── outbox_processor.rs                # [M4] Asynchronous worker with retry
│   ├── api/
│   │   ├── middleware/
│   │   │   ├── auth_extractor.rs              # (Enhanced with TenantContext)
│   │   │   └── tenant_extractor.rs            # [M1] Strict TenantContext extractor
│   │   ├── router.rs
│   │   ├── tenants.rs                         # [M1] /api/v1/tenants routes
│   │   ├── accounting.rs                      # [M2] /api/v1/accounting routes
│   │   ├── invoices.rs                        # [M3] /api/v1/invoices routes
│   │   └── receivables.rs                     # [M3] /api/v1/receivables routes
└── tests/
    ├── m1_persistence_tests.rs                # Synchronized table count (26)
    ├── v4_tenant_isolation_tests.rs           # [M1] Isolation verification
    ├── v4_double_entry_accounting_tests.rs    # [M2] Debit==Credit & tax vectors
    ├── v4_invoicing_receivables_tests.rs      # [M3] Invoicing lifecycle & payments
    └── v4_transactional_outbox_tests.rs       # [M4] Outbox atomic commit & worker

frontend/
├── src/
│   ├── stores/
│   │   └── workspace.js                       # [M5] useWorkspaceStore (Pinia)
│   ├── services/
│   │   └── api.js                             # [M5] X-Tenant-ID injection & fix 404 mock fallback
│   └── components/layout/
│       ├── AppHeader.vue                      # [M5] Workspace switcher dropdown
│       ├── DesktopSidebar.vue                 # [M5] Capability-driven navigation
│       └── MobileBottomNav.vue                # [M5] Capability-driven navigation

e2e_tests/
├── harness/
│   ├── client.py                              # [Test] Tenant-aware E2E HTTP client
│   └── server.py                              # [Test] Synchronized reference oracle
└── runner.sh                                  # [Test] TAP v13 test runner
```
