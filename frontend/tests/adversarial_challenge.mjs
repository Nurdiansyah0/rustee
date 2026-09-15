/**
 * Empirical Adversarial Challenge Test Harness for Milestone 2 UI Primitives & Tokens
 * Personal Finance PWA
 */

import * as hd from 'happy-dom';
import fs from 'fs';
import path from 'path';

// Setup DOM environment before importing Vue runtime
const window = new hd.GlobalWindow();
globalThis.window = window;
globalThis.document = window.document;
Object.assign(globalThis, {
  HTMLElement: window.HTMLElement,
  Element: window.Element,
  Node: window.Node,
  SVGElement: window.SVGElement,
  DocumentFragment: window.DocumentFragment,
  MouseEvent: window.MouseEvent,
  KeyboardEvent: window.KeyboardEvent,
  CustomEvent: window.CustomEvent,
  FocusEvent: window.FocusEvent,
  TouchEvent: window.TouchEvent,
  Event: window.Event,
  requestAnimationFrame: (cb) => setTimeout(cb, 16),
  cancelAnimationFrame: (id) => clearTimeout(id),
});

const { createApp, h, nextTick, ref } = await import('vue');
const { build } = await import('vite');
const vuePlugin = (await import('@vitejs/plugin-vue')).default;

const FRONTEND_ROOT = '/home/nurdiansyah/teamwork_projects/personal_finance_pwa/frontend';
const TMP_BUNDLE_PATH = path.join(FRONTEND_ROOT, 'node_modules', '.tmp_m2_challenge_bundle.mjs');

console.log('='.repeat(70));
console.log('M2 ADVERSARIAL CHALLENGE TEST HARNESS');
console.log('Target: Button.vue, Input.vue, Badge.vue, ModalSheet.vue, Tokens, Build');
console.log('='.repeat(70));

let passedCount = 0;
let failedCount = 0;
const failures = [];

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

// -----------------------------------------------------------------------------
// Test 1: Vite Compilation & UI Component Library Build
// -----------------------------------------------------------------------------
console.log('\n--- 1. Compilation & Module Transformation ---');
let components;
try {
  const startTime = Date.now();
  const buildResult = await build({
    root: FRONTEND_ROOT,
    configFile: false,
    plugins: [vuePlugin()],
    resolve: {
      alias: {
        '@': path.join(FRONTEND_ROOT, 'src'),
      },
    },
    build: {
      write: false,
      rollupOptions: {
        external: ['vue'],
      },
      lib: {
        entry: 'src/components/ui/index.ts',
        formats: ['es'],
        fileName: () => 'bundle.js',
      },
    },
  });
  const elapsed = Date.now() - startTime;
  assert(Boolean(buildResult && buildResult[0] && buildResult[0].output), `Vite library build completes in ${elapsed}ms`);

  const code = buildResult[0].output[0].code;
  fs.writeFileSync(TMP_BUNDLE_PATH, code, 'utf-8');

  components = await import(TMP_BUNDLE_PATH);
  assert(Boolean(components.Button), 'Exports Button primitive');
  assert(Boolean(components.Input), 'Exports Input primitive');
  assert(Boolean(components.Badge), 'Exports Badge primitive');
  assert(Boolean(components.ModalSheet), 'Exports ModalSheet primitive');
  assert(Boolean(components.Card), 'Exports Card primitive');
} catch (err) {
  assert(false, 'Vite compilation succeeded without errors', err.message);
}

// Helper to mount and unmount a test component
function mount(component, props = {}, slots = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const app = createApp({
    render() {
      return h(component, props, slots);
    },
  });
  app.mount(container);
  return {
    container,
    app,
    unmount: () => {
      app.unmount();
      container.remove();
    },
  };
}

// -----------------------------------------------------------------------------
// Test 2: Button.vue Adversarial Stress Tests
// -----------------------------------------------------------------------------
console.log('\n--- 2. Button.vue Stress Tests ---');
const { Button } = components;

