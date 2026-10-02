# INVINITE BUSINESS OS v4.1

## Full Platform Product Requirements Document

**Product:** Invinite Business OS
**Version:** 4.1
**Status:** Master Product & Architecture Specification
**Target:** Multi-tenant Business Operating System + Accounting Platform
**Primary Platform:** Universal PWA + Rust/Axum Backend
**Deployment:** ARM64-compatible, A20s pilot/staging + cloud ARM production path

---

# 1. Executive Summary

Invinite Business OS adalah platform operasional dan accounting yang memungkinkan satu sistem digunakan oleh berbagai jenis bisnis tanpa membuat aplikasi terpisah untuk setiap vertical.

Prinsip utamanya:

> **One Core, Many Businesses, One Financial Truth.**

Sistem menyediakan satu platform yang menangani:

* customer dan supplier;
* produk dan jasa;
* penjualan;
* invoice;
* piutang;
* pembayaran;
* accounting;
* inventory;
* purchasing;
* project/service operation;
* rental;
* F&B;
* agriculture;
* automation;
* messaging;
* document generation;
* reporting;
* subscription;
* workspace personalization.

Business type tidak menjadi codebase terpisah. Business type merupakan konfigurasi capability dan experience di atas core platform.

Arsitektur utama:

```text
                         INVINITE BUSINESS OS
                                  │
              ┌───────────────────┼───────────────────┐
              │                   │                   │
        PLATFORM CORE       FINANCIAL CORE       BUSINESS CORE
              │                   │                   │
        Identity              Accounting            CRM
        Tenant                Invoice               Commerce
        Membership            Payment               Inventory
        RBAC                  Receivable            Project
        Subscription          Payable               Rental
        Audit                 Tax                   F&B
                                                   Agriculture
                                  │
                                  ▼
                         TRANSACTIONAL OUTBOX
                                  │
                 ┌────────────────┼────────────────┐
                 │                │                │
             AUTOMATION       MESSAGING          DOCUMENT
              SIDECAR          SIDECAR            SIDECAR
                 │                │                │
             Scheduler        WhatsApp             PDF
             Rules            Email                Export
             Jobs             Notification
                                  │
                                  ▼
                           UNIVERSAL PWA
```

---

# 2. Product Vision

Invinite tidak diposisikan sebagai aplikasi invoice, aplikasi kasir, atau aplikasi accounting yang hanya melayani satu jenis bisnis.

Produk diposisikan sebagai:

> **Business Operating System yang dapat dikonfigurasi sesuai cara kerja bisnis, dengan accounting sebagai financial source of truth.**

Pemilik bisnis memilih business type saat onboarding.

Contoh:

* Warung
* Retail
* Online Shop
* Distributor
* Grosir
* Rental Motor/Mobil
* Bengkel
* Contractor
* Construction
* Renovation
* Electrical
* Plumbing
* AC Service
* Cleaning Service
* Freelancer
* Event Service
* Restaurant
* Café
* Agriculture

Setiap business type memperoleh workspace, module, terminology, dashboard, workflow, dan automation yang relevan.

Namun seluruh bisnis menggunakan core yang sama.

---

# 3. Product Philosophy

## 3.1 Core owns truth

Core bertanggung jawab terhadap:

* tenant;
* identity;
* authorization;
* customer;
* invoice;
* payment;
* receivable;
* payable;
* accounting;
* financial state;
* subscription;
* audit;
* domain state.

Sidecar tidak menjadi sumber kebenaran bisnis.

---

## 3.2 Sidecars execute peripheral work

Sidecar menangani pekerjaan yang:

* asynchronous;
* retryable;
* external;
* scheduled;
* computational;
* document-oriented;
* messaging-oriented.

Contoh:

* WhatsApp;
* email;
* PDF;
* scheduler;
* automation;
* notification;
* export.

---

## 3.3 Modular monolith first

Backend tetap menggunakan modular monolith.

Tidak ada kewajiban memecah sistem menjadi microservices.

```text
Rust/Axum
    │
    ├── auth
    ├── tenant
    ├── billing
    ├── customer
    ├── invoice
    ├── payment
    ├── accounting
    ├── inventory
    ├── rental
    ├── project
    ├── automation
    └── ...
```

Sidecar hanya digunakan ketika isolation dan asynchronous execution memberikan manfaat nyata.

---

# 4. Target Architecture

```text
                       ┌──────────────────────────┐
                       │       Universal PWA      │
                       │ Vue / PWA / Mobile       │
                       └────────────┬─────────────┘
                                    │ HTTPS / WS
                                    ▼
                       ┌──────────────────────────┐
                       │      Rust + Axum Core    │
                       │      Modular Monolith     │
                       └────────────┬─────────────┘
                                    │
          ┌─────────────────────────┼─────────────────────────┐
          │                         │                         │
          ▼                         ▼                         ▼
   Platform Domain           Financial Domain          Business Domain
          │                         │                         │
   Tenant / User             Accounting               CRM
   Membership                Invoice                  Commerce
   RBAC                      Payment                  Inventory
   Subscription              Receivable               Rental
   Audit                     Payable                  Project
                             Tax                      F&B
                                                      Agriculture
                                    │
                                    ▼
                          Transactional Outbox
                                    │
             ┌──────────────────────┼──────────────────────┐
             ▼                      ▼                      ▼
       Automation Sidecar     Messaging Sidecar       PDF Sidecar
             │                      │                      │
       Scheduler              WhatsApp                 PDF
       Rule Engine             Email                   Documents
       Job Queue               Notifications            Reports
       Retry/Dedup
```

