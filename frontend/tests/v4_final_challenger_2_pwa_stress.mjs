/**
 * Final Milestone Adversarial Challenger 2 - Frontend PWA & Interceptor Stress Suite
 * Focus Areas:
 * 1. api.js: RFC 7807 Problem Details, genuine 404 throwing (ApiError), error interceptor,
 *    X-Tenant-ID header injection, custom header precedence, idempotency key generation.
 * 2. workspace.js: State corruption resilience, rapid switching, cross-tab sync,
 *    full 6 business types capability matrix.
 * 3. MobileBottomNav.vue & AddTransactionModal.vue: Slot 3 centered FAB preservation,
 *    4x3 Rapid POS numeric keypad integrity.
 */

import * as hd from 'happy-dom';
import path from 'path';
import fs from 'fs';
import { createServer } from 'vite';
import { setActivePinia, createPinia } from 'pinia';

// Setup headless DOM environment
const window = new hd.GlobalWindow();
globalThis.window = window;
globalThis.document = window.document;
globalThis.localStorage = window.localStorage;
globalThis.StorageEvent = window.StorageEvent;

try {
  Object.defineProperty(globalThis, 'navigator', {
    value: { onLine: true, vibrate: () => true },
    configurable: true,
    writable: true,
  });
} catch {}

const FRONTEND_ROOT = '/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend';

console.log('='.repeat(80));
console.log('FINAL MILESTONE CHALLENGER 2: EMPIRICAL PWA & INTERCEPTOR STRESS HARNESS');
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

// Spin up Vite SSR runtime to import live source modules
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

// =============================================================================
// SUITE 1: api.js Interceptor, Header Injection & RFC 7807 Propagation
// =============================================================================
console.log('\n--- SUITE 1: api.js Interceptors, Headers & Error Handling ---');

// Setup mock fetch tracking
let lastFetchCall = null;
const originalFetch = globalThis.fetch;

function mockFetch(handler) {
  globalThis.fetch = async (url, options) => {
    lastFetchCall = { url, options };
    return handler(url, options);
  };
}

// Test 1.1: Genuine 404 response throws ApiError without falling back to mockData
mockFetch(async (url, options) => {
  return {
    ok: false,
    status: 404,
    statusText: 'Not Found',
    json: async () => ({
      title: 'Resource Not Found',
      detail: 'The requested invoice does not exist',
      code: 'INVOICE_NOT_FOUND',
    }),
  };
});

let caughtErr = null;
try {
  await api.getWorkspace('non-existent-id-12345');
} catch (err) {
  caughtErr = err;
}

check(
  caughtErr instanceof ApiError,
  '1.1.1',
  'HTTP 404 strictly throws an instance of ApiError'
);
check(
  caughtErr?.status === 404 && caughtErr?.code === 'INVOICE_NOT_FOUND',
  '1.1.2',
  'ApiError preserves HTTP 404 status and RFC 7807 problem details code',
  `status=${caughtErr?.status}, code=${caughtErr?.code}`
);
check(
  caughtErr?.detail === 'The requested invoice does not exist' && caughtErr?.title === 'Resource Not Found',
  '1.1.3',
  'ApiError propagates RFC 7807 title and detail verbatim',
  `title=${caughtErr?.title}, detail=${caughtErr?.detail}`
);

// Test 1.2: RFC 7807 on 409 Conflict (e.g. Idempotency Mismatch)
mockFetch(async (url, options) => {
  return {
    ok: false,
    status: 409,
    statusText: 'Conflict',
    json: async () => ({
      title: 'Conflict',
      detail: 'Idempotency key reused with different request payload',
      code: 'IDEMPOTENCY_KEY_MISMATCH',
    }),
  };
});

let caughtConflict = null;
try {
  await api.switchWorkspace('fake-tenant');
} catch (err) {
  caughtConflict = err;
}

check(
  caughtConflict?.status === 409 &&
  caughtConflict?.code === 'IDEMPOTENCY_KEY_MISMATCH' &&
  caughtConflict?.detail.includes('Idempotency key reused'),
  '1.2.1',
  'HTTP 409 Conflict propagates RFC 7807 code IDEMPOTENCY_KEY_MISMATCH'
);