// 2.1 Disabled + Loading
{
  let clickEmitted = false;
  const { container, unmount } = mount(Button, {
    disabled: true,
    loading: true,
    onClick: () => { clickEmitted = true; },
  }, () => 'Submit');

  const btn = container.querySelector('button');
  assert(btn !== null, 'Button element exists');
  assert(btn.getAttribute('disabled') !== null, 'Disabled attribute is set on DOM element');
  assert(btn.getAttribute('aria-busy') === 'true', 'aria-busy="true" is set during loading');
  assert(container.querySelector('svg.animate-spin') !== null, 'Loading spinner SVG is present');

  // Trigger click event
  btn.click();
  btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  assert(!clickEmitted, 'Click event is NOT emitted when disabled and loading');

  unmount();
}

// 2.2 Empty Slots
{
  try {
    const { container, unmount } = mount(Button, {}, {});
    const btn = container.querySelector('button');
    assert(btn !== null && btn.textContent.trim() === '', 'Empty slot renders gracefully without crashing');
    unmount();
  } catch (err) {
    assert(false, 'Empty slot does not crash Button.vue', err.message);
  }
}

// 2.3 Long Text Overflow
{
  const longText = 'A'.repeat(500);
  const { container, unmount } = mount(Button, {}, () => longText);
  const span = container.querySelector('button > span.truncate');
  assert(span !== null, 'Long text is wrapped in span.truncate for text overflow handling');
  assert(span.textContent.trim() === longText, 'Long text rendered intact in span');
  unmount();
}

// 2.4 Loading Text prop
{
  const { container, unmount } = mount(Button, {
    loading: true,
    loadingText: 'Menyimpan...',
  }, () => 'Original Text');

  const btn = container.querySelector('button');
  assert(btn.textContent.includes('Menyimpan...'), 'Loading text is displayed when loading');
  assert(!btn.textContent.includes('Original Text'), 'Default slot is suppressed when loadingText is active');
  unmount();
}

// 2.5 Variants & Sizes Matrix
{
  const variants = ['primary', 'secondary', 'outline', 'ghost', 'destructive'];
  for (const v of variants) {
    const { container, unmount } = mount(Button, { variant: v }, () => v);
    const btn = container.querySelector('button');
    assert(btn.className.length > 0, `Button renders variant: ${v}`);
    unmount();
  }

  const sizes = ['sm', 'md', 'lg', 'icon'];
  for (const s of sizes) {
    const { container, unmount } = mount(Button, { size: s }, () => s);
    const btn = container.querySelector('button');
    assert(btn.className.length > 0, `Button renders size: ${s}`);
    unmount();
  }
}

// 2.6 Full Width & Mobile Full Width
{
  const { container: c1, unmount: u1 } = mount(Button, { fullWidth: true }, () => 'Full');
  assert(c1.querySelector('button').classList.contains('w-full'), 'fullWidth applies w-full');
  u1();

  const { container: c2, unmount: u2 } = mount(Button, { mobileFullWidth: true }, () => 'MobileFull');
  const btnClass = c2.querySelector('button').className;
  assert(btnClass.includes('w-full') && btnClass.includes('sm:w-auto'), 'mobileFullWidth applies w-full sm:w-auto');
  u2();
}

// -----------------------------------------------------------------------------
// Test 3: Input.vue Adversarial Stress Tests
// -----------------------------------------------------------------------------
console.log('\n--- 3. Input.vue Stress Tests ---');
const { Input } = components;