---

# 5. Multi-Tenancy

Setiap business workspace adalah tenant.

Minimal entities:

```text
User
Tenant
Membership
Role
Permission
BusinessProfile
BusinessType
Subscription
AuditEvent
```

Relationship:

```text
User
  │
  └── Membership
          │
          ▼
        Tenant
          │
          ├── BusinessProfile
          ├── Customers
          ├── Invoices
          ├── Payments
          ├── Journal
          └── Business Data
```

Tenant isolation bersifat mandatory.

Setiap repository operation yang membaca atau mengubah business data harus menerima `TenantContext`.

```rust
struct TenantContext {
    tenant_id: TenantId,
    actor_id: UserId,
    role: Role,
}
```

Repository tidak boleh menyediakan business-data API tanpa tenant context.

CI wajib memiliki cross-tenant isolation tests.

---

# 6. Identity and Access

Minimal:

* owner;
* administrator;
* manager;
* staff;
* accountant;
* custom role.

RBAC diterapkan di backend.

Frontend permission hanya UX control dan tidak dianggap sebagai security boundary.

Owner account mendukung:

* password/passkey;
* 2FA;
* session management;
* recovery mechanism.

---

# 7. Business Profile

Setiap tenant memiliki:

```text
BusinessProfile
├── business_name
├── legal_name
├── logo
├── address
├── phone
├── email
├── tax_identity
├── timezone
├── currency
├── invoice_prefix
├── business_type
└── branding
```

Default currency:

```text
IDR
```

Default locale:

```text
id-ID
```

Timezone:

```text
Asia/Jakarta
Asia/Makassar
Asia/Jayapura
```

Business owner dapat mengubah timezone sesuai lokasi bisnis.

---

# 8. Business Type Architecture

Business type adalah configuration layer.

Tidak boleh menghasilkan codebase berbeda.

```text
BusinessType
      │
      ├── Capability Set
      ├── Module Set
      ├── Terminology
      ├── Dashboard
      ├── Default CoA
      ├── Workflow
      └── Automation Templates
```

Contoh:

```json
{
  "type": "rental",
  "modules": [
    "customers",
    "assets",
    "reservations",
    "rental_orders",
    "invoices",
    "payments",
    "reports"
  ]
}
```

Template awal disimpan sebagai static manifests/versioned configuration.

Visual Workspace Builder merupakan capability lanjutan, bukan requirement untuk setiap template.

---

# 9. Business Capability Model

Capability reusable:

```text
CRM
Commerce
Inventory
Purchasing
Sales
Invoice
Payment
Receivable
Payable
Accounting
Project
Rental
Employee
Expense
Reporting
Automation
Messaging
Document
```

Business template memilih capability.

Contoh:

```text
Contractor
    = CRM
    + Project
    + Expense
    + Invoice
    + Payment
    + Receivable
    + Accounting
    + Automation
```

---

# 10. Core Domain Model

## Platform

```text
User
Tenant
Membership
Role
Permission
BusinessProfile
BusinessType
Subscription
AuditEvent
```

## CRM

```text
Contact
Customer
Supplier
Employee
CustomerAddress
CustomerChannel
```

## Commerce

```text
Product
Service
Price
SalesOrder
SalesOrderItem
Invoice
InvoiceItem
Payment
PaymentAllocation
```

## Accounting

```text
Account
JournalEntry
JournalLine
LedgerEntry
Receivable
Payable
Expense
Tax
FinancialPeriod
```

## Inventory

```text
Warehouse
StockItem
StockMovement
PurchaseOrder
PurchaseOrderItem
StockAdjustment
```

## Project

```text
Project
ProjectMember
Milestone
Task
ProjectExpense
ProjectMaterial
ProgressRecord
```

## Rental

```text
Asset
AssetCategory
Availability
Reservation
RentalOrder
RentalDeposit
RentalReturn
DamageCharge
```

## Automation

```text
AutomationRule
AutomationTrigger
AutomationCondition
AutomationAction
Job
JobAttempt
```

## Messaging

```text
MessageTemplate
Message
Delivery
ContactChannel
OptOut
```

---

# 11. Accounting Core

Accounting adalah salah satu foundation utama platform.

## 11.1 Double-entry bookkeeping

Setiap posted journal harus memenuhi:

```text
SUM(debit) == SUM(credit)
```

Invariant ini harus ditegakkan oleh:

* application logic;
* database constraints bila memungkinkan;
* integration tests;
* property-based tests.

---

## 11.2 Journal immutability

Posted journal tidak boleh:

* diedit;
* dihapus;
* dipindahkan secara langsung.

Correction dilakukan melalui reversal.

```text
Original Journal
      ↓
Reversal Journal
      ↓
Replacement Journal
```

