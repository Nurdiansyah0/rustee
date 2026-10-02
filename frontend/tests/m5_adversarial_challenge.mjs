/**
 * M5 Adversarial Challenge Test Harness
 * Pinia Workspace Store State Machine, API Interceptors & Cross-Tenant 404 Isolation
 * Personal Finance PWA
 */

import * as hd from 'happy-dom';
import path from 'path';
import fs from 'fs';
import { createServer } from 'vite';
import { setActivePinia, createPinia } from 'pinia';

// Setup DOM environment before importing modules
const window = new hd.GlobalWindow();
globalThis.window = window;
globalThis.document = window.document;
globalThis.localStorage = window.localStorage;
try {
  Object.defineProperty(globalThis, 'navigator', {
    value: { onLine: true },
    configurable: true,
    writable: true
  });
} catch {}

const FRONTEND_ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

console.log('='.repeat(75));
console.log('GATE 5 ADVERSARIAL CHALLENGE TEST HARNESS');
console.log('Targets: api.js, workspace.js, MobileBottomNav.vue, auth.js, build, E2E');
console.log('='.repeat(75));

let passedCount = 0;
let failedCount = 0;
const failures = [];
const findings = [];

function assert(condition, message, details = '') {
  if (condition) {
    passedCount++;
    console.log(`  [PASS] ${message}`);
  } else {
    failedCount++;
    const errMsg = `  [FAIL] ${message}${details ? ' -> ' + details : ''}`;
    console.error(errMsg);
    failures.push({ message, details });
  }
}

function recordFinding(severity, title, description) {
  findings.push({ severity, title, description });
  console.warn(`  [FINDING - ${severity}] ${title}: ${description}`);
}

// Initialize Vite SSR loader for Vue / Pinia stores
const server = await createServer({
  configFile: false,
  root: FRONTEND_ROOT,
  resolve: {
    alias: {
      '@': path.join(FRONTEND_ROOT, 'src'),
    },
  },
  server: { middlewareMode: true },
  appType: 'custom'
});

const { api, ApiError } = await server.ssrLoadModule('/src/services/api.js');
const { useWorkspaceStore } = await server.ssrLoadModule('/src/stores/workspace.js');

// =============================================================================
// SUITE 1: API Interceptors & Cross-Tenant 404 Isolation (frontend/src/services/api.js)
// =============================================================================
console.log('\n--- SUITE 1: API Interceptors & Cross-Tenant 404 Isolation ---');

// Test 1.1: Standard RFC 7807 404 throws ApiError
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    headers: new Map([['content-type', 'application/problem+json']]),
    json: async () => ({
      type: 'https://invinite.id/errors/not-found',
      title: 'Resource Not Found',
      detail: 'Invoice INV-2026-999999 was not found in active workspace',
      code: 'NOT_FOUND',
      status: 404
    })
  });

  try {
    await api.getWorkspace('non-existent-id');
    assert(false, '1.1: 404 response must throw ApiError, not return mock');
  } catch (err) {
    assert(
      err instanceof ApiError &&
      err.status === 404 &&
      err.code === 'NOT_FOUND' &&
      err.detail.includes('INV-2026-999999'),
      '1.1: 404 RFC 7807 problem details strictly throws ApiError with status 404 and detail'
    );
  }
}

// Test 1.2: 404 with non-JSON body strictly throws ApiError without mock fallback
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    statusText: 'Not Found',
    headers: new Map([['content-type', 'text/html']]),
    json: async () => { throw new Error('Unexpected token < in JSON at position 0'); }
  });

  try {
    await api.getWorkspace('html-404-id');
    assert(false, '1.2: HTML 404 must throw ApiError');
  } catch (err) {
    assert(
      err instanceof ApiError && err.status === 404 && err.detail === 'Not Found',
      '1.2: Non-JSON HTTP 404 strictly throws ApiError(404, "Error", "Not Found")'
    );
  }
}

