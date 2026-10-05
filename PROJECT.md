# Project: Invinite Business OS v4.1 Phase 2 Inventory & Multi-Location Stock Management

## Architecture
Phase 2 extends the Invinite Business OS v4.1 multi-tenant architecture with multi-warehouse tracking, stock movements, purchase orders, inventory adjustments, and automated double-entry GL ledger integration for COGS and inventory valuation.

### Core Principles
1. **Strict Multi-Tenancy & Isolation (R1)**:
   All inventory domain models (`Warehouse`, `StockItem`, `Product`, `StockMovement`, `PurchaseOrder`, `PurchaseOrderItem`, `StockAdjustment`) enforce strict `TenantContext` isolation. Cross-tenant lookups strictly return HTTP 404 Not Found (preventing enumeration), while tenant-internal unauthorized role actions return HTTP 403 Forbidden.
2. **Stock Mutation Integrity & Atomic Negative Balance Prevention (R2)**:
   Atomic stock movements (`INBOUND`, `OUTBOUND`, `TRANSFER`, `ADJUSTMENT`) with immutable audit trails. Negative balances are strictly prohibited across all warehouses (`CHECK (quantity_on_hand >= 0)`); deductions exceeding available balance abort with HTTP 422 Unprocessable Entity or HTTP 409 Conflict. Concurrently racing stock movements are serialized at the database/repository level using `BEGIN IMMEDIATE` write locks. Replayed mutations with identical `Idempotency-Key` headers return original responses.
3. **Purchase Orders & Goods Receipt State Machine (R3)**:
   Purchase order lifecycle (`DRAFT` -> `ORDERED` -> `PARTIALLY_RECEIVED` -> `RECEIVED` / `CANCELLED`) with server-side sequential numbering (`PO-YYYY-XXXXXX`). Inbound goods receipts atomically increment warehouse stock levels, update weighted average cost records, and capture supplier batch and cost details.
4. **Weighted Average Cost (WAC) & Double-Entry Accounting Invariants (R4)**:
   All unit costs, valuation totals, and COGS calculations operate strictly on integer Rupiah (`i64`) using `round_half_up_i128` without floating-point arithmetic. Inbound goods receipts generate balanced journal entries (`Debit Inventory 1300 / Credit Payables 2000`). Invoice fulfillments generate Cost of Goods Sold entries (`Debit COGS 5000 / Credit Inventory 1300`), strictly enforcing `SUM(debit) == SUM(credit)`. Canonical account `1300` ("Persediaan Barang Dagang") is seeded with deletion protection.
5. **Transactional Outbox & Universal PWA Alignment (R5)**:
   Transactional outbox events (`StockReceived`, `StockAdjusted`, `StockTransferred`) commit in the same SQLite transaction as domain mutations. Universal Vue 3 PWA navigation and Pinia stores dynamically surface inventory and purchasing capabilities when enabled, while strictly preserving personal workspace isolation and the rapid POS keypad trigger. All existing backend and E2E test suites maintain zero regressions.

---

