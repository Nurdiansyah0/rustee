/**
 * Milestone 4 Empirical Challenger 2 Test Harness
 * Adversarial Testing for Universal Vue 3 PWA Frontend (Features 22 & 23)
 *
 * Challenge Objectives:
 * 1. Personal Workspace Isolation Stress Test:
 *    - Adversarial capability injection when isPersonalWorkspace === true
 *    - Reactive tab eviction when switching workspaces in App.vue
 * 2. POS Keypad Trigger Integrity:
 *    - DesktopSidebar.vue "Catat Transaksi" button emits 'open-add'
 *    - MobileBottomNav.vue Slot 3 emits 'open-add'
 *    - Non-conflict with inventory/purchasing modals
 * 3. Offline Mock Data Resilience:
 *    - Full lifecycle test of handleMockApiRequest in mockData.js
 *    - Warehouses, products, stock items, transfers, adjustments, and purchase orders
 */

import * as hd from 'happy-dom';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';
import { createServer } from 'vite';
import vuePlugin from '@vitejs/plugin-vue';
import { setActivePinia, createPinia } from 'pinia';
import { createSSRApp, h, nextTick, ref, computed, watch } from 'vue';
import { renderToString } from '@vue/server-renderer';

// Setup Headless DOM environment
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
console.log('MILESTONE 4 EMPIRICAL CHALLENGER 2 HARNESS');
console.log('Focus: Personal Isolation, Tab Eviction, POS Trigger Integrity, Mock Data');
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

// Spin up Vite SSR runtime to load Vue modules cleanly
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

// Helper to render component to HTML string
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

// Load modules under test
const { useWorkspaceStore } = await viteServer.ssrLoadModule('/src/stores/workspace.js');
const { useInventoryStore } = await viteServer.ssrLoadModule('/src/stores/inventory.js');
const { handleMockApiRequest } = await viteServer.ssrLoadModule('/src/services/mockData.js');
const DesktopSidebar = (await viteServer.ssrLoadModule('/src/components/layout/DesktopSidebar.vue')).default;
const MobileBottomNav = (await viteServer.ssrLoadModule('/src/components/layout/MobileBottomNav.vue')).default;

