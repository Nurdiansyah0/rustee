/**
 * Milestone 4 Empirical Challenger Test Harness
 * Universal Vue 3 PWA Frontend & API Integration (Features 22 & 23)
 *
 * Targets:
 * 1. Capability resolution matrix across workspace types (retail, fnb, rental, general, contractor, personal)
 * 2. Strict personal workspace isolation defense-in-depth & boundary tests
 * 3. Reactive Pinia Inventory Store (useInventoryStore) state, getters, math precision, and edge cases
 * 4. DesktopSidebar.vue dynamic reactivity and POS keypad trigger preservation
 * 5. MobileBottomNav.vue adaptive Slot 4 and Slot 3 POS keypad trigger preservation
 * 6. InventoryView.vue & PurchasingView.vue component mounting, KPI cards, low stock alerts, and mock resilience
 */

import * as hd from 'happy-dom';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';
import { createServer } from 'vite';
import vuePlugin from '@vitejs/plugin-vue';
import { setActivePinia, createPinia } from 'pinia';
import { createSSRApp, h, nextTick } from 'vue';
import { renderToString } from '@vue/server-renderer';

// Setup Headless DOM
const window = new hd.GlobalWindow();
globalThis.window = window;
globalThis.document = window.document;
globalThis.localStorage = window.localStorage;
Object.assign(globalThis, {
  HTMLElement: window.HTMLElement,
  Element: window.Element,
  Node: window.Node,
  SVGElement: window.SVGElement,
  DocumentFragment: window.DocumentFragment,
  MouseEvent: window.MouseEvent,
  KeyboardEvent: window.KeyboardEvent,
  CustomEvent: window.CustomEvent,
  Event: window.Event,
  requestAnimationFrame: (cb) => setTimeout(cb, 16),
  cancelAnimationFrame: (id) => clearTimeout(id),
});

try {
  Object.defineProperty(globalThis, 'navigator', {
    value: { onLine: true, vibrate: () => true },
    configurable: true,
    writable: true,
  });
} catch {}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FRONTEND_ROOT = path.resolve(__dirname, '..');

console.log('='.repeat(80));
console.log('MILESTONE 4 EMPIRICAL CHALLENGER TEST HARNESS');
console.log('Targets: workspace.js, inventory.js, DesktopSidebar, MobileBottomNav, Views');
console.log('='.repeat(80));

let passed = 0;
let failed = 0;
const failureList = [];

function check(condition, testId, message, details = '') {
  if (condition) {
    passed++;
    console.log(`  [PASS] ${testId}: ${message}`);
  } else {
    failed++;
    const fullMsg = `${testId}: ${message}${details ? ' -> ' + details : ''}`;
    console.error(`  [FAIL] ${fullMsg}`);
    failureList.push(fullMsg);
  }
}

// Spin up Vite SSR runtime with Vue plugin
const viteServer = await createServer({
  configFile: false,
  root: FRONTEND_ROOT,
  plugins: [vuePlugin()],
  resolve: {
    alias: {
      '@': path.join(FRONTEND_ROOT, 'src'),
    },
  },
  server: { middlewareMode: true },
  appType: 'custom',
});

// Helper to render a Vue component to HTML string using SSR context
async function renderComponent(component, props = {}, pinia = null) {
  const app = createSSRApp({
    render() {
      return h(component, props);
    },
  });
  if (pinia) {
    app.use(pinia);
  }
  const html = await renderToString(app);
  return html;
}

// Load Modules
const { api } = await viteServer.ssrLoadModule('/src/services/api.js');
const { useWorkspaceStore } = await viteServer.ssrLoadModule('/src/stores/workspace.js');
const { useInventoryStore } = await viteServer.ssrLoadModule('/src/stores/inventory.js');
const DesktopSidebar = (await viteServer.ssrLoadModule('/src/components/layout/DesktopSidebar.vue')).default;
const MobileBottomNav = (await viteServer.ssrLoadModule('/src/components/layout/MobileBottomNav.vue')).default;
const InventoryView = (await viteServer.ssrLoadModule('/src/views/InventoryView.vue')).default;
const PurchasingView = (await viteServer.ssrLoadModule('/src/views/PurchasingView.vue')).default;

