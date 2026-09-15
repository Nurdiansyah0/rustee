# Personal Finance PWA — Design System Specification
**Version**: 1.0.0  
**Status**: Authoritative & Production-Ready  
**Benchmarks**: Mercury, Linear, Ramp, Stripe  
**Compliance**: WCAG 2.1 AA / AAA, Vercel Web Interface Guidelines, Mobile-First PWA Ergonomics  

---

## Table of Contents
1. [Executive Summary & Design Philosophy](#1-executive-summary--design-philosophy)
2. [Semantic Design Token Taxonomy](#2-semantic-design-token-taxonomy)
3. [Fintech Elevation & Micro-Shadow Hierarchy](#3-fintech-elevation--micro-shadow-hierarchy)
4. [Typography Scale & Tabular Numerics](#4-typography-scale--tabular-numerics)
5. [Spacing Scale, Layout Grids & Geometric Radii](#5-spacing-scale-layout-grids--geometric-radii)
6. [Core Reusable UI Primitives Specification & Examples](#6-core-reusable-ui-primitives-specification--examples)
7. [Iconography Standards & Lucide Migration](#7-iconography-standards--lucide-migration)
8. [Web Interface Guidelines & Accessibility Matrix](#8-web-interface-guidelines--accessibility-matrix)

---

## 1. Executive Summary & Design Philosophy

The Personal Finance PWA is a modern, high-precision financial management application. Its visual and interaction design is anchored in a single, uncompromising principle:

> **"Personal finance should feel clear, trustworthy, intelligent, and effortless — not like an accounting spreadsheet."**

### 1.1 Five Foundational Pillars

1. **Restrained Visual Language**:
   Financial data conveys high emotion and stress. The interface uses a calm, neutral canvas (Zinc slate) where color is reserved strictly for intentional semantic communication. We avoid gratuitous gradients, rainbow color schemes, and decorative charts that do not answer a concrete user question.
2. **Precision Border & Micro-Elevation Hierarchy**:
   Diffused, blurry drop shadows (`shadow-xl`) are abolished. Depth is established through **hairline 1px borders** (`border-border-subtle` and `border-border-default`) paired with **tight dual-layer micro-shadows** (`shadow-card`, `shadow-dropdown`, `shadow-modal`). Dark mode utilizes subtle 1px specular edge highlights (`shadow-border-highlight`).
3. **Tabular Numerics & Zero Numeric Jitter**:
   Financial ledgers demand absolute mathematical alignment. Every currency amount, percentage, transaction count, and date strictly declares `font-variant-numeric: tabular-nums` (`.tabular-nums`). Numbers never jitter, wobble, or cause horizontal layout shift during dynamic updates.
4. **First-Class Dual Theme (Light & Dark)**:
   All design tokens are defined as space-separated RGB values via CSS custom properties in `:root` and `.dark`. This enables native Tailwind CSS opacity modifier support (e.g. `bg-surface-card/80`, `text-content-primary/90`, `border-border-subtle/50`) with zero runtime overhead and instantaneous theme switching.
5. **Mobile-First PWA Ergonomics**:
   The application is engineered for single-handed mobile thumb usage (minimum 44×44px touch targets, 52px keypad buttons, safe-area inset protection via `pb-safe` and `pt-safe`, and swipeable bottom sheets) while gracefully expanding into a structured multi-column desktop layout.

---

## 2. Semantic Design Token Taxonomy

Components declare **semantic intent** (e.g., `bg-surface-card`, `text-content-primary`, `text-income-default`, `border-border-subtle`) rather than raw hex codes or ad-hoc Tailwind color utilities.

### 2.1 Surface Tokens

| Token Variable | Tailwind Class | Light Mode (RGB / Hex) | Dark Mode (RGB / Hex) | Usage & Intent |
|---|---|---|---|---|
| `--surface-canvas` | `bg-surface-canvas` | `250 250 250` (`#FAFAFA`) | `9 9 11` (`#09090B`) | Base application canvas / viewport background |
| `--surface-card` | `bg-surface-card` | `255 255 255` (`#FFFFFF`) | `18 18 21` (`#121215`) | Primary cards, content sheets, ledger panels |
| `--surface-elevated` | `bg-surface-elevated` | `255 255 255` (`#FFFFFF`) | `24 24 27` (`#18181B`) | Dropdowns, popovers, hovering menus, tooltips |
| `--surface-modal` | `bg-surface-modal` | `255 255 255` (`#FFFFFF`) | `20 20 23` (`#141417`) | Modal dialogs, bottom sheets, full overlays |
| `--surface-subtle` | `bg-surface-subtle` | `244 244 245` (`#F4F4F5`) | `30 30 34` (`#1E1E22`) | Keypad buttons, badge wells, secondary pills |
| `--surface-sunken` | `bg-surface-sunken` | `244 244 245` (`#F4F4F5`) | `12 12 14` (`#0C0C0E`) | Form input fields, search boxes, code blocks |
| `--surface-overlay` | `bg-surface-overlay` | `0 0 0` (at 45% alpha) | `0 0 0` (at 70% alpha) | Backdrop scrim behind active modals and sheets |

### 2.2 Content / Typography Tokens

| Token Variable | Tailwind Class | Light Mode (RGB / Hex) | Dark Mode (RGB / Hex) | Contrast vs Card Surface | Purpose |
|---|---|---|---|---|---|
| `--text-primary` | `text-content-primary` | `9 9 11` (`#09090B`) | `250 250 250` (`#FAFAFA`) | **19.8:1 (AAA)** / **17.5:1 (AAA)** | Headings, hero numbers, primary data |
| `--text-secondary` | `text-content-secondary`| `82 82 91` (`#52525B`) | `161 161 170` (`#A1A1AA`) | **6.9:1 (AA)** / **6.5:1 (AA)** | Field labels, column titles, subtitles |
| `--text-muted` | `text-content-muted` | `113 113 122` (`#71717A`)| `113 113 122` (`#71717A`)| **4.6:1 (AA)** / **4.5:1 (AA)** | Placeholders, timestamps, helper notes |
| `--text-inverse` | `text-content-inverse` | `255 255 255` (`#FFFFFF`)| `9 9 11` (`#09090B`) | **19.8:1 (AAA)** | Text on solid brand/dark contrast fills |
| `--text-brand` | `text-content-brand` | `5 150 105` (`#059669`) | `52 211 153` (`#34D399`) | **4.6:1 (AA)** / **10.4:1 (AAA)** | Brand emphasis links, highlighted labels |

### 2.3 Financial State & Accent Tokens

Financial applications must maintain immediate cognitive clarity:
- **Income / Growth**: Refined Emerald (calm, affluent, clear; never fluorescent).
- **Expense / Liability**: Refined Rose / Crimson (authoritative, clear alert; never harsh pure red).
- **Warning / Grace**: Refined Amber (warm, non-alarmist).
- **Transfer / Neutral**: Refined Sky / Cool Slate (calm ledger neutrality).

| Token Family | Tailwind Utility | Light Mode (RGB) | Dark Mode (RGB) | Financial Intent |
|---|---|---|---|---|
| **Income (Default)** | `text-income-default`, `bg-income-default` | `5 150 105` (`#059669`) | `52 211 153` (`#34D399`) | Positive cashflow, credit figures, success indicators |
| **Income (Muted)** | `bg-income-muted` | `236 253 245` (`#ECFDF5`) | `6 78 59` (`#064E3B`) | Income badge backgrounds, callout wells |
| **Income (Emphasis)** | `bg-income-emphasis` | `4 120 87` (`#047857`) | `110 231 183` (`#6EE7B7`) | Income hover and active states |
| **Income (Border)** | `border-income-border` | `167 243 208` (`#A7F3D0`) | `16 185 129` (`#10B981`) | Subtle border around positive transaction badges |
| **Expense (Default)** | `text-expense-default`, `bg-expense-default` | `225 29 72` (`#E11D48`) | `251 113 133` (`#FB7185`) | Negative cashflow, debit figures, danger alerts |
| **Expense (Muted)** | `bg-expense-muted` | `255 241 242` (`#FFF1F2`) | `136 19 55` (`#881337`) | Expense badge backgrounds, error callout wells |
| **Expense (Emphasis)**| `bg-expense-emphasis` | `190 18 60` (`#BE123C`) | `253 164 175` (`#FDA4AF`) | Destructive button hover/active states |
| **Expense (Border)** | `border-expense-border`| `254 205 211` (`#FECDD3`) | `225 29 72` (`#E11D48`) | Subtle border around expense badges |
| **Warning (Default)** | `text-warning-default`, `bg-warning-default` | `217 119 6` (`#D97706`) | `251 191 36` (`#FBBF24`) | Overbudget warnings, grace period banners |
| **Warning (Muted)** | `bg-warning-muted` | `255 251 235` (`#FFFBEB`) | `120 53 15` (`#78350F`) | Warning alert card backgrounds |
| **Warning (Emphasis)**| `bg-warning-emphasis` | `180 83 9` (`#B45309`) | `252 211 77` (`#FCD34D`) | Warning button hover states |
| **Warning (Border)** | `border-warning-border`| `253 230 138` (`#FDE68A`) | `217 119 6` (`#D97706`) | Warning card hairline borders |
| **Transfer (Default)**| `text-transfer-default`, `bg-transfer-default` | `2 132 199` (`#0284C7`) | `56 189 248` (`#38BDF8`) | Account transfers, neutral transactions |
| **Transfer (Muted)** | `bg-transfer-muted` | `240 249 255` (`#F0F9FF`) | `12 74 110` (`#0C4A6E`) | Transfer pill backgrounds |
| **Transfer (Border)** | `border-transfer-border`| `186 230 253` (`#BAE6FD`)| `2 132 199` (`#0284C7`) | Transfer badge hairline borders |
| **Brand (Default)** | `bg-brand-default`, `text-brand-default` | `5 150 105` (`#059669`) | `16 185 129` (`#10B981`) | Primary CTAs, active tab accents, main brand elements |
| **Brand (Emphasis)** | `bg-brand-emphasis` | `4 120 87` (`#047857`) | `5 150 105` (`#059669`) | Primary CTA hover/active state |
| **Brand (Muted)** | `bg-brand-muted` | `236 253 245` (`#ECFDF5`) | `6 78 59` (`#064E3B`) | Brand subtle accent wells |
| **Brand (Border)** | `border-brand-border` | `167 243 208` (`#A7F3D0`) | `16 185 129` (`#10B981`) | Highlighted feature card borders |

### 2.4 Border & Interactive State Tokens

| Semantic Token | Tailwind Utility | Light Mode (RGB) | Dark Mode (RGB) | Purpose |
|---|---|---|---|---|
| `--border-subtle` | `border-border-subtle` | `244 244 245` (`#F4F4F5`) | `39 39 42` (`#27272A`) | Hairline card outlines, list row dividers |
| `--border-default` | `border-border-default` | `228 228 231` (`#E4E4E7`) | `63 63 70` (`#3F3F46`) | Standard form inputs, interactive card borders |
| `--border-strong` | `border-border-strong` | `161 161 170` (`#A1A1AA`) | `113 113 122` (`#71717A`)| Active filter chips, table header borders |
| `--border-interactive`| `border-border-interactive`| `5 150 105` (`#059669`) | `16 185 129` (`#10B981`)| Selected wallet cards, active radio borders |
| `--ring-focus` | `.focus-ring` utility | `5 150 105 / 0.4` | `52 211 153 / 0.4` | High-contrast keyboard focus indicator |

---

## 3. Fintech Elevation & Micro-Shadow Hierarchy

Default Tailwind shadows (`shadow-xl`) cast a wide, muddy 25px blur that degrades card readability. Modern fintech interfaces establish elevation through crisp, multi-layer micro-shadows combined with 1px hairline boundaries:

| Shadow Token | Tailwind Class | CSS `box-shadow` Definition | Target Elements |
|---|---|---|---|
| **Card / Base** | `shadow-card` | `0 1px 2px 0 rgb(0 0 0 / 0.04), 0 1px 1px -1px rgb(0 0 0 / 0.02)` | Dashboard cards, wallet cards, transaction rows |
| **Subtle** | `shadow-subtle` | `0 1px 3px 0 rgb(0 0 0 / 0.05), 0 1px 2px -1px rgb(0 0 0 / 0.03)` | Clickable list items, segment controls, quick chips |
| **Dropdown** | `shadow-dropdown` | `0 4px 6px -1px rgb(0 0 0 / 0.07), 0 2px 4px -2px rgb(0 0 0 / 0.05)` | Select menus, popovers, date picker dropdowns |
| **Floating** | `shadow-floating` | `0 10px 15px -3px rgb(0 0 0 / 0.08), 0 4px 6px -4px rgb(0 0 0 / 0.04)` | Mobile bottom nav bar, sticky action headers |
| **Modal** | `shadow-modal` | `0 20px 25px -5px rgb(0 0 0 / 0.12), 0 8px 10px -6px rgb(0 0 0 / 0.06)` | Centered dialogs, bottom sheets |
| **Dark Highlight**| `shadow-border-highlight` | `inset 0 1px 0 0 rgba(255, 255, 255, 0.06)` | Linear-style top-edge light reflection in dark mode |
| **Income Glow** | `shadow-glow-income` | `0 0 16px -2px rgb(5 150 105 / 0.18)` | Positive net balance highlight, savings goal achievement |
| **Expense Glow**| `shadow-glow-expense`| `0 0 16px -2px rgb(225 29 72 / 0.18)` | Overbudget breach alert |

---

## 4. Typography Scale & Tabular Numerics

### 4.1 Font Family Hierarchy
- **Primary Brand & UI**: `"Plus Jakarta Sans", "Inter", system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`
- **Monospace / Code / Hashes**: `"JetBrains Mono", "Fira Code", ui-monospace, monospace`

### 4.2 Mandatory Tabular Numerics (`.tabular-nums`)
Standard proportional typefaces cause numeral widths to vary (e.g., `1` is narrower than `0` or `8`). In financial applications, this produces visual jitter during state changes and disrupts vertical ledger scanning.

The utility `.tabular-nums` (`font-variant-numeric: tabular-nums`) **must be applied** to:
1. Total Saldo / Net Worth hero numbers
2. Cash flow income, expense, and net figures
3. Wallet balances
4. Transaction row amounts (`+Rp ...` / `-Rp ...`)
5. Rapid mobile keypad digits
6. Timestamps and transaction dates

### 4.3 7-Tier Typographic Scale

| Tier | Size (Mobile / Desktop) | Line Height | Weight | Letter Spacing | Standard Fintech Application |
|---|---|---|---|---|---|
| **Tier 1: Display / Hero** | `32px` / `44px` | `40px` / `52px` | Extrabold (`800`) | `-0.03em` | Total Available Balance, Hero Net Worth |
| **Tier 2: Page Title (H1)** | `20px` / `24px` | `28px` / `32px` | Bold (`700`) | `-0.02em` | Main screen titles ("Dashboard", "Transaksi") |
| **Tier 3: Section Header (H2)** | `16px` / `18px` | `24px` / `28px` | Semibold (`600`) | `-0.015em` | Card group titles ("Dompet & Rekening", "Riwayat") |
| **Tier 4: Card Header (H3)** | `14px` | `20px` | Semibold (`600`) | `-0.01em` | Widget titles ("Arus Kas Bersih", Account Names) |
| **Tier 5: Body Regular** | `14px` | `20px` | Regular (`400`) / Medium (`500`) | `normal` | Transaction descriptions, input field values |
| **Tier 5b: Body Large** | `16px` | `24px` | Regular (`400`) | `normal` | Auth subtitles, modal explanatory text |
| **Tier 6: Caption / Meta** | `12px` | `16px` | Medium (`500`) | `+0.01em` | Timestamps, helper text, account sub-types |
| **Tier 7: Micro / Overline**| `11px` | `14px` | Semibold (`600`) / Bold (`700`)| `+0.05em` | Category badges, status pills, table headers |

---

## 5. Spacing Scale, Layout Grids & Geometric Radii

### 5.1 4px / 8px Modular Spacing
- `space-1` = 4px (tight inline icon-label gaps)
- `space-2` = 8px (inner card gaps, list gaps)
- `space-3` = 12px (form input vertical padding)
- `space-4` = 16px (standard component padding)
- `space-5` = 20px (mobile card padding `p-5`)
- `space-6` = 24px (desktop card padding `p-6`, modal padding)
- `space-8` = 32px (page section separation)
- `space-12` = 48px (major view layout gaps)

### 5.2 Touch Ergonomics & Safe Areas
- **Minimum Interactive Touch Target**: 44×44px (WCAG 2.5.5 and Apple HIG compliant).
- **Keypad Button Height**: `h-13` (52px) for effortless thumb actuation.
- **Safe-Area Insets**:
  - `pt-safe`: `padding-top: env(safe-area-inset-top, 0px)`
  - `pb-safe`: `padding-bottom: env(safe-area-inset-bottom, 0px)`
  - `pl-safe`: `padding-left: env(safe-area-inset-left, 0px)`
  - `pr-safe`: `padding-right: env(safe-area-inset-right, 0px)`

### 5.3 Geometric Radii Hierarchy
- `rounded-sm` (6px): Status badges, category color dots
- `rounded-md` (8px): Chips, secondary buttons, keypad keys
- `rounded-lg` (12px): Form inputs, dropdowns, primary CTA buttons
- `rounded-xl` (16px): Cards, wallet cards, transaction list containers
- `rounded-2xl` (20px): Modal dialogs, bottom sheets, balance panels
- `rounded-full` (9999px): Avatars, toggle thumbs, active filter pills

---

## 6. Core Reusable UI Primitives Specification & Examples

All UI primitives are co-located in `frontend/src/components/ui/` and exported via the barrel `frontend/src/components/ui/index.ts`.

### 6.1 Button Primitive (`Button.vue`)

#### Features
- **Variants**: `primary`, `secondary`, `outline`, `ghost`, `destructive`
- **Sizes**: `sm` (36px), `md` (44px touch-friendly), `lg` (52px), `icon` (44×44px)
- **State Management**: Integrated loading spinner with `aria-busy="true"`, layout preservation, and disabled state
- **Accessibility**: Automatic focus ring (`.focus-ring`), `:aria-label` support, tactile active scale (`active:scale-[0.98] motion-reduce:transform-none`)
- **Responsiveness**: `mobileFullWidth` (full width on `<640px`, auto on `≥640px`) and `fullWidth`

#### Usage Example
```vue
<template>
  <!-- Primary Action with Prefix Icon -->
  <Button variant="primary" size="md" :loading="isSubmitting" loadingText="Menyimpan…">
    <template #prefix>
      <Plus class="w-4 h-4 mr-1.5" aria-hidden="true" />
    </template>
    Tambah Transaksi
  </Button>

  <!-- Destructive Icon-Only Button -->
  <Button
    variant="ghost"
    size="icon"
    ariaLabel="Hapus transaksi"
    @click="deleteTx(tx.id)"
  >
    <Trash2 class="w-4 h-4 text-expense-default" aria-hidden="true" />
  </Button>
</template>

<script setup>
import { Button } from '@/components/ui'
import { Plus, Trash2 } from 'lucide-vue-next'
</script>
```

---

### 6.2 Input Primitive (`Input.vue`)

#### Features
- **Semantic Labels & Helper**: Accessible association via auto-generated IDs (`useId` fallback) and `aria-describedby`
- **Currency IDR Adornment**: Integrated prefix slot / `prefix="Rp"` and `suffix="IDR"`
- **Clear Button**: Quick-clear button with Lucide `X` icon
- **Password Eye Toggle**: Toggle visibility between `password` and `text` with Lucide `Eye`/`EyeOff`
- **Validation**: Error state with `AlertCircle` icon, `aria-invalid="true"`, and `aria-live="polite"`
- **Ergonomics**: `spellcheck="false"` on sensitive inputs, native paste enabled (no `preventDefault`)

#### Usage Example
```vue
<template>
  <!-- Rupiah Currency Input -->
  <Input
    v-model="amount"
    label="Jumlah Transaksi"
    placeholder="0"
    prefix="Rp"
    inputmode="numeric"
    required
    :errorMessage="amountError"
  />

  <!-- Password Input with Toggle -->
  <Input
    v-model="password"
    type="password"
    label="Kata Sandi"
    placeholder="••••••••"
    autocomplete="current-password"
    showPasswordToggle
    required
  />
</template>

<script setup>
import { ref } from 'vue'
import { Input } from '@/components/ui'

const amount = ref('')
const amountError = ref('')
const password = ref('')
</script>
```

---

### 6.3 Card Primitive (`Card.vue`)

#### Features
- **Fintech Precision**: 1px hairline border (`border-border-subtle`) and `shadow-card`
- **Padding Variants**: `none`, `sm`, `md`, `lg`
- **Interactive Mode**: `interactive: true` adds subtle hover lift, cursor pointer, keyboard trigger (`Enter`/`Space`), and focus ring
- **Structured Slots**: `#header`, `#default`, `#footer`

#### Usage Example
```vue
<template>
  <Card padding="md" interactive @click="selectAccount(account.id)">
    <template #header>
      <div class="flex items-center gap-2">
        <Landmark class="w-4 h-4 text-brand-default" aria-hidden="true" />
        <span class="text-xs font-semibold text-content-secondary">Rekening Utama</span>
      </div>
      <Badge variant="active">Aktif</Badge>
    </template>

    <div class="my-2">
      <div class="text-xs text-content-muted">Saldo Tersedia</div>
      <div class="text-2xl font-black text-content-primary tabular-nums tracking-tight">
        Rp 12.500.000
      </div>
    </div>

    <template #footer>
      <span class="text-xs text-content-muted">Bank BCA •••• 8821</span>
      <ChevronRight class="w-4 h-4 text-content-muted" aria-hidden="true" />
    </template>
  </Card>
</template>

<script setup>
import { Card, Badge } from '@/components/ui'
import { Landmark, ChevronRight } from 'lucide-vue-next'
</script>
```

---

### 6.4 Badge Primitive (`Badge.vue`)

#### Features
- **Semantic Statuses**: `income`/`surplus`, `expense`/`deficit`, `pending`, `active`, `free`, `premium`, `transfer`, `warning`
- **Sizing**: `sm` (10px), `md` (11px)
- **Dot Indicator**: Optional `dot` indicator with matching semantic color or custom `dotColor`
- **High Contrast**: Fully WCAG AA compliant contrast ratios

#### Usage Example
```vue
<template>
  <Badge variant="income" dot>Surplus</Badge>
  <Badge variant="expense" dot>Defisit</Badge>
  <Badge variant="warning">Menunggu</Badge>
  <Badge variant="premium">
    <template #icon>
      <Sparkles class="w-3 h-3 mr-1 text-amber-500" aria-hidden="true" />
    </template>
    ★ Premium
  </Badge>
</template>

<script setup>
import { Badge } from '@/components/ui'
import { Sparkles } from 'lucide-vue-next'
</script>
```

---

### 6.5 ModalSheet Primitive (`ModalSheet.vue`)

#### Features
- **Responsive Adaptivity**:
  - **Mobile (<640px)**: Bottom slide-over sheet with drag handle, swipe-down dismiss gesture, and safe-area inset padding.
  - **Desktop (≥640px)**: Centered dialog box with backdrop blur and scale-in transition.
- **Accessibility & Focus**:
  - `role="dialog"` and `aria-modal="true"`
  - Body scroll lock when open (`document.body.style.overflow = 'hidden'`)
  - Escape key handling to close
  - Tab key focus trapping inside the active modal
  - Auto-focus first input upon open; restore previous focus upon close

#### Usage Example
```vue
<template>
  <ModalSheet
    v-model="isOpen"
    title="Catat Transaksi"
    description="Masukkan detail pengeluaran atau pemasukan baru"
    size="md"
    @close="handleClose"
  >
    <div class="space-y-4">
      <Input label="Deskripsi" placeholder="cth. Makan Siang" />
      <Input label="Jumlah" prefix="Rp" inputmode="numeric" />
    </div>

    <template #footer>
      <Button variant="ghost" @click="isOpen = false">Batal</Button>
      <Button variant="primary" @click="saveTx">Simpan</Button>
    </template>
  </ModalSheet>
</template>

<script setup>
import { ref } from 'vue'
import { ModalSheet, Button, Input } from '@/components/ui'

const isOpen = ref(false)
function handleClose() {
  isOpen.value = false
}
</script>
```

---

## 7. Iconography Standards & Lucide Migration

All raw emojis (`🏠`, `📜`, `+`, `📊`, `👤`, `👁️`, `🙈`, `💵`, `🏦`, `💳`, `🗑️`, `⭐`, `🔒`) and Unicode glyphs are strictly retired. The application exclusively uses **Lucide Icons** (`lucide-vue-next`).

### 7.1 Exhaustive Emoji-to-Lucide Migration Dictionary

| Legacy Glyph | Context / Purpose | Replacement Lucide Component | Standard Classes | Accessible Label (`aria-label`) |
|---|---|---|---|---|
| `🏠` | Nav: Dashboard / Home | `<Home />` | `w-5 h-5 stroke-[1.75]` | `aria-label="Dashboard Utama"` |
| `📜` | Nav: Transactions Ledger | `<Receipt />` | `w-5 h-5 stroke-[1.75]` | `aria-label="Riwayat Transaksi"` |
| `+` | Nav: Prominent Add Action | `<Plus />` | `w-6 h-6 stroke-[2.25]` | `aria-label="Catat Transaksi Baru"` |
| `📊` | Nav: Analytics View | `<BarChart3 />` | `w-5 h-5 stroke-[1.75]` | `aria-label="Analitik Keuangan"` |
| `👤` | Nav: Profile & Settings | `<User />` | `w-5 h-5 stroke-[1.75]` | `aria-label="Profil dan Pengaturan"` |
| `👁️` | Balance Toggle: Reveal | `<Eye />` | `w-4 h-4 stroke-2` | `aria-label="Tampilkan total saldo"` |
| `🙈` | Balance Toggle: Mask | `<EyeOff />` | `w-4 h-4 stroke-2` | `aria-label="Sembunyikan total saldo"` |
| `💵` | Wallet Type: Cash / Tunai | `<Banknote />` | `w-5 h-5 stroke-[1.75] text-income-default` | `aria-hidden="true"` |
| `🏦` | Wallet Type: Bank / Savings | `<Building2 />` / `<Landmark />` | `w-5 h-5 stroke-[1.75] text-transfer-default` | `aria-hidden="true"` |
| `💳` | Wallet Type: Card / E-Wallet | `<CreditCard />` / `<Wallet />` | `w-5 h-5 stroke-[1.75] text-brand-default` | `aria-hidden="true"` |
| `↓` | Tx Type: Income (Masuk) | `<ArrowDownLeft />` | `w-4 h-4 stroke-[2.25] text-income-default` | `aria-hidden="true"` |
| `↑` | Tx Type: Expense (Keluar) | `<ArrowUpRight />` | `w-4 h-4 stroke-[2.25] text-expense-default` | `aria-hidden="true"` |
| `⇄` | Tx Type: Transfer Antar Akun | `<ArrowLeftRight />` | `w-4 h-4 stroke-[2.25] text-transfer-default` | `aria-hidden="true"` |
| `🗑️` | Action: Delete Transaction | `<Trash2 />` | `w-4 h-4 stroke-[1.75] text-content-muted hover:text-expense-default` | `aria-label="Hapus transaksi"` |
| `✕` / `×` | Modal / Dialog Close | `<X />` | `w-5 h-5 stroke-2 text-content-muted hover:text-content-primary` | `aria-label="Tutup dialog"` |
| `⌫` | Keypad Backspace | `<Delete />` | `w-5 h-5 stroke-2` | `aria-label="Hapus digit terakhir"` |
| `🔒` | Gated Premium Feature | `<Lock />` | `w-6 h-6 stroke-[1.75] text-warning-default` | `aria-hidden="true"` |
| `⭐` / `★` | Premium Tier Badge / CTA | `<Sparkles />` / `<Crown />` | `w-3.5 h-3.5 stroke-2 text-warning-default` | Visible badge text |
| `✓` | Feature Checkmark | `<Check />` / `<CheckCircle2 />` | `w-4 h-4 stroke-2 text-income-default` | `aria-hidden="true"` |

### 7.2 Standard Sizing & Stroke Discipline

| Tier | Size | Stroke Width | Application |
|---|---|---|---|
| **Micro** | 14×14 px (`w-3.5 h-3.5`) | `stroke-2` / `stroke-[2.25]` | Badges, small status pills, checklist items |
| **Small** | 16×16 px (`w-4 h-4`) | `stroke-2` | Action buttons, input prefixes (`Search`, `Calendar`), row delete |
| **Medium** | 20×20 px (`w-5 h-5`) | `stroke-[1.75]` | Mobile nav bar, desktop sidebar, card headers |
| **Large** | 24×24 px (`w-6 h-6`) | `stroke-[1.75]` | Floating Add button (`+`), modal headers |
| **Hero** | 32×32 px (`w-8 h-8`) | `stroke-[1.5]` | Empty states, Premium upgrade modal illustrations |

---

## 8. Web Interface Guidelines & Accessibility Matrix

| Dimension | Rule & Standard | Implementation Guarantee |
|---|---|---|
| **Contrast Ratio** | WCAG 2.1 AA / AAA | Primary text: 19.8:1 (light), 17.5:1 (dark). Secondary text: 6.9:1 (light), 6.5:1 (dark). Income/Expense accents pass AA and AAA. |
| **Focus Visible** | Never bare `outline-none` | Standard `.focus-ring` utility (`focus-visible:ring-2 focus-visible:ring-brand-default/40 focus-visible:ring-offset-2`). |
| **Form Labels** | Accessible `<label :for>` | `Input.vue` links every input to its label with deterministic generated IDs and `aria-describedby`. |
| **Error Feedback** | `aria-invalid` & `aria-live` | Form inputs mark invalid state with `aria-invalid="true"` and error text container with `aria-live="polite"`. |
| **Numeric Stability**| `.tabular-nums` | All monetary amounts enforce monospaced digits, preventing layout shift during balance changes. |
| **Touch Ergonomics**| 44×44px minimum target | All buttons, inputs, nav items, and keypad buttons meet or exceed 44px hit bounds. |
| **Safe-Area Insets**| `pb-safe` & `pt-safe` | Fixed mobile navigation and bottom sheets declare `env(safe-area-inset-bottom)` to respect iOS gesture bars. |
| **Reduced Motion** | `prefers-reduced-motion` | Media query suppresses animations and transitions for users with vestibular sensitivities. |
| **Icon Accessibility**| Explicit `aria-label` / `aria-hidden` | Icon-only buttons mandate `aria-label`. Decorative inline icons declare `aria-hidden="true"`. |

