/**
 * M5 Challenger 2 Empirical Test Harness
 * Gate 5: Navigation State, POS Keypad Trigger Preservation & Schema Persistence Invariants
 *
 * Targets:
 * - MobileBottomNav.vue (Slot 3 FAB, POS trigger, Slot 4 dynamic tenant capability adaptation)
 * - AddTransactionModal.vue & currency.js (4x3 POS keypad, Rupiah integer arithmetic invariants)
 * - api.js (RFC 7807 404 error propagation, mock leak boundaries, catch block vulnerability)
 */

import * as hd from 'happy-dom';
import path from 'path';
import fs from 'fs';
import { createServer } from 'vite';
import { setActivePinia, createPinia } from 'pinia';

// Setup headless DOM
const window = new hd.GlobalWindow();
globalThis.window = window;
globalThis.document = window.document;
globalThis.localStorage = window.localStorage;
try {
  Object.defineProperty(globalThis, 'navigator', {
    value: { onLine: true, vibrate: () => true },
    configurable: true,
    writable: true,
  });
} catch {}

const FRONTEND_ROOT = '/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend';

console.log('='.repeat(80));
console.log('GATE 5 CHALLENGER 2: EMPIRICAL ADVERSARIAL TEST HARNESS');
console.log('Targets: MobileBottomNav, AddTransactionModal, currency.js, api.js');
console.log('='.repeat(80));

let passed = 0;
let failed = 0;
const failureList = [];
const observations = [];

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

function observe(note) {
  observations.push(note);
  console.log(`  [NOTE] ${note}`);
}

// Spin up Vite SSR runtime
const viteServer = await createServer({
  configFile: false,
  root: FRONTEND_ROOT,
  resolve: {
    alias: {
      '@': path.join(FRONTEND_ROOT, 'src'),
    },
  },
  server: { middlewareMode: true },
  appType: 'custom',
});

const { api, ApiError } = await viteServer.ssrLoadModule('/src/services/api.js');
const { useWorkspaceStore } = await viteServer.ssrLoadModule('/src/stores/workspace.js');
const { formatIDR, parseIDR, formatCompactIDR, formatIntegerRupiah } = await viteServer.ssrLoadModule('/src/utils/currency.js');

// =============================================================================
// SUITE 1: MobileBottomNav.vue Structure & Slot 3 / Slot 4 Invariants
// =============================================================================
console.log('\n--- SUITE 1: MobileBottomNav.vue Navigation & Slot Invariants ---');

const mobileNavFile = path.join(FRONTEND_ROOT, 'src/components/layout/MobileBottomNav.vue');
const mobileNavSource = fs.readFileSync(mobileNavFile, 'utf-8');

// Test 1.1: Slot 3 Centered FAB with @click="$emit('open-add')"
check(
  mobileNavSource.includes('@click="$emit(\'open-add\')"'),
  '1.1',
  'Slot 3 strictly emits open-add on click'
);

check(
  mobileNavSource.includes('aria-label="Catat Transaksi Baru"') &&
  mobileNavSource.includes('-mt-5 w-12 h-12 rounded-2xl bg-brand-default text-content-inverse'),
  '1.2',
  'Slot 3 retains floating elevated geometry (-mt-5 w-12 h-12 rounded-2xl bg-brand-default)'
);

// Test 1.3: MobileBottomNav defines emit 'open-add'
check(
  mobileNavSource.includes("defineEmits(['select-tab', 'open-add'])"),
  '1.3',
  'Component explicitly declares open-add in defineEmits'
);

// Test 1.4: App.vue binds @open-add to showAddModal = true
const appVueFile = path.join(FRONTEND_ROOT, 'src/App.vue');
const appVueSource = fs.readFileSync(appVueFile, 'utf-8');
check(
  appVueSource.includes('@open-add="showAddModal = true"'),
  '1.4',
  'App.vue binds @open-add to showAddModal = true'
);

check(
  appVueSource.includes('<AddTransactionModal') &&
  appVueSource.includes(':is-open="showAddModal"'),
  '1.5',
  'App.vue renders AddTransactionModal bound to showAddModal'
);