// =============================================================================
// SUITE 1: Capability Resolution Matrix Across All Workspace Types
// =============================================================================
console.log('\n--- SUITE 1: Capability Resolution Matrix Across All Workspace Types ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const workspaceStore = useWorkspaceStore();

  // Test 1.1: Retail Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-retail-01',
    name: 'Toko Elektronik Makmur',
    business_type: 'retail',
    is_personal: false,
  };
  workspaceStore.capabilities = ['pos', 'inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'];

  check(workspaceStore.isPersonalWorkspace === false, '1.1a', 'Retail workspace is marked as business workspace');
  check(workspaceStore.hasCapability('inventory') === true, '1.1b', 'Retail surfaces inventory capability');
  check(workspaceStore.hasCapability('purchasing') === true, '1.1c', 'Retail surfaces purchasing capability');
  check(workspaceStore.hasCapability('pos') === true, '1.1d', 'Retail surfaces POS capability');

  // Test 1.2: FnB Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-fnb-01',
    name: 'Kopi Nusantara',
    business_type: 'fnb',
    is_personal: false,
  };
  workspaceStore.capabilities = ['pos', 'tables', 'kitchen', 'inventory', 'purchasing', 'accounting', 'reports'];

  check(workspaceStore.hasCapability('inventory') === true, '1.2a', 'FnB surfaces inventory capability');
  check(workspaceStore.hasCapability('purchasing') === true, '1.2b', 'FnB surfaces purchasing capability');
  check(workspaceStore.hasCapability('kitchen') === true, '1.2c', 'FnB surfaces kitchen capability');

  // Test 1.3: Rental Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-rental-01',
    name: 'Rental Kamera Pro',
    business_type: 'rental',
    is_personal: false,
  };
  workspaceStore.capabilities = ['inventory', 'purchasing', 'bookings', 'invoicing', 'receivables', 'accounting'];

  check(workspaceStore.hasCapability('inventory') === true, '1.3a', 'Rental surfaces inventory capability');
  check(workspaceStore.hasCapability('purchasing') === true, '1.3b', 'Rental surfaces purchasing capability');
  check(workspaceStore.hasCapability('bookings') === true, '1.3c', 'Rental surfaces bookings capability');

  // Test 1.4: General Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-gen-01',
    name: 'PT Usaha Bersama',
    business_type: 'general',
    is_personal: false,
  };
  workspaceStore.capabilities = ['inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'];

  check(workspaceStore.hasCapability('inventory') === true, '1.4a', 'General workspace surfaces inventory capability');
  check(workspaceStore.hasCapability('purchasing') === true, '1.4b', 'General workspace surfaces purchasing capability');

  // Test 1.5: Contractor Workspace (Should NOT have inventory by default)
  workspaceStore.currentWorkspace = {
    id: 'ws-contractor-01',
    name: 'Kontraktor Mandiri',
    business_type: 'contractor',
    is_personal: false,
  };
  workspaceStore.capabilities = ['projects', 'milestones', 'invoicing', 'receivables', 'accounting'];

  check(workspaceStore.hasCapability('projects') === true, '1.5a', 'Contractor surfaces projects capability');
  check(workspaceStore.hasCapability('inventory') === false, '1.5b', 'Contractor strictly omits inventory capability');
  check(workspaceStore.hasCapability('purchasing') === false, '1.5c', 'Contractor strictly omits purchasing capability');

  // Test 1.6: Personal Workspace - Strict Isolation
  workspaceStore.currentWorkspace = {
    id: 'ws-personal-01',
    name: 'Dompet Pribadi',
    business_type: 'personal',
    is_personal: true,
  };
  workspaceStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];

  check(workspaceStore.isPersonalWorkspace === true, '1.6a', 'Personal workspace correctly flagged as isPersonalWorkspace');
  check(workspaceStore.hasCapability('inventory') === false, '1.6b', 'Personal workspace strictly denies inventory');
  check(workspaceStore.hasCapability('purchasing') === false, '1.6c', 'Personal workspace strictly denies purchasing');
  check(workspaceStore.hasCapability('invoicing') === false, '1.6d', 'Personal workspace strictly denies invoicing');
  check(workspaceStore.hasCapability('accounting') === false, '1.6e', 'Personal workspace strictly denies accounting');
  check(workspaceStore.hasCapability('accounts') === true, '1.6f', 'Personal workspace allows accounts');
  check(workspaceStore.hasCapability('budgets') === true, '1.6g', 'Personal workspace allows budgets');

  // Test 1.7: Adversarial Personal Workspace Leak Attempt
  // Even if an attacker or corrupted API injects 'inventory' into capabilities array,
  // isPersonalWorkspace guard MUST unconditionally block it.
  workspaceStore.capabilities = ['inventory', 'purchasing', 'invoicing', 'accounting', 'analytics'];
  check(workspaceStore.hasCapability('inventory') === false, '1.7a', 'Adversarial: Corrupted capabilities array blocked by personal isolation guard (inventory)');
  check(workspaceStore.hasCapability('purchasing') === false, '1.7b', 'Adversarial: Corrupted capabilities array blocked by personal isolation guard (purchasing)');

  // Test 1.8: Personal Workspace Variant Flags
  const personalVariants = [
    { is_personal: true },
    { is_personal: 1 },
    { is_default: true },
    { is_default: 1 },
    { business_type: 'personal' },
    { type: 'personal' },
  ];
  for (let i = 0; i < personalVariants.length; i++) {
    workspaceStore.currentWorkspace = { id: `ws-p-${i}`, ...personalVariants[i] };
    check(workspaceStore.isPersonalWorkspace === true, `1.8.${i}`, `Personal flag variant matches: ${JSON.stringify(personalVariants[i])}`);
  }

  // Test 1.9: Dynamic Purchasing Rule (Purchasing auto-enabled if Inventory is enabled)
  workspaceStore.currentWorkspace = {
    id: 'ws-retail-02',
    name: 'Toko B',
    business_type: 'retail',
    is_personal: false,
  };
  workspaceStore.capabilities = ['inventory']; // Notice 'purchasing' is NOT in array
  check(workspaceStore.hasCapability('purchasing') === true, '1.9', 'Purchasing automatically resolves true when inventory is present');

  // Test 1.10: Null, Empty, and Malformed Queries
  check(workspaceStore.hasCapability(null) === false, '1.10a', 'hasCapability(null) strictly returns false');
  check(workspaceStore.hasCapability('') === false, '1.10b', 'hasCapability("") strictly returns false');
  check(workspaceStore.hasCapability(undefined) === false, '1.10c', 'hasCapability(undefined) strictly returns false');
}