## Feature Inventory

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Multi-Location Warehouse Management | Manage multiple warehouses per tenant with default designation and isolation | M1 | PRD §10, §27; ORIGINAL_REQUEST §R1 |
| 2 | Product Catalog & Sequential SKU Engine | Product catalog with unique sequential SKU (`SKU-XXXXXX`), unit, cost, sale price | M1 | PRD §10, §27; ORIGINAL_REQUEST §R1 |
| 3 | Multi-Location Stock Level Tracking | Per-location `quantity_on_hand`, `quantity_reserved`, `reorder_threshold`, `bin_location` | M1 | PRD §10, §27; ORIGINAL_REQUEST §R1 |
| 4 | Reorder Threshold & Low Stock Alerts | Filter and alert on items where `quantity_on_hand <= reorder_threshold` | M2 | PRD §27; ORIGINAL_REQUEST §R1 |
| 5 | Atomic Stock Movement Engine | Atomic stock movements (`INBOUND`, `OUTBOUND`, `TRANSFER`, `ADJUSTMENT`) with audit record | M2 | PRD §10, §27; ORIGINAL_REQUEST §R2 |
| 6 | Inter-Warehouse Transfer | Atomic transfer decrements source and increments destination; prevents source == destination | M2 | PRD §27; ORIGINAL_REQUEST §R2 |
| 7 | Immutable Stock Audit Trail | SQLite triggers prevent direct UPDATE or DELETE on `stock_movements` and `stock_adjustments` | M1 | PRD §10, §27, §72; ORIGINAL_REQUEST §R2 |
| 8 | Concurrency Serialization & Write Locking | `BEGIN IMMEDIATE` transaction write locks serialize racing mutations, preventing overselling | M2 | PRD §72; ORIGINAL_REQUEST §R2 |
| 9 | Mutation Idempotency Header Handling | `Idempotency-Key` header with SHA-256 payload caching returns cached replay | M2 | PRD §18, §72; ORIGINAL_REQUEST §R2 |
| 10 | Purchase Order Lifecycle State Machine | PO workflow: `DRAFT` -> `ORDERED` -> `PARTIALLY_RECEIVED` -> `RECEIVED` / `CANCELLED` (`PO-YYYY-XXXXXX`) | M3 | PRD §10, §12, §27; ORIGINAL_REQUEST §R3 |
| 11 | Inbound Goods Receipt Workflow | Inbound receipt increments stock, updates WAC, captures batch and unit cost | M3 | PRD §27; ORIGINAL_REQUEST §R3 |
| 12 | Purchase Order Cancellation | Cancellation allowed only prior to receipt; once partially or fully received, cancel is blocked | M3 | PRD §27; ORIGINAL_REQUEST §R3 |
| 13 | Weighted Average Cost (WAC) Engine | Integer Rupiah `i64` moving WAC math via `round_half_up_i128`, zero floating point | M3 | PRD §27, §72; ORIGINAL_REQUEST §R4 |
| 14 | Inbound Receipt Double-Entry Posting | Auto-post balanced journal: `Debit 1300 (Persediaan) / Credit 2000 (Utang Usaha)` | M3 | PRD §11, §27, §62; ORIGINAL_REQUEST §R4 |
| 15 | Invoice Fulfillment COGS Double-Entry | Auto-post balanced COGS journal: `Debit 5000 (Beban Pokok) / Credit 1300 (Persediaan)` | M3 | PRD §11, §27, §62; ORIGINAL_REQUEST §R4 |
| 16 | Canonical Account 1300 Seeding | Seed Account `1300` in Chart of Accounts with system protection and backfill | M1 | PRD §11.3; ORIGINAL_REQUEST §R4 |
| 17 | Physical Stock Count Adjustment | Cycle count adjustment (`ADJ-YYYY-XXXXXX`) with variance, reason, and GL journal | M2 | PRD §10, §27; ORIGINAL_REQUEST §R2 |
| 18 | StockReceived Transactional Outbox Event | Commit `StockReceived` event in same SQLite transaction as PO receipt | M3 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 19 | StockAdjusted Transactional Outbox Event | Commit `StockAdjusted` event in same SQLite transaction as stock adjustment | M3 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 20 | StockTransferred Transactional Outbox Event | Commit `StockTransferred` event in same SQLite transaction as warehouse transfer | M3 | PRD §20, §55; ORIGINAL_REQUEST §R5 |
| 21 | Outbox At-Least-Once Delivery & Deduplication | Outbox processor polls and delivers stock events with UUID deduplication | M3 | PRD §20; ORIGINAL_REQUEST §R5 |
| 22 | Frontend Capability-Driven Navigation | Dynamically surface "Inventaris & Stok" and "Pesanan Pembelian" in `DesktopSidebar` & `MobileBottomNav` | M4 | PRD §35, §37; ORIGINAL_REQUEST §R5 |
| 23 | Pinia Inventory Workspace Store Alignment | Store managing inventory state, active warehouse, and capability resolution | M4 | PRD §37; ORIGINAL_REQUEST §R5 |
| 24 | Schema Persistence Synchronization | Synchronize table count in `m1_persistence_tests.rs` & outbox tests from 30 to 37 tables | M1 | ORIGINAL_REQUEST §Acceptance Criteria |
| 25 | Full E2E Test Suite Pass (Tiers 1-4) | 100% pass across all 4 tiers of requirement-driven E2E tests with zero regressions | M5 | ORIGINAL_REQUEST §Acceptance Criteria |
| 26 | Adversarial Coverage Hardening (Tier 5) | Comprehensive adversarial coverage testing and clean forensic audit | M5 | Project Pattern |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| Test | E2E Testing Track | Design and maintain requirement-driven opaque-box test suite (Tiers 1-4) covering all 26 inventoried features; publishes `TEST_READY.md` | none | DONE |
| M1 | Schema & Persistence Foundation | Features 1, 2, 3, 7, 16, 24: Migration `0011_v4_1_inventory_management.sql` (7 tables), domain models, Account 1300 seeding, immutability triggers, test count sync (30->37) | none | DONE |
| M2 | Stock Movements Engine & Negative Balance Prevention | Features 4, 5, 6, 8, 9, 17: Atomic movements (`INBOUND`, `OUTBOUND`, `TRANSFER`, `ADJUSTMENT`), negative balance prevention (HTTP 422/409), `BEGIN IMMEDIATE` concurrency locks, `Idempotency-Key` caching, audit trails | M1 | PLANNED |
| M3 | Purchase Orders, Inbound Receipt & WAC GL Accounting | Features 10, 11, 12, 13, 14, 15, 18, 19, 20, 21: PO state machine, goods receipt, integer Rupiah WAC math, GL journal postings (1300/2000, 5000/1300), transactional outbox events | M1, M2 | PLANNED |
| M4 | Universal Vue 3 PWA Frontend & API Integration | Features 22, 23: Capability-driven navigation in `DesktopSidebar.vue` & `MobileBottomNav.vue`, `useWorkspaceStore`, POS keypad preservation, API routes mounting under `/api/v1/` | M1, M2, M3 | PLANNED |
| M5 | Final System Integration & Adversarial Hardening | Features 25, 26: Phase 1: 100% pass of E2E test suite (Tiers 1-4), `cargo test`, `cargo clippy --all-targets -- -D warnings`, `npm run build`. Phase 2: Adversarial coverage hardening (Tier 5) with Challengers and Forensic Auditor | Test, M1, M2, M3, M4 | PLANNED |

