# TEST_INFRA: Invinite Business OS v4.1 Acceptance Testing Architecture

## 1. Testing Philosophy & Guiding Principles

The Invinite Business OS test infrastructure is designed around the principles of **opaque-box requirement verification**, **mathematical financial rigor**, and **defense-in-depth isolation**.

1. **Opaque-Box Requirement Verification**:
   Tests treat the system under test strictly through public HTTP APIs and database schemas, verifying contract compliance, HTTP status codes (RFC 7807 problem details), response structure, and observable business state. Tests do not inspect or rely on private implementation details.
2. **Absolute Zero-Floating-Point Financial Invariant**:
   All monetary amounts, unit costs, weighted average calculations, and GL journals operate exclusively on 64-bit signed integers (`i64`) representing Indonesian Rupiah (IDR). No floating-point types (`f32`, `f64`) are permitted in financial paths. Rounding is strictly governed by statutory integer half-up arithmetic (`round_half_up_i128`).
3. **Double-Entry Ledger Balancing Invariant**:
   Every domain mutation resulting in accounting journal creation must strictly satisfy the double-entry invariant:
   $$\sum \text{Debit} \equiv \sum \text{Credit}$$
   Any transaction violating this invariant is rejected atomically with HTTP 422 (`UNBALANCED_JOURNAL_ENTRY`).
4. **Negative Stock Prohibition & Concurrency Invariance**:
   Physical stock levels cannot drop below zero (`CHECK (quantity_on_hand >= 0)`). Any allocation or deduction exceeding available balance must be rejected atomically with HTTP 422 (`INSUFFICIENT_STOCK`) or HTTP 409 (`CONFLICT`). Concurrent mutations on identical SKUs are serialized via database write transactions (`BEGIN IMMEDIATE`) to prevent overselling.
5. **Strict Multi-Tenancy & Anti-Enumeration Principle**:
   Every inventory operation requires an explicit `TenantContext`. Cross-tenant lookups must return HTTP 404 Not Found to prevent entity existence enumeration. Cross-tenant modification attempts are blocked with zero leakage of metadata.
6. **Immutable Audit Trails**:
   Stock movements (`stock_movements`), stock adjustments (`stock_adjustments`), and posted journal entries (`journal_entries`) are immutable write-once records. Update and deletion are blocked at both database trigger and application repository levels.

---

## 2. Phase 2 Feature Inventory (F33 – F42)

| # | Feature Identifier | Feature Name | Description & Invariants | Interface Endpoints | Milestone |
|---|--------------------|--------------|--------------------------|---------------------|-----------|
| 1 | **F33** | Multi-Warehouse Management | Manage multi-location warehouses per tenant. Support default warehouse designation. Prevent cross-tenant lookup (404) and duplicate warehouse codes (409). | `POST /api/v1/warehouses`<br>`GET /api/v1/warehouses`<br>`GET /api/v1/warehouses/{id}` | M1 |
| 2 | **F34** | Product Catalog & SKU Engine | Central product catalog with unique sequential SKU generation (`SKU-XXXXXX`), unit, cost price, and sale price. Non-positive price rejection. | `POST /api/v1/products`<br>`GET /api/v1/products`<br>`GET /api/v1/products/{id}` | M1 |
| 3 | **F35** | Stock Level Tracking & Alerts | Per-warehouse stock tracking (`quantity_on_hand`, `quantity_reserved`, `bin_location`). Dynamic low-stock alerts when `quantity_on_hand <= reorder_threshold`. | `GET /api/v1/inventory`<br>`GET /api/v1/inventory?low_stock=true` | M1, M2 |
| 4 | **F36** | Atomic Stock Movement Engine | Atomic inventory movements (`INBOUND`, `OUTBOUND`). Atomic negative stock deduction prevention (HTTP 422/409). Immutable audit trail. | `POST /api/v1/inventory/movements` | M2 |
| 5 | **F37** | Inter-Warehouse Stock Transfer | Two-legged atomic transfer: decrements source warehouse and increments destination warehouse. Disallows same-source-and-destination transfers (400). | `POST /api/v1/inventory/transfer` | M2 |
| 6 | **F38** | Physical Stock Adjustment | Cycle count adjustment (`ADJ-YYYY-XXXXXX`) with positive/negative variance recording, reason documentation, and auto-balancing journal posting. | `POST /api/v1/inventory/adjust` | M2 |
| 7 | **F39** | Purchase Order Lifecycle | PO state machine: `DRAFT` -> `ORDERED` -> `PARTIALLY_RECEIVED` -> `RECEIVED` / `CANCELLED` (`PO-YYYY-XXXXXX`). Immutable lines once ordered. | `POST /api/v1/purchase-orders`<br>`POST /api/v1/purchase-orders/{id}/order`<br>`POST /api/v1/purchase-orders/{id}/cancel` | M3 |
| 8 | **F40** | Inbound Goods Receipt & Batches | Inbound goods receipt against ordered PO. Increments warehouse stock, updates WAC, captures supplier batch numbers and unit costs. Prohibits over-receipt (422). | `POST /api/v1/purchase-orders/{id}/receive` | M3 |
| 9 | **F41** | Weighted Average Cost (WAC) Engine | Integer Rupiah moving weighted average cost valuation: $C_{new} = \lfloor \frac{Q_0 C_0 + Q_i C_i}{Q_0 + Q_i} \rfloor$. Zero floating-point arithmetic. Overflow protection up to signed 64-bit limits. | `GET /api/v1/inventory`<br>`POST /api/v1/purchase-orders/{id}/receive` | M3 |
| 10 | **F42** | Double-Entry GL Integration | Automated balanced journal entries: Inbound Receipt (`Debit 1300 Persediaan / Credit 2000 Utang Usaha`), Invoice Outbound Fulfillment (`Debit 5000 COGS / Credit 1300 Persediaan`). Balanced invariant. | `POST /api/v1/purchase-orders/{id}/receive`<br>`POST /api/v1/inventory/movements`<br>`GET /api/v1/accounting/journals` | M3 |