// =============================================================================
// SUITE 2: Pinia Inventory Store (useInventoryStore) Getters & Edge Cases
// =============================================================================
console.log('\n--- SUITE 2: useInventoryStore Getters, Math & Edge Cases ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const inventoryStore = useInventoryStore();

  // Test 2.1: State Initialization
  check(Array.isArray(inventoryStore.warehouses) && inventoryStore.warehouses.length === 0, '2.1a', 'Initial warehouses is empty array');
  check(inventoryStore.activeWarehouseId === null, '2.1b', 'Initial activeWarehouseId is null');
  check(Array.isArray(inventoryStore.products) && inventoryStore.products.length === 0, '2.1c', 'Initial products is empty array');
  check(Array.isArray(inventoryStore.stockItems) && inventoryStore.stockItems.length === 0, '2.1d', 'Initial stockItems is empty array');
  check(Array.isArray(inventoryStore.movements) && inventoryStore.movements.length === 0, '2.1e', 'Initial movements is empty array');
  check(Array.isArray(inventoryStore.purchaseOrders) && inventoryStore.purchaseOrders.length === 0, '2.1f', 'Initial purchaseOrders is empty array');
  check(inventoryStore.loading === false, '2.1g', 'Initial loading is false');
  check(inventoryStore.error === null, '2.1h', 'Initial error is null');

  // Populate test fixtures
  const wh1 = { id: 'wh-01', code: 'WH-MAIN', name: 'Gudang Utama', is_default: true };
  const wh2 = { id: 'wh-02', code: 'WH-BRANCH', name: 'Gudang Cabang', is_default: false };
  inventoryStore.warehouses = [wh1, wh2];

  const p1 = { id: 'prod-01', sku: 'SKU-000001', name: 'Kopi Arabika 1kg' };
  const p2 = { id: 'prod-02', sku: 'SKU-000002', name: 'Gula Pasir 1kg' };
  const p3 = { id: 'prod-03', sku: 'SKU-000003', name: 'Susu UHT 1L' };
  inventoryStore.products = [p1, p2, p3];

  inventoryStore.stockItems = [
    // wh-01 items
    { id: 'stk-01', warehouse_id: 'wh-01', product_id: 'prod-01', quantity_on_hand: 50, reorder_threshold: 20, average_cost: 100000 },
    { id: 'stk-02', warehouse_id: 'wh-01', product_id: 'prod-02', quantity_on_hand: 10, reorder_threshold: 15, average_cost: 18000 }, // Low stock!
    { id: 'stk-03', warehouse_id: 'wh-01', product_id: 'prod-03', quantity_on_hand: 5, reorder_threshold: 5, average_cost: 20000 },  // Exact threshold boundary!
    // wh-02 items
    { id: 'stk-04', warehouse_id: 'wh-02', product_id: 'prod-01', quantity_on_hand: 20, reorder_threshold: 10, average_cost: 105000 },
    { id: 'stk-05', warehouse_id: 'wh-02', product_id: 'prod-02', quantity_on_hand: 0, reorder_threshold: 10, average_cost: 18000 },  // Zero stock!
  ];

  // Test 2.2: activeWarehouse Getter
  check(inventoryStore.activeWarehouse === null, '2.2a', 'activeWarehouse is null when activeWarehouseId is null');
  inventoryStore.setActiveWarehouse('wh-01');
  check(inventoryStore.activeWarehouse?.name === 'Gudang Utama', '2.2b', 'activeWarehouse resolves Gudang Utama');
  inventoryStore.setActiveWarehouse('non-existent-wh');
  check(inventoryStore.activeWarehouse === null, '2.2c', 'activeWarehouse resolves null for non-existent warehouse ID');
  inventoryStore.setActiveWarehouse(null);

  // Test 2.3: filteredStockItems Getter
  check(inventoryStore.filteredStockItems.length === 5, '2.3a', 'filteredStockItems returns all 5 items when activeWarehouseId is null');
  inventoryStore.setActiveWarehouse('wh-01');
  check(inventoryStore.filteredStockItems.length === 3, '2.3b', 'filteredStockItems returns 3 items for wh-01');
  inventoryStore.setActiveWarehouse('wh-02');
  check(inventoryStore.filteredStockItems.length === 2, '2.3c', 'filteredStockItems returns 2 items for wh-02');
  inventoryStore.setActiveWarehouse(null);

  // Test 2.4: lowStockItems Getter & Boundary Stress Testing
  // Unfiltered: stk-02 (10 <= 15), stk-03 (5 <= 5), stk-05 (0 <= 10) => 3 items
  const lowUnfiltered = inventoryStore.lowStockItems;
  check(lowUnfiltered.length === 3, '2.4a', 'lowStockItems detects 3 low stock items across all warehouses');
  check(lowUnfiltered.some(s => s.id === 'stk-02'), '2.4b', 'Detects stk-02 where qty < threshold');
  check(lowUnfiltered.some(s => s.id === 'stk-03'), '2.4c', 'Detects stk-03 on exact boundary qty == threshold');
  check(lowUnfiltered.some(s => s.id === 'stk-05'), '2.4d', 'Detects stk-05 where qty == 0');

  // Filtered by warehouse
  inventoryStore.setActiveWarehouse('wh-01');
  const lowWh01 = inventoryStore.lowStockItems;
  check(lowWh01.length === 2, '2.4e', 'lowStockItems filtered to wh-01 detects exactly 2 items');
  inventoryStore.setActiveWarehouse('wh-02');
  const lowWh02 = inventoryStore.lowStockItems;
  check(lowWh02.length === 1 && lowWh02[0].id === 'stk-05', '2.4f', 'lowStockItems filtered to wh-02 detects stk-05');
  inventoryStore.setActiveWarehouse(null);

  // Stress: String coercion in quantity_on_hand and reorder_threshold
  inventoryStore.stockItems.push({
    id: 'stk-str',
    warehouse_id: 'wh-01',
    product_id: 'prod-03',
    quantity_on_hand: '8',
    reorder_threshold: '12',
    average_cost: '22000'
  });
  check(inventoryStore.lowStockItems.some(s => s.id === 'stk-str'), '2.4g', 'String-typed quantities coerced correctly in lowStockItems');
  inventoryStore.stockItems.pop(); // cleanup

  // Test 2.5: totalStockValuation Getter & Arithmetic Precision
  // Calculation across all 5 items:
  // stk-01: 50 * 100,000 = 5,000,000
  // stk-02: 10 * 18,000  =   180,000
  // stk-03:  5 * 20,000  =   100,000
  // stk-04: 20 * 105,000 = 2,100,000
  // stk-05:  0 * 18,000  =         0
  // Total = 7,380,000
  check(inventoryStore.totalStockValuation === 7380000, '2.5a', 'totalStockValuation correctly computes 7,380,000 IDR across all warehouses');

  inventoryStore.setActiveWarehouse('wh-01');
  // wh-01: 5,000,000 + 180,000 + 100,000 = 5,280,000
  check(inventoryStore.totalStockValuation === 5280000, '2.5b', 'totalStockValuation correctly computes 5,280,000 IDR for wh-01');

  inventoryStore.setActiveWarehouse('wh-02');
  // wh-02: 2,100,000 + 0 = 2,100,000
  check(inventoryStore.totalStockValuation === 2100000, '2.5c', 'totalStockValuation correctly computes 2,100,000 IDR for wh-02');
  inventoryStore.setActiveWarehouse(null);

  // Stress: High-valuation stock item (e.g. 500,000 units @ 1,200,000 IDR = 600,000,000,000 IDR)
  inventoryStore.stockItems.push({
    id: 'stk-high',
    warehouse_id: 'wh-01',
    product_id: 'prod-01',
    quantity_on_hand: 500000,
    average_cost: 1200000,
    reorder_threshold: 100
  });
  const expectedBig = 7380000 + (500000 * 1200000);
  check(inventoryStore.totalStockValuation === expectedBig, '2.5d', `High stock valuation computed accurately (${inventoryStore.totalStockValuation} IDR)`);
  inventoryStore.stockItems.pop(); // cleanup

  // Test 2.6: totalQuantityOnHand Getter
  // 50 + 10 + 5 + 20 + 0 = 85
  check(inventoryStore.totalQuantityOnHand === 85, '2.6a', 'totalQuantityOnHand computes 85 across all warehouses');
  inventoryStore.setActiveWarehouse('wh-01');
  check(inventoryStore.totalQuantityOnHand === 65, '2.6b', 'totalQuantityOnHand computes 65 for wh-01');
  inventoryStore.setActiveWarehouse(null);

  // Test 2.7: totalSkuCount Getter (Deduplication)
  // Products: prod-01 (wh-01, wh-02), prod-02 (wh-01, wh-02), prod-03 (wh-01) => 3 unique products
  check(inventoryStore.totalSkuCount === 3, '2.7a', 'totalSkuCount deduplicates products to 3 across all stock records');
  inventoryStore.setActiveWarehouse('wh-02');
  // wh-02 has prod-01 and prod-02 => 2 unique products
  check(inventoryStore.totalSkuCount === 2, '2.7b', 'totalSkuCount computes 2 unique SKUs in wh-02');
  inventoryStore.setActiveWarehouse(null);

  // Test 2.8: Helper Lookup Functions
  check(inventoryStore.getWarehouseById('wh-01')?.name === 'Gudang Utama', '2.8a', 'getWarehouseById resolves warehouse');
  check(inventoryStore.getWarehouseById('wh-none') === undefined, '2.8b', 'getWarehouseById returns undefined for missing ID');
  check(inventoryStore.getProductById('prod-02')?.name === 'Gula Pasir 1kg', '2.8c', 'getProductById resolves product');
  check(inventoryStore.getProductById('prod-none') === undefined, '2.8d', 'getProductById returns undefined for missing ID');
}