---

## Interface Contracts

### M1 ↔ Repositories & Domain
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warehouse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub cost_price: Rupiah,
    pub sale_price: Rupiah,
    pub reorder_threshold: i64,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub warehouse_id: Uuid,
    pub product_id: Uuid,
    pub quantity_on_hand: i64,
    pub quantity_reserved: i64,
    pub reorder_threshold: i64,
    pub bin_location: Option<String>,
    pub average_cost: Rupiah,
    pub updated_at: DateTime<Utc>,
}
```

### M2 ↔ Stock Movement & Concurrency Contract
```rust
pub enum StockMovementType {
    Inbound,
    Outbound,
    Transfer,
    Adjustment,
}

pub struct CreateStockMovementCommand {
    pub tenant_id: Uuid,
    pub movement_type: StockMovementType,
    pub product_id: Uuid,
    pub source_warehouse_id: Option<Uuid>,
    pub destination_warehouse_id: Option<Uuid>,
    pub quantity: i64, // strictly > 0
    pub unit_cost: Option<Rupiah>,
    pub reference_type: Option<String>, // "PURCHASE_ORDER", "INVOICE", "ADJUSTMENT"
    pub reference_id: Option<Uuid>,
    pub batch_number: Option<String>,
    pub notes: Option<String>,
}