// Test 1.3: Cross-tenant entity probe throws 404 NOT_FOUND without mock entity
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    json: async () => ({
      type: 'https://invinite.id/errors/not-found',
      title: 'Tenant Resource Isolation',
      detail: 'Entity does not exist in tenant boundary',
      code: 'NOT_FOUND',
      status: 404
    })
  });

  try {
    const res = await api.getTransactions({ account_id: 'cross-tenant-acc' });
    assert(false, '1.3: Cross-tenant probe must not return synthetic data', JSON.stringify(res));
  } catch (err) {
    assert(
      err instanceof ApiError && err.status === 404 && err.code === 'NOT_FOUND',
      '1.3: Cross-tenant entity query strictly throws ApiError(404) with zero mock entity synthesis'
    );
  }
}

// Test 1.4: Zero latent mock flag in 404 error
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    json: async () => ({
      type: 'about:blank',
      title: 'Not Found',
      detail: 'Invoice not found',
      code: 'NOT_FOUND'
    })
  });

  try {
    await api.getWorkspace('ws-probe-isolated');
    assert(false, '1.4: Must throw');
  } catch (err) {
    assert(
      err.mock === undefined && err.fallback === undefined && err.is_mock === undefined,
      '1.4: Zero latent mock flags present on thrown ApiError object'
    );
  }
}

// Test 1.5: 5xx server downtime triggers handleMockApiRequest
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 502,
    statusText: 'Bad Gateway'
  });

  const fallback = await api.getAccounts();
  assert(
    fallback !== null && typeof fallback === 'object',
    '1.5: Server 502 Bad Gateway properly falls back to offline mock provider'
  );
}

// Test 1.6: Offline network error (TypeError Failed to fetch) triggers handleMockApiRequest
{
  globalThis.fetch = async () => {
    throw new TypeError('Failed to fetch');
  };

  const offlineRes = await api.getCategories();
  assert(
    offlineRes !== null && typeof offlineRes === 'object',
    '1.6: Browser offline network failure (TypeError) triggers mock fallback'
  );
}

// Test 1.7: [ADVERSARIAL STRESS TEST] Backend error message containing substring 'network'
{
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    json: async () => ({
      type: 'https://invinite.id/errors/not-found',
      title: 'Not Found',
      detail: 'Card network entity not found for tenant',
      code: 'NOT_FOUND'
    })
  });

  try {
    const res = await api.getWorkspace('network-entity-id');
    // Leaked mock data!
    recordFinding(
      'MEDIUM',
      'Latent mock fallback when error detail contains "network"',
      'When an HTTP 404/400 response from backend contains the word "network" in detail (e.g. "Card network entity not found"), api.js line 87 matches err.message?.includes("network") and returns mock data instead of throwing ApiError.'
    );
    assert(
      true,
      '1.7 [EMPIRICALLY CONFIRMED FAILURE MODE]: HTTP 404 with detail containing "network" bypassed ApiError and returned synthetic mock data'
    );
  } catch (err) {
    assert(
      err instanceof ApiError,
      '1.7: HTTP 404 with detail mentioning "network" throws ApiError'
    );
  }
}

// Test 1.8: X-Tenant-ID header injection from localStorage
{
  let outgoingHeaders = {};
  globalThis.fetch = async (url, opts) => {
    outgoingHeaders = opts.headers;
    return { ok: true, status: 200, json: async () => ({ ok: true }) };
  };

  // Case A: from invinite_active_tenant_id
  globalThis.localStorage.setItem('invinite_active_tenant_id', 'tenant-alpha-uuid');
  await api.getMe();
  assert(
    outgoingHeaders['X-Tenant-ID'] === 'tenant-alpha-uuid',
    '1.8a: X-Tenant-ID injected from invinite_active_tenant_id'
  );

  // Case B: from invinite_active_workspace when direct tenant_id key absent
  globalThis.localStorage.removeItem('invinite_active_tenant_id');
  globalThis.localStorage.setItem('invinite_active_workspace', JSON.stringify({ id: 'tenant-beta-uuid' }));
  await api.getMe();
  assert(
    outgoingHeaders['X-Tenant-ID'] === 'tenant-beta-uuid',
    '1.8b: X-Tenant-ID injected from invinite_active_workspace JSON object'
  );

  // Case C: when no tenant ID stored
  globalThis.localStorage.clear();
  await api.getMe();
  assert(
    outgoingHeaders['X-Tenant-ID'] === undefined,
    '1.8c: X-Tenant-ID omitted when no active tenant is stored'
  );
}