// =============================================================================
// SUITE 3: Pinia Inventory Store Actions & Coordinated Refreshes
// =============================================================================
console.log('\n--- SUITE 3: useInventoryStore Actions & State Transitions ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const inventoryStore = useInventoryStore();

  // Test 3.1: fetchWarehouses auto-selection
  let mockWarehouses = [
    { id: 'w-1', name: 'Gudang Non-Default', is_default: false },
    { id: 'w-2', name: 'Gudang Default Utama', is_default: true }
  ];
  api.getWarehouses = async () => mockWarehouses;

  await inventoryStore.fetchWarehouses();
  check(inventoryStore.warehouses.length === 2, '3.1a', 'fetchWarehouses populates warehouses array');
  check(inventoryStore.activeWarehouseId === 'w-2', '3.1b', 'fetchWarehouses automatically selects default warehouse as activeWarehouseId');

  // Test 3.2: createProduct coordinates fetchProducts and fetchStock
  let fetchedProductsCount = 0;
  let fetchedStockCount = 0;
  api.createProduct = async (data) => ({ id: 'new-p', ...data });
  api.getProducts = async () => { fetchedProductsCount++; return [{ id: 'new-p', sku: 'SKU-000099' }]; };
  api.getStockItems = async () => { fetchedStockCount++; return []; };

  await inventoryStore.createProduct({ name: 'Beras Premium', unit: 'kg', cost_price: 15000, sale_price: 18000 });
  check(fetchedProductsCount === 1, '3.2a', 'createProduct invokes fetchProducts');
  check(fetchedStockCount === 1, '3.2b', 'createProduct invokes fetchStock');

  // Test 3.3: Stock Mutations Coordinate Refreshes
  let stockRefreshCount = 0;
  let movementsRefreshCount = 0;
  api.getStockItems = async () => { stockRefreshCount++; return []; };
  api.getStockMovements = async () => { movementsRefreshCount++; return []; };
  api.transferStock = async () => ({ id: 'mov-1', type: 'TRANSFER' });
  api.adjustStock = async () => ({ id: 'adj-1', type: 'ADJUSTMENT' });

  await inventoryStore.transferStock({
    product_id: 'p-1',
    source_warehouse_id: 'w-1',
    destination_warehouse_id: 'w-2',
    quantity: 5
  });
  check(stockRefreshCount === 1 && movementsRefreshCount === 1, '3.3a', 'transferStock refetches both stock and movements');

  await inventoryStore.adjustStock({
    product_id: 'p-1',
    warehouse_id: 'w-1',
    actual_quantity: 10,
    reason: 'Opname Bulanan'
  });
  check(stockRefreshCount === 2 && movementsRefreshCount === 2, '3.3b', 'adjustStock refetches both stock and movements');

  // Test 3.4: PO Goods Receipt Coordinates 3 Refreshes
  let poRefreshCount = 0;
  api.getPurchaseOrders = async () => { poRefreshCount++; return []; };
  api.receivePurchaseOrder = async (id, data) => ({ id, status: 'RECEIVED' });

  await inventoryStore.receivePurchaseOrder('po-01', {
    received_items: [{ item_id: 'item-1', received_quantity: 10, actual_unit_cost: 15000 }]
  });
  check(poRefreshCount === 1, '3.4a', 'receivePurchaseOrder refetches purchase orders');
  check(stockRefreshCount === 3, '3.4b', 'receivePurchaseOrder refetches stock items');
  check(movementsRefreshCount === 3, '3.4c', 'receivePurchaseOrder refetches stock movements');

  // Test 3.5: refreshAll Resets Loading in Finally Block
  api.getWarehouses = async () => [];
  api.getProducts = async () => [];
  api.getStockItems = async () => { throw new Error('Network Disconnect'); };
  api.getStockMovements = async () => [];
  api.getPurchaseOrders = async () => [];

  await inventoryStore.refreshAll();
  check(inventoryStore.loading === false, '3.5', 'refreshAll ensures loading is reset to false even if one query fails');
}