// 3.1 Extreme Character Lengths & Adversarial Payloads
{
  const payloads = [
    { name: '10,000 characters', val: 'X'.repeat(10000) },
    { name: 'Unicode / Emoji symbols', val: '💸💎📈✨🔥💳🏦💰' },
    { name: 'XSS script injection string', val: '<script>alert("xss")</script><img src=x onerror=alert(1)>' },
    { name: 'SQL injection string', val: "'; DROP TABLE users; -- ' OR 1=1" },
  ];

  for (const p of payloads) {
    let emittedVal = null;
    const { container, unmount } = mount(Input, {
      modelValue: p.val,
      'onUpdate:modelValue': (val) => { emittedVal = val; },
    });
    const input = container.querySelector('input');
    assert(input !== null, `Input element exists for ${p.name}`);
    assert(input.value === p.val, `Input correctly reflects value for ${p.name}`);

    // Trigger input event
    input.value = p.val + '_updated';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    assert(emittedVal === p.val + '_updated', `Emitted update:modelValue correctly for ${p.name}`);
    unmount();
  }
}

// 3.2 Password Visibility Toggle State Changes
{
  let currentVal = 'SecretP@ssw0rd!';
  const { container, unmount } = mount(Input, {
    type: 'password',
    showPasswordToggle: true,
    modelValue: currentVal,
  });

  const input = container.querySelector('input');
  assert(input.type === 'password', 'Initial input type is password');

  const toggleBtn = container.querySelector('button[type="button"]');
  assert(toggleBtn !== null, 'Password toggle button is present');
  assert(toggleBtn.getAttribute('aria-label') === 'Tampilkan kata sandi', 'Initial toggle aria-label is Tampilkan kata sandi');

  // Click toggle to reveal password
  toggleBtn.click();
  await nextTick();
  assert(input.type === 'text', 'Input type switches to text after clicking toggle');
  assert(toggleBtn.getAttribute('aria-label') === 'Sembunyikan kata sandi', 'Toggle aria-label changes to Sembunyikan kata sandi');

  // Click toggle again to hide password
  toggleBtn.click();
  await nextTick();
  assert(input.type === 'password', 'Input type reverts to password on second click');
  assert(toggleBtn.getAttribute('aria-label') === 'Tampilkan kata sandi', 'Toggle aria-label reverts to Tampilkan kata sandi');

  unmount();

  // Disabled password input should NOT render toggle button
  const { container: cDis, unmount: uDis } = mount(Input, {
    type: 'password',
    showPasswordToggle: true,
    disabled: true,
  });
  assert(cDis.querySelector('button') === null, 'Password toggle button is hidden when disabled');
  uDis();
}

// 3.3 Prefix / Suffix Rendering (Props and Slots)
{
  // Props
  const { container: c1, unmount: u1 } = mount(Input, {
    prefix: 'Rp',
    suffix: 'IDR',
  });
  assert(c1.textContent.includes('Rp'), 'Prefix "Rp" renders via prop');
  assert(c1.textContent.includes('IDR'), 'Suffix "IDR" renders via prop');
  u1();

  // Slots
  const { container: c2, unmount: u2 } = mount(Input, {}, {
    prefix: () => 'PrefixSlot',
    suffix: () => 'SuffixSlot',
    leading: () => 'LeadingSlot',
    trailing: () => 'TrailingSlot',
  });
  assert(c2.textContent.includes('PrefixSlot'), 'Prefix renders via slot');
  assert(c2.textContent.includes('SuffixSlot'), 'Suffix renders via slot');
  assert(c2.textContent.includes('LeadingSlot'), 'Leading icon renders via slot');
  assert(c2.textContent.includes('TrailingSlot'), 'Trailing icon renders via slot');
  u2();
}