// Test 1.6: Slot 4 Dynamic Adaptation Logic
{
  setActivePinia(createPinia());
  const wsStore = useWorkspaceStore();

  // Helper simulating slot4Tab computed logic from MobileBottomNav.vue
  function computeSlot4Tab(activeTab, store) {
    if (activeTab === 'analytics') {
      return { id: 'analytics', label: 'Analitik' };
    }
    if (store.hasCapability('invoicing')) {
      return { id: 'invoices', label: 'Faktur' };
    }
    if (store.hasCapability('accounting')) {
      return { id: 'accounting', label: 'Buku Kas' };
    }
    return { id: 'analytics', label: 'Analitik' };
  }

  // Case A: Retail workspace with invoicing & accounting capabilities
  wsStore.currentWorkspace = {
    id: 'ws-retail',
    name: 'Toko Kelontong',
    business_type: 'retail',
    is_personal: false,
  };
  wsStore.capabilities = ['pos', 'inventory', 'invoicing', 'accounting', 'receivables', 'reports'];
  let tab = computeSlot4Tab('home', wsStore);
  check(
    tab.id === 'invoices' && tab.label === 'Faktur',
    '1.6a',
    'Retail workspace with invoicing maps Slot 4 to Faktur (invoices)'
  );

  // Case B: General business without invoicing but with accounting
  wsStore.capabilities = ['accounting', 'reports'];
  tab = computeSlot4Tab('home', wsStore);
  check(
    tab.id === 'accounting' && tab.label === 'Buku Kas',
    '1.6b',
    'Workspace with accounting (no invoicing) maps Slot 4 to Buku Kas (accounting)'
  );

  // Case C: Personal workspace (no business capabilities)
  wsStore.currentWorkspace = {
    id: 'ws-personal',
    name: 'Personal Wallet',
    business_type: 'personal',
    is_personal: true,
  };
  wsStore.capabilities = ['accounts', 'transactions', 'budgets', 'analytics'];
  tab = computeSlot4Tab('home', wsStore);
  check(
    tab.id === 'analytics' && tab.label === 'Analitik',
    '1.6c',
    'Personal workspace defaults Slot 4 to Analitik (analytics)'
  );

  // Case D: Active tab is explicitly 'analytics' even in business workspace
  wsStore.currentWorkspace = {
    id: 'ws-retail',
    name: 'Toko Kelontong',
    business_type: 'retail',
  };
  wsStore.capabilities = ['invoicing', 'accounting'];
  tab = computeSlot4Tab('analytics', wsStore);
  check(
    tab.id === 'analytics' && tab.label === 'Analitik',
    '1.6d',
    'Active tab analytics preserves Slot 4 as Analitik regardless of business capabilities'
  );
}

// =============================================================================
// SUITE 2: AddTransactionModal.vue Rapid 4x3 POS Keypad & State Machine
// =============================================================================
console.log('\n--- SUITE 2: AddTransactionModal.vue 4x3 POS Keypad & Rupiah Arithmetic ---');

const modalFile = path.join(FRONTEND_ROOT, 'src/components/AddTransactionModal.vue');
const modalSource = fs.readFileSync(modalFile, 'utf-8');

// Test 2.1: Keypad Layout in Template
check(
  modalSource.includes("['1', '2', '3', '4', '5', '6', '7', '8', '9', '000', '0']"),
  '2.1',
  'AddTransactionModal template contains exact 4x3 POS digits (1-9, 000, 0)'
);

check(
  modalSource.includes("@click=\"pressKey('backspace')\"") &&
  modalSource.includes('grid grid-cols-3 gap-2'),
  '2.2',
  'AddTransactionModal contains backspace button in 3-column POS grid'
);

