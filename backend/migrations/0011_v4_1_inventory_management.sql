-- backend/migrations/0011_v4_1_inventory_management.sql
-- Milestone 1: Schema & Persistence Foundation for Phase 2 Inventory & Multi-Location Stock Management
-- Creates 7 domain tables: warehouses, products, stock_items, stock_movements, purchase_orders, purchase_order_items, stock_adjustments
-- Immutability triggers for stock_movements and stock_adjustments
-- Canonical Account 1300 seeding

-- 1. Warehouses Table
CREATE TABLE IF NOT EXISTS warehouses (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    address TEXT,
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_warehouses_tenant_code UNIQUE (tenant_id, code)
);

CREATE INDEX IF NOT EXISTS idx_warehouses_tenant ON warehouses(tenant_id);
CREATE INDEX IF NOT EXISTS idx_warehouses_tenant_code ON warehouses(tenant_id, code);

-- 2. Products Table
CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sku TEXT NOT NULL,
    name TEXT NOT NULL,
    unit TEXT NOT NULL DEFAULT 'PCS',
    cost_price INTEGER NOT NULL DEFAULT 0 CHECK (cost_price >= 0),
    sale_price INTEGER NOT NULL DEFAULT 0 CHECK (sale_price >= 0),
    reorder_threshold INTEGER NOT NULL DEFAULT 0 CHECK (reorder_threshold >= 0),
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_products_tenant_sku UNIQUE (tenant_id, sku)
);

CREATE INDEX IF NOT EXISTS idx_products_tenant ON products(tenant_id);
CREATE INDEX IF NOT EXISTS idx_products_tenant_sku ON products(tenant_id, sku);

-- 3. Stock Items Table (Multi-Location Stock Levels)
CREATE TABLE IF NOT EXISTS stock_items (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    warehouse_id TEXT NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity_on_hand INTEGER NOT NULL DEFAULT 0 CHECK (quantity_on_hand >= 0),
    quantity_reserved INTEGER NOT NULL DEFAULT 0 CHECK (quantity_reserved >= 0),
    reorder_threshold INTEGER NOT NULL DEFAULT 0 CHECK (reorder_threshold >= 0),
    bin_location TEXT,
    average_cost INTEGER NOT NULL DEFAULT 0 CHECK (average_cost >= 0),
    updated_at TEXT NOT NULL,
    CONSTRAINT chk_stock_items_on_hand CHECK (quantity_on_hand >= 0),
    CONSTRAINT chk_stock_items_reserved CHECK (quantity_reserved >= 0),
    CONSTRAINT uq_stock_items_tenant_wh_prod UNIQUE (tenant_id, warehouse_id, product_id)
);

CREATE INDEX IF NOT EXISTS idx_stock_items_tenant ON stock_items(tenant_id);
CREATE INDEX IF NOT EXISTS idx_stock_items_lookup ON stock_items(tenant_id, warehouse_id, product_id);
CREATE INDEX IF NOT EXISTS idx_stock_items_product ON stock_items(tenant_id, product_id);
CREATE INDEX IF NOT EXISTS idx_stock_items_warehouse ON stock_items(tenant_id, warehouse_id);