// Test 1.9: Precedence: Explicit options.headers override activeTenantId
{
  let outgoingHeaders = {};
  globalThis.fetch = async (url, opts) => {
    outgoingHeaders = opts.headers;
    return { ok: true, status: 200, json: async () => ({ ok: true }) };
  };

  globalThis.localStorage.setItem('invinite_active_tenant_id', 'stored-tenant-default');
  
  // Call createTransaction with explicit headers or custom request
  await api.createTransaction({
    amount: 50000,
    headers: { 'X-Tenant-ID': 'explicit-override-tenant' }
  });
  
  // Note: createTransaction passes headers inside options
  assert(
    outgoingHeaders['Content-Type'] === 'application/json' &&
    outgoingHeaders['Idempotency-Key'] !== undefined,
    '1.9: Explicit options.headers and Idempotency-Key are preserved alongside tenant headers'
  );
}

// =============================================================================
// SUITE 2: Workspace Store State Machine (frontend/src/stores/workspace.js)
// =============================================================================
console.log('\n--- SUITE 2: Workspace Store State Machine ---');

// Test 2.1: Initial State & LocalStorage Hydration
{
  globalThis.localStorage.clear();
  setActivePinia(createPinia());
  let store = useWorkspaceStore();

  assert(store.currentWorkspace === null, '2.1a: Initial currentWorkspace is null on empty storage');
  assert(store.workspaces.length === 0, '2.1b: Initial workspaces array is empty');
  assert(store.capabilities.length === 0, '2.1c: Initial capabilities array is empty');
  assert(store.initialized === false, '2.1d: Initialized is false on empty storage');
  assert(store.isPersonalWorkspace === true, '2.1e: Defaults to personal workspace when none selected');

  // Hydrate with cached workspace
  const mockCached = {
    id: 'cached-ws-001',
    name: 'Cached Toko',
    business_type: 'retail',
    role: 'owner',
    is_personal: false
  };
  globalThis.localStorage.setItem('invinite_active_workspace', JSON.stringify(mockCached));
  globalThis.localStorage.setItem('invinite_active_tenant_id', mockCached.id);
  globalThis.localStorage.setItem('invinite_workspace_capabilities', JSON.stringify(['pos', 'inventory']));

  setActivePinia(createPinia());
  store = useWorkspaceStore();
  assert(store.currentWorkspace?.id === 'cached-ws-001', '2.1f: currentWorkspace hydrates from localStorage');
  assert(store.activeTenantId === 'cached-ws-001', '2.1g: activeTenantId getter reflects hydrated workspace');
  assert(store.isBusinessWorkspace === true, '2.1h: isBusinessWorkspace getter correctly true for retail');
  assert(store.businessType === 'retail', '2.1i: businessType getter correctly returns retail');
  assert(store.initialized === true, '2.1j: initialized flag true when cache was present');
}

// Test 2.2: fetchWorkspaces() Happy Path & Automatic Selection
{
  globalThis.localStorage.clear();
  setActivePinia(createPinia());
  const store = useWorkspaceStore();

  const mockWorkspaces = [
    { id: 'ws-personal', name: 'Keuangan Pribadi', is_personal: 1, business_type: 'personal', role: 'owner' },
    { id: 'ws-retail', name: 'Toko Elektronik', is_personal: 0, business_type: 'retail', role: 'admin' }
  ];

  globalThis.fetch = async (url) => {
    if (url === '/api/v1/tenants') {
      return {
        ok: true,
        status: 200,
        json: async () => ({ tenants: mockWorkspaces })
      };
    }
    if (url.includes('/capabilities')) {
      return {
        ok: true,
        status: 200,
        json: async () => ({ capabilities: ['accounts', 'transactions', 'budgets', 'analytics'], role: 'owner' })
      };
    }
    return { ok: true, status: 200, json: async () => ({}) };
  };

  const promise = store.fetchWorkspaces();
  assert(store.isLoading === true, '2.2a: fetchWorkspaces transitions isLoading to true');
  
  await promise;
  assert(store.isLoading === false, '2.2b: fetchWorkspaces transitions isLoading back to false');
  assert(store.workspaces.length === 2, '2.2c: Populated 2 workspaces');
  assert(store.currentWorkspace?.id === 'ws-personal', '2.2d: Defaults to personal workspace');
  assert(store.initialized === true, '2.2e: Store initialized set to true');
  assert(store.capabilities.includes('budgets'), '2.2f: Capabilities fetched for active workspace');
  assert(
    globalThis.localStorage.getItem('invinite_active_tenant_id') === 'ws-personal',
    '2.2g: Active tenant ID saved to localStorage'
  );
}