// Invariant: If quantity_on_hand - deduction < 0 => AppError::UnprocessableEntity("INSUFFICIENT_STOCK")
// Concurrency: Must execute within tx opened via pool.begin_with("BEGIN IMMEDIATE")
```

### M3 ↔ WAC & Double-Entry Accounting Contract
```rust
// Integer Moving Weighted Average Cost:
// new_wac = round_half_up_i128((prev_qty * prev_wac + in_qty * in_cost), total_qty)

// Inbound Goods Receipt Journal:
// Debit 1300 (Persediaan Barang Dagang) = in_qty * in_cost
// Credit 2000 (Utang Usaha)             = in_qty * in_cost

// Invoice Fulfillment COGS Journal:
// Debit 5000 (Beban Pokok Penjualan)   = out_qty * current_wac
// Credit 1300 (Persediaan Barang Dagang) = out_qty * current_wac

// Transactional Outbox Events:
// Event types: "StockReceived", "StockAdjusted", "StockTransferred"
// Aggregate type: "Inventory"
```

---

## Code Layout

```text
backend/
├── migrations/
│   ├── 0001_initial_schema.sql through 0010_password_reset_tokens.sql (Preserved)
│   └── 0011_v4_1_inventory_management.sql      # [M1] 7 inventory tables, account 1300 seed, triggers
├── src/
│   ├── domain/
│   │   ├── money.rs                            # Checked Rupiah(i64) math
│   │   ├── accounting.rs                       # [M1, M3] Account 1300 in canonical system accounts
│   │   ├── outbox.rs                           # [M3] OutboxEventDraft constructors
│   │   └── inventory.rs                        # [M1, M2, M3] Warehouse, Product, StockItem, Movements, POs
│   ├── repository/
│   │   └── inventory_repo.rs                   # [M1, M2, M3] SqlxInventoryRepository with TenantContext
│   ├── service/
│   │   └── inventory_service.rs                # [M2, M3] Movements, WAC, PO state machine, GL postings
│   ├── api/
│   │   ├── router.rs                           # [M4] Mount inventory API routes
│   │   ├── warehouses.rs                       # [M4] /api/v1/warehouses
│   │   ├── products.rs                         # [M4] /api/v1/products
│   │   ├── inventory.rs                        # [M4] /api/v1/inventory (movements, stock, adjustments)
│   │   └── purchase_orders.rs                  # [M4] /api/v1/purchase-orders
└── tests/
    ├── m1_persistence_tests.rs                 # [M1] Synchronized table count (37)
    ├── v4_m4_challenger_outbox_tests.rs        # [M1] Synchronized table count (37)
    └── v4_inventory_management_tests.rs        # [M2, M3] Full integration test suite for R1-R5

frontend/
├── src/
│   ├── stores/
│   │   └── workspace.js                        # [M4] Capabilities matrix for inventory & purchasing
│   ├── components/layout/
│   │   ├── DesktopSidebar.vue                  # [M4] Dynamic nav items for inventory & purchasing
│   │   └── MobileBottomNav.vue                 # [M4] Adaptive nav slot 4, preserved POS button
│   └── views/
│       ├── InventoryView.vue                   # [M4] Inventory & stock management view
│       └── PurchasingView.vue                  # [M4] Purchase orders view

e2e_tests/
├── harness/
│   ├── client.py                               # [Test] Extended client with inventory endpoints
│   └── server.py                               # [Test] Reference oracle with inventory support (33 tables)
├── tier1/test_tier1.py                         # [Test] F33-F42 coverage & table count sync (33)
├── tier2/test_tier2.py                         # [Test] B33-B42 corner cases & table count sync (33)
├── tier3/test_tier3.py                         # [Test] PAIR-37-PAIR-45 cross-feature tests
├── tier4/test_tier4.py                         # [Test] SCENARIO-21-SCENARIO-24 real-world workflows
└── runner.sh                                   # Master TAP v13 test runner
```