// =============================================================================
// SUITE 1: Personal Workspace Isolation & Adversarial Capability Injection
// =============================================================================
console.log('\n--- SUITE 1: Personal Workspace Isolation & Adversarial Capability Injection ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const workspaceStore = useWorkspaceStore();

  // 1.1 Baseline Personal Workspace
  workspaceStore.currentWorkspace = {
    id: 'ws-personal-standard',
    name: 'Dompet Pribadi',
    business_type: 'personal',
    is_personal: true,
  };
  workspaceStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];

  check(workspaceStore.isPersonalWorkspace === true, '1.1a', 'Standard personal workspace is recognized as personal');
  check(workspaceStore.hasCapability('inventory') === false, '1.1b', 'Personal workspace strictly denies inventory');
  check(workspaceStore.hasCapability('purchasing') === false, '1.1c', 'Personal workspace strictly denies purchasing');
  check(workspaceStore.hasCapability('invoicing') === false, '1.1d', 'Personal workspace strictly denies invoicing');
  check(workspaceStore.hasCapability('accounting') === false, '1.1e', 'Personal workspace strictly denies accounting');
  check(workspaceStore.hasCapability('accounts') === true, '1.1f', 'Personal workspace permits legitimate personal accounts');
  check(workspaceStore.hasCapability('budgets') === true, '1.1g', 'Personal workspace permits legitimate personal budgets');

  // 1.2 Adversarial Capability Injection into Personal Workspace
  workspaceStore.capabilities = [
    'inventory',
    'purchasing',
    'invoicing',
    'accounting',
    'pos',
    'accounts',
    'transactions'
  ];

  check(workspaceStore.hasCapability('inventory') === false, '1.2a', 'Adversarial injection: inventory blocked despite presence in capabilities');
  check(workspaceStore.hasCapability('purchasing') === false, '1.2b', 'Adversarial injection: purchasing blocked despite presence in capabilities');
  check(workspaceStore.hasCapability('invoicing') === false, '1.2c', 'Adversarial injection: invoicing blocked despite presence in capabilities');
  check(workspaceStore.hasCapability('accounting') === false, '1.2d', 'Adversarial injection: accounting blocked despite presence in capabilities');
  check(workspaceStore.hasCapability('accounts') === true, '1.2e', 'Adversarial injection: non-restricted personal capabilities still function');

  // 1.3 Personal Workspace Flag Variations Defense
  const personalFlagCases = [
    { name: 'is_personal: true', ws: { id: 'p1', is_personal: true } },
    { name: 'is_personal: 1 (numeric)', ws: { id: 'p2', is_personal: 1 } },
    { name: 'is_default: true', ws: { id: 'p3', is_default: true } },
    { name: 'is_default: 1 (numeric)', ws: { id: 'p4', is_default: 1 } },
    { name: 'business_type: "personal"', ws: { id: 'p5', business_type: 'personal' } },
    { name: 'type: "personal"', ws: { id: 'p6', type: 'personal' } },
    { name: 'null workspace (uninitialized)', ws: null },
    { name: 'undefined workspace', ws: undefined },
  ];

  personalFlagCases.forEach((c, idx) => {
    workspaceStore.currentWorkspace = c.ws;
    workspaceStore.capabilities = ['inventory', 'purchasing', 'invoicing', 'accounting'];
    const isPersonal = workspaceStore.isPersonalWorkspace;
    const invAllowed = workspaceStore.hasCapability('inventory');
    const purAllowed = workspaceStore.hasCapability('purchasing');
    check(isPersonal === true, `1.3.${idx+1}a`, `Flag variation (${c.name}): evaluated as personal workspace`);
    check(invAllowed === false && purAllowed === false, `1.3.${idx+1}b`, `Flag variation (${c.name}): inventory and purchasing blocked`);
  });

  // 1.4 Adversarial Input Edge Cases & Malformed Queries
  workspaceStore.currentWorkspace = { id: 'ws-retail', is_personal: false, business_type: 'retail' };
  workspaceStore.capabilities = ['inventory', 'purchasing'];

  check(workspaceStore.hasCapability(null) === false, '1.4a', 'hasCapability(null) returns false safely');
  check(workspaceStore.hasCapability(undefined) === false, '1.4b', 'hasCapability(undefined) returns false safely');
  check(workspaceStore.hasCapability('') === false, '1.4c', 'hasCapability("") returns false safely');
  check(workspaceStore.hasCapability('INVENTORY') === false, '1.4d', 'hasCapability("INVENTORY") case-sensitivity check returns false');
  check(workspaceStore.hasCapability(' inventory ') === false, '1.4e', 'hasCapability(" inventory ") padded string returns false');
  check(workspaceStore.hasCapability('__proto__') === false, '1.4f', 'hasCapability("__proto__") prototype pollution check returns false');

  // 1.5 Dynamic Purchasing Rule Under Business vs Personal
  workspaceStore.currentWorkspace = { id: 'ws-biz', is_personal: false, business_type: 'general' };
  workspaceStore.capabilities = ['inventory'];
  check(workspaceStore.hasCapability('purchasing') === true, '1.5a', 'Business workspace: purchasing auto-granted when inventory is present');

  workspaceStore.currentWorkspace = { id: 'ws-pers', is_personal: true };
  workspaceStore.capabilities = ['inventory'];
  check(workspaceStore.hasCapability('purchasing') === false, '1.5b', 'Personal workspace: purchasing strictly denied even if inventory in array');
}