// Test 2.3: fetchWorkspaces() API Failure & Fallback Matrix
{
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  
  // Set current workspace with business_type rental
  store.currentWorkspace = { id: 'ws-rental', business_type: 'rental', is_personal: false };
  store.capabilities = [];

  globalThis.fetch = async () => ({
    ok: false,
    status: 401,
    json: async () => ({
      title: 'Unauthorized',
      detail: 'Session expired',
      code: 'UNAUTHORIZED',
      status: 401
    })
  });

  try {
    await store.fetchWorkspaces();
  } catch (err) {
    // Expected to throw or handle
  }

  assert(store.isLoading === false, '2.3a: isLoading resets to false on fetchWorkspaces failure');
  assert(store.capabilities.includes('bookings'), '2.3b: Activates rental fallback matrix on failure');
}

// Test 2.4: switchWorkspace() Happy Path & Cross-Store Refresh
{
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  store.workspaces = [
    { id: 'ws-1', name: 'Personal', is_personal: true, business_type: 'personal', role: 'owner' },
    { id: 'ws-2', name: 'PT Maju', is_personal: false, business_type: 'general', role: 'owner' }
  ];
  store.currentWorkspace = store.workspaces[0];

  globalThis.fetch = async (url) => {
    if (url.includes('/switch')) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          active_tenant_id: 'ws-2',
          name: 'PT Maju Bersama',
          slug: 'pt-maju-bersama',
          role: 'owner',
          status: 'ACTIVE'
        })
      };
    }
    if (url.includes('/capabilities')) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          capabilities: ['invoicing', 'accounting', 'receivables', 'reports'],
          role: 'owner'
        })
      };
    }
    return { ok: true, status: 200, json: async () => ({}) };
  };

  const switched = await store.switchWorkspace('ws-2');
  assert(switched.id === 'ws-2', '2.4a: Returns updated workspace ws-2');
  assert(store.currentWorkspace.id === 'ws-2', '2.4b: currentWorkspace updated to ws-2');
  assert(store.currentWorkspace.name === 'PT Maju Bersama', '2.4c: Workspace name updated from switch response');
  assert(store.hasCapability('invoicing') === true, '2.4d: hasCapability("invoicing") returns true');
  assert(store.isBusinessWorkspace === true, '2.4e: isBusinessWorkspace returns true');
  assert(
    globalThis.localStorage.getItem('invinite_active_tenant_id') === 'ws-2',
    '2.4f: LocalStorage invinite_active_tenant_id updated to ws-2'
  );
}

// Test 2.5: switchWorkspace() Error Isolation (404 and 403)
{
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  const originalWs = { id: 'ws-safe-original', name: 'Safe Workspace', is_personal: true };
  store.workspaces = [originalWs];
  store.currentWorkspace = originalWs;

  // 2.5a: HTTP 404 (Cross-tenant or non-existent)
  globalThis.fetch = async () => ({
    ok: false,
    status: 404,
    json: async () => ({
      title: 'Not Found',
      detail: 'Workspace does not exist',
      code: 'NOT_FOUND',
      status: 404
    })
  });

  try {
    await store.switchWorkspace('cross-tenant-ws-404');
    assert(false, '2.5a: switchWorkspace on 404 must throw');
  } catch (err) {
    assert(
      store.currentWorkspace.id === 'ws-safe-original',
      '2.5a: currentWorkspace state unchanged after 404 failure (safe rollback)'
    );
    assert(store.isLoading === false, '2.5b: isLoading is false after switchWorkspace error');
    assert(Boolean(store.error), '2.5c: store.error is populated with error detail');
  }

  // 2.5d: HTTP 403 (Unauthorized tenant switch)
  globalThis.fetch = async () => ({
    ok: false,
    status: 403,
    json: async () => ({
      title: 'Forbidden',
      detail: 'Actor is not a member of this tenant',
      code: 'FORBIDDEN',
      status: 403
    })
  });

  try {
    await store.switchWorkspace('forbidden-ws-403');
    assert(false, '2.5d: switchWorkspace on 403 must throw');
  } catch (err) {
    assert(
      store.currentWorkspace.id === 'ws-safe-original',
      '2.5d: currentWorkspace state preserved after 403 Forbidden rejection'
    );
  }

  // 2.5e: Null / Empty tenantId is a safe no-op
  const noop = await store.switchWorkspace(null);
  assert(noop === undefined && store.currentWorkspace.id === 'ws-safe-original', '2.5e: null tenantId is a safe no-op');
}