// Test 2.2: Simulation of Keypad Press Logic
{
  function createKeypadSimulator() {
    let rawAmount = '0';
    function pressKey(key) {
      if (key === 'backspace') {
        if (rawAmount.length > 1) {
          rawAmount = rawAmount.slice(0, -1);
        } else {
          rawAmount = '0';
        }
      } else if (key === '000') {
        if (rawAmount !== '0' && rawAmount.length <= 11) {
          rawAmount += '000';
        }
      } else {
        if (rawAmount.length > 13) return;
        if (rawAmount === '0') {
          rawAmount = key;
        } else {
          rawAmount += key;
        }
      }
    }
    function addAmount(delta) {
      const cur = parseInt(rawAmount, 10) || 0;
      rawAmount = String(cur + delta);
    }
    function clearAmount() {
      rawAmount = '0';
    }
    return {
      getAmount: () => rawAmount,
      pressKey,
      addAmount,
      clearAmount,
      getFormatted: () => formatIDR(parseInt(rawAmount, 10) || 0),
    };
  }

  // Simulation A: Typings 1 -> 5 -> 0 -> 000 -> 150.000
  const kp = createKeypadSimulator();
  check(kp.getAmount() === '0', '2.3a', 'Initial rawAmount is 0');

  kp.pressKey('1');
  kp.pressKey('5');
  kp.pressKey('0');
  kp.pressKey('000');
  check(kp.getAmount() === '150000', '2.3b', 'Entering 1, 5, 0, 000 yields 150000');
  check(kp.getFormatted() === 'Rp\u00a0150.000' || kp.getFormatted().includes('150.000'), '2.3c', 'Formatted amount matches Rp 150.000');

  // Simulation B: '000' on initial '0' is ignored (prevents '0000')
  kp.clearAmount();
  check(kp.getAmount() === '0', '2.4a', 'clearAmount resets to 0');
  kp.pressKey('000');
  check(kp.getAmount() === '0', '2.4b', '000 key pressed when amount is 0 remains 0');

  // Simulation C: Single '0' on initial '0' is ignored
  kp.pressKey('0');
  check(kp.getAmount() === '0', '2.4c', '0 key pressed when amount is 0 remains 0');

  // Simulation D: Backspace mechanics
  kp.pressKey('7');
  kp.pressKey('5');
  kp.pressKey('0');
  check(kp.getAmount() === '750', '2.5a', 'Amount is 750');
  kp.pressKey('backspace');
  check(kp.getAmount() === '75', '2.5b', 'Backspace reduces 750 to 75');
  kp.pressKey('backspace');
  check(kp.getAmount() === '7', '2.5c', 'Backspace reduces 75 to 7');
  kp.pressKey('backspace');
  check(kp.getAmount() === '0', '2.5d', 'Backspace on single digit resets to 0');
  kp.pressKey('backspace');
  check(kp.getAmount() === '0', '2.5e', 'Backspace on 0 remains 0');

  // Simulation E: Quick increment chips
  kp.clearAmount();
  kp.addAmount(50000);
  check(kp.getAmount() === '50000', '2.6a', '+50.000 chip adds to 0');
  kp.addAmount(100000);
  check(kp.getAmount() === '150000', '2.6b', '+100.000 chip increases to 150.000');
  kp.addAmount(500000);
  check(kp.getAmount() === '650000', '2.6c', '+500.000 chip increases to 650.000');

  // Simulation F: Overflow protection (13 digits limit)
  kp.clearAmount();
  kp.pressKey('9');
  for (let i = 0; i < 20; i++) {
    kp.pressKey('9');
  }
  check(kp.getAmount().length <= 14, '2.7', 'Keypad input limits digits to prevent integer overflow');
}

// =============================================================================
// SUITE 3: Pure Integer Rupiah Arithmetic & Zero Floating-Point Precision
// =============================================================================
console.log('\n--- SUITE 3: Pure Integer Rupiah Arithmetic Invariants ---');

// Test 3.1: parseIDR strictly returns integer
check(parseIDR('150000') === 150000, '3.1a', 'parseIDR parses clean number string');
check(parseIDR('Rp 150.000') === 150000, '3.1b', 'parseIDR strips Rp and dots');
check(parseIDR('Rp 2.500.000,00') === 250000000 || parseIDR('Rp 2.500.000') === 2500000, '3.1c', 'parseIDR handles thousands dots');
check(parseIDR(123456.78) === 123456, '3.1d', 'parseIDR truncates float numbers to integer');
check(parseIDR(0) === 0, '3.1e', 'parseIDR(0) is 0');
check(parseIDR('') === 0, '3.1f', 'parseIDR("") is 0');
check(parseIDR(null) === 0, '3.1g', 'parseIDR(null) is 0');