---

## 11.3 Chart of Accounts

Default system accounts:

```text
1000 Cash
1100 Bank
1200 Accounts Receivable

2000 Accounts Payable
2100 Tax Payable

4000 Revenue
5000 Cost of Goods Sold
6000 Operating Expense
```

System-critical accounts tidak dapat dihapus.

Business template dapat menambahkan account sesuai domain.

---

# 12. Financial State Machine

## Invoice

```text
DRAFT
  ↓
ISSUED
  ↓
PARTIALLY_PAID
  ↓
PAID
```

Alternative:

```text
ISSUED
  ↓
VOIDED
```

Issued invoice tidak dapat diedit secara bebas.

---

## Payment

```text
PENDING
   ↓
CONFIRMED
```

atau:

```text
PENDING
   ↓
FAILED
```

---

## Receivable

```text
OPEN
PARTIALLY_PAID
PAID
OVERDUE
WRITTEN_OFF
```

State transition harus dilakukan oleh backend domain logic.

---

# 13. Invoice Engine

Invoice merupakan financial document.

Invoice harus memiliki:

```text
invoice_id
tenant_id
invoice_number
customer_id
issue_date
due_date
currency
subtotal
discount
tax
total
status
```

Line item:

```text
description
quantity
unit_price
discount
tax
line_total
```

---

# 14. Invoice Numbering

Invoice number:

* dibuat server;
* tenant scoped;
* assigned ketika invoice di-issue;
* tidak dibuat oleh client;
* tidak boleh berubah setelah issue.

Format dapat dikonfigurasi:

```text
INV-2026-000001
```

atau template lain.

Gapless numbering hanya diterapkan pada dokumen yang memang membutuhkan semantics tersebut; draft tidak memperoleh nomor final.

---

# 15. Issued Invoice Snapshot

Saat invoice diterbitkan, sistem menyimpan snapshot:

```text
customer_name_snapshot
customer_address_snapshot
line_item_snapshot
price_snapshot
tax_snapshot
subtotal
total
```

Perubahan customer/product setelah invoice diterbitkan tidak mengubah historical document.

---

# 16. Indonesian Tax

Tax engine harus mendukung architecture untuk:

* PPN;
* UMKM final tax;
* tax-inclusive pricing;
* tax-exclusive pricing;
* rounding;
* tax exemption;
* tax category.

Tax rules harus dipisahkan dari UI.

Semua perhitungan memiliki golden tests.

Perubahan peraturan pajak harus dapat menghasilkan versioned tax configuration.

---

# 17. Receivable

Receivable menjadi financial representation dari tagihan customer.

```text
Invoice
   ↓
Receivable
   ↓
Payment Allocation
   ↓
Outstanding Balance
```

Sistem menghitung:

```text
invoice_total
- allocated_payment
= outstanding
```

Overdue ditentukan berdasarkan:

```text
due_date < current_business_date
AND outstanding > 0
```

---

# 18. Payment

Payment dapat berasal dari:

* cash;
* bank transfer;
* QRIS;
* payment gateway;
* manual confirmation;
* future payment integrations.

Payment harus idempotent.

Setiap mutation menerima:

```text
Idempotency-Key
```

Server menyimpan hasil mutation sehingga retry menghasilkan response yang konsisten.

---

# 19. Payment Gateway Architecture

Payment gateway menggunakan adapter:

```text
PaymentGateway
├── ManualPayment
├── MidtransAdapter
├── XenditAdapter
└── FutureProvider
```

Core tidak bergantung pada vendor tertentu.

Payment success dari provider menghasilkan domain event.

---

# 20. Transactional Outbox

Semua domain event yang membutuhkan reliable asynchronous delivery menggunakan outbox.

Contoh payment:

```text
BEGIN

INSERT payment
INSERT journal
INSERT journal_lines
INSERT outbox_event

COMMIT
```

Kemudian:

```text
outbox
   ↓
sidecar
```

Outbox:

```text
id
tenant_id
event_type
aggregate_type
aggregate_id
payload_json
created_at
published_at
attempt_count
last_error
```

Delivery semantics:

```text
AT-LEAST-ONCE
```

Consumer wajib idempotent berdasarkan `event_id`.

---

# 21. Sidecar Boundary

Sidecar tidak boleh melakukan direct writes ke core tables.

Sidecar:

```text
READ EVENT
    ↓
EXECUTE
    ↓
STORE SIDECAR STATE
    ↓
CALL CORE API WHEN NEEDED
```

Core tetap menjadi authority.

---

# 22. Automation Sidecar

Automation engine:

```text
Trigger
   ↓
Condition
   ↓
Action
   ↓
Job
   ↓
Execution
   ↓
Retry / Failure
```

Trigger:

```text
InvoiceIssued
InvoiceDueSoon
InvoiceOverdue
PaymentReceived
CustomerCreated
ProjectMilestoneReached
RentalEnding
StockLow
```

Actions:

```text
SendMessage
CreateReminder
CreateTask
GenerateDocument
Webhook
UpdateWorkflow
```

---

# 23. Job Queue

Job memiliki:

```text
job_id
tenant_id
job_type
payload
status
scheduled_at
attempt_count
last_error
created_at
completed_at
```

States:

```text
QUEUED
RUNNING
SUCCEEDED
FAILED
RETRYING
CANCELLED
```

Retry menggunakan exponential backoff.

Deduplication menggunakan deterministic key/event ID sesuai job type.

---

# 24. Messaging Engine

Messaging menjadi abstraction:

```text
Message
   ↓
Channel Adapter
   ├── WhatsApp
   ├── Email
   └── Future channels
```

Messaging engine memiliki:

* templates;
* scheduling;
* queue;
* retry;
* deduplication;
* delivery status;
* opt-out;
* audit.

---

# 25. WhatsApp

Dua mode:

## Assisted

Free/pilot mode.

```text
Invoice overdue
      ↓
Reminder generated
      ↓
Prefilled wa.me link
      ↓
Owner taps Send
```

Tidak membutuhkan WhatsApp API automation.

## Automated

Paid/advanced mode.

```text
Automation
   ↓
WhatsApp adapter
   ↓
Official Cloud API / licensed provider
```

Platform tidak bergantung pada private/reverse-engineered WhatsApp protocol.

Queue wajib menghormati:

* timezone;
* quiet hours;
* opt-out;
* rate limits;
* provider failures.

---

# 26. Document/PDF Sidecar

Document sidecar menangani:

* invoice PDF;
* quotation;
* receipt;
* reports;
* statements;
* future business documents.

Core hanya menyimpan document metadata/reference.

Rendering dilakukan asynchronously bila tidak membutuhkan synchronous response.

---

# 27. Inventory

Inventory engine menyediakan:

```text
Product
Warehouse
StockItem
StockMovement
PurchaseOrder
StockAdjustment
```

Semua stock mutation menghasilkan financial/business event bila relevan.

Inventory valuation harus dipilih secara eksplisit sebelum production accounting inventory digunakan:

```text
Average Cost
```

atau:

```text
FIFO
```

Pilihan valuation menjadi domain invariant dan tidak boleh berubah sembarangan setelah transaksi production berjalan.

---

# 28. Retail / Warung / Online Shop Template

Default modules:

```text
Customers
Products
Inventory
Purchasing
Sales
Invoices
Payments
Expenses
Receivables
Reports
Accounting
Automation
```

Use cases:

* penjualan;
* stok;
* pembelian;
* hutang supplier;
* piutang customer;
* invoice;
* pembayaran;
* laporan laba/rugi.

---

# 29. Distributor / Grosir Template

Modules:

```text
Customers
Suppliers
Products
Warehouse
Purchasing
Sales
Delivery
Invoices
Receivables
Payments
Accounting
Reports
Automation
```

Focus:

* wholesale pricing;
* warehouse;
* customer credit;
* receivable;
* delivery;
* purchasing.

---

# 30. Rental Template

Entities:

```text
Customer
Asset
Reservation
RentalOrder
Deposit
Return
DamageCharge
Invoice
Payment
```

Workflow:

```text
Reservation
   ↓
Rental
   ↓
Return
   ↓
Additional Charges
   ↓
Invoice
   ↓
Payment
```

Asset availability menjadi domain state.

---

# 31. Project / Contractor Template

Entities:

```text
Customer
Project
Milestone
Task
Material
Labor
Expense
Progress
Invoice
Payment
```

Workflow:

```text
Project
   ↓
Milestone
   ↓
Progress
   ↓
Invoice
   ↓
Receivable
   ↓
Payment
```

Cocok untuk:

* contractor;
* construction;
* renovation;
* electrical;
* plumbing;
* AC;
* cleaning;
* interior;
* freelancer;
* event service.

---

# 32. Food & Beverage Template

Entities:

```text
Menu
Ingredient
Recipe
Table
Order
Inventory
Purchasing
Customer
Payment
Expense
```

Workflow:

```text
Order
 ↓
Recipe consumption
 ↓
Inventory movement
 ↓
Payment
 ↓
Accounting
```

---

# 33. Agriculture Template

Entities:

```text
Land
Crop
PlantingCycle
Seed
Fertilizer
Pesticide
Labor
Harvest
Buyer
Sale
Expense
```

Workflow:

```text
Planting
 ↓
Cultivation
 ↓
Expense
 ↓
Harvest
 ↓
Sale
 ↓
Revenue
 ↓
Profit
```

Agriculture merupakan capability template dan tetap menggunakan Financial Core yang sama.

---

# 34. Custom Fields

Custom fields didukung tanpa mengubah financial schema.

Model awal:

```text
custom_data JSON
schema_version
```

Custom fields tidak boleh memodifikasi:

* journal;
* ledger;
* financial state;
* invoice totals;
* tenant isolation rules;
* authentication.

Index/generate columns hanya dibuat untuk custom attributes yang terbukti membutuhkan query performance.

Pure EAV tidak menjadi default architecture.

---

# 35. Workspace Configuration

Workspace configuration:

```text
BusinessType
CapabilitySet
ModuleSet
Navigation
Dashboard
Terminology
Branding
AutomationTemplates
```

Contoh:

