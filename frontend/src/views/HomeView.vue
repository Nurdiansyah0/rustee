<template>
  <div class="space-y-5 lg:space-y-6">
    <!-- Initial Fetching Skeleton: Calibrated to match actual component geometry -->
    <div
      v-if="walletStore.loading && !computedAccounts.length"
      class="grid grid-cols-1 md:grid-cols-12 gap-5 lg:gap-6 animate-pulse"
      aria-busy="true"
      aria-label="Memuat data dashboard..."
    >
      <div class="md:col-span-7 space-y-4 sm:space-y-5">
        <div class="h-28 bg-surface-card rounded-xl border border-border-subtle"></div>
        <div class="h-32 bg-surface-card rounded-xl border border-border-subtle"></div>
      </div>
      <div class="md:col-span-5 space-y-4 sm:space-y-5">
        <div class="h-36 bg-surface-card rounded-xl border border-border-subtle"></div>
        <div class="h-44 bg-surface-card rounded-xl border border-border-subtle"></div>
      </div>
    </div>

    <!-- Core Financial Dashboard: Dual-Column Responsive Grid (Desktop/Tablet) | Single Stream (Mobile) -->
    <div v-else class="space-y-5 lg:space-y-6">
      <!-- Business Workspace Operational Command Banner -->
      <div
        v-if="workspaceStore.isBusinessWorkspace"
        class="p-4 sm:p-5 rounded-2xl bg-gradient-to-r from-brand-default/15 via-brand-default/10 to-surface-card border border-brand-default/30 shadow-card flex flex-col sm:flex-row sm:items-center justify-between gap-4"
      >
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-brand-default text-white flex items-center justify-center shrink-0 shadow-md">
            <Building2 class="w-5 h-5 stroke-[2.5]" />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-base sm:text-lg font-black text-content-primary">
                {{ workspaceStore.activeTenantName }}
              </h2>
              <span class="text-[10px] font-extrabold uppercase px-2 py-0.5 rounded-full bg-brand-default/20 text-brand-default border border-brand-default/30">
                {{ workspaceStore.businessType || 'Bisnis' }}
              </span>
            </div>
            <p class="text-xs text-content-secondary mt-0.5">
              Ruang Kerja Operasional Terpadu • Peran: <span class="capitalize font-semibold text-content-primary">{{ workspaceStore.activeRole }}</span>
            </p>
          </div>
        </div>

        <!-- Quick Action Modules -->
        <div class="flex items-center flex-wrap gap-2">
          <button
            v-if="workspaceStore.hasCapability('projects')"
            type="button"
            @click="$emit('nav', 'projects')"
            class="px-3 py-1.5 rounded-xl bg-surface-card hover:bg-surface-subtle border border-border-default text-xs font-bold text-content-primary flex items-center gap-1.5 transition cursor-pointer"
          >
            <FolderKanban class="w-3.5 h-3.5 text-brand-default" />
            <span>Proyek</span>
          </button>
          <button
            v-if="workspaceStore.hasCapability('invoicing')"
            type="button"
            @click="$emit('nav', 'invoices')"
            class="px-3 py-1.5 rounded-xl bg-surface-card hover:bg-surface-subtle border border-border-default text-xs font-bold text-content-primary flex items-center gap-1.5 transition cursor-pointer"
          >
            <FileText class="w-3.5 h-3.5 text-brand-default" />
            <span>Faktur</span>
          </button>
          <button
            v-if="workspaceStore.hasCapability('inventory')"
            type="button"
            @click="$emit('nav', 'inventory')"
            class="px-3 py-1.5 rounded-xl bg-surface-card hover:bg-surface-subtle border border-border-default text-xs font-bold text-content-primary flex items-center gap-1.5 transition cursor-pointer"
          >
            <Package class="w-3.5 h-3.5 text-brand-default" />
            <span>Stok</span>
          </button>
          <button
            v-if="workspaceStore.hasCapability('purchasing')"
            type="button"
            @click="$emit('nav', 'purchasing')"
            class="px-3 py-1.5 rounded-xl bg-surface-card hover:bg-surface-subtle border border-border-default text-xs font-bold text-content-primary flex items-center gap-1.5 transition cursor-pointer"
          >
            <ShoppingCart class="w-3.5 h-3.5 text-brand-default" />
            <span>Pesanan</span>
          </button>
          <button
            v-if="workspaceStore.hasCapability('accounting')"
            type="button"
            @click="$emit('nav', 'accounting')"
            class="px-3 py-1.5 rounded-xl bg-surface-card hover:bg-surface-subtle border border-border-default text-xs font-bold text-content-primary flex items-center gap-1.5 transition cursor-pointer"
          >
            <BookOpen class="w-3.5 h-3.5 text-brand-default" />
            <span>Buku Kas</span>
          </button>
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-12 gap-5 lg:gap-6 items-start">
        <!-- LEFT COLUMN: Asset Command Center -->
        <div class="md:col-span-7 space-y-4 sm:space-y-5 min-w-0">

          <!-- 1. Hero Metric: Total Saldo (Pure and Authoritative, Zero Redundant Context) -->
          <section
            class="bg-surface-card border border-border-subtle shadow-card rounded-xl p-5 sm:p-6 relative overflow-hidden"
            aria-labelledby="hero-balance-heading"
          >
            <div class="flex items-center justify-between gap-4">
              <div class="flex items-center gap-2">
                <span class="w-2 h-2 rounded-full bg-brand-default" aria-hidden="true"></span>
                <h2 id="hero-balance-heading" class="text-xs font-semibold uppercase tracking-wider text-content-muted">
                  {{ workspaceStore.isBusinessWorkspace ? 'Kas & Bank Operasional Usaha' : 'Total Saldo' }}
                </h2>
              </div>

              <!-- Global Balance Visibility Toggle (WCAG 44x44px target) -->
              <button
                type="button"
                @click="toggleHide"
                :aria-label="isHideBalance ? 'Tampilkan saldo' : 'Sembunyikan saldo'"
                :aria-pressed="isHideBalance"
                class="min-w-[44px] min-h-[44px] px-3 py-1.5 rounded-lg bg-surface-subtle hover:bg-surface-card border border-border-subtle hover:border-border-default text-content-secondary hover:text-content-primary text-xs font-semibold transition focus-ring flex items-center gap-2 cursor-pointer select-none"
              >
                <component :is="isHideBalance ? Eye : EyeOff" class="w-4 h-4 shrink-0 text-content-muted" aria-hidden="true" />
                <span>{{ isHideBalance ? 'Tampilkan' : 'Sembunyikan' }}</span>
              </button>
            </div>

            <!-- Primary Financial Value -->
            <div class="mt-3 sm:mt-4">
              <div
                v-if="!isHideBalance"
                class="text-3xl sm:text-4xl lg:text-5xl font-extrabold text-content-primary tracking-tight tabular-nums truncate"
              >
                {{ formatIDR(computedTotalBalance) }}
              </div>
              <div
                v-else
                class="text-3xl sm:text-4xl lg:text-5xl font-extrabold text-content-muted tracking-tight font-mono truncate"
              >
                Rp ••••••••
              </div>
            </div>
          </section>

          <!-- 2. Dompet & Rekening (High-Density Account Rows, Zero Redundant Badges) -->
          <section class="min-w-0" aria-labelledby="wallet-section-heading">
            <div class="flex items-center gap-2 mb-2.5 px-0.5">
              <Wallet class="w-4 h-4 text-brand-default" aria-hidden="true" />
              <h3 id="wallet-section-heading" class="text-sm font-bold text-content-primary">
                {{ workspaceStore.isBusinessWorkspace ? 'Kas & Rekening Operasional Usaha' : 'Dompet & Rekening' }}
              </h3>
            </div>

            <!-- Unified Account Deck (Continuous Surface with Hairline Dividers) -->
            <div
              v-if="computedAccounts.length"
              class="bg-surface-card rounded-xl border border-border-subtle shadow-card divide-y divide-border-subtle overflow-hidden"
            >
              <div
                v-for="acc in computedAccounts"
                :key="acc.id"
                class="flex items-center justify-between px-3.5 sm:px-4 py-2.5 hover:bg-surface-subtle/50 transition min-w-0"
              >
                <div class="flex items-center gap-2.5 min-w-0">
                  <div class="w-7 h-7 rounded-lg bg-surface-subtle text-brand-default flex items-center justify-center shrink-0" aria-hidden="true">
                    <component :is="getAccountIcon(acc.account_type)" class="w-3.5 h-3.5 stroke-[2]" />
                  </div>
                  <span class="text-xs sm:text-sm font-medium text-content-primary truncate" :title="acc.name">
                    {{ acc.name }}
                  </span>
                </div>
                <span class="text-xs sm:text-sm font-bold text-content-primary tabular-nums shrink-0 ml-3">
                  {{ isHideBalance ? '••••••••' : formatIDR(acc.current_balance || acc.balance || 0) }}
                </span>
              </div>
            </div>

            <!-- Empty State -->
            <div
              v-else
              class="p-5 text-center bg-surface-card rounded-xl border border-dashed border-border-default"
            >
              <Wallet class="w-6 h-6 text-content-muted mx-auto mb-1.5 opacity-40" aria-hidden="true" />
              <div class="text-xs font-semibold text-content-primary">Belum ada akun terdaftar</div>
              <p class="text-[11px] text-content-secondary mt-0.5 max-w-xs mx-auto">
                Akun kas/bank disiapkan otomatis saat setup ruang kerja aktif.
              </p>
            </div>
          </section>

        </div>

        <!-- RIGHT COLUMN: Cash Flow & Activity Stream -->
        <div class="md:col-span-5 space-y-4 sm:space-y-5 min-w-0">

        <!-- 3. Arus Kas Bulan Ini (Single Definitive Source for Cash Flow) -->
        <section
          class="p-5 bg-surface-card rounded-xl border border-border-subtle shadow-card min-w-0"
          aria-labelledby="cash-flow-heading"
        >
          <div class="flex items-center justify-between gap-2 mb-3">
            <div class="flex items-center gap-2 min-w-0">
              <component
                :is="computedCashFlow.net_cash_flow >= 0 ? TrendingUp : TrendingDown"
                :class="[
                  'w-4 h-4 shrink-0',
                  computedCashFlow.net_cash_flow >= 0 ? 'text-income-default' : 'text-expense-default'
                ]"
                aria-hidden="true"
              />
              <div>
                <h3 id="cash-flow-heading" class="text-sm font-bold text-content-primary truncate">
                  Arus Kas Bulan Ini
                </h3>
                <div v-if="computedCashFlow.period_label" class="text-[10px] text-content-muted font-medium">
                  {{ computedCashFlow.period_label }}
                </div>
              </div>
            </div>

            <!-- Surplus / Defisit Status Pill -->
            <div
              :class="[
                'px-2 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wider shrink-0 border',
                computedCashFlow.net_cash_flow >= 0
                  ? 'bg-income-muted text-income-default border-income-border'
                  : 'bg-expense-muted text-expense-default border-expense-border'
              ]"
            >
              {{ computedCashFlow.net_cash_flow >= 0 ? 'Surplus' : 'Defisit' }}
            </div>
          </div>

          <!-- Primary Net Cash Flow Display -->
          <div class="flex items-baseline gap-2">
            <div
              :class="[
                'text-2xl sm:text-3xl font-extrabold tracking-tight tabular-nums truncate',
                computedCashFlow.net_cash_flow >= 0 ? 'text-income-default' : 'text-expense-default'
              ]"
            >
              {{ isHideBalance ? '••••••••' : formatSignedIDR(computedCashFlow.net_cash_flow) }}
            </div>
          </div>

          <!-- Supporting Context: Subdued Income vs Expense Breakdown with Volume Counts -->
          <div class="mt-4 pt-3.5 border-t border-border-subtle grid grid-cols-2 gap-3 text-xs">
            <div class="min-w-0">
              <div class="flex items-center gap-1.5 text-content-muted">
                <span class="w-1.5 h-1.5 rounded-full bg-income-default shrink-0" aria-hidden="true"></span>
                <span class="truncate">{{ incomeTitle }}</span>
                <span v-if="computedCashFlow.income_count !== undefined" class="text-[10px] text-content-muted tabular-nums">({{ computedCashFlow.income_count }})</span>
              </div>
              <div class="font-semibold text-content-primary mt-0.5 tabular-nums truncate">
                {{ isHideBalance ? '••••••••' : formatIDR(computedCashFlow.total_income) }}
              </div>
            </div>
            <div class="min-w-0">
              <div class="flex items-center gap-1.5 text-content-muted">
                <span class="w-1.5 h-1.5 rounded-full bg-expense-default shrink-0" aria-hidden="true"></span>
                <span class="truncate">{{ expenseTitle }}</span>
                <span v-if="computedCashFlow.expense_count !== undefined" class="text-[10px] text-content-muted tabular-nums">({{ computedCashFlow.expense_count }})</span>
              </div>
              <div class="font-semibold text-content-primary mt-0.5 tabular-nums truncate">
                {{ isHideBalance ? '••••••••' : formatIDR(computedCashFlow.total_expenses) }}
              </div>
            </div>
          </div>
        </section>

        <!-- 4. Transaksi Terbaru (High-Density Ledger Preview) -->
        <section
          class="bg-surface-card rounded-xl border border-border-subtle shadow-card p-4 sm:p-5 min-w-0"
          aria-labelledby="recent-tx-heading"
        >
          <div class="flex items-center justify-between mb-2 px-0.5">
            <div class="flex items-center gap-2">
              <Receipt class="w-4 h-4 text-brand-default" aria-hidden="true" />
              <h3 id="recent-tx-heading" class="text-sm font-bold text-content-primary">
                Transaksi Terbaru
              </h3>
            </div>
            <button
              type="button"
              @click="$emit('nav', 'transactions')"
              class="min-h-[44px] inline-flex items-center text-xs font-semibold text-brand-default hover:text-brand-emphasis hover:underline transition focus-ring rounded-lg px-2 cursor-pointer"
              aria-label="Lihat seluruh riwayat transaksi"
            >
              Lihat semua
            </button>
          </div>

          <!-- Transactions List: Clean Hairline Dividers, Compact Ledger Preview -->
          <div v-if="computedRecentTransactions.length" class="divide-y divide-border-subtle">
            <div
              v-for="tx in computedRecentTransactions.slice(0, 4)"
              :key="tx.id"
              class="flex items-center justify-between py-2 px-1 hover:bg-surface-subtle/50 rounded-lg transition min-w-0"
            >
              <div class="flex items-center gap-2 min-w-0">
                <div
                  :class="[
                    'w-7 h-7 rounded-md flex items-center justify-center font-bold text-xs shrink-0',
                    tx.transaction_type === 'income' ? 'bg-income-muted text-income-default' :
                    tx.transaction_type === 'expense' ? 'bg-expense-muted text-expense-default' : 'bg-transfer-muted text-transfer-default'
                  ]"
                  aria-hidden="true"
                >
                  <component
                    :is="tx.transaction_type === 'income' ? ArrowDownLeft : tx.transaction_type === 'expense' ? ArrowUpRight : ArrowLeftRight"
                    class="w-3.5 h-3.5 stroke-[2]"
                  />
                </div>
                <div class="min-w-0">
                  <div class="text-xs font-medium text-content-primary truncate" :title="tx.description">
                    {{ tx.description || 'Transaksi' }}
                  </div>
                  <div class="text-[10px] text-content-muted mt-0.5">
                    {{ formatDate(tx) }}
                  </div>
                </div>
              </div>

              <div
                :class="[
                  'text-xs sm:text-sm font-bold tabular-nums shrink-0 ml-3',
                  tx.transaction_type === 'income' ? 'text-income-default' :
                  tx.transaction_type === 'expense' ? 'text-expense-default' : 'text-content-primary'
                ]"
              >
                {{ isHideBalance ? '••••••' : (tx.transaction_type === 'income' ? '+' : tx.transaction_type === 'expense' ? '-' : '') + ' ' + formatIDR(tx.amount) }}
              </div>
            </div>
          </div>

          <!-- Empty State -->
          <div v-else class="py-6 text-center">
            <Receipt class="w-6 h-6 text-content-muted mx-auto mb-1.5 opacity-40" aria-hidden="true" />
            <div class="text-xs font-medium text-content-secondary">Belum ada transaksi bulan ini</div>
            <button
              type="button"
              @click="$emit('open-add')"
              class="mt-2.5 min-h-[44px] inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-brand-default text-white text-xs font-bold hover:bg-brand-emphasis focus-ring transition cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" aria-hidden="true" />
              <span>Catat transaksi</span>
            </button>
          </div>
        </section>

        </div>

      </div><!-- end inner grid -->

    </div><!-- end outer wrapper -->
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { formatIDR } from '@/utils/currency'
import { formatFinancialDate } from '@/utils/datetime'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useAnalyticsStore } from '@/stores/analytics'
import { useWorkspaceStore } from '@/stores/workspace'
import {
  Wallet,
  Building2,
  CreditCard,
  Smartphone,
  Eye,
  EyeOff,
  TrendingUp,
  TrendingDown,
  ArrowDownLeft,
  ArrowUpRight,
  ArrowLeftRight,
  Receipt,
  Plus,
  FolderKanban,
  FileText,
  Package,
  ShoppingCart,
  BookOpen
} from 'lucide-vue-next'