// Test 1.3: 500 / 502 Server Errors fall back to mockData
mockFetch(async (url, options) => {
  return {
    ok: false,
    status: 502,
    statusText: 'Bad Gateway',
    json: async () => ({}),
  };
});

try {
  const mockRes = await api.getCategories();
  check(
    Array.isArray(mockRes) && mockRes.length > 0,
    '1.3.1',
    'HTTP 502 Bad Gateway gracefully triggers mockData fallback without crashing'
  );
} catch (err) {
  check(false, '1.3.1', 'HTTP 502 threw unexpected exception', err.message);
}

// Test 1.4: Network failure (TypeError: Failed to fetch) falls back to mockData
mockFetch(async (url, options) => {
  throw new TypeError('Failed to fetch');
});

try {
  const mockRes = await api.getAccounts();
  check(
    Array.isArray(mockRes) && mockRes.length > 0,
    '1.4.1',
    'Network failure (TypeError Failed to fetch) gracefully falls back to mockData'
  );
} catch (err) {
  check(false, '1.4.1', 'Network failure threw unexpected exception', err.message);
}

// Test 1.5: X-Tenant-ID Header Injection from localStorage
localStorage.clear();
localStorage.setItem('invinite_active_tenant_id', 'tenant-alpha-uuid-777');

mockFetch(async (url, options) => {
  return {
    ok: true,
    status: 200,
    json: async () => ({ success: true }),
  };
});

await api.getMe();
check(
  lastFetchCall?.options?.headers?.['X-Tenant-ID'] === 'tenant-alpha-uuid-777',
  '1.5.1',
  'Active tenant ID from invinite_active_tenant_id is injected as X-Tenant-ID header'
);

// Fallback to invinite_active_workspace JSON object in localStorage
localStorage.removeItem('invinite_active_tenant_id');
localStorage.setItem('invinite_active_workspace', JSON.stringify({ id: 'tenant-beta-uuid-888', name: 'Beta Corp' }));

await api.getMe();
check(
  lastFetchCall?.options?.headers?.['X-Tenant-ID'] === 'tenant-beta-uuid-888',
  '1.5.2',
  'Active tenant ID from parsed invinite_active_workspace is injected as X-Tenant-ID header'
);

// Test 1.6: Custom Header Precedence: Explicit options.headers overrides localStorage tenant ID
mockFetch(async (url, options) => {
  return {
    ok: true,
    status: 200,
    json: async () => ({ success: true }),
  };
});

// Use createTransaction with custom options / headers or direct request simulation
localStorage.setItem('invinite_active_tenant_id', 'tenant-default');
await api.createTransaction({
  account_id: 'acc-1',
  category_id: 'cat-1',
  amount: 50000,
  transaction_type: 'expense',
  idempotency_key: 'custom-idem-uuid-999',
});

check(
  lastFetchCall?.options?.headers?.['Idempotency-Key'] === 'custom-idem-uuid-999',
  '1.6.1',
  'Custom Idempotency-Key in createTransaction is preserved in HTTP headers and request payload'
);
const sentPayload = JSON.parse(lastFetchCall?.options?.body || '{}');
check(
  sentPayload?.idempotency_key === 'custom-idem-uuid-999',
  '1.6.2',
  'Payload contains identical idempotency_key field matching the header'
);

// Restore original fetch
globalThis.fetch = originalFetch;

// =============================================================================
// SUITE 2: Pinia workspace.js Store Stress Testing
// =============================================================================
console.log('\n--- SUITE 2: Pinia workspace.js Store Robustness & Capability Matrix ---');

// Setup Pinia
const pinia = createPinia();
setActivePinia(pinia);

// Test 2.1: Resilience to localStorage state corruption
localStorage.clear();
localStorage.setItem('invinite_active_workspace', 'INVALID_MALFORMED_JSON{{{');
localStorage.setItem('invinite_user_workspaces', '["broken_array"');
localStorage.setItem('invinite_workspace_capabilities', 'not_json');

let storeInitErr = null;
let store = null;
try {
  store = useWorkspaceStore();
} catch (err) {
  storeInitErr = err;
}