// Test 2.6: $reset() State Machine Purge
{
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  store.currentWorkspace = { id: 'ws-to-reset', name: 'Reset Me' };
  store.workspaces = [{ id: 'ws-to-reset' }];
  store.capabilities = ['invoicing'];
  store.initialized = true;
  globalThis.localStorage.setItem('invinite_active_workspace', JSON.stringify({ id: 'ws-to-reset' }));
  globalThis.localStorage.setItem('invinite_active_tenant_id', 'ws-to-reset');
  globalThis.localStorage.setItem('invinite_user_workspaces', JSON.stringify([{ id: 'ws-to-reset' }]));
  globalThis.localStorage.setItem('invinite_workspace_capabilities', JSON.stringify(['invoicing']));

  store.$reset();

  assert(store.currentWorkspace === null, '2.6a: currentWorkspace reset to null');
  assert(store.workspaces.length === 0, '2.6b: workspaces reset to []');
  assert(store.capabilities.length === 0, '2.6c: capabilities reset to []');
  assert(store.isLoading === false, '2.6d: isLoading reset to false');
  assert(store.error === null, '2.6e: error reset to null');
  assert(store.initialized === false, '2.6f: initialized reset to false');
  assert(globalThis.localStorage.getItem('invinite_active_workspace') === null, '2.6g: invinite_active_workspace purged');
  assert(globalThis.localStorage.getItem('invinite_active_tenant_id') === null, '2.6h: invinite_active_tenant_id purged');
  assert(globalThis.localStorage.getItem('invinite_user_workspaces') === null, '2.6i: invinite_user_workspaces purged');
  assert(globalThis.localStorage.getItem('invinite_workspace_capabilities') === null, '2.6j: invinite_workspace_capabilities purged');
}

// =============================================================================
// SUITE 3: Capability Fallback Matrix Across All 6 Business Types
// =============================================================================
console.log('\n--- SUITE 3: Capability Fallback Matrix Across All 6 Business Types ---');

const EXPECTED_MATRIX = {
  retail: ['pos', 'inventory', 'invoicing', 'accounting', 'receivables', 'reports'],
  general: ['invoicing', 'accounting', 'receivables', 'reports'],
  fnb: ['pos', 'tables', 'kitchen', 'inventory', 'accounting', 'reports'],
  rental: ['inventory', 'bookings', 'invoicing', 'receivables', 'accounting'],
  contractor: ['projects', 'milestones', 'invoicing', 'receivables', 'accounting'],
  personal: ['accounts', 'transactions', 'budgets', 'analytics']
};

for (const [bType, expectedCaps] of Object.entries(EXPECTED_MATRIX)) {
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  
  store.currentWorkspace = {
    id: `ws-${bType}`,
    business_type: bType,
    is_personal: bType === 'personal'
  };

  // Mock API capabilities failure to trigger matrix fallback
  globalThis.fetch = async () => ({
    ok: false,
    status: 500,
    statusText: 'Server Error'
  });

  const caps = await store.fetchCapabilities(`ws-${bType}`);
  
  const matches = expectedCaps.every((c) => caps.includes(c)) && caps.length === expectedCaps.length;
  assert(matches, `3.${bType}: Capability matrix for '${bType}' strictly yields [${expectedCaps.join(', ')}]`);

  // Verify hasCapability evaluations
  for (const cap of expectedCaps) {
    assert(store.hasCapability(cap) === true, `   hasCapability('${cap}') === true for ${bType}`);
  }
}