// 3.4 Error State Transitions & Accessibility Links
{
  const errRef = ref('');
  const helpRef = ref('Bantuan input');
  const container = document.createElement('div');
  document.body.appendChild(container);

  const app = createApp({
    render() {
      return h(Input, {
        errorMessage: errRef.value,
        helperText: helpRef.value,
        label: 'Email',
      });
    },
  });
  app.mount(container);
  await nextTick();

  const input = container.querySelector('input');
  assert(input.getAttribute('aria-invalid') === null, 'No aria-invalid when no error');
  assert(container.querySelector('p').textContent.includes('Bantuan input'), 'Helper text shown when no error');
  const helperId = container.querySelector('p').id;
  assert(input.getAttribute('aria-describedby') === helperId, 'aria-describedby links to helper text');

  // Trigger error state
  errRef.value = 'Email tidak valid';
  await nextTick();

  assert(input.getAttribute('aria-invalid') === 'true', 'aria-invalid="true" set on error');
  const errP = container.querySelector('p');
  assert(errP.textContent.includes('Email tidak valid'), 'Error message displayed');
  assert(errP.getAttribute('aria-live') === 'polite', 'aria-live="polite" present on error message');
  assert(input.getAttribute('aria-describedby') === errP.id, 'aria-describedby updated to link to error message');
  assert(container.querySelector('svg') !== null, 'AlertCircle icon displayed on error');

  // Clear error state
  errRef.value = '';
  await nextTick();

  assert(input.getAttribute('aria-invalid') === null, 'aria-invalid removed after error is cleared');
  assert(container.querySelector('p').textContent.includes('Bantuan input'), 'Helper text restored after error is cleared');

  app.unmount();
  container.remove();
}

// 3.5 Clearable Input Behavior & Numeric Edge Cases
{
  let cleared = false;
  let modelVal = 'Initial Text';
  const { container, unmount } = mount(Input, {
    clearable: true,
    modelValue: modelVal,
    'onUpdate:modelValue': (v) => { modelVal = v; },
    onClear: () => { cleared = true; },
  });

  const clearBtn = container.querySelector('button[aria-label="Hapus teks"]');
  assert(clearBtn !== null, 'Clear button renders when clearable and modelValue has text');
  clearBtn.click();
  await nextTick();

  assert(cleared === true, 'onClear event emitted on click');
  assert(modelVal === '', 'modelValue reset to empty string');
  unmount();

  // Adversarial edge case: numeric 0
  const { container: cZero, unmount: uZero } = mount(Input, {
    clearable: true,
    modelValue: 0,
  });
  const zeroClearBtn = cZero.querySelector('button[aria-label="Hapus teks"]');
  console.log('    [EMPIRICAL NOTE] When modelValue is numeric 0, clear button rendered:', Boolean(zeroClearBtn));
  uZero();
}

// -----------------------------------------------------------------------------
// Test 4: Badge.vue Stress Tests (Variants, Dot, Sizes)
// -----------------------------------------------------------------------------
console.log('\n--- 4. Badge.vue Stress Tests ---');
const { Badge } = components;

// 4.1 All 6 Semantic Variant States + Aliases
{
  const variantChecks = [
    { variant: 'income', expectedClass: 'bg-income-muted', label: 'income' },
    { variant: 'surplus', expectedClass: 'bg-income-muted', label: 'surplus (alias)' },
    { variant: 'active', expectedClass: 'bg-income-muted', label: 'active (alias)' },
    { variant: 'expense', expectedClass: 'bg-expense-muted', label: 'expense' },
    { variant: 'deficit', expectedClass: 'bg-expense-muted', label: 'deficit (alias)' },
    { variant: 'warning', expectedClass: 'bg-warning-muted', label: 'warning' },
    { variant: 'pending', expectedClass: 'bg-warning-muted', label: 'pending (alias)' },
    { variant: 'transfer', expectedClass: 'bg-transfer-muted', label: 'transfer' },
    { variant: 'brand', expectedClass: 'bg-brand-muted', label: 'brand' },
    { variant: 'premium', expectedClass: 'border-brand-border', label: 'premium' },
    { variant: 'default', expectedClass: 'bg-surface-subtle', label: 'default' },
    { variant: 'free', expectedClass: 'bg-surface-subtle', label: 'free (alias)' },
    { variant: 'neutral', expectedClass: 'bg-surface-subtle', label: 'neutral (alias)' },
    { variant: 'nonexistent_variant', expectedClass: 'bg-surface-subtle', label: 'unknown variant fallback' },
  ];

  for (const vc of variantChecks) {
    const { container, unmount } = mount(Badge, { variant: vc.variant }, () => 'BadgeText');
    const badgeEl = container.querySelector('span');
    assert(badgeEl.className.includes(vc.expectedClass), `Badge handles ${vc.label}`);
    unmount();
  }
}