// =============================================================================
// SUITE 4: DesktopSidebar.vue Dynamic Reactivity & POS Trigger Preservation
// =============================================================================
console.log('\n--- SUITE 4: DesktopSidebar.vue Navigation & POS Keypad Preservation ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const workspaceStore = useWorkspaceStore();

  // Test 4.1: Personal Workspace Navigation
  workspaceStore.currentWorkspace = {
    id: 'ws-pers-side',
    name: 'Personal Account',
    business_type: 'personal',
    is_personal: true
  };
  workspaceStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];

  const htmlPersonal = await renderComponent(DesktopSidebar, { activeTab: 'home' }, pinia);

  check(!htmlPersonal.includes('Inventaris &amp; Stok') && !htmlPersonal.includes('Inventaris & Stok'), '4.1a', 'Sidebar strictly omits Inventaris & Stok in personal workspace');
  check(!htmlPersonal.includes('Pesanan Pembelian'), '4.1b', 'Sidebar strictly omits Pesanan Pembelian in personal workspace');
  check(!htmlPersonal.includes('Faktur &amp; Tagihan') && !htmlPersonal.includes('Faktur & Tagihan'), '4.1c', 'Sidebar strictly omits Faktur in personal workspace');
  check(htmlPersonal.includes('Catat Transaksi'), '4.1d', 'Sidebar preserves "Catat Transaksi" button in personal workspace');

  // Test 4.2: POS Trigger Invariant in Source Code
  const sidebarSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/components/layout/DesktopSidebar.vue'), 'utf-8');
  check(sidebarSource.includes('@click="$emit(\'open-add\')"'), '4.2a', 'DesktopSidebar button emits open-add on click');
  check(sidebarSource.includes('variant="primary"'), '4.2b', 'Catat Transaksi button uses primary variant styling');

  // Test 4.3: Switch to Business Workspace (Retail)
  workspaceStore.currentWorkspace = {
    id: 'ws-retail-side',
    name: 'Retail Store',
    business_type: 'retail',
    is_personal: false
  };
  workspaceStore.capabilities = ['pos', 'inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'];

  const htmlBusiness = await renderComponent(DesktopSidebar, { activeTab: 'home' }, pinia);

  check(htmlBusiness.includes('Inventaris &amp; Stok') || htmlBusiness.includes('Inventaris & Stok'), '4.3a', 'Sidebar dynamically surfaces Inventaris & Stok when business workspace active');
  check(htmlBusiness.includes('Pesanan Pembelian'), '4.3b', 'Sidebar dynamically surfaces Pesanan Pembelian when business workspace active');
  check(htmlBusiness.includes('Catat Transaksi'), '4.3c', 'Catat Transaksi remains primary action in business workspace');
}