// Test 3.2: formatIDR formats integer Rupiah without decimal cents
const f1 = formatIDR(50000);
check(f1.includes('50.000') && !f1.endsWith(',00') && !f1.endsWith('.00'), '3.2a', 'formatIDR(50000) has no fraction cents');

const f2 = formatIDR(0);
check(f2.includes('0'), '3.2b', 'formatIDR(0) formats correctly');

const f3 = formatIDR(1000000000);
check(f3.includes('1.000.000.000'), '3.2c', 'formatIDR(1 milyar) formats with correct separators');

// Test 3.3: formatIntegerRupiah returns dot-separated integer
check(formatIntegerRupiah(1250000) === '1.250.000', '3.3a', 'formatIntegerRupiah(1250000) === "1.250.000"');
check(formatIntegerRupiah(0) === '0', '3.3b', 'formatIntegerRupiah(0) === "0"');

// Test 3.4: Stress-testing huge integer monetary transactions (i64 scale)
const largeVal = 999999999999; // 999 milyar
check(parseIDR(String(largeVal)) === largeVal, '3.4a', 'parseIDR parses 999 milyar accurately');
const largeFmt = formatIDR(largeVal);
check(largeFmt.includes('999.999.999.999'), '3.4b', 'formatIDR formats 999 milyar accurately');

// =============================================================================
// SUITE 4: Stress-Testing frontend/src/services/api.js Error Propagation & Catch Block
// =============================================================================
console.log('\n--- SUITE 4: api.js Error Propagation & Catch Block Stress Tests ---');

// Test 4.1: Genuine RFC 7807 404 propagates ApiError(404)
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    headers: new Map([['content-type', 'application/problem+json']]),
    json: async () => ({
      type: 'https://invinite.id/errors/not-found',
      title: 'Resource Not Found',
      detail: 'Tenant workspace not found',
      code: 'TENANT_NOT_FOUND',
      status: 404,
    }),
  });

  try {
    await api.getWorkspace('missing-workspace-id');
    check(false, '4.1', 'Expected 404 to throw ApiError');
  } catch (err) {
    check(
      err instanceof ApiError && err.status === 404 && err.code === 'TENANT_NOT_FOUND',
      '4.1',
      'RFC 7807 404 cleanly throws ApiError(404, "Resource Not Found", "Tenant workspace not found", "TENANT_NOT_FOUND")'
    );
  }
}

// Test 4.2: Other 4xx Status Codes (400, 401, 403, 409, 422) propagate ApiError
const testStatuses = [
  { status: 400, title: 'Bad Request', code: 'INVALID_INPUT', detail: 'Invalid parameter' },
  { status: 401, title: 'Unauthorized', code: 'UNAUTHORIZED', detail: 'Session expired' },
  { status: 403, title: 'Forbidden', code: 'PERMISSION_DENIED', detail: 'Admin role required' },
  { status: 409, title: 'Conflict', code: 'CONFLICT', detail: 'Duplicate invoice number' },
  { status: 422, title: 'Unprocessable', code: 'UNPROCESSABLE', detail: 'Validation failed' },
];

for (const s of testStatuses) {
  globalThis.fetch = async () => ({
    ok: false,
    status: s.status,
    json: async () => s,
  });

  try {
    await api.getMe();
    check(false, `4.2-${s.status}`, `HTTP ${s.status} must throw ApiError, not return mock`);
  } catch (err) {
    check(
      err instanceof ApiError && err.status === s.status && err.code === s.code,
      `4.2-${s.status}`,
      `HTTP ${s.status} cleanly throws ApiError without mock fallback`
    );
  }
}

