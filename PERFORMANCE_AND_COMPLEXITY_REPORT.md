# 📊 Laporan Analisis Time Complexity, Algoritma & N+1 Query
### Invinite Business OS v4.1 — Review Backend · Frontend · Database
**Tanggal:** 2026-10-06 | **Reviewer:** Senior Software Engineer

---

## 1. Ringkasan Temuan Kritis

| Layer | N+1 Problem | Kompleksitas Terburuk | Severity |
|-------|------------|----------------------|----------|
| **DB** | ✅ Bersih (project) | O(N) dengan index | Low |
| **DB** | 🔴 **ADA** (purchase_orders) | O(N × M) tanpa batching | HIGH |
| **BE** | ✅ Bersih (project) | O(N) per operasi | Low |
| **BE** | ⚠️ Minor (create_task validation) | 2 round-trips | Medium |
| **FE** | ⚠️ Ada (refreshAll pattern) | 5 parallel fetches | Medium |
| **FE** | ⚠️ `getWarehouseById` O(N) linear scan | O(N) per lookup | Medium |

---

## 2. Database Layer (DB)

### 2.1 Index Coverage dan Query Complexity

Semua tabel Phase 3 memiliki index yang relevan. Berikut analisis per query:

#### `list_projects()` — O(N log N) → Efisien ✅

```sql
SELECT * FROM projects 
WHERE tenant_id = ?1            -- idx_projects_tenant → B-tree lookup O(log N)
  [AND status = ?]              -- idx_projects_status (composite) → O(log N)
  [AND customer_id = ?]        -- idx_projects_customer → O(log N)
  [AND name LIKE '%...%']      -- ⚠️ Full table scan dalam partisi tenant
ORDER BY created_at DESC       -- idx_projects_created → B-tree scan
LIMIT ? OFFSET ?               -- Constant time per page
```

**Catatan penting:** `LIKE '%search%'` dengan leading wildcard **tidak dapat menggunakan index prefix scan** — SQLite akan melakukan full scan atas semua rows tenant. Untuk tenant kecil (<10.000 proyek) ini acceptable, namun untuk tenant besar ini adalah O(N). **Rekomendasi:** Gunakan SQLite FTS5 (Full Text Search) untuk pencarian teks.

#### `list_milestones()` — O(M) → Efisien ✅

```sql
SELECT * FROM milestones 
WHERE project_id = ?1 AND tenant_id = ?2
ORDER BY sequence_order ASC    -- idx_milestones_sequence → B-tree, O(M log M)
```
Di mana M = jumlah milestones per project. Dalam konteks bisnis normal M ≪ 100, sangat efisien.

#### `list_tasks()` — O(K) → Efisien ✅

```sql
SELECT * FROM tasks 
WHERE project_id = ? AND tenant_id = ?
  [AND milestone_id = ?]       -- idx_tasks_milestone
  [AND status = ?]             -- idx_tasks_status
  [AND assignee_id = ?]        -- idx_tasks_assignee
ORDER BY ...
```
Semua filter menggunakan composite index. K = jumlah tasks per project.

---

### 2.2 🔴 N+1 Query Problem — `list_purchase_orders()` TERIDENTIFIKASI