// =============================================================================
// SUITE 2: Reactive Tab Eviction Stress Testing (App.vue Watcher Simulation)
// =============================================================================
console.log('\n--- SUITE 2: Reactive Tab Eviction Stress Testing ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const workspaceStore = useWorkspaceStore();

  // Reproduce exact reactive state & watcher logic from App.vue (lines 263-288)
  const tabOrder = ['home', 'transactions', 'inventory', 'purchasing', 'invoices', 'accounting', 'analytics', 'profile'];
  const currentTab = ref('home');
  const transitionName = ref('slide-left');

  function handleSelectTab(tab) {
    if (tab === currentTab.value) return;
    const oldIndex = tabOrder.indexOf(currentTab.value);
    const newIndex = tabOrder.indexOf(tab);
    transitionName.value = newIndex >= oldIndex ? 'slide-left' : 'slide-right';
    currentTab.value = tab;
  }

  // Exact watcher from App.vue lines 281-288
  watch(
    () => workspaceStore.isPersonalWorkspace,
    (isPersonal) => {
      if (isPersonal && ['inventory', 'purchasing', 'invoices', 'accounting'].includes(currentTab.value)) {
        handleSelectTab('home');
      }
    }
  );

  // Initialize workspace to business
  workspaceStore.currentWorkspace = { id: 'biz-1', is_personal: false, business_type: 'retail' };
  await nextTick();

  // 2.1 Tab Eviction from 'inventory' to 'home'
  currentTab.value = 'inventory';
  await nextTick();
  check(currentTab.value === 'inventory', '2.1a', 'Pre-condition: User is on "inventory" tab in business workspace');

  workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true, business_type: 'personal' };
  await nextTick();
  check(currentTab.value === 'home', '2.1b', 'Reactive eviction: Tab evicted from "inventory" to "home" upon switching to personal');

  // 2.2 Tab Eviction from 'purchasing' to 'home'
  workspaceStore.currentWorkspace = { id: 'biz-1', is_personal: false, business_type: 'retail' };
  await nextTick();
  currentTab.value = 'purchasing';
  await nextTick();
  check(currentTab.value === 'purchasing', '2.2a', 'Pre-condition: User is on "purchasing" tab in business workspace');

  workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true, business_type: 'personal' };
  await nextTick();
  check(currentTab.value === 'home', '2.2b', 'Reactive eviction: Tab evicted from "purchasing" to "home" upon switching to personal');

  // 2.3 Tab Eviction from 'invoices' to 'home'
  workspaceStore.currentWorkspace = { id: 'biz-1', is_personal: false, business_type: 'retail' };
  await nextTick();
  currentTab.value = 'invoices';
  await nextTick();
  workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true, business_type: 'personal' };
  await nextTick();
  check(currentTab.value === 'home', '2.3', 'Reactive eviction: Tab evicted from "invoices" to "home" upon switching to personal');

  // 2.4 Tab Eviction from 'accounting' to 'home'
  workspaceStore.currentWorkspace = { id: 'biz-1', is_personal: false, business_type: 'retail' };
  await nextTick();
  currentTab.value = 'accounting';
  await nextTick();
  workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true, business_type: 'personal' };
  await nextTick();
  check(currentTab.value === 'home', '2.4', 'Reactive eviction: Tab evicted from "accounting" to "home" upon switching to personal');

  // 2.5 Non-Eviction for Non-Business Tabs
  const allowedPersonalTabs = ['transactions', 'analytics', 'profile', 'home'];
  for (const tab of allowedPersonalTabs) {
    workspaceStore.currentWorkspace = { id: 'biz-1', is_personal: false, business_type: 'retail' };
    await nextTick();
    currentTab.value = tab;
    await nextTick();

    workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true, business_type: 'personal' };
    await nextTick();
    check(currentTab.value === tab, `2.5.${tab}`, `Non-eviction: Tab "${tab}" remains unchanged when switching to personal workspace`);
  }

  // 2.6 Personal to Personal Workspace Switch
  workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true, business_type: 'personal' };
  await nextTick();
  currentTab.value = 'transactions';
  await nextTick();

  workspaceStore.currentWorkspace = { id: 'pers-2', is_personal: true, business_type: 'personal' };
  await nextTick();
  check(currentTab.value === 'transactions', '2.6', 'Personal-to-personal workspace switch preserves currentTab');

  // 2.7 Rapid Oscillation Stress Test (100 rapid cycles)
  let oscillationSuccess = true;
  for (let i = 0; i < 100; i++) {
    workspaceStore.currentWorkspace = { id: 'biz-1', is_personal: false };
    await nextTick();
    currentTab.value = 'inventory';
    await nextTick();

    workspaceStore.currentWorkspace = { id: 'pers-1', is_personal: true };
    await nextTick();

    if (currentTab.value !== 'home') {
      oscillationSuccess = false;
      break;
    }
  }
  check(oscillationSuccess, '2.7', 'Rapid oscillation (100 cycles): Tab deterministically evicts to "home" 100% of the time');

  // 2.8 Verify App.vue source code contains exact watcher implementation
  const appVueSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/App.vue'), 'utf-8');
  check(appVueSource.includes('() => workspaceStore.isPersonalWorkspace'), '2.8a', 'App.vue source contains isPersonalWorkspace watcher');
  check(appVueSource.includes("['inventory', 'purchasing', 'invoices', 'accounting'].includes(currentTab.value)"), '2.8b', 'App.vue watcher checks all 4 business tabs');
  check(appVueSource.includes("handleSelectTab('home')"), '2.8c', 'App.vue watcher redirects to home tab upon eviction');
}