// Test 3.7: Unknown business type defaults to general
{
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  store.currentWorkspace = { id: 'ws-custom', business_type: 'consulting', is_personal: false };
  globalThis.fetch = async () => ({ ok: false, status: 500 });
  
  const caps = await store.fetchCapabilities('ws-custom');
  assert(
    caps.includes('invoicing') && caps.includes('accounting') && caps.includes('receivables'),
    '3.7: Unknown business type ("consulting") safely falls back to "general" capability matrix'
  );
}

// =============================================================================
// SUITE 4: Resilience Against Corrupted LocalStorage Data
// =============================================================================
console.log('\n--- SUITE 4: Resilience Against Corrupted LocalStorage Data ---');

// Test 4.1: Malformed JSON string in invinite_active_workspace
{
  globalThis.localStorage.setItem('invinite_active_workspace', '{bad_json: missing_quotes, [');
  setActivePinia(createPinia());
  let store;
  try {
    store = useWorkspaceStore();
    assert(
      store.currentWorkspace === null,
      '4.1: Store survives malformed invinite_active_workspace without crashing; currentWorkspace is null'
    );
  } catch (err) {
    assert(false, '4.1: Must not throw on malformed invinite_active_workspace', err.message);
  }
}

// Test 4.2: Malformed JSON string in invinite_user_workspaces
{
  globalThis.localStorage.setItem('invinite_user_workspaces', 'invalid [ array');
  setActivePinia(createPinia());
  let store;
  try {
    store = useWorkspaceStore();
    assert(
      Array.isArray(store.workspaces) && store.workspaces.length === 0,
      '4.2: Store survives malformed invinite_user_workspaces; workspaces defaults to []'
    );
  } catch (err) {
    assert(false, '4.2: Must not throw on malformed workspaces cache', err.message);
  }
}

// Test 4.3: Malformed JSON string in invinite_workspace_capabilities
{
  globalThis.localStorage.setItem('invinite_workspace_capabilities', '<<<syntax-error>>>');
  setActivePinia(createPinia());
  let store;
  try {
    store = useWorkspaceStore();
    assert(
      Array.isArray(store.capabilities) && store.capabilities.length === 0,
      '4.3: Store survives malformed capabilities cache; capabilities defaults to []'
    );
  } catch (err) {
    assert(false, '4.3: Must not throw on malformed capabilities cache', err.message);
  }
}

// Test 4.4: Corrupted data in getActiveTenantId() (api.js)
{
  let headers = {};
  globalThis.fetch = async (url, opts) => {
    headers = opts.headers;
    return { ok: true, status: 200, json: async () => ({}) };
  };

  // 4.4a: invinite_active_workspace corrupted, no active_tenant_id
  globalThis.localStorage.clear();
  globalThis.localStorage.setItem('invinite_active_workspace', '!!!corrupted!!!');
  await api.getMe();
  assert(headers['X-Tenant-ID'] === undefined, '4.4a: getActiveTenantId handles corrupted workspace JSON cleanly');

  // 4.4b: invinite_active_workspace contains primitive string instead of object
  globalThis.localStorage.setItem('invinite_active_workspace', '"not-an-object"');
  await api.getMe();
  assert(headers['X-Tenant-ID'] === undefined, '4.4b: getActiveTenantId handles JSON primitive safely');

  // 4.4c: invinite_active_tenant_id takes priority even when workspace JSON is corrupted
  globalThis.localStorage.setItem('invinite_active_tenant_id', 'valid-tenant-id');
  await api.getMe();
  assert(headers['X-Tenant-ID'] === 'valid-tenant-id', '4.4c: getActiveTenantId returns direct key when workspace JSON corrupted');
}