// 4.2 Dot Indicator (Default vs Custom Color)
{
  // Without dot: verify dot element (span.rounded-full) is absent
  const { container: cNoDot, unmount: uNoDot } = mount(Badge, { dot: false }, () => 'NoDot');
  assert(cNoDot.querySelector('span.rounded-full') === null, 'No dot element when dot=false');
  uNoDot();

  // With default dot
  const { container: cDot, unmount: uDot } = mount(Badge, { dot: true, variant: 'income' }, () => 'WithDot');
  const dotEl = cDot.querySelector('span.rounded-full');
  assert(dotEl !== null, 'Dot element rendered when dot=true');
  assert(dotEl.className.includes('bg-income-default'), 'Dot uses matching semantic class bg-income-default');
  uDot();

  // With custom dotColor
  const { container: cCustom, unmount: uCustom } = mount(Badge, { dot: true, dotColor: '#123456' }, () => 'CustomDot');
  const customDotEl = cCustom.querySelector('span.rounded-full');
  assert(customDotEl.style.backgroundColor === '#123456' || customDotEl.style.backgroundColor === 'rgb(18, 52, 86)', 'Custom dotColor applied via inline style');
  uCustom();
}

// 4.3 Badge Sizes & Icon Slot
{
  const { container: cSm, unmount: uSm } = mount(Badge, { size: 'sm' }, () => 'Small');
  assert(cSm.querySelector('span').className.includes('text-[10px]'), 'Badge size="sm" applies text-[10px]');
  uSm();

  const { container: cMd, unmount: uMd } = mount(Badge, { size: 'md' }, () => 'Medium');
  assert(cMd.querySelector('span').className.includes('text-[11px]'), 'Badge size="md" applies text-[11px]');
  uMd();

  // Icon slot
  const { container: cIcon, unmount: uIcon } = mount(Badge, {}, {
    icon: () => h('svg', { class: 'custom-icon' }),
    default: () => 'IconBadge',
  });
  assert(cIcon.querySelector('svg.custom-icon') !== null, 'Badge renders #icon slot');
  uIcon();
}

// -----------------------------------------------------------------------------
// Test 5: ModalSheet.vue Adversarial Stress Tests
// -----------------------------------------------------------------------------
console.log('\n--- 5. ModalSheet.vue Stress Tests ---');
const { ModalSheet } = components;

// 5.1 Open / Close with modelValue and isOpen props
{
  // Closed by default
  const { unmount: uClosed } = mount(ModalSheet, { modelValue: false });
  await nextTick();
  assert(document.body.querySelector('[role="dialog"]') === null, 'Modal dialog is not in DOM when modelValue=false');
  uClosed();

  // Open with modelValue
  let closeEmitted = false;
  let updateEmittedVal = null;
  const { unmount: uOpen } = mount(ModalSheet, {
    modelValue: true,
    title: 'Test Modal',
    description: 'Test Description',
    'onUpdate:modelValue': (v) => { updateEmittedVal = v; },
    onClose: () => { closeEmitted = true; },
  }, () => 'Modal Body Content');
  await nextTick();

  const dialog = document.body.querySelector('[role="dialog"]');
  assert(dialog !== null, 'Modal dialog teleports to body when active');
  assert(dialog.getAttribute('aria-modal') === 'true', 'aria-modal="true" is set on dialog');
  assert(dialog.textContent.includes('Test Modal'), 'Title is rendered in dialog');
  assert(dialog.textContent.includes('Test Description'), 'Description is rendered in dialog');
  assert(dialog.textContent.includes('Modal Body Content'), 'Body slot content is rendered');

  // Click close button
  const closeBtn = dialog.querySelector('button[aria-label="Tutup dialog"]');
  assert(closeBtn !== null, 'Close button is present in header');
  closeBtn.click();
  await nextTick();

  assert(closeEmitted === true, 'close event emitted on close button click');
  assert(updateEmittedVal === false, 'update:modelValue false emitted on close button click');

  uOpen();
}