// =============================================================================
// SUITE 3: POS Keypad Trigger Integrity & Modal Non-Conflict Invariants
// =============================================================================
console.log('\n--- SUITE 3: POS Keypad Trigger Integrity & Modal Non-Conflict Invariants ---');
{
  const pinia = createPinia();
  setActivePinia(pinia);
  const workspaceStore = useWorkspaceStore();

  // 3.1 DesktopSidebar Component Inspection & Rendering
  // Verify emits definition in component
  check(Array.isArray(DesktopSidebar.emits) && DesktopSidebar.emits.includes('open-add'), '3.1a', 'DesktopSidebar explicitly declares "open-add" in emits');
  check(DesktopSidebar.emits.includes('select-tab'), '3.1b', 'DesktopSidebar declares "select-tab" in emits');

  // Inspect DesktopSidebar source code for button click binding
  const sidebarSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/components/layout/DesktopSidebar.vue'), 'utf-8');
  check(sidebarSource.includes('@click="$emit(\'open-add\')"'), '3.1c', 'DesktopSidebar button template binds @click to $emit("open-add")');
  check(sidebarSource.includes('variant="primary"'), '3.1d', 'Catat Transaksi button uses primary variant');

  // Render in Personal Workspace
  workspaceStore.currentWorkspace = { id: 'pers-sb', is_personal: true, business_type: 'personal' };
  workspaceStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];
  const htmlPersonalSidebar = await renderComponent(DesktopSidebar, { activeTab: 'home' }, pinia);

  check(htmlPersonalSidebar.includes('Catat Transaksi'), '3.1e', 'DesktopSidebar renders "Catat Transaksi" in personal workspace');
  check(!htmlPersonalSidebar.includes('Inventaris &amp; Stok') && !htmlPersonalSidebar.includes('Inventaris & Stok'), '3.1f', 'DesktopSidebar omits "Inventaris & Stok" in personal workspace');
  check(!htmlPersonalSidebar.includes('Pesanan Pembelian'), '3.1g', 'DesktopSidebar omits "Pesanan Pembelian" in personal workspace');

  // Render in Business Workspace
  workspaceStore.currentWorkspace = { id: 'biz-sb', is_personal: false, business_type: 'retail' };
  workspaceStore.capabilities = ['pos', 'inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'];
  const htmlBusinessSidebar = await renderComponent(DesktopSidebar, { activeTab: 'home' }, pinia);

  check(htmlBusinessSidebar.includes('Catat Transaksi'), '3.1h', 'DesktopSidebar renders "Catat Transaksi" in business workspace');
  check(htmlBusinessSidebar.includes('Inventaris &amp; Stok') || htmlBusinessSidebar.includes('Inventaris & Stok'), '3.1i', 'DesktopSidebar dynamically renders "Inventaris & Stok" in retail workspace');
  check(htmlBusinessSidebar.includes('Pesanan Pembelian'), '3.1j', 'DesktopSidebar dynamically renders "Pesanan Pembelian" in retail workspace');

  // 3.2 MobileBottomNav Component Inspection & Rendering
  check(Array.isArray(MobileBottomNav.emits) && MobileBottomNav.emits.includes('open-add'), '3.2a', 'MobileBottomNav explicitly declares "open-add" in emits');
  check(MobileBottomNav.emits.includes('select-tab'), '3.2b', 'MobileBottomNav declares "select-tab" in emits');

  const bottomNavSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/components/layout/MobileBottomNav.vue'), 'utf-8');
  check(bottomNavSource.includes('@click="$emit(\'open-add\')"'), '3.2c', 'Slot 3 button template binds @click to $emit("open-add")');
  check(bottomNavSource.includes('-mt-5 w-12 h-12 rounded-2xl bg-brand-default text-content-inverse'), '3.2d', 'Slot 3 has elevated floating -mt-5 geometry');
  check(bottomNavSource.includes('aria-label="Catat Transaksi Baru"'), '3.2e', 'Slot 3 has aria-label="Catat Transaksi Baru"');

  // Render MobileBottomNav in Personal Workspace
  workspaceStore.currentWorkspace = { id: 'pers-mob', is_personal: true, business_type: 'personal' };
  workspaceStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];
  const htmlPersonalMob = await renderComponent(MobileBottomNav, { activeTab: 'home' }, pinia);

  check(htmlPersonalMob.includes('Catat Transaksi Baru'), '3.2f', 'Slot 3 POS keypad trigger rendered in personal workspace');
  check(htmlPersonalMob.includes('Analitik'), '3.2g', 'Slot 4 dynamically displays "Analitik" in personal workspace');
  check(!htmlPersonalMob.includes('Stok') && !htmlPersonalMob.includes('Beli'), '3.2h', 'Slot 4 strictly omits "Stok" and "Beli" in personal workspace');

  // Render MobileBottomNav in Retail Workspace
  workspaceStore.currentWorkspace = { id: 'biz-mob', is_personal: false, business_type: 'retail' };
  workspaceStore.capabilities = ['pos', 'inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'];
  const htmlRetailMob = await renderComponent(MobileBottomNav, { activeTab: 'home' }, pinia);

  check(htmlRetailMob.includes('Catat Transaksi Baru'), '3.2i', 'Slot 3 POS keypad trigger rendered in retail workspace');
  check(htmlRetailMob.includes('Stok'), '3.2j', 'Slot 4 dynamically displays "Stok" in retail workspace');

  // Render MobileBottomNav on Purchasing Tab
  const htmlPurchasingMob = await renderComponent(MobileBottomNav, { activeTab: 'purchasing' }, pinia);
  check(htmlPurchasingMob.includes('Beli'), '3.2k', 'Slot 4 dynamically displays "Beli" when activeTab is purchasing');

  // 3.3 Modal Isolation & Zero-Conflict Invariants in App.vue and Views
  const appVueSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/App.vue'), 'utf-8');
  const invSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/views/InventoryView.vue'), 'utf-8');
  const purSource = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/views/PurchasingView.vue'), 'utf-8');

  // In App.vue: showAddModal is the dedicated POS modal ref
  check(appVueSource.includes('const showAddModal = ref(false)'), '3.3a', 'App.vue declares dedicated showAddModal state');
  check(appVueSource.includes('@open-add="showAddModal = true"'), '3.3b', 'App.vue wires @open-add from navigation to showAddModal');
  check(appVueSource.includes('<AddTransactionModal\n      :is-open="showAddModal"'), '3.3c', 'App.vue binds showAddModal to AddTransactionModal');

  // InventoryView modals are isolated local refs
  check(invSource.includes('const showCreateProductModal = ref(false)'), '3.3d', 'InventoryView: showCreateProductModal is locally scoped');
  check(invSource.includes('const showCreateWarehouseModal = ref(false)'), '3.3e', 'InventoryView: showCreateWarehouseModal is locally scoped');
  check(invSource.includes('const showTransferModal = ref(false)'), '3.3f', 'InventoryView: showTransferModal is locally scoped');
  check(invSource.includes('const showAdjustmentModal = ref(false)'), '3.3g', 'InventoryView: showAdjustmentModal is locally scoped');
  check(!invSource.includes('showAddModal'), '3.3h', 'InventoryView does not reference or overwrite showAddModal');

  // PurchasingView modals are isolated local refs
  check(purSource.includes('const showCreatePOModal = ref(false)'), '3.3i', 'PurchasingView: showCreatePOModal is locally scoped');
  check(purSource.includes('const showReceiveModal = ref(false)'), '3.3j', 'PurchasingView: showReceiveModal is locally scoped');
  check(!purSource.includes('showAddModal'), '3.3k', 'PurchasingView does not reference or overwrite showAddModal');

  // Views propagate @open-add up to App.vue without suppression
  check(appVueSource.includes('<InventoryView\n                v-else-if="currentTab === \'inventory\'"\n                key="inventory"\n                @open-add="showAddModal = true"'), '3.3l', 'InventoryView passes @open-add to App.vue');
  check(appVueSource.includes('<PurchasingView\n                v-else-if="currentTab === \'purchasing\'"\n                key="purchasing"\n                @open-add="showAddModal = true"'), '3.3m', 'PurchasingView passes @open-add to App.vue');
}