```text
Rental
"Customer" → "Penyewa"
"Asset"    → "Kendaraan"
"Invoice"  → "Tagihan Sewa"
```

Terminology override tidak mengubah domain semantics.

---

# 36. Workspace Builder

Workspace Builder memungkinkan tenant melakukan customization.

Kemampuan:

* enable/disable module;
* reorder navigation;
* dashboard widgets;
* custom fields;
* terminology;
* branding;
* workflow configuration.

Namun builder harus berada di atas governed capabilities.

Tenant tidak boleh membuat arbitrary financial logic yang bypass accounting invariants.

---

# 37. Universal PWA

PWA menjadi universal frontend.

Core components:

```text
AppShell
NavigationEngine
DashboardEngine
FormEngine
TableEngine
ReportEngine
ModuleRenderer
WorkspaceConfig
Branding
NotificationCenter
```

PWA menerima workspace configuration dari backend.

Tidak perlu build frontend berbeda untuk setiap business type.

---

# 38. Dashboard

Dashboard bersifat capability-driven.

Contoh common widgets:

```text
Revenue
Outstanding Receivable
Overdue Invoice
Cash
Expenses
Profit
Pending Payment
Upcoming Due
```

Vertical-specific widgets:

Rental:

```text
Available Assets
Active Rentals
Upcoming Returns
```

Project:

```text
Active Projects
Milestone Progress
Unbilled Work
```

Retail:

```text
Stock
Low Stock
Sales
Purchase
```

---

# 39. Import / Export

Platform wajib menyediakan:

```text
CSV Import
CSV Export
```

Import awal:

* customers;
* products;
* suppliers;
* opening balances;
* invoices where safe;
* inventory opening balance.

Export:

* customers;
* invoices;
* payments;
* ledger;
* reports;
* business data.

Import harus memiliki:

```text
preview
validation
error report
dry run
commit
```

---

# 40. Public Invoice

Invoice dapat memiliki public payment/view link.

Token:

* cryptographically random;
* minimal 128-bit entropy;
* revocable;
* rate-limited;
* noindex;
* tidak menggunakan sequential ID.

Contoh:

```text
https://app.example/invoice/<random-token>
```

Public page hanya mengekspos data yang memang diperlukan.

---

# 41. Routing and Tenant Domain

Initial routing:

```text
app.invinite.id/<tenant-slug>
```

Future:

```text
tenant.invinite.id
```

Custom domain dapat menjadi future capability.

Reserved names:

```text
admin
api
app
www
support
billing
status
docs
```

Slug validation mencegah impersonation dan takeover.

---

# 42. Audit

Semua financial/security-sensitive mutation dicatat.

Audit event:

```text
id
tenant_id
actor_id
action
entity_type
entity_id
payload
timestamp
ip
previous_hash
current_hash
```

Audit chain bersifat tamper-evident.

Audit tidak dapat diubah melalui normal application API.

---

# 43. Offline

Offline digunakan secara terbatas.

Allowed offline:

```text
Customer draft
Invoice draft
Notes
Non-critical UI state
```

Server authoritative:

```text
Invoice issue
Payment confirmation
Stock mutation
Journal posting
Financial settlement
```

Setiap mutation memiliki idempotency key.

Conflict policy ditentukan per entity.

Contoh:

```text
Issued Invoice → server authoritative
Payment        → server authoritative
Notes          → last-write-wins acceptable
Draft          → merge policy
```

---

# 44. Subscription

Model awal:

```text
Free
Premium Monthly
Premium Annual
```

Business plan dapat ditambahkan.

Subscription menentukan:

* enabled capabilities;
* automation limits;
* messaging limits;
* storage;
* users;
* integrations.

Financial/accounting data tidak boleh hilang hanya karena subscription downgrade.

---

# 45. Security

Minimum:

```text
TLS
Password hashing
Session security
RBAC
2FA
Tenant isolation
Idempotency
Audit
Rate limiting
CSRF protection where applicable
Input validation
SQL parameterization
Secret isolation
Encrypted backups
```

Public resources menggunakan random tokens.

Sensitive data tidak dimasukkan ke log secara sembarangan.

---

# 46. Backup

Backup strategy:

```text
Local backup
      +
Off-device backup
      +
Encrypted backup
      +
Restore verification
```

Untuk SQLite:

* WAL-aware backup;
* backup consistency;
* periodic snapshot;
* off-device replication where appropriate.

Restore drill harus dilakukan secara berkala.

---

# 47. RPO / RTO

Target awal:

```text
RPO ≤ 15 minutes
RTO ≤ 1 hour
```

Target tersebut harus dianggap sebagai engineering requirement hanya setelah infrastructure yang mendukungnya tersedia.

A20s pilot tidak dianggap memenuhi production redundancy requirement.

---

# 48. Deployment Architecture

## Pilot

```text
Samsung A20s
├── Samsung Kernel/BSP
├── Minimal Android hardware bridge
├── Native ARM64 server binaries
├── SQLite/PostgreSQL where required
├── Rust/Axum
├── Nginx
├── Cloudflare Tunnel
├── Automation
└── Messaging
```

A20s digunakan untuk:

* development;
* staging;
* demo;
* pilot;
* edge experimentation.

---

## Production

```text
ARM VPS
   │
   ├── Rust/Axum
   ├── Database
   ├── Sidecars
   ├── Reverse Proxy
   ├── Backup
   ├── Monitoring
   └── TLS
```

Binary production tetap ARM64-compatible.

A20s tidak menjadi single production dependency untuk paying customers.

---

# 49. Database Strategy

Initial database:

```text
SQLite WAL
```

Reason:

* simple;
* low operational overhead;
* cocok untuk initial deployment;
* cocok dengan resource-constrained ARM device.

Requirements:

```text
short write transactions
busy_timeout
controlled concurrency
repository abstraction
backup strategy
```

Database access harus melalui repository/application boundary.

SQL business logic tidak boleh tersebar di seluruh domain.

---

# 50. PostgreSQL Exit Path

Business logic tidak boleh bergantung secara langsung pada SQLite-only behavior.

Repository layer harus memungkinkan future PostgreSQL implementation.

```text
Domain
 ↓
Repository Trait
 ├── SQLite
 └── PostgreSQL
```

Migration ke PostgreSQL harus berupa infrastructure migration sebanyak mungkin, bukan business-domain rewrite.

---

# 51. Observability

Metrics:

```text
HTTP request latency
p50
p95
p99

RPS
4xx
5xx
timeouts

CPU
RAM
RSS
DB latency
DB connections
disk usage
disk I/O
queue depth
job failures
outbox lag
backup age
```

Alerts:

```text
queue depth high
outbox backlog
backup stale
disk nearly full
high error rate
high latency
job failure spike
database failure
```

---

# 52. Performance SLO

Initial target:

```text
Common reads:
p95 < 300 ms

Common writes:
p95 < 500 ms

Health endpoint:
p95 < 100 ms
```

Actual production SLO dapat disesuaikan berdasarkan workload nyata.

---

# 53. Testing Strategy

Testing wajib mencakup:

## Unit Tests

Domain logic.

## Integration Tests

Database + application.

## Property-based Tests

Contoh invariant:

```text
For any valid sequence of financial operations:

trial_balance == 0
```

## Golden Tests

* tax;
* rounding;
* invoice totals;
* payment allocation.

## Contract Tests

OpenAPI contract antara backend dan PWA.

## Migration Tests

Migration harus diuji menggunakan representative data dari Invinite v3.1.

## Tenant Isolation Tests

Tenant A tidak boleh:

```text
read Tenant B
write Tenant B
delete Tenant B
infer protected Tenant B data
```

---

# 54. API Architecture

REST/HTTP API menjadi primary interface.

WebSocket digunakan bila realtime diperlukan.

API contracts:

```text
/api/v1/auth
/api/v1/tenants
/api/v1/customers
/api/v1/products
/api/v1/invoices
/api/v1/payments
/api/v1/receivables
/api/v1/accounting
/api/v1/inventory
/api/v1/projects
/api/v1/rentals
/api/v1/automation
/api/v1/messages
/api/v1/reports
```

OpenAPI menjadi contract source.

---

# 55. Domain Events

Contoh:

```text
CustomerCreated
InvoiceIssued
InvoiceDueSoon
InvoiceOverdue
PaymentCreated
PaymentConfirmed
PaymentFailed
ExpenseRecorded
StockChanged
RentalStarted
RentalEnding
ProjectMilestoneReached
SubscriptionChanged
```

Domain event tidak otomatis berarti external event.

Outbox hanya digunakan ketika reliable asynchronous delivery diperlukan.

---

# 56. Failure Isolation

Failure sidecar tidak boleh merusak financial core.

Contoh:

```text
WhatsApp DOWN
    ≠
Payment FAILED
```

Payment tetap sukses.

Kemudian:

```text
outbox
 ↓
retry
 ↓
WhatsApp restored
 ↓
message delivered
```

Demikian juga:

```text
PDF renderer DOWN
    ≠
Invoice creation FAILED
```

Invoice tetap valid.

---

# 57. Automation Examples

## Invoice Reminder

```text
Invoice issued
       ↓
Due date approaching
       ↓
Automation rule
       ↓
Create reminder job
       ↓
Messaging adapter
       ↓
WhatsApp
```

## Overdue

```text
Invoice overdue
       ↓
Condition
       ↓
Check opt-out
       ↓
Generate message
       ↓
Queue
       ↓
Deliver
```

## Payment Received

```text
Payment confirmed
       ↓
Journal posted
       ↓
Outbox event
       ↓
Automation
       ↓
Receipt generated
       ↓
Customer notification
```

---

# 58. Business Automation

Tenant dapat membuat rules seperti:

```text
IF invoice overdue > 3 days
THEN notify owner
```

```text
IF invoice overdue > 7 days
THEN generate customer reminder
```

```text
IF payment received
THEN generate receipt
```

```text
IF stock < minimum
THEN notify manager
```

```text
IF rental ending tomorrow
THEN remind customer
```

---

# 59. Business OS Personalization

Tenant dapat memiliki:

```text
Logo
Business name
Colors
Terminology
Modules
Navigation
Dashboard
Invoice branding
Message templates
Automation
```