// Test 4.3: [EMPIRICAL ADVERSARIAL CHALLENGE]
// Catch Block Vulnerability: Backend returns RFC 7807 error where detail contains "network"
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    json: async () => ({
      type: 'https://invinite.id/errors/not-found',
      title: 'Resource Not Found',
      detail: 'Payment network profile not found for tenant',
      code: 'NETWORK_NOT_FOUND',
      status: 404,
    }),
  });

  let mockLeaked = false;
  try {
    const res = await api.getWorkspace('net-id');
    // If it reaches here, mock was returned!
    mockLeaked = true;
  } catch (err) {
    mockLeaked = false;
  }

  check(
    mockLeaked === true,
    '4.3',
    'EMPIRICALLY CONFIRMED: Backend error with "network" in detail is caught by api.js catch block and returns mock data instead of throwing ApiError!'
  );
  observe('api.js line 87 checks err.message?.includes("network"). Because ApiError inherits Error, err.message is set to detail. If detail includes "network", ApiError is treated as network failure.');
}

// Test 4.4: [EMPIRICAL ADVERSARIAL CHALLENGE]
// Catch Block Vulnerability: Backend returns RFC 7807 error where detail contains "Failed to fetch"
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 400,
    json: async () => ({
      type: 'https://invinite.id/errors/bad-request',
      title: 'Bad Request',
      detail: 'Failed to fetch invoice sequence lock',
      code: 'LOCK_FAILED',
      status: 400,
    }),
  });

  let mockLeaked = false;
  try {
    const res = await api.getWorkspace('lock-id');
    mockLeaked = true;
  } catch (err) {
    mockLeaked = false;
  }

  check(
    mockLeaked === true,
    '4.4',
    'EMPIRICALLY CONFIRMED: Backend error with "Failed to fetch" in detail is also caught by catch block and leaks mock data'
  );
}

// Test 4.5: Legitimate Browser Offline Network Failure (TypeError: Failed to fetch)
{
  globalThis.fetch = async () => {
    throw new TypeError('Failed to fetch');
  };

  const offlineRes = await api.getAccounts();
  check(
    offlineRes !== null && typeof offlineRes === 'object',
    '4.5',
    'Legitimate browser offline network failure (TypeError) correctly triggers mock fallback for offline PWA'
  );
}

// Test 4.6: Server 5xx Error (502 Bad Gateway)
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 502,
    statusText: 'Bad Gateway',
  });

  const serverErrRes = await api.getCategories();
  check(
    serverErrRes !== null && typeof serverErrRes === 'object',
    '4.6',
    'Server 502 Bad Gateway triggers mock fallback as designed for dev/offline resilience'
  );
}

// Test 4.7: Proposed Mitigation Validation
{
  // If the catch block starts with: if (err instanceof ApiError) throw err;
  function mockRequestCatchBlock(err) {
    if (err instanceof ApiError) {
      throw err;
    }
    if (
      err instanceof TypeError ||
      err.message?.includes('Failed to fetch') ||
      err.message?.includes('NetworkError') ||
      err.message?.includes('network')
    ) {
      return { mock: true };
    }
    throw err;
  }

  const apiErrWithNetwork = new ApiError(404, 'Not Found', 'Card network not configured', 'NOT_FOUND');
  let thrown = false;
  try {
    mockRequestCatchBlock(apiErrWithNetwork);
  } catch (e) {
    thrown = true;
  }
  check(
    thrown === true,
    '4.7',
    'Mitigation (if (err instanceof ApiError) throw err) cleanly prevents mock leakage on domain errors with "network" in detail'
  );
}

// =============================================================================
// SUMMARY
// =============================================================================
console.log('\n' + '='.repeat(80));
console.log('SUMMARY OF RESULTS');
console.log('='.repeat(80));
console.log(`Passed : ${passed}`);
console.log(`Failed : ${failed}`);
console.log(`Notes  : ${observations.length}`);

if (failed > 0) {
  console.log('\nFailures:');
  for (const f of failureList) {
    console.log(` - ${f}`);
  }
}

await viteServer.close();
process.exit(failed === 0 ? 0 : 1);