// =============================================================================
// SUITE 5: MobileBottomNav.vue Adaptive Slot 4 & Slot 3 POS Keypad Preservation
// =============================================================================
console.log('\n--- SUITE 5: MobileBottomNav.vue Adaptive Slot 4 & Slot 3 Invariants ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const workspaceStore = useWorkspaceStore();

  // Test 5.1: Slot 3 Invariant (Elevated Floating Button with -mt-5)
  const bottomNavSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/components/layout/MobileBottomNav.vue'), 'utf-8');
  check(bottomNavSource.includes('@click="$emit(\'open-add\')"'), '5.1a', 'MobileBottomNav Slot 3 emits open-add on click');
  check(bottomNavSource.includes('-mt-5 w-12 h-12 rounded-2xl bg-brand-default text-content-inverse'), '5.1b', 'Slot 3 has elevated floating -mt-5 geometry');
  check(bottomNavSource.includes('aria-label="Catat Transaksi Baru"'), '5.1c', 'Slot 3 has aria-label for Catat Transaksi Baru');

  // Test 5.2: Slot 4 Under Retail Business Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-retail-mob',
    name: 'Retail Store',
    business_type: 'retail',
    is_personal: false
  };
  workspaceStore.capabilities = ['pos', 'inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'];

  const htmlRetail = await renderComponent(MobileBottomNav, { activeTab: 'home' }, pinia);
  check(htmlRetail.includes('Stok'), '5.2', 'Slot 4 dynamically displays Stok when inventory capability active');

  // Test 5.3: Slot 4 Under Purchasing Active Tab
  const htmlPurchasing = await renderComponent(MobileBottomNav, { activeTab: 'purchasing' }, pinia);
  check(htmlPurchasing.includes('Beli'), '5.3', 'Slot 4 dynamically displays Beli when activeTab is purchasing');

  // Test 5.4: Slot 4 Under Inventory Active Tab
  const htmlInventory = await renderComponent(MobileBottomNav, { activeTab: 'inventory' }, pinia);
  check(htmlInventory.includes('Stok'), '5.4', 'Slot 4 dynamically displays Stok when activeTab is inventory');

  // Test 5.5: Slot 4 Under Personal Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-pers-mob',
    name: 'Personal Account',
    business_type: 'personal',
    is_personal: true
  };
  workspaceStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];

  const htmlPersonal = await renderComponent(MobileBottomNav, { activeTab: 'home' }, pinia);
  check(htmlPersonal.includes('Analitik'), '5.5', 'Slot 4 dynamically falls back to Analitik in personal workspace');
}

