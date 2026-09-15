<template>
  <div class="space-y-6 pb-24 md:pb-8">
    <!-- TIER 1: Total Saldo Dompet & Hero Pulse Card -->
    <div class="relative overflow-hidden rounded-2xl sm:rounded-3xl bg-gradient-to-br from-zinc-900 via-zinc-900 to-emerald-950 p-6 sm:p-8 text-white shadow-xl border border-white/5">
      <!-- Background Ambient Glow -->
      <div class="absolute -right-12 -top-12 w-64 h-64 bg-emerald-500/10 rounded-full blur-3xl pointer-events-none"></div>

      <div class="relative z-10 flex items-center justify-between text-zinc-400 text-xs font-semibold uppercase tracking-wider">
        <div class="flex items-center gap-2">
          <span>Total Saldo Dompet</span>
          <span class="w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
        </div>
        <button
          type="button"
          @click="toggleHide"
          class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-white/5 hover:bg-white/10 text-zinc-300 text-xs transition cursor-pointer"
        >
          <component :is="isHideBalance ? Eye : EyeOff" class="w-3.5 h-3.5" />
          <span>{{ isHideBalance ? 'Tampilkan' : 'Sembunyikan' }}</span>
        </button>
      </div>

      <!-- Hero Tabular Balance -->
      <div class="relative z-10 mt-3 sm:mt-4">
        <div v-if="!isHideBalance" class="text-3xl sm:text-4xl md:text-5xl font-black tracking-tight tabular-nums">
          {{ formatIDR(computedTotalBalance) }}
        </div>
        <div v-else class="text-3xl sm:text-4xl md:text-5xl font-black tracking-tight font-mono text-zinc-400">
          Rp ••••••••
        </div>
      </div>

      <!-- Quick Net Cash Flow bar (Pemasukan vs Pengeluaran) -->
      <div class="relative z-10 mt-6 sm:mt-8 pt-4 border-t border-white/10 grid grid-cols-2 gap-4">
        <div>
          <div class="flex items-center gap-1.5 text-xs text-zinc-400 font-medium">
            <span class="w-2 h-2 rounded-full bg-emerald-400"></span>
            <span>Total Pemasukan</span>
          </div>
          <div class="text-base sm:text-lg font-bold text-white mt-1 tabular-nums">
            {{ isHideBalance ? '••••' : formatIDR(computedCashFlow.total_income) }}
          </div>
        </div>
        <div>
          <div class="flex items-center gap-1.5 text-xs text-zinc-400 font-medium">
            <span class="w-2 h-2 rounded-full bg-rose-400"></span>
            <span>Total Pengeluaran</span>
          </div>
          <div class="text-base sm:text-lg font-bold text-white mt-1 tabular-nums">
            {{ isHideBalance ? '••••' : formatIDR(computedCashFlow.total_expenses) }}
          </div>
        </div>
      </div>
    </div>

    <!-- TIER 2: Cash Flow Context & Status Card -->
    <div class="p-5 bg-surface-card rounded-2xl border border-border-subtle shadow-card flex items-center justify-between">
      <div class="flex items-center gap-3.5">
        <div :class="[
          'w-11 h-11 rounded-xl flex items-center justify-center shrink-0',
          computedCashFlow.net_cash_flow >= 0 ? 'bg-income-muted text-income-default' : 'bg-expense-muted text-expense-default'
        ]">
          <component :is="computedCashFlow.net_cash_flow >= 0 ? TrendingUp : TrendingDown" class="w-5 h-5 stroke-[2]" />
        </div>
        <div>
          <div class="text-xs text-content-secondary font-medium">Arus Kas Bersih (Bulan Ini)</div>
          <div :class="[
            'text-lg sm:text-xl font-black mt-0.5 tabular-nums',
            computedCashFlow.net_cash_flow >= 0 ? 'text-income-default' : 'text-expense-default'
          ]">
            {{ isHideBalance ? '••••' : formatIDR(computedCashFlow.net_cash_flow) }}
          </div>
        </div>
      </div>

      <div :class="[
        'px-3 py-1.5 rounded-full text-xs font-bold uppercase tracking-wider',
        computedCashFlow.net_cash_flow >= 0 ? 'bg-income-muted text-income-default border border-income-border' : 'bg-expense-muted text-expense-default border border-expense-border'
      ]">
        {{ computedCashFlow.net_cash_flow >= 0 ? 'Surplus' : 'Defisit' }}
      </div>
    </div>

    <!-- TIER 2: Multi-Wallet Deck -->
    <div>
      <div class="flex items-center justify-between mb-3 px-1">
        <div class="flex items-center gap-2">
          <Wallet class="w-4 h-4 text-brand-default" />
          <h3 class="text-sm font-extrabold text-content-primary">Dompet & Rekening</h3>
        </div>
        <span class="text-xs text-content-muted">{{ computedAccounts.length }} Akun</span>
      </div>

      <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
        <div
          v-for="acc in computedAccounts"
          :key="acc.id"
          class="p-4 bg-surface-card rounded-2xl border border-border-subtle shadow-card hover:border-border-default transition flex flex-col justify-between"
        >
          <div class="flex items-center justify-between mb-3">
            <div class="w-8 h-8 rounded-xl bg-surface-subtle text-brand-default flex items-center justify-center shrink-0">
              <component :is="getAccountIcon(acc.account_type)" class="w-4 h-4 stroke-[2]" />
            </div>
            <span class="text-[10px] font-bold uppercase text-content-muted px-1.5 py-0.5 rounded bg-surface-subtle">
              {{ acc.account_type || 'cash' }}
            </span>
          </div>
          <div>
            <div class="text-xs font-semibold text-content-secondary truncate">{{ acc.name }}</div>
            <div class="text-sm sm:text-base font-black text-content-primary mt-1 tabular-nums">
              {{ isHideBalance ? '••••' : formatIDR(acc.current_balance || acc.balance || 0) }}
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- TIER 3: Transaksi Terbaru -->
    <div>
      <div class="flex items-center justify-between mb-3 px-1">
        <div class="flex items-center gap-2">
          <Receipt class="w-4 h-4 text-brand-default" />
          <h3 class="text-sm font-extrabold text-content-primary">Transaksi Terbaru</h3>
        </div>
        <button
          type="button"
          @click="$emit('nav', 'transactions')"
          class="text-xs font-semibold text-brand-default hover:underline cursor-pointer"
        >
          Lihat Semua Transaksi →
        </button>
      </div>

      <div v-if="computedRecentTransactions.length" class="space-y-2.5">
        <div
          v-for="tx in computedRecentTransactions"
          :key="tx.id"
          class="flex items-center justify-between p-3.5 sm:p-4 bg-surface-card rounded-2xl border border-border-subtle shadow-card hover:border-border-default transition"
        >
          <div class="flex items-center gap-3.5 min-w-0">
            <div :class="[
              'w-10 h-10 rounded-xl flex items-center justify-center font-bold text-sm shrink-0',
              tx.transaction_type === 'income' ? 'bg-income-muted text-income-default' :
              tx.transaction_type === 'expense' ? 'bg-expense-muted text-expense-default' : 'bg-transfer-muted text-transfer-default'
            ]">
              <component
                :is="tx.transaction_type === 'income' ? ArrowDownLeft : tx.transaction_type === 'expense' ? ArrowUpRight : ArrowLeftRight"
                class="w-5 h-5 stroke-[2]"
              />
            </div>
            <div class="min-w-0">
              <div class="text-xs sm:text-sm font-bold text-content-primary truncate">{{ tx.description || 'Transaksi' }}</div>
              <div class="text-[11px] text-content-muted mt-0.5">{{ formatDate(tx.date) }}</div>
            </div>
          </div>

          <div :class="[
            'text-xs sm:text-sm font-black tabular-nums shrink-0 ml-3',
            tx.transaction_type === 'income' ? 'text-income-default' :
            tx.transaction_type === 'expense' ? 'text-expense-default' : 'text-content-primary'
          ]">
            {{ tx.transaction_type === 'income' ? '+' : tx.transaction_type === 'expense' ? '-' : '' }}
            {{ formatIDR(tx.amount) }}
          </div>
        </div>
      </div>

      <div v-else class="p-8 text-center bg-surface-card rounded-2xl border border-dashed border-border-default">
        <Receipt class="w-8 h-8 text-content-muted mx-auto mb-2 opacity-50" />
        <div class="text-content-secondary text-sm font-medium">Belum ada transaksi yang tercatat</div>
        <button
          type="button"
          @click="$emit('open-add')"
          class="mt-3 inline-flex items-center gap-1.5 px-4 py-2 rounded-xl bg-brand-default text-white text-xs font-bold hover:bg-brand-emphasis transition cursor-pointer"
        >
          <Plus class="w-3.5 h-3.5" />
          <span>Catat Transaksi Pertama</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { formatIDR } from '@/utils/currency'
import { useWalletStore } from '@/stores/wallets'
import { useAnalyticsStore } from '@/stores/analytics'
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
  Plus
} from 'lucide-vue-next'

const props = defineProps({
  dashboard: {
    type: Object,
    default: () => ({})
  }
})

defineEmits(['nav', 'open-add', 'open-accounts'])

const walletStore = useWalletStore()
const analyticsStore = useAnalyticsStore()

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

function formatDate(isoStr) {
  if (!isoStr) return ''
  try {
    const d = new Date(isoStr)
    return d.toLocaleDateString('id-ID', { day: 'numeric', month: 'short' })
  } catch {
    return isoStr
  }
}
</script>