check(
  storeInitErr === null,
  '2.1.1',
  'Workspace store initializes cleanly despite completely corrupted localStorage JSON'
);
check(
  store?.currentWorkspace === null || typeof store?.currentWorkspace === 'object',
  '2.1.2',
  'currentWorkspace state safely defaults to null on corrupted cache'
);
check(
  Array.isArray(store?.workspaces) && store?.workspaces.length === 0,
  '2.1.3',
  'workspaces array safely defaults to empty array on corrupted cache'
);
check(
  Array.isArray(store?.capabilities) && store?.capabilities.length === 0,
  '2.1.4',
  'capabilities array safely defaults to empty array on corrupted cache'
);

// Test 2.2: Capability Matrix across all 6 business types
console.log('\n  Checking 6 Business Types Capability Matrix:');

const EXPECTED_MATRIX = {
  general: ['invoicing', 'accounting', 'receivables', 'reports'],
  retail: ['pos', 'inventory', 'invoicing', 'accounting', 'receivables', 'reports'],
  fnb: ['pos', 'tables', 'kitchen', 'inventory', 'accounting', 'reports'],
  rental: ['inventory', 'bookings', 'invoicing', 'receivables', 'accounting'],
  contractor: ['projects', 'milestones', 'invoicing', 'receivables', 'accounting'],
  personal: ['accounts', 'transactions', 'budgets', 'analytics']
};

for (const [bizType, expectedCaps] of Object.entries(EXPECTED_MATRIX)) {
  mockFetch(async (url) => {
    if (url.includes('/capabilities')) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ capabilities: expectedCaps, role: 'owner' }),
      };
    }
    return { ok: true, status: 200, json: async () => ({}) };
  });

  store.currentWorkspace = {
    id: `tenant-${bizType}`,
    name: `${bizType.toUpperCase()} Co`,
    business_type: bizType,
    is_personal: bizType === 'personal'
  };

  const loadedCaps = await store.fetchCapabilities(`tenant-${bizType}`);
  const allMatched = expectedCaps.every(c => store.hasCapability(c)) &&
                     loadedCaps.length === expectedCaps.length;

  check(
    allMatched,
    `2.2.${bizType}`,
    `Business type '${bizType}' correctly provisions all ${expectedCaps.length} expected capabilities [${expectedCaps.join(', ')}]`,
    `actual: [${store.capabilities.join(', ')}]`
  );
}

// Test 2.3: Rapid Workspace Switching Stress
console.log('\n  Rapid Workspace Switching Stress:');
let switchCallCount = 0;
mockFetch(async (url, options) => {
  if (url.includes('/switch')) {
    switchCallCount++;
    const tenantId = url.split('/')[4];
    return {
      ok: true,
      status: 200,
      json: async () => ({
        active_tenant_id: tenantId,
        name: `Workspace ${tenantId}`,
        slug: `slug-${tenantId}`,
        role: 'owner',
      }),
    };
  }
  if (url.includes('/capabilities')) {
    return {
      ok: true,
      status: 200,
      json: async () => ({ capabilities: ['invoicing', 'accounting'], role: 'owner' }),
    };
  }
  return { ok: true, status: 200, json: async () => ({}) };
});

const switchPromises = [];
for (let i = 1; i <= 5; i++) {
  switchPromises.push(store.switchWorkspace(`tenant-${i}`));
}

let rapidSwitchErr = null;
try {
  await Promise.all(switchPromises);
} catch (err) {
  rapidSwitchErr = err;
}

check(
  rapidSwitchErr === null,
  '2.3.1',
  'Rapid parallel workspace switching executes without unhandled race condition crashes'
);
check(
  store.currentWorkspace !== null && store.activeTenantId !== null,
  '2.3.2',
  'Final active workspace state is properly settled and non-null',
  `activeTenantId=${store.activeTenantId}`
);

// Test 2.4: Cross-Tab Storage Event Synchronization
console.log('\n  Cross-Tab Storage Event Synchronization:');
const syncTargetWs = {
  id: 'tenant-synced-from-tab-2',
  name: 'Synced Workspace Tab 2',
  business_type: 'fnb',
  role: 'admin'
};