// 5.2 ESC Key Handling
{
  let escClosed = false;
  const { unmount } = mount(ModalSheet, {
    modelValue: true,
    closeOnEscape: true,
    onClose: () => { escClosed = true; },
  });
  await nextTick();

  // Dispatch ESC keydown on window
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await nextTick();
  assert(escClosed === true, 'Escape key triggers modal close event');
  unmount();

  // closeOnEscape: false
  let escBlocked = false;
  const { unmount: uBlocked } = mount(ModalSheet, {
    modelValue: true,
    closeOnEscape: false,
    onClose: () => { escBlocked = true; },
  });
  await nextTick();

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await nextTick();
  assert(escBlocked === false, 'Escape key does NOT close modal when closeOnEscape=false');
  uBlocked();
}

// 5.3 Background / Backdrop Click Handling
{
  let backdropClosed = false;
  const { unmount } = mount(ModalSheet, {
    modelValue: true,
    closeOnBackdrop: true,
    onClose: () => { backdropClosed = true; },
  });
  await nextTick();

  const backdrop = document.body.querySelector('[role="dialog"] > div.fixed.inset-0');
  assert(backdrop !== null, 'Backdrop scrim exists');
  backdrop.click();
  await nextTick();
  assert(backdropClosed === true, 'Backdrop click triggers modal close when closeOnBackdrop=true');
  unmount();

  // closeOnBackdrop: false
  let backdropBlocked = false;
  const { unmount: uBlocked } = mount(ModalSheet, {
    modelValue: true,
    closeOnBackdrop: false,
    onClose: () => { backdropBlocked = true; },
  });
  await nextTick();

  const blockedBackdrop = document.body.querySelector('[role="dialog"] > div.fixed.inset-0');
  blockedBackdrop.click();
  await nextTick();
  assert(backdropBlocked === false, 'Backdrop click does NOT close modal when closeOnBackdrop=false');
  uBlocked();
}

// 5.4 Body Scroll Locking
{
  const activeRef = ref(false);
  const container = document.createElement('div');
  document.body.appendChild(container);

  const app = createApp({
    render() {
      return h(ModalSheet, { modelValue: activeRef.value });
    },
  });
  app.mount(container);
  await nextTick();

  assert(document.body.style.overflow === '', 'Initial body overflow is not locked');

  // Open modal
  activeRef.value = true;
  await nextTick();
  assert(document.body.style.overflow === 'hidden', 'document.body.style.overflow="hidden" when modal opens');

  // Close modal
  activeRef.value = false;
  await nextTick();
  assert(document.body.style.overflow === '', 'document.body.style.overflow restored to empty string when modal closes');

  // Open and then unmount while open (ensure cleanup in onUnmounted)
  activeRef.value = true;
  await nextTick();
  assert(document.body.style.overflow === 'hidden', 'Body overflow locked again when modal opens');

  app.unmount();
  container.remove();
  assert(document.body.style.overflow === '', 'Body overflow restored to empty string when unmounted while active');
}