// =============================================================================
// SUITE 4: Offline Mock Data Resilience (handleMockApiRequest)
// =============================================================================
console.log('\n--- SUITE 4: Offline Mock Data Resilience (handleMockApiRequest) ---');
{
  if (globalThis.localStorage) {
    globalThis.localStorage.clear();
  }

  // 4.1 Warehouses Mock Handlers
  const resWhList = handleMockApiRequest('/api/v1/warehouses', { method: 'GET' });
  check(Array.isArray(resWhList.warehouses), '4.1a', 'GET /api/v1/warehouses returns array');
  check(resWhList.warehouses.length >= 2, '4.1b', 'GET /api/v1/warehouses has at least 2 default warehouses');

  const defaultWh = resWhList.warehouses.find(w => w.is_default);
  check(Boolean(defaultWh), '4.1c', 'Initial warehouses include a designated default warehouse');

  // Create new warehouse
  const newWhData = {
    code: 'WH-TEST-09',
    name: 'Gudang Uji Coba Cikarang',
    address: 'Kawasan Industri Cikarang',
    is_default: true,
  };
  const createdWh = handleMockApiRequest('/api/v1/warehouses', {
    method: 'POST',
    body: JSON.stringify(newWhData)
  });
  check(createdWh.id && createdWh.code === 'WH-TEST-09', '4.1d', 'POST /api/v1/warehouses successfully creates warehouse');
  check(createdWh.is_default === true, '4.1e', 'POST /api/v1/warehouses sets is_default');

  // Verify other warehouses were unset from default
  const resWhListAfter = handleMockApiRequest('/api/v1/warehouses', { method: 'GET' });
  const oldDefaultWh = resWhListAfter.warehouses.find(w => w.id === defaultWh.id);
  check(oldDefaultWh.is_default === false, '4.1f', 'POST /api/v1/warehouses clears previous default warehouse');

  // GET specific warehouse
  const fetchedWh = handleMockApiRequest(`/api/v1/warehouses/${createdWh.id}`, { method: 'GET' });
  check(fetchedWh.name === 'Gudang Uji Coba Cikarang', '4.1g', 'GET /api/v1/warehouses/:id returns matching warehouse');

  // GET invalid warehouse error
  let whErrorThrown = false;
  try {
    handleMockApiRequest('/api/v1/warehouses/non_existent_wh_id', { method: 'GET' });
  } catch (err) {
    whErrorThrown = true;
  }
  check(whErrorThrown, '4.1h', 'GET /api/v1/warehouses/:id throws Error on non-existent warehouse');

  // 4.2 Products Mock Handlers
  const resPrdList = handleMockApiRequest('/api/v1/products', { method: 'GET' });
  check(Array.isArray(resPrdList.products) && resPrdList.products.length >= 3, '4.2a', 'GET /api/v1/products returns default products');

  // Create new product
  const newPrdData = {
    name: 'Teh Hitam Celup Premium',
    unit: 'BOX',
    cost_price: 18000,
    sale_price: 25000,
    reorder_threshold: 12
  };
  const createdPrd = handleMockApiRequest('/api/v1/products', {
    method: 'POST',
    body: JSON.stringify(newPrdData)
  });
  check(createdPrd.id && createdPrd.sku.startsWith('SKU-'), '4.2b', 'POST /api/v1/products generates sequential SKU');
  check(createdPrd.cost_price === 18000, '4.2c', 'POST /api/v1/products stores integer cost_price');

  // Verify automatic stock item seeding for the new product across all existing warehouses
  const resStockForNewPrd = handleMockApiRequest(`/api/v1/inventory/stock?product_id=${createdPrd.id}`, { method: 'GET' });
  check(resStockForNewPrd.stock_items.length === resWhListAfter.warehouses.length, '4.2d', 'POST /api/v1/products automatically seeds stock items across all warehouses');
  check(resStockForNewPrd.stock_items.every(s => s.quantity_on_hand === 0), '4.2e', 'Seeded stock items initialize with quantity_on_hand = 0');

  // 4.3 Inventory Stock & Filter Handlers
  const resAllStock = handleMockApiRequest('/api/v1/inventory', { method: 'GET' });
  check(resAllStock.stock_items.length > 0, '4.3a', 'GET /api/v1/inventory returns stock list');

  const resWhFilter = handleMockApiRequest('/api/v1/inventory?warehouse_id=wh_main_01', { method: 'GET' });
  check(resWhFilter.stock_items.every(s => s.warehouse_id === 'wh_main_01'), '4.3b', 'GET /api/v1/inventory?warehouse_id filters by warehouse');

  const resLowStockFilter = handleMockApiRequest('/api/v1/inventory?low_stock=true', { method: 'GET' });
  check(resLowStockFilter.stock_items.every(s => s.quantity_on_hand <= s.reorder_threshold), '4.3c', 'GET /api/v1/inventory?low_stock=true filters low stock items');

  // 4.4 Stock Movements (INBOUND & OUTBOUND)
  const stockBeforeInbound = handleMockApiRequest(`/api/v1/inventory?warehouse_id=wh_main_01&product_id=prd_01`, { method: 'GET' }).stock_items[0];
  const initialQty = stockBeforeInbound.quantity_on_hand;

  // Inbound movement
  const inboundRes = handleMockApiRequest('/api/v1/inventory/movements', {
    method: 'POST',
    body: JSON.stringify({
      movement_type: 'INBOUND',
      product_id: 'prd_01',
      destination_warehouse_id: 'wh_main_01',
      quantity: 15,
      unit_cost: 120000,
      notes: 'Test Inbound'
    })
  });
  check(inboundRes.resulting_stock === initialQty + 15, '4.4a', 'POST /api/v1/inventory/movements INBOUND increments quantity_on_hand');

  // Outbound movement
  const outboundRes = handleMockApiRequest('/api/v1/inventory/movements', {
    method: 'POST',
    body: JSON.stringify({
      movement_type: 'OUTBOUND',
      product_id: 'prd_01',
      source_warehouse_id: 'wh_main_01',
      quantity: 5,
      notes: 'Test Outbound'
    })
  });
  check(outboundRes.resulting_stock === initialQty + 10, '4.4b', 'POST /api/v1/inventory/movements OUTBOUND decrements quantity_on_hand');

  // 4.5 Inter-Warehouse Stock Transfer
  const srcStockBefore = handleMockApiRequest(`/api/v1/inventory?warehouse_id=wh_main_01&product_id=prd_01`, { method: 'GET' }).stock_items[0].quantity_on_hand;
  const dstStockBefore = handleMockApiRequest(`/api/v1/inventory?warehouse_id=wh_sec_02&product_id=prd_01`, { method: 'GET' }).stock_items[0].quantity_on_hand;

  const transferRes = handleMockApiRequest('/api/v1/inventory/transfer', {
    method: 'POST',
    body: JSON.stringify({
      product_id: 'prd_01',
      source_warehouse_id: 'wh_main_01',
      destination_warehouse_id: 'wh_sec_02',
      quantity: 8,
      notes: 'Transfer to transit'
    })
  });
  check(transferRes.status === 'COMPLETED', '4.5a', 'POST /api/v1/inventory/transfer returns COMPLETED status');
  check(transferRes.source_remaining === srcStockBefore - 8, '4.5b', 'Source warehouse stock decremented by transfer quantity');
  check(transferRes.destination_total === dstStockBefore + 8, '4.5c', 'Destination warehouse stock incremented by transfer quantity');

  // 4.6 Physical Stock Count Adjustment
  const adjRes = handleMockApiRequest('/api/v1/inventory/adjust', {
    method: 'POST',
    body: JSON.stringify({
      warehouse_id: 'wh_main_01',
      product_id: 'prd_02',
      actual_quantity: 20,
      reason: 'Physical cycle count variance'
    })
  });
  check(adjRes.adjustment_number.startsWith('ADJ-2026-'), '4.6a', 'Stock adjustment generates ADJ-2026-XXXXXX number');
  check(adjRes.actual_quantity === 20, '4.6b', 'Stock adjustment sets actual_quantity');
  check(adjRes.variance === 20 - adjRes.previous_quantity, '4.6c', 'Stock adjustment computes variance accurately');

  // 4.7 Purchase Order Lifecycle & Goods Receipt with Moving WAC
  const newPO = handleMockApiRequest('/api/v1/purchase-orders', {
    method: 'POST',
    body: JSON.stringify({
      supplier_name: 'PT Pemasok Kopi Unggul',
      destination_warehouse_id: 'wh_main_01',
      notes: 'Pengadaan Batch 1',
      items: [
        { product_id: 'prd_01', quantity_ordered: 20, unit_cost: 130000 }
      ]
    })
  });
  check(newPO.status === 'DRAFT', '4.7a', 'New purchase order initializes in DRAFT status');
  check(newPO.total_amount === 2600000, '4.7b', 'Purchase order total_amount calculated as 20 * 130000 = 2,600,000 IDR');

  // Transition to ORDERED
  const orderedPO = handleMockApiRequest(`/api/v1/purchase-orders/${newPO.id}/order`, { method: 'POST' });
  check(orderedPO.status === 'ORDERED', '4.7c', 'POST /order transitions status to ORDERED');

  // Receive partial goods: 10 units at 140,000 IDR
  const itemBeforeReceipt = handleMockApiRequest(`/api/v1/inventory?warehouse_id=wh_main_01&product_id=prd_01`, { method: 'GET' }).stock_items[0];
  const prevQtyWAC = itemBeforeReceipt.quantity_on_hand;
  const prevCostWAC = itemBeforeReceipt.average_cost;

  const receivePartial = handleMockApiRequest(`/api/v1/purchase-orders/${newPO.id}/receive`, {
    method: 'POST',
    body: JSON.stringify({
      items: [
        { product_id: 'prd_01', quantity_received: 10, unit_cost: 140000, batch_number: 'BATCH-PO-P1' }
      ]
    })
  });
  check(receivePartial.status === 'PARTIALLY_RECEIVED', '4.7d', 'Partial receipt transitions PO status to PARTIALLY_RECEIVED');

  // Verify WAC math: round((prevQty * prevCost + 10 * 140000) / (prevQty + 10))
  const itemAfterPartial = handleMockApiRequest(`/api/v1/inventory?warehouse_id=wh_main_01&product_id=prd_01`, { method: 'GET' }).stock_items[0];
  const expectedWAC = Math.round((prevQtyWAC * prevCostWAC + 10 * 140000) / (prevQtyWAC + 10));
  check(itemAfterPartial.quantity_on_hand === prevQtyWAC + 10, '4.7e', 'Stock on hand incremented by 10 units');
  check(itemAfterPartial.average_cost === expectedWAC, '4.7f', `Moving WAC updated: expected ${expectedWAC}, got ${itemAfterPartial.average_cost}`);

  // Receive remaining 10 units
  const receiveRemaining = handleMockApiRequest(`/api/v1/purchase-orders/${newPO.id}/receive`, {
    method: 'POST',
    body: JSON.stringify({
      items: [
        { product_id: 'prd_01', quantity_received: 10, unit_cost: 140000, batch_number: 'BATCH-PO-P2' }
      ]
    })
  });
  check(receiveRemaining.status === 'RECEIVED', '4.7g', 'Final receipt transitions PO status to RECEIVED');

  // Cancel workflow on new draft PO
  const poToCancel = handleMockApiRequest('/api/v1/purchase-orders', {
    method: 'POST',
    body: JSON.stringify({
      supplier_name: 'Pemasok Batal',
      destination_warehouse_id: 'wh_main_01',
      items: [{ product_id: 'prd_02', quantity_ordered: 5, unit_cost: 65000 }]
    })
  });
  const cancelRes = handleMockApiRequest(`/api/v1/purchase-orders/${poToCancel.id}/cancel`, { method: 'POST' });
  check(cancelRes.status === 'CANCELLED', '4.7h', 'POST /cancel transitions PO status to CANCELLED');

  // 4.8 Mock Data Resilience on Malformed / Empty Ingestion
  let emptyBodyHandled = false;
  try {
    const res = handleMockApiRequest('/api/v1/purchase-orders', { method: 'POST', body: '{}' });
    if (res && res.id) emptyBodyHandled = true;
  } catch {}
  check(emptyBodyHandled, '4.8a', 'handleMockApiRequest safely handles empty POST body without crashing');

  const fallbackRes = handleMockApiRequest('/api/v1/unknown-route', { method: 'GET' });
  check(fallbackRes.success === true, '4.8b', 'Unknown route returns graceful fallback { success: true }');
}

// =============================================================================
// SUMMARY
// =============================================================================
console.log('\n' + '='.repeat(80));
console.log(`TOTAL TESTS: ${passed + failed}`);
console.log(`PASSED: ${passed}`);
console.log(`FAILED: ${failed}`);
console.log('='.repeat(80));

if (failed > 0) {
  console.error('\nFAILURES SUMMARY:');
  failureList.forEach(f => console.error(`  - ${f}`));
  process.exit(1);
} else {
  console.log('\nALL EMPIRICAL CHALLENGES PASSED WITH 100% SUCCESS!');
  process.exit(0);
}