// =============================================================================
// SUITE 6: Views Mounting, KPI Cards & Mock/API Resilience
// =============================================================================
console.log('\n--- SUITE 6: InventoryView.vue & PurchasingView.vue Mounting & Flow ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const inventoryStore = useInventoryStore();

  // Populate mock data for InventoryView
  inventoryStore.warehouses = [
    { id: 'w-1', code: 'WH-01', name: 'Gudang Pusat', is_default: true }
  ];
  inventoryStore.activeWarehouseId = 'w-1';
  inventoryStore.products = [
    { id: 'p-1', sku: 'SKU-0001', name: 'Barang A', cost_price: 10000, sale_price: 15000, unit: 'pcs' }
  ];
  inventoryStore.stockItems = [
    { id: 's-1', warehouse_id: 'w-1', product_id: 'p-1', quantity_on_hand: 3, reorder_threshold: 5, average_cost: 10000 }
  ];

  // Test 6.1: Render InventoryView with Low Stock
  const invHtmlLow = await renderComponent(InventoryView, {}, pinia);
  check(invHtmlLow.includes('Inventaris &amp; Stok Multi-Gudang') || invHtmlLow.includes('Inventaris & Stok Multi-Gudang'), '6.1a', 'InventoryView renders main header');
  check(invHtmlLow.includes('Peringatan Stok Menipis'), '6.1b', 'InventoryView renders low stock alert banner when lowStockItems > 0');
  check(invHtmlLow.includes('Daftar Stok Barang'), '6.1c', 'InventoryView renders sub-tab Daftar Stok Barang');
  check(invHtmlLow.includes('Riwayat Mutasi Stok'), '6.1d', 'InventoryView renders sub-tab Riwayat Mutasi Stok');
  check(invHtmlLow.includes('Master Lokasi Gudang'), '6.1e', 'InventoryView renders sub-tab Master Lokasi Gudang');

  // Test 6.2: Low stock banner hides when lowStockItems is healthy
  inventoryStore.stockItems = [
    { id: 's-1', warehouse_id: 'w-1', product_id: 'p-1', quantity_on_hand: 100, reorder_threshold: 5, average_cost: 10000 }
  ];
  const invHtmlHealthy = await renderComponent(InventoryView, {}, pinia);
  check(!invHtmlHealthy.includes('Peringatan Stok Menipis'), '6.2', 'InventoryView hides low stock alert banner when all items healthy');

  // Test 6.3: Render PurchasingView
  inventoryStore.purchaseOrders = [
    { id: 'po-1', po_number: 'PO-2026-000001', supplier_name: 'PT Mitra Sukses', status: 'DRAFT', total_amount: 1500000, lines: [] },
    { id: 'po-2', po_number: 'PO-2026-000002', supplier_name: 'CV Makmur', status: 'ORDERED', total_amount: 2500000, lines: [] },
    { id: 'po-3', po_number: 'PO-2026-000003', supplier_name: 'PT Mitra Sukses', status: 'RECEIVED', total_amount: 5000000, lines: [] }
  ];

  const poHtml = await renderComponent(PurchasingView, {}, pinia);
  check(poHtml.includes('Pesanan Pembelian &amp; Penerimaan Barang') || poHtml.includes('Pesanan Pembelian & Penerimaan Barang'), '6.3a', 'PurchasingView renders main header');
  check(poHtml.includes('Total Seluruh PO'), '6.3b', 'PurchasingView renders Total Seluruh PO stat card');
  check(poHtml.includes('Draft PO'), '6.3c', 'PurchasingView renders Draft PO stat card');
  check(poHtml.includes('Menunggu Pengiriman'), '6.3d', 'PurchasingView renders Menunggu Pengiriman stat card');
  check(poHtml.includes('Selesai Diterima'), '6.3e', 'PurchasingView renders Selesai Diterima stat card');
}

// Cleanup Vite Server
await viteServer.close();

console.log('\n' + '='.repeat(80));
console.log(`TEST SUMMARY: ${passed} PASSED, ${failed} FAILED`);
console.log('='.repeat(80));

if (failed > 0) {
  console.error('\nFAILURES:');
  failureList.forEach(f => console.error(`  - ${f}`));
  process.exit(1);
} else {
  console.log('\nALL EMPIRICAL CHALLENGER TESTS PASSED SUCCESSFULLY (100%)!');
  process.exit(0);
}