---

## 3. Test Design Methodology

The test suite applies formal black-box software engineering testing techniques across all test tiers:

### 3.1 Category-Partition Testing
Each input domain is partitioned into mutually exclusive equivalence classes:
- **Warehouse Codes**: Valid unique code, existing duplicate code, cross-tenant code, whitespace-only code.
- **Stock Quantities**: Zero quantity, standard positive quantity, quantity equal to stock on hand, quantity exceeding stock on hand, negative quantity.
- **PO States**: `DRAFT`, `ORDERED`, `PARTIALLY_RECEIVED`, `RECEIVED`, `CANCELLED`.
- **Movement Types**: `INBOUND`, `OUTBOUND`, `TRANSFER`, `ADJUSTMENT`.
- **Tenancy Contexts**: Authorized tenant owner/admin, valid member without permissions, foreign tenant attacker, unauthenticated client.

### 3.2 Boundary Value Analysis (BVA)
Extremes and transition points are tested explicitly:
- **Stock Deductions**:
  - Deduct 0 units (rejected with 400).
  - Deduct exactly $Q$ units when $Q$ units are on hand (resulting in exactly 0, accepted).
  - Deduct $Q + 1$ units when $Q$ units are on hand (rejected with 422).
- **PO Receipts**:
  - Receive 0 units (rejected with 400).
  - Receive partial quantity $Q_{part} < Q_{ordered}$ (transitions to `PARTIALLY_RECEIVED`).
  - Receive remaining balance $(Q_{ordered} - Q_{part})$ (transitions to `RECEIVED`).
  - Over-receive $> Q_{ordered}$ (rejected with 422).
- **Integer Arithmetic & Large Numbers**:
  - Truncation precision: $(3 \times 10.000 + 4 \times 12.000) / 7 = 78.000 / 7 = \text{Rp } 11.142$ integer truncation.
  - Large volume valuations: $20.000 \text{ units} \times \text{Rp } 1.500.000.000 = \text{Rp } 30.000.000.000.000$ ($3 \times 10^{13}$ IDR) without 64-bit integer overflow.

### 3.3 All-Pairs (Pairwise) Combinatorial Testing (Tier 3)
Features are systematically crossed to discover edge-case interactions:
- **PAIR-37**: F33 Multi-Warehouse x F34 Product Catalog (StockItem automatic initialization per warehouse location).
- **PAIR-38**: F36 Stock Movement x F35 Reorder Alerts (Outbound deduction breaches threshold and triggers reactive low stock alert).
- **PAIR-39**: F37 Stock Transfer x F42 Double-Entry GL (Internal warehouse transfer moves stock without altering aggregate tenant asset valuation).
- **PAIR-40**: F38 Stock Adjustment x F42 Double-Entry GL (Physical count discrepancy triggers automated revaluation journal).
- **PAIR-41**: F39 Purchase Order x F40 Goods Receipt (Partial receipt workflow updates state machine to `PARTIALLY_RECEIVED`, then `RECEIVED`).
- **PAIR-42**: F40 Goods Receipt x F41 WAC Valuation (Sequential receipts at disparate unit costs update moving average cost).
- **PAIR-43**: F40 Goods Receipt x F42 Double-Entry GL (Inbound goods receipt auto-posts balanced `Debit 1300 / Credit 2000`).
- **PAIR-44**: F36 Stock Movement x F42 Double-Entry GL (Outbound fulfillment posts COGS journal `Debit 5000 / Credit 1300`).
- **PAIR-45**: F39 Purchase Order x F21 Idempotency (Replayed PO goods receipt with identical `Idempotency-Key` returns original cached response with `X-Cache-Replay: true`).