**Lokasi:** [`inventory_repo.rs` L1347–1400](file:///home/nurdiansyah/teamwork_projects/final_project/backend/src/repository/inventory_repo.rs#L1347-L1400)

```rust
// 1 query untuk fetch semua PO headers
let po_rows = sqlx::query("SELECT * FROM purchase_orders WHERE tenant_id = ?1")
    .fetch_all(&self.pool).await?;  // → N rows

// ❌ N+1 PROBLEM: loop dengan 1 query per PO untuk items-nya
for row in &po_rows {
    let po = Self::row_to_purchase_order_header(row)?;
    let item_rows = sqlx::query(
        "SELECT * FROM purchase_order_items WHERE purchase_order_id = ?1 AND tenant_id = ?2"
    )
    .bind(po.id.to_string())
    .fetch_all(&self.pool).await?;  // ← Query ke-2, ke-3, ke-(N+1) !!
    // ...
}
```

**Dampak:** Jika ada **N PO**, maka ada **N+1 database round-trips**.
- 50 PO = 51 queries
- 200 PO = 201 queries
- **Setiap query = network round-trip ke SQLite + parsing overhead**

**Solusi yang Benar:**

```sql
-- Ganti dengan satu JOIN query:
SELECT po.*, poi.id AS item_id, poi.product_id, poi.product_name,
       poi.quantity_ordered, poi.quantity_received, poi.unit_price, poi.line_total
FROM purchase_orders po
LEFT JOIN purchase_order_items poi ON poi.purchase_order_id = po.id AND poi.tenant_id = po.tenant_id
WHERE po.tenant_id = ?1
ORDER BY po.created_at DESC, poi.rowid ASC;
-- Kemudian group di aplikasi → O(N + M) total, bukan O(N × M)
```

---

### 2.3 `get_project_cost_summary_tx()` — Correlated Subqueries

**Lokasi:** [`project_repo.rs` L2530–2616](file:///home/nurdiansyah/teamwork_projects/final_project/backend/src/repository/project_repo.rs#L2530-L2616)

```sql
SELECT
    p.id,
    p.budget_amount,
    COALESCE((SELECT SUM(pm.total_cost) FROM project_materials pm WHERE pm.project_id = p.id ...), 0),
    COALESCE((SELECT SUM(pl.total_cost) FROM project_labor pl WHERE pl.project_id = p.id ...), 0),
    COALESCE((SELECT SUM(pe.amount) FROM project_expenses pe WHERE pe.project_id = p.id ...), 0),
    COALESCE((SELECT SUM(m.billable_amount) FROM milestones m WHERE m.project_id = p.id ...), 0),
    COALESCE((SELECT SUM(i.total_amount) FROM invoices i WHERE i.id IN (
        SELECT invoice_id FROM milestones WHERE ...
        UNION
        SELECT invoice_id FROM progress_records WHERE ...
    )), 0)
FROM projects p
WHERE p.id = ?1 AND p.tenant_id = ?2;
```

**Analisis Complexity:**
- Query terhadap **single project** (p.id = ?1) → 1 row di `projects`
- 5 correlated subqueries → masing-masing O(M) dengan index `project_id`
- Sub-subquery `UNION` pada `invoices` → dua full scans + dedup

**Verdict:** ✅ Acceptable untuk single-project context. Karena query hanya dipanggil per `project_id` (bukan untuk list semua project sekaligus), kompleksitas efektif adalah **O(M + K + L + E + I)** di mana setiap variabel = jumlah rows per entitas per project — sangat kecil dalam praktik.

**⚠️ Risiko:** Jika method ini dipanggil dalam loop untuk dashboard "list semua proyek dengan profitabilitas", ini AKAN menjadi N+1. Saat ini tidak ada evidence hal tersebut terjadi, namun perlu diwaspadai di M5 (dashboard frontend).

---

### 2.4 `get_next_project_number_tx()` — MAX() Sequential Numbering

```sql
SELECT MAX(CAST(SUBSTR(project_number, LENGTH(?2) + 1) AS INTEGER))
FROM projects
WHERE tenant_id = ?1 AND project_number LIKE ?3;
```

**Algoritma:** MAX() + CAST + SUBSTR pada kolom TEXT yang di-LIKE filter.

**Complexity:** O(N) scan atas semua project numbers tenant yang cocok dengan prefix-year. Tidak ada index pada `project_number` parsial.

**Perbaikan potensial:**
```sql
-- Tambah index:
CREATE INDEX idx_projects_number_prefix ON projects(tenant_id, project_number);
-- Kemudian query bisa pakai range scan: project_number >= 'PRJ-2026-' AND project_number < 'PRJ-2026A'
```

Namun untuk jumlah proyek yang realistis (<10.000 per tenant per tahun), ini tidak menjadi masalah performa nyata.

---

### 2.5 Algoritma WAC (Weighted Average Cost) — Inventory

**Algoritma:** Weighted Average Cost per stock movement.

```
WAC_baru = (quantity_lama × WAC_lama + quantity_masuk × harga_beli) / (quantity_lama + quantity_masuk)
```

**Complexity:** O(1) per transaksi masuk (menggunakan WAC yang tersimpan di `stock_items.average_cost`). Tidak ada rekalkulasi historical. Ini adalah **online algorithm** yang tepat — sangat efisien.

---

## 3. Backend Layer (BE)

### 3.1 `issue_material()` — Sequence Complexity

```
Step 1: RBAC check                    → O(1)
Step 2: BEGIN IMMEDIATE               → O(1)
Step 3: find_project_by_id_tx         → O(log N) via index
Step 4: find_material_by_id_tx        → O(log M) via index
Step 5: quantity check                → O(1)
Step 6: inventory_service.issue_stock → O(log S) via stock_items index
Step 7: integer multiply              → O(1)
Step 8: post_journal_command_tx       → O(1) insert
Step 9: update_material_tx            → O(log M) via index
Step 10: outbox insert                → O(1)
Step 11: tx.commit                    → O(1) I/O
```

**Total: O(log N)** — operasi dominan adalah B-tree lookups. ✅ Sangat efisien.

---

### 3.2 `bill_progress()` — Cumulative Guard Complexity

```rust
// Ini query O(M) di mana M = jumlah progress records per project yang is_billed=1
let cumulative_billed: i64 = sqlx::query_scalar(
    "SELECT COALESCE(SUM(percentage), 0) FROM progress_records 
     WHERE project_id = ?1 AND tenant_id = ?2 AND is_billed = 1"
)
```

**Complexity:** O(M) SUM aggregation di mana M = jumlah billed progress records.

**Index yang digunakan:** `idx_progress_records_project` → B-tree scan O(M log M) → SUM.

Dalam praktik M sangat kecil (umumnya < 20 records per project seumur hidup). ✅ Acceptable.

---

### 3.3 ⚠️ `create_task()` — Double Validation Round-Trip

**Lokasi:** [`project_service.rs` L560–605](file:///home/nurdiansyah/teamwork_projects/final_project/backend/src/service/project_service.rs#L560-L605)

```rust
// Round-trip 1: Validasi milestone exists (jika milestone_id diberikan)
if let Some(m_id) = req.milestone_id {
    let milestone = self.repo.find_milestone_by_id(ctx, project_id, m_id).await?;
    // → 1 DB query
}

// Round-trip 2: Validasi assignee exists (jika assignee_id diberikan)
if let Some(assignee_id) = req.assignee_id {
    let member = self.repo.find_member(ctx, project_id, assignee_id).await?;
    // → 1 DB query
    if member.is_none() {
        let membership = self.membership_repo.get_membership(...).await?;
        // → kemungkinan 1 DB query LAGI (worst case: 3 round-trips sebelum insert)
    }
}
```

**Worst case skenario (task dengan milestone + assignee non-member):** 4 DB round-trips sebelum INSERT.

**Pola yang lebih efisien:**
```rust
// Bisa gunakan satu JOIN query di repo layer untuk memvalidasi keduanya sekaligus
// SELECT COUNT(*) FROM milestones m, users u WHERE m.id = ?milestone_id AND u.id = ?assignee_id AND ...
```

**Severity:** Medium — masih acceptable secara performa karena jarang dipanggil massal. Bukan N+1 dalam loop.

---

### 3.4 `list_tasks()` — Preflight project check

```rust
pub async fn list_tasks(...) {
    let _project = self.get_project(ctx, project_id).await?;  // ← 1 extra query!
    self.repo.list_tasks(ctx, project_id, &filter).await
}
```

Setiap `list_tasks`, `get_task`, `list_progress_records`, `create_progress_record` melakukan `get_project()` preflight check — 1 query ekstra. Total: **2 queries per list operation** (1 project check + 1 list).

**Pattern ini konsisten di:** `list_tasks`, `get_task`, `list_progress_records`, `create_progress_record`.

**Analisis:** Ini adalah **intentional design** untuk anti-enumeration (project existence validation), bukan N+1. Complexity tambahan: O(log N) per call. Acceptable.

---

### 3.5 `get_project_cost_summary()` di Service Layer

```rust
pub async fn get_project_profitability(...) {
    let _project = self.get_project(ctx, project_id).await?;  // Query 1: project
    let summary = self.repo.get_project_cost_summary(ctx, project_id).await?; // Query 2: complex aggregation
    Ok(ProjectProfitabilitySummary::from_cost_totals(summary, ...))
}
```

2 queries total. Query 2 adalah correlated subquery yang sudah dianalisis di §2.3. ✅ Acceptable.

---

## 4. Frontend Layer (FE)

### 4.1 `refreshAll()` — 5 Parallel Fetches (Acceptable)

**Lokasi:** [`inventory.js` L284–298](file:///home/nurdiansyah/teamwork_projects/final_project/frontend/src/stores/inventory.js#L284-L298)

```javascript
async function refreshAll() {
    await Promise.allSettled([
        fetchWarehouses(),      // API call 1
        fetchProducts(),        // API call 2
        fetchStock(),           // API call 3
        fetchMovements(),       // API call 4
        fetchPurchaseOrders()   // API call 5
    ])
}
```

**Verdict:** ✅ `Promise.allSettled` digunakan dengan benar — 5 request berjalan **paralel**, bukan sequential. Total waktu tunggu = max(T1, T2, T3, T4, T5), bukan T1+T2+T3+T4+T5.

**⚠️ Catatan:** Setelah `createProduct()`, dipanggil `Promise.all([fetchProducts(), fetchStock()])` — ini benar. Tapi setelah `receivePurchaseOrder()` dipanggil `Promise.all([fetchPurchaseOrders(), fetchStock(), fetchMovements()])` — 3 parallel fetches post-mutation, yang tidak masalah performa namun meningkatkan beban network.

---

### 4.2 ⚠️ `getWarehouseById` dan `getProductById` — Linear Scan O(N)

**Lokasi:** [`inventory.js` L52–53](file:///home/nurdiansyah/teamwork_projects/final_project/frontend/src/stores/inventory.js#L52-L53)

```javascript
const getWarehouseById = (id) => warehouses.value.find((w) => w.id === id)
const getProductById = (id) => products.value.find((p) => p.id === id)
```

**Complexity:** O(N) per lookup di mana N = jumlah warehouses/products.

**Digunakan di template InventoryView.vue:**
```javascript
// Dipanggil dalam filter() yang digunakan pada computed filteredStockTable
const name = (it.product_name || resolveProduct(it.product_id)?.name || '').toLowerCase()
//                              ↑ resolveProduct memanggil find() di dalam loop filter!
```

**Pattern ini: O(N × M)** di mana N = jumlah stock items, M = jumlah products.

Misal: 200 stock items × 50 products = **10.000 iterasi** per computed re-render.

**Solusi yang Lebih Efisien:**

```javascript
// Buat computed Map/Dictionary sekali, O(1) lookup:
const warehouseMap = computed(() => 
    new Map(warehouses.value.map(w => [w.id, w]))
)
const productMap = computed(() => 
    new Map(products.value.map(p => [p.id, p]))
)

// Lookup: O(1)
const getWarehouseById = (id) => warehouseMap.value.get(id)
const getProductById = (id) => productMap.value.get(id)
```

**Severity:** Medium — untuk skala SMB (< 1.000 produk), tidak terasa. Namun jika user memiliki 500+ produk × 1.000+ stock items = 500.000 iterasi per render cycle.

---

### 4.3 ⚠️ `filteredStockItems` → `lowStockItems` → `totalStockValuation` — Chained Computed

```javascript
const filteredStockItems = computed(() => {
    if (!activeWarehouseId.value) return stockItems.value      // O(1)
    return stockItems.value.filter(s => s.warehouse_id === activeWarehouseId.value) // O(N)
})

const lowStockItems = computed(() => {
    const list = activeWarehouseId.value ? filteredStockItems.value : stockItems.value
    return list.filter(s => Number(s.quantity_on_hand) <= Number(s.reorder_threshold)) // O(N)
})

const totalStockValuation = computed(() => {
    const list = activeWarehouseId.value ? filteredStockItems.value : stockItems.value
    return list.reduce((acc, s) => acc + qty * cost, 0) // O(N)
})
```

**Verdict:** ✅ Vue 3 `computed` adalah **lazy** dan **memoized** — hanya dihitung ulang ketika dependency berubah. Tidak ada redundant recalculation. Chaining seperti ini adalah pola yang benar di Vue 3.

---

### 4.4 `totalSkuCount` — Set Construction O(N)

```javascript
const totalSkuCount = computed(() => {
    const uniqueProducts = new Set(list.map(s => s.product_id)) // O(N) space + time
    return uniqueProducts.size
})
```

**Complexity:** O(N) time dan O(K) space di mana K = unique products. ✅ Acceptable.

---

### 4.5 FE Tidak Ada N+1 Request dalam Loop

Pemeriksaan kode menunjukkan tidak ada pola seperti ini di FE:
```javascript
// ❌ Pola N+1 FE yang TIDAK ditemukan:
for (const item of stockItems) {
    await api.getProductDetails(item.product_id)  // ← N round-trips
}
```

Semua data diambil secara bulk (`fetchAll`) kemudian di-resolve secara lokal via `find()`. ✅

---

## 5. Tabel Algoritma yang Digunakan

| Algoritma | Lokasi | Complexity | Keterangan |
|-----------|--------|-----------|------------|
| B-tree index lookup | Semua `find_*_by_id()` | O(log N) | Via SQLite B-tree index |
| Sequential scan + filter | `list_projects` LIKE search | O(N) | Leading wildcard; no FTS |
| MAX() + CAST + LIKE | Sequential numbering | O(N) | N = projects per tenant per year |
| Online WAC recalc | `issue_stock` | O(1) | Per transaction incremental |
| Correlated subquery | `get_project_cost_summary` | O(M+K+L+E+I) | Per single project |
| SUM aggregation | `bill_progress` cumulative | O(M) | M = billed records per project |
| Linear scan | FE `getWarehouseById` | O(N) | Dalam loop = O(N²) risiko |
| Hash Map lookup | FE (disarankan perbaikan) | O(1) | Saat ini belum diimplementasi |
| Set deduplication | FE `totalSkuCount` | O(N) | One-time per compute |
| Parallel Promise | FE `refreshAll` | O(max Ti) | Benar menggunakan allSettled |

---

## 6. N+1 Problem — Ringkasan Lengkap

### 6.1 🔴 Terkonfirmasi ADA: `list_purchase_orders()` (DB/BE)

```
Severity: HIGH
Pattern: 1 query PO headers + N queries untuk PO items
Lokasi: inventory_repo.rs L1364–1377
Dampak: 50 PO → 51 queries; 200 PO → 201 queries
Fix: JOIN query dengan grouping di aplikasi
```

### 6.2 ✅ Tidak Ada N+1: Project Domain (DB/BE)

Semua list operations di project domain menggunakan **batch queries**:
- `list_milestones()` → 1 query
- `list_tasks()` → 1 query  
- `list_labor_by_project()` → 1 query
- `list_materials_by_project()` → 1 query
- `list_members_with_user()` → **1 query dengan JOIN** ✅

Khususnya `list_members_with_user()` menggunakan JOIN yang tepat:
```sql
SELECT pm.*, u.display_name AS user_name, u.email AS user_email
FROM project_members pm
JOIN users u ON pm.user_id = u.id
WHERE pm.project_id = ?1 AND pm.tenant_id = ?2
```
Ini adalah cara yang benar — **tidak ada N+1** meskipun fetching user data.

### 6.3 ✅ Tidak Ada N+1: Frontend

Frontend **tidak** melakukan API call dalam loop. Semua data diambil bulk, resolve dilakukan secara lokal.

### 6.4 ⚠️ Pseudo N+1: FE linear scan dalam computed

Bukan N+1 request ke API, tapi linear scan O(N) di dalam loop O(M) = O(N×M) CPU work per render. Bukan N+1 network problem, tapi performance concern pada data besar.

---

## 7. Rekomendasi Perbaikan (Prioritas)

### Priority 1 — HARUS DIPERBAIKI (High)

#### Fix N+1 pada `list_purchase_orders()`

**File:** [`backend/src/repository/inventory_repo.rs`](file:///home/nurdiansyah/teamwork_projects/final_project/backend/src/repository/inventory_repo.rs)

```rust
// Ganti pattern for-loop + query dengan JOIN:
async fn list_purchase_orders(&self, ctx: &TenantContext) -> Result<Vec<PurchaseOrderWithItems>, DbError> {
    let rows = sqlx::query(r#"
        SELECT po.id AS po_id, po.po_number, po.supplier_name, po.status,
               po.total_amount, po.notes, po.created_at, po.updated_at,
               poi.id AS item_id, poi.product_id, poi.product_name,
               poi.quantity_ordered, poi.quantity_received, poi.unit_price, poi.line_total
        FROM purchase_orders po
        LEFT JOIN purchase_order_items poi ON poi.purchase_order_id = po.id AND poi.tenant_id = po.tenant_id
        WHERE po.tenant_id = ?1
        ORDER BY po.created_at DESC, poi.rowid ASC
    "#).bind(ctx.tenant_id_str()).fetch_all(&self.pool).await?;

    // Group rows by po_id dalam Rust (O(N+M) total)
    let mut result: IndexMap<String, PurchaseOrderWithItems> = IndexMap::new();
    for row in rows {
        let po_id: String = row.get("po_id");
        let entry = result.entry(po_id).or_insert_with(|| { /* parse header */ });
        if let Some(item) = parse_item_row(&row)? {
            entry.items.push(item);
        }
    }
    Ok(result.into_values().collect())
}
```

---

### Priority 2 — DISARANKAN (Medium)

#### Fix FE `getWarehouseById` / `getProductById` O(N) → O(1) Map

**File:** [`frontend/src/stores/inventory.js`](file:///home/nurdiansyah/teamwork_projects/final_project/frontend/src/stores/inventory.js)

```javascript
// Tambahkan computed Maps:
const warehouseMap = computed(() => new Map(warehouses.value.map(w => [w.id, w])))
const productMap = computed(() => new Map(products.value.map(p => [p.id, p])))

// Update getters:
const getWarehouseById = (id) => warehouseMap.value.get(id) ?? null
const getProductById = (id) => productMap.value.get(id) ?? null
```

---

### Priority 3 — OPSIONAL (Low)

#### Tambah SQLite FTS5 untuk pencarian project

```sql
CREATE VIRTUAL TABLE projects_fts USING fts5(
    name, project_number, customer_name,
    content='projects', content_rowid='rowid'
);
-- Trigger untuk keep in sync
```

Mengubah LIKE search O(N) menjadi O(log N) FTS lookup.

---

## 8. Kesimpulan

| Area | Status | Catatan |
|------|--------|---------|
| DB — Project queries | ✅ BAIK | Index coverage lengkap, single queries |
| DB — PO list query | 🔴 N+1 PROBLEM | Harus diperbaiki sebelum go-live |
| DB — Profitability aggregation | ✅ ACCEPTABLE | Correlated subquery tapi single-project scope |
| DB — WAC algorithm | ✅ OPTIMAL | O(1) online incremental |
| BE — Project service | ✅ BAIK | O(log N) dominan, preflight checks intentional |
| BE — Task creation | ⚠️ MINOR | Max 3 validation round-trips, bukan loop |
| FE — API request pattern | ✅ BAIK | Semua bulk, parallelized, no N+1 |
| FE — Computed performance | ⚠️ MINOR | `find()` dalam loop → disarankan Map |
| FE — Vue reactivity | ✅ BAIK | Lazy memoized computed, chaining benar |

**Kesimpulan:** Satu masalah N+1 **nyata** ditemukan di `list_purchase_orders()` (Phase 2, bukan Phase 3). Semua kode Phase 3 (project domain) **bebas N+1**. Frontend menggunakan pola async yang benar. Perbaikan utama yang diperlukan adalah JOIN query di inventory_repo untuk PO listing.

---

*Analisis berdasarkan pembacaan langsung source code pada 2026-10-06.*
*Cakupan: `project_repo.rs` (2616 baris), `inventory_repo.rs` (1479 baris), `project_service.rs` (2261 baris), `inventory.js` (340 baris), `InventoryView.vue` (1244 baris).*