-- 4. Stock Movements Table (Immutable Audit Ledger)
CREATE TABLE IF NOT EXISTS stock_movements (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    movement_type TEXT NOT NULL CHECK (movement_type IN ('INBOUND', 'OUTBOUND', 'TRANSFER', 'ADJUSTMENT')),
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    source_warehouse_id TEXT REFERENCES warehouses(id) ON DELETE SET NULL,
    destination_warehouse_id TEXT REFERENCES warehouses(id) ON DELETE SET NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_cost INTEGER CHECK (unit_cost >= 0),
    reference_type TEXT,
    reference_id TEXT,
    batch_number TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    actor_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_stock_movements_tenant ON stock_movements(tenant_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_product ON stock_movements(tenant_id, product_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_created ON stock_movements(tenant_id, created_at);
CREATE INDEX IF NOT EXISTS idx_stock_movements_ref ON stock_movements(tenant_id, reference_type, reference_id);

-- 5. Purchase Orders Table
CREATE TABLE IF NOT EXISTS purchase_orders (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    po_number TEXT NOT NULL,
    supplier_name TEXT NOT NULL,
    destination_warehouse_id TEXT NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'DRAFT' CHECK (status IN ('DRAFT', 'ORDERED', 'PARTIALLY_RECEIVED', 'RECEIVED', 'CANCELLED')),
    total_amount INTEGER NOT NULL DEFAULT 0 CHECK (total_amount >= 0),
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CONSTRAINT uq_purchase_orders_tenant_number UNIQUE (tenant_id, po_number)
);

CREATE INDEX IF NOT EXISTS idx_purchase_orders_tenant ON purchase_orders(tenant_id);
CREATE INDEX IF NOT EXISTS idx_purchase_orders_number ON purchase_orders(tenant_id, po_number);
CREATE INDEX IF NOT EXISTS idx_purchase_orders_status ON purchase_orders(tenant_id, status);

-- 6. Purchase Order Items Table
CREATE TABLE IF NOT EXISTS purchase_order_items (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    purchase_order_id TEXT NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity_ordered INTEGER NOT NULL CHECK (quantity_ordered > 0),
    quantity_received INTEGER NOT NULL DEFAULT 0 CHECK (quantity_received >= 0),
    unit_cost INTEGER NOT NULL CHECK (unit_cost >= 0),
    total_cost INTEGER NOT NULL CHECK (total_cost >= 0)
);

CREATE INDEX IF NOT EXISTS idx_purchase_order_items_po ON purchase_order_items(purchase_order_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_items_tenant ON purchase_order_items(tenant_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_items_product ON purchase_order_items(tenant_id, product_id);

-- 7. Stock Adjustments Table
CREATE TABLE IF NOT EXISTS stock_adjustments (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    adjustment_number TEXT NOT NULL,
    warehouse_id TEXT NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variance_quantity INTEGER NOT NULL,
    previous_quantity INTEGER NOT NULL CHECK (previous_quantity >= 0),
    new_quantity INTEGER NOT NULL CHECK (new_quantity >= 0),
    reason TEXT NOT NULL,
    journal_entry_id TEXT REFERENCES journal_entries(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    actor_id TEXT,
    CONSTRAINT uq_stock_adjustments_tenant_number UNIQUE (tenant_id, adjustment_number)
);

CREATE INDEX IF NOT EXISTS idx_stock_adjustments_tenant ON stock_adjustments(tenant_id);
CREATE INDEX IF NOT EXISTS idx_stock_adjustments_number ON stock_adjustments(tenant_id, adjustment_number);
CREATE INDEX IF NOT EXISTS idx_stock_adjustments_wh_prod ON stock_adjustments(tenant_id, warehouse_id, product_id);

-- 8. SQLite Triggers for Immutability
CREATE TRIGGER IF NOT EXISTS trg_stock_movements_prevent_update
BEFORE UPDATE ON stock_movements
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Stock movements are immutable and cannot be updated');
END;

CREATE TRIGGER IF NOT EXISTS trg_stock_movements_prevent_delete
BEFORE DELETE ON stock_movements
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Stock movements are immutable and cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS trg_stock_adjustments_prevent_update
BEFORE UPDATE ON stock_adjustments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Stock adjustments are immutable and cannot be updated');
END;

CREATE TRIGGER IF NOT EXISTS trg_stock_adjustments_prevent_delete
BEFORE DELETE ON stock_adjustments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Stock adjustments are immutable and cannot be deleted');
END;

-- 9. Seed Canonical Account 1300 (Persediaan Barang Dagang) for All Existing Tenants
INSERT OR IGNORE INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
SELECT 
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))),
    t.id, '1300', 'Persediaan Barang Dagang', 'asset', 1, t.created_at, t.created_at
FROM tenants t;