const props = defineProps({
  dashboard: {
    type: Object,
    default: () => ({})
  }
})

defineEmits(['nav', 'open-add'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const analyticsStore = useAnalyticsStore()
const workspaceStore = useWorkspaceStore()

const incomeTitle = computed(() => authStore.incomeTitle || 'Pemasukan')
const expenseTitle = computed(() => authStore.expenseTitle || 'Pengeluaran')

const isHideBalance = computed(() => walletStore.hideBalance)

function toggleHide() {
  walletStore.toggleHideBalance()
}

const computedTotalBalance = computed(() => {
  if (walletStore.wallets.length > 0) {
    return walletStore.totalBalance
  }
  return props.dashboard.total_balance || 0
})

const computedCashFlow = computed(() => {
  if (analyticsStore.dashboardData?.cash_flow) {
    return analyticsStore.dashboardData.cash_flow
  }
  return props.dashboard.cash_flow || { total_income: 0, total_expenses: 0, net_cash_flow: 0 }
})


function formatSignedIDR(amount) {
  const num = typeof amount === 'number' ? amount : (parseInt(amount, 10) || 0)
  const sign = num > 0 ? '+' : num < 0 ? '-' : ''
  return `${sign} ${formatIDR(Math.abs(num))}`
}

const computedAccounts = computed(() => {
  if (walletStore.activeWallets.length > 0) {
    return walletStore.activeWallets
  }
  return props.dashboard.accounts || []
})

const computedRecentTransactions = computed(() => {
  if (analyticsStore.dashboardData?.recent_transactions) {
    return analyticsStore.dashboardData.recent_transactions
  }
  return props.dashboard.recent_transactions || []
})

function getAccountIcon(type) {
  switch (type) {
    case 'savings':
      return Building2
    case 'credit':
      return CreditCard
    case 'e-wallet':
      return Smartphone
    default:
      return Wallet
  }
}

function formatDate(input) {
  return formatFinancialDate(input, { monthFormat: 'short' })
}
</script>