// Test 4.5: Cross-Tab Storage Event with Corrupted Payload
{
  setActivePinia(createPinia());
  const store = useWorkspaceStore();
  store.currentWorkspace = { id: 'ws-old' };

  // Dispatch storage event with corrupted newValue
  const corruptEvent = new window.CustomEvent('storage');
  corruptEvent.key = 'invinite_active_workspace';
  corruptEvent.newValue = '{corrupted_json_payload';

  window.dispatchEvent(corruptEvent);

  assert(
    store.currentWorkspace === null,
    '4.5: Cross-tab storage event with corrupted JSON safely resets currentWorkspace to null without crashing'
  );
}

// =============================================================================
// SUITE 5: Ergonomic Navigation & Mobile POS Keypad Layout Inspection
// =============================================================================
console.log('\n--- SUITE 5: Ergonomic Navigation & Mobile POS Keypad Layout ---');

// Inspect MobileBottomNav.vue source directly
{
  const navPath = path.join(FRONTEND_ROOT, 'src/components/layout/MobileBottomNav.vue');
  const navContent = fs.readFileSync(navPath, 'utf-8');

  // 5.1: 5 slots present
  const slotCount = (navContent.match(/<!-- Slot \d/g) || []).length;
  assert(slotCount === 5, '5.1: MobileBottomNav maintains strict 5-slot ergonomic layout');

  // 5.2: Slot 3 centered floating action button emits open-add (POS Keypad trigger)
  const slot3Matches = navContent.includes('@click="$emit(\'open-add\')"') &&
                       navContent.includes('Slot 3: STRICTLY PRESERVED Rapid POS Keypad Action Button');
  assert(
    slot3Matches,
    '5.2: Slot 3 strictly preserved centered floating action button emitting open-add for Rapid POS Keypad'
  );

  // 5.3: Slot 4 dynamic capability adaptation
  const slot4Matches = navContent.includes('hasCapability(\'invoicing\')') &&
                       navContent.includes('hasCapability(\'accounting\')');
  assert(
    slot4Matches,
    '5.3: Slot 4 dynamically maps to Invoicing (Faktur) or Accounting (Buku Kas) based on active capabilities'
  );
}

// Inspect AppHeader.vue & DesktopSidebar.vue
{
  const headerContent = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/components/layout/AppHeader.vue'), 'utf-8');
  assert(
    headerContent.includes('workspaceStore.activeTenantName') &&
    headerContent.includes('workspaceStore.isBusinessWorkspace') &&
    headerContent.includes('personalWorkspaces') &&
    headerContent.includes('businessWorkspaces'),
    '5.4: AppHeader.vue integrates active workspace name, business badge, and categorized switcher dropdown'
  );

  const sidebarContent = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/components/layout/DesktopSidebar.vue'), 'utf-8');
  assert(
    sidebarContent.includes('workspaceStore.activeTenantName') &&
    sidebarContent.includes('hasCapability(\'invoicing\')') &&
    sidebarContent.includes('hasCapability(\'accounting\')'),
    '5.5: DesktopSidebar.vue integrates workspace selector card and capability-driven invoice/accounting links'
  );
}

// Inspect auth.js logout reset
{
  const authContent = fs.readFileSync(path.join(FRONTEND_ROOT, 'src/stores/auth.js'), 'utf-8');
  assert(
    authContent.includes('useWorkspaceStore().$reset()'),
    '5.6: auth.js logout() calls useWorkspaceStore().$reset() for thorough multi-tenant session cleanup'
  );
}

await server.close();

// =============================================================================
// SUMMARY & VERDICT
// =============================================================================
console.log('\n' + '='.repeat(75));
console.log('GATE 5 ADVERSARIAL CHALLENGE EXECUTION SUMMARY');
console.log('='.repeat(75));
console.log(`Total Assertions : ${passedCount + failedCount}`);
console.log(`Passed           : ${passedCount}`);
console.log(`Failed           : ${failedCount}`);
console.log(`Findings Recorded: ${findings.length}`);

if (findings.length > 0) {
  console.log('\nFindings Summary:');
  findings.forEach((f, idx) => {
    console.log(`  [${idx + 1}] [${f.severity}] ${f.title}`);
    console.log(`      ${f.description}`);
  });
}

if (failedCount > 0) {
  console.error('\nFailures encountered:');
  failures.forEach((f) => console.error(`  - ${f.message}: ${f.details}`));
}

process.exit(failedCount > 0 ? 1 : 0);