// -----------------------------------------------------------------------------
// Test 6: Dark Theme Token Resolution & Styling
// -----------------------------------------------------------------------------
console.log('\n--- 6. Dark Theme Token Resolution & Consistency ---');
{
  const tailwindPath = path.join(FRONTEND_ROOT, 'tailwind.config.js');
  const indexCssPath = path.join(FRONTEND_ROOT, 'src', 'index.css');

  const twContent = fs.readFileSync(tailwindPath, 'utf-8');
  const cssContent = fs.readFileSync(indexCssPath, 'utf-8');

  // 1. Dark mode configured as 'class'
  assert(twContent.includes('darkMode: "class"'), 'Tailwind config declares darkMode: "class"');

  // 2. Extract referenced CSS variables in tailwind.config.js
  const varRegex = /var\((--[a-zA-Z0-9_-]+)\)/g;
  const referencedVars = new Set();
  let m;
  while ((m = varRegex.exec(twContent)) !== null) {
    referencedVars.add(m[1]);
  }
  assert(referencedVars.size >= 30, `Found ${referencedVars.size} semantic CSS variables referenced in Tailwind`);

  // 3. Extract :root and .dark blocks
  const rootMatch = cssContent.match(/:root\s*\{([^}]+)\}/s);
  const darkMatch = cssContent.match(/\.dark\s*\{([^}]+)\}/s);
  assert(Boolean(rootMatch), 'src/index.css contains :root block');
  assert(Boolean(darkMatch), 'src/index.css contains .dark block');

  const rootContent = rootMatch ? rootMatch[1] : '';
  const darkContent = darkMatch ? darkMatch[1] : '';

  const rootVars = new Set([...rootContent.matchAll(/(--[a-zA-Z0-9_-]+):/g)].map(x => x[1]));
  const darkVars = new Set([...darkContent.matchAll(/(--[a-zA-Z0-9_-]+):/g)].map(x => x[1]));

  // Check complete variable parity
  const missingInRoot = [...referencedVars].filter(v => !rootVars.has(v));
  const missingInDark = [...referencedVars].filter(v => !darkVars.has(v));
  assert(missingInRoot.length === 0, 'Zero variables missing in :root', missingInRoot.join(', '));
  assert(missingInDark.length === 0, 'Zero variables missing in .dark', missingInDark.join(', '));

  const rootDarkDiff = [...rootVars].filter(v => !darkVars.has(v));
  const darkRootDiff = [...darkVars].filter(v => !rootVars.has(v));
  assert(rootDarkDiff.length === 0, 'All :root variables exist in .dark', rootDarkDiff.join(', '));
  assert(darkRootDiff.length === 0, 'All .dark variables exist in :root', darkRootDiff.join(', '));

  // 4. Verify channel values format
  let invalidFormatCount = 0;
  for (const block of [rootContent, darkContent]) {
    for (const line of block.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed.startsWith('--')) continue;
      const match = trimmed.match(/^(--[a-zA-Z0-9_-]+)\s*:\s*([^;]+);/);
      if (!match) { invalidFormatCount++; continue; }
      const channels = match[2].split('/*')[0].trim().split(/\s+/);
      if (channels.length !== 3 || channels.some(c => isNaN(Number(c)) || Number(c) < 0 || Number(c) > 255)) {
        invalidFormatCount++;
      }
    }
  }
  assert(invalidFormatCount === 0, 'All CSS variable values are valid 3-channel space-separated RGB (0-255)');
}

// -----------------------------------------------------------------------------
// Cleanup temporary bundle
// -----------------------------------------------------------------------------
try {
  if (fs.existsSync(TMP_BUNDLE_PATH)) {
    fs.unlinkSync(TMP_BUNDLE_PATH);
  }
} catch {}

// -----------------------------------------------------------------------------
// Test Summary & Verdict
// -----------------------------------------------------------------------------
console.log('\n' + '='.repeat(70));
console.log(`TEST SUMMARY: ${passedCount} Passed | ${failedCount} Failed`);
if (failures.length > 0) {
  console.log('\nFailures:');
  for (const f of failures) {
    console.log(`  - ${f.message} (${f.details})`);
  }
}
console.log('='.repeat(70));

process.exit(failedCount === 0 ? 0 : 1);