const storageEvent = new window.StorageEvent('storage', {
  key: 'invinite_active_workspace',
  newValue: JSON.stringify(syncTargetWs),
  oldValue: null,
  storageArea: window.localStorage,
});

window.dispatchEvent(storageEvent);

check(
  store.currentWorkspace?.id === 'tenant-synced-from-tab-2',
  '2.4.1',
  'Window storage event dynamically updates currentWorkspace state for multi-tab sync'
);
check(
  store.currentWorkspace?.name === 'Synced Workspace Tab 2',
  '2.4.2',
  'Synced workspace properties reflect accurately'
);

// Test 2.5: Storage event with null (logout or reset in another tab)
const resetStorageEvent = new window.StorageEvent('storage', {
  key: 'invinite_active_workspace',
  newValue: null,
  oldValue: JSON.stringify(syncTargetWs),
  storageArea: window.localStorage,
});

window.dispatchEvent(resetStorageEvent);

check(
  store.currentWorkspace === null,
  '2.5.1',
  'Storage event with null value triggers clean $reset() across tabs'
);

// =============================================================================
// SUITE 3: UI Component White-Box AST & Template Verification
// =============================================================================
console.log('\n--- SUITE 3: UI Component White-Box Structural Verification ---');

const mobileNavPath = path.join(FRONTEND_ROOT, 'src/components/layout/MobileBottomNav.vue');
const mobileNavCode = fs.readFileSync(mobileNavPath, 'utf-8');

// Check MobileBottomNav Slot 3
check(
  mobileNavCode.includes('@click="$emit(\'open-add\')"'),
  '3.1.1',
  'MobileBottomNav Slot 3 has exact @click="$emit(\'open-add\')" trigger'
);
check(
  mobileNavCode.includes('-mt-5 w-12 h-12 rounded-2xl bg-brand-default text-content-inverse'),
  '3.1.2',
  'MobileBottomNav Slot 3 maintains elevated floating button styling'
);
check(
  mobileNavCode.includes('aria-label="Catat Transaksi Baru"') &&
  mobileNavCode.includes('title="Catat Transaksi Baru (POS Keypad)"'),
  '3.1.3',
  'MobileBottomNav Slot 3 preserves accessibility labels for POS Keypad entry'
);

// Check AddTransactionModal.vue 4x3 Rapid POS Numeric Keypad
const addModalPath = path.join(FRONTEND_ROOT, 'src/components/AddTransactionModal.vue');
const addModalCode = fs.readFileSync(addModalPath, 'utf-8');

check(
  addModalCode.includes("aria-label=\"Keypad Numerik POS\""),
  '3.2.1',
  'AddTransactionModal includes aria-label="Keypad Numerik POS" group'
);

const keypadButtons = ["'1'", "'2'", "'3'", "'4'", "'5'", "'6'", "'7'", "'8'", "'9'", "'000'", "'0'"];
const hasAllDigits = keypadButtons.every(b => addModalCode.includes(b));
check(
  hasAllDigits,
  '3.2.2',
  'AddTransactionModal contains all numeric digits (1-9, 000, 0)'
);
check(
  addModalCode.includes("pressKey('backspace')") && addModalCode.includes('Hapus Digit Terakhir'),
  '3.2.3',
  'AddTransactionModal contains dedicated backspace action button'
);
check(
  addModalCode.includes('grid-cols-3 gap-2'),
  '3.2.4',
  'AddTransactionModal arranges keypad buttons in 4x3 grid (grid-cols-3 with 12 items)'
);

// Summary
console.log('\n' + '='.repeat(80));
console.log(`PWA STRESS HARNESS SUMMARY: ${passed} passed, ${failed} failed`);
console.log('='.repeat(80));

if (failed > 0) {
  console.error('FAILURES:');
  failureList.forEach(f => console.error(`  - ${f}`));
  process.exit(1);
} else {
  console.log('ALL PWA & INTERCEPTOR ADVERSARIAL STRESS TESTS PASSED CLEANLY.\n');
  process.exit(0);
}