### 3.4 Real-World Workload Scenarios (Tier 4)
End-to-end multi-step user journeys simulating realistic Indonesian business operations:
- **SCENARIO-21 [Multi-Location Retail Fulfillment]**: Supplier PO -> Inbound Receipt at Central Warehouse -> Inter-Warehouse Transfer to Branch Outlet -> Commercial POS Outbound Sale -> Reorder Alert Trigger.
- **SCENARIO-22 [Moving Average Cost Revaluation & GL Reconciliation]**: Initial stock receipt -> High-inflation replenishment receipt -> Integer WAC revaluation -> Outbound fulfillment at current WAC -> General Ledger Trial Balance verification ($\sum \text{Debit} \equiv \sum \text{Credit}$).
- **SCENARIO-23 [Physical Inventory Cycle Count & Variance Settlement]**: Periodic audit discovers shrinkage -> Cycle count adjustment -> Variance journal posting -> Immutable movement and adjustment audit trail verification.
- **SCENARIO-24 [High-Throughput Inbound Purchasing & Supplier Batch Audit]**: Multi-item purchase order -> Staged partial receipts with batch identifiers -> Complete fulfillment -> Idempotent replay resilience.

---

## 4. Test Harness Architecture

```
e2e_tests/
├── harness/
│   ├── client.py        # Independent TAP v13 test client & HTTP API wrapper
│   ├── server.py        # Reference Oracle with SQLite WAL backend (33 relational tables)
│   └── crypto_keys.py   # Cryptographic helper routines
├── tier1/
│   └── test_tier1.py    # Tier 1: Feature Coverage (210 tests across F01–F42)
├── tier2/
│   └── test_tier2.py    # Tier 2: Boundary & Corner Cases (210 tests across B01–B42)
├── tier3/
│   └── test_tier3.py    # Tier 3: Cross-Feature Integration (45 pairwise tests)
├── tier4/
│   └── test_tier4.py    # Tier 4: Real-World Scenarios (24 comprehensive workflows)
├── tier1_feature_coverage.sh
├── tier2_boundary_corner.sh
├── tier3_cross_feature.sh
├── tier4_real_world.sh
└── runner.sh            # Master TAP v13 Test Runner with automated lifecycle management
```

### 4.1 Schema Synchronization & Invariant Metrics
The schema persistence verification probes enforce strict relational invariants:
- **Relational Tables**: Exactly **33** tables (26 core foundation + 7 inventory tables: `warehouses`, `products`, `stock_items`, `stock_movements`, `purchase_orders`, `purchase_order_items`, `stock_adjustments`).
- **Chart of Accounts**: Standard accounts include Asset Account `1300` ("Persediaan") alongside 8 baseline canonical accounts.
- **Database Engine Pragmas**: `journal_mode = wal`, `foreign_keys = 1`, `integrity_check = ok`.

---

## 5. Coverage Thresholds & Quality Gates

| Test Tier | Scope | Total Tests | Pass Requirement | Execution Budget |
|-----------|-------|-------------|------------------|------------------|
| **Tier 1: Feature Coverage** | Primary happy-path coverage across F01–F42 | 210 | 100% (0 failures) | < 1.0s |
| **Tier 2: Boundary & Corner Cases** | Edge cases, error handling, BVA across B01–B42 | 210 | 100% (0 failures) | < 1.0s |
| **Tier 3: Cross-Feature Pairwise** | All-Pairs combinatorial feature interactions | 45 | 100% (0 failures) | < 0.5s |
| **Tier 4: Real-World Scenarios** | Multi-step end-to-end operational workflows | 24 | 100% (0 failures) | < 1.5s |
| **Total Acceptance Suite** | Master TAP v13 test runner across all tiers | **489** | **100% (0 failures)** | **< 4.0s** |

All tests output standard **TAP version 13** format. Non-zero exit code on any assertion failure triggers automated pipeline rejection.