Namun financial invariants tetap platform-controlled.

---

# 60. Development Strategy

Development dilakukan sebagai satu platform, tetapi dalam dependency order.

```text
1. Platform Foundation
2. Tenant / Identity / RBAC
3. Accounting Core
4. Invoice / Receivable / Payment
5. Transactional Outbox
6. Inventory
7. Project
8. Rental
9. F&B
10. Agriculture
11. Automation
12. Messaging
13. Document
14. Universal PWA
15. Workspace Configuration
16. Custom Fields
17. Business Templates
18. Integrations
19. Production Cloud
20. Hardening
```

Full platform scope tetap dipertahankan.

Namun setiap phase harus menghasilkan working system, bukan hanya schema.

---

# 61. Vertical Development Model

Setiap vertical harus mengikuti pola:

```text
Business Operation
       ↓
Domain State
       ↓
Business Event
       ↓
Financial Event
       ↓
Accounting
       ↓
Reporting
       ↓
Automation
```

Contoh:

```text
Rental Return
      ↓
Damage Charge
      ↓
Additional Invoice
      ↓
Receivable
      ↓
Payment
      ↓
Journal
```

---

# 62. Universal Financial Model

Semua vertical menghasilkan financial primitives yang sama:

```text
Revenue
Expense
Asset
Liability
Receivable
Payable
Cash
Tax
Inventory
```

Business module hanya menentukan bagaimana financial event tersebut terjadi.

---

# 63. Reporting

Common reports:

```text
Profit & Loss
Balance Sheet
Cash Flow
Trial Balance
General Ledger
Accounts Receivable Aging
Accounts Payable Aging
Sales
Expenses
Tax
```

Vertical reports:

```text
Rental utilization
Project profitability
Inventory turnover
F&B ingredient consumption
Agriculture harvest profitability
```

---

# 64. Data Portability

Tenant harus dapat mengekspor data penting.

Export mencakup:

```text
Customers
Products
Invoices
Payments
Expenses
Journal
Ledger
Reports
Business configuration
```

Platform tidak boleh menciptakan intentional data lock-in.

---

# 65. Migration from Invinite v3.1

Invinite v3.1 menjadi source system untuk migration.

Migration strategy:

```text
v3.1
 ↓
Data Mapping
 ↓
Canonical Model
 ↓
Validation
 ↓
Dry Run
 ↓
Migration
 ↓
Reconciliation
 ↓
v4
```

Migration tidak boleh langsung mengubah production data tanpa backup dan validation.

Existing financial semantics harus dipetakan ke Financial Core v4.

Spesifikasi v3.1 sudah menetapkan Rust/Axum modular monolith, SQLite WAL, financial domain, authentication, tenancy, idempotency, realtime, testing, dan deployment boundaries sehingga komponen tersebut menjadi starting point, bukan aplikasi yang dibuang.

---

# 66. Existing PWA

PWA v3.1 tidak dihapus.

PWA di-refactor menuju:

```text
Universal Business OS PWA
```

Existing screens yang masih valid dipertahankan dan dipindahkan secara bertahap ke capability-based architecture.

Tidak dilakukan destructive rewrite tanpa alasan teknis.

---

# 67. Definition of Done

Feature tidak dianggap selesai hanya karena UI sudah bekerja.

## Accounting

```text
✓ Double-entry enforced
✓ Trial balance always zero
✓ Posted journals immutable
✓ Reversal supported
✓ Tax golden tests
✓ Rounding deterministic
```

## Tenant

```text
✓ TenantContext mandatory
✓ Cross-tenant reads blocked
✓ Cross-tenant writes blocked
✓ CI isolation tests
```

## Invoice

```text
✓ Server-side numbering
✓ Snapshot
✓ Immutable issued state
✓ Idempotent mutation
```

## Outbox

```text
✓ Atomic with domain transaction
✓ At-least-once delivery
✓ Event ID deduplication
✓ Retry
```

## Automation

```text
✓ Durable job state
✓ Retry
✓ Deduplication
✓ Failure isolation
```

## Messaging

```text
✓ Assisted mode
✓ Adapter architecture
✓ Opt-out
✓ Quiet hours
✓ Rate limiting
```

## Security

```text
✓ RBAC
✓ 2FA
✓ Audit
✓ Public token security
✓ Rate limiting
✓ Encrypted backups
```

## Backup

```text
✓ Backup
✓ Off-device copy
✓ Restore test
✓ Documented RPO/RTO
```

## Performance

```text
✓ p95 monitored
✓ Queue depth monitored
✓ Error rate monitored
✓ Database latency monitored
```

---

# 68. Product Success Metrics

Success tidak hanya diukur berdasarkan jumlah feature.

Core metrics:

```text
Active Tenants
Paying Tenants
Monthly Active Businesses
Invoices Created
Invoices Paid
Receivable Recovery Rate
Automation Execution Success
Payment Processing Success
Customer Retention
```

Operational metrics:

```text
p95 latency
5xx rate
outbox lag
queue depth
backup age
database errors
```

Business adoption metrics:

```text
Time-to-first-invoice
Time-to-first-payment
Time-to-first-business-report
Onboarding completion
```

Target onboarding:

> Tenant memperoleh usable workspace dalam waktu kurang dari 5 menit.

---

# 69. Product Experience

Onboarding:

```text
Register
   ↓
Choose Business Type
   ↓
Business Profile
   ↓
Logo
   ↓
Workspace Generated
   ↓
Add Customer
   ↓
Create Invoice
   ↓
Send Invoice
   ↓
Track Payment
```

Business owner tidak perlu memahami accounting untuk mulai menggunakan produk.

Accounting bekerja di belakang layar.

---

# 70. User Value Proposition

Core promise:

> **Kelola operasi bisnis dan keuangan dari satu sistem yang mengikuti cara kerja bisnis Anda.**

Contoh konkret:

Warung:

> jual → stok berkurang → uang masuk → laporan berubah.

Contractor:

> project → progress → invoice → piutang → pembayaran → profit.

Rental:

> booking → kendaraan keluar → kembali → charge → invoice → pembayaran.

Distributor:

> pembelian → warehouse → penjualan → delivery → piutang → pembayaran.

F&B:

> order → ingredient consumption → payment → accounting.

Agriculture:

> planting → expense → harvest → sale → profit.

---

# 71. Long-Term Platform Model

Arsitektur jangka panjang:

```text
                    INVINITE OS
                         │
       ┌─────────────────┼─────────────────┐
       │                 │                 │
    PLATFORM          FINANCIAL         CAPABILITIES
       │                 │                 │
     Tenant           Ledger           Commerce
     Identity         Invoice          Inventory
     RBAC             Payment          Rental
     Billing          Tax              Project
     Audit            Receivable       F&B
                       │                Agriculture
                       │
                       ▼
                 AUTOMATION
                       │
                       ▼
                  INTEGRATIONS
                       │
        ┌──────────────┼──────────────┐
        │              │              │
     WhatsApp        Payment        Email
     Providers       Gateway        Other APIs
```

Business templates hanya merupakan preset experience di atas platform.

---

# 72. Architecture Rules

Rules yang tidak boleh dilanggar:

1. Core owns business truth.
2. Financial core owns financial truth.
3. Sidecars never directly mutate core tables.
4. Every tenant-scoped repository requires TenantContext.
5. Posted journals are immutable.
6. Financial corrections use reversal.
7. Money uses integer representation.
8. Invoice issue is server-authoritative.
9. Payment mutation is idempotent.
10. Domain transaction and required outbox event commit atomically.
11. Sidecar delivery is at-least-once.
12. Consumers are idempotent.
13. External provider failure cannot corrupt financial state.
14. Business templates configure capabilities; they do not create separate codebases.
15. Custom data cannot bypass financial invariants.
16. Production infrastructure must not depend permanently on one physical phone.
17. Database implementation remains replaceable.
18. Security is enforced server-side.
19. Audit data is tamper-evident.
20. Every critical financial invariant has automated tests.

---

# 73. What Is Explicitly Not the Goal

Platform tidak bertujuan:

* menjadi ERP enterprise penuh sejak hari pertama;
* membuat microservice untuk setiap domain;
* membuat Kubernetes deployment tanpa kebutuhan;
* membuat visual builder sebelum capability stabil;
* membuat private WhatsApp protocol;
* membuat setiap business type menjadi aplikasi berbeda;
* mengorbankan financial correctness demi UX;
* mengorbankan tenant isolation demi development speed.

---

# 74. Final Product Decision

Invinite Business OS v4.1 tetap menggunakan visi full-platform.

Keputusan produk:

> **Build the Business OS, not a collection of vertical apps.**

Keputusan engineering:

> **One modular core, one financial truth, reusable capabilities, sidecars for peripheral execution, configurable business templates, universal PWA.**

Keputusan financial:

> **Double-entry, immutable journal, server-authoritative financial state, transactional outbox, idempotency, tenant isolation.**

Keputusan deployment:

> **ARM64-native architecture; Samsung A20s sebagai pilot/staging/edge appliance dan ARM VPS/cloud sebagai production path.**

Keputusan product architecture:

```text
Business Type
      ↓
Capability Configuration
      ↓
Universal Workspace
      ↓
Business Operations
      ↓
Financial Events
      ↓
Accounting Core
      ↓
Automation
      ↓
Messaging / Documents / Integrations
```

---

# 75. North Star

Invinite tidak hanya menjawab:

> “Bagaimana saya membuat invoice?”

Invinite menjawab:

> **“Bagaimana seluruh bisnis saya dapat berjalan dari satu sistem yang memahami operasi, uang, pelanggan, pekerjaan, aset, dan automation?”**

Satu business dapat memulai dari invoice.

Kemudian berkembang menjadi:

```text
Customer
→ Sales
→ Inventory
→ Purchasing
→ Project/Rental
→ Invoice
→ Payment
→ Accounting
→ Reporting
→ Automation
→ Messaging
```

Semua tetap berada pada tenant dan financial model yang sama.

Itulah Business OS.

**Invinite Business OS v4.1 = one platform, many business models, one financial truth.**
