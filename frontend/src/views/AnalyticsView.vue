<template>
  <div class="space-y-5 lg:space-y-6">
    <!-- View Header -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold text-content-primary tracking-tight">Analisis & Proyeksi</h2>
        <p class="text-xs text-content-secondary mt-0.5">Metrik kesehatan finansial, simulasi runway, dan kecerdasan akuntansi pribadi</p>
      </div>
      <div :class="[
        'px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border',
        isPro
          ? 'bg-brand-default/10 text-brand-default border-brand-border'
          : 'bg-surface-subtle text-content-muted border-border-subtle'
      ]">
        <Sparkles v-if="isPro" class="w-3 h-3 text-brand-default" />
        <span>{{ isPro ? (subscriptionStore.isTrialing ? 'Pro Trial (90 Hari)' : 'FinRep Pro Aktif') : 'Free Tier' }}</span>
      </div>
    </div>

    <!-- Basic Metrics Card: Cash Flow Summary -->
    <div class="p-4 sm:p-6 bg-surface-card rounded-xl border border-border-subtle shadow-card space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-bold text-content-muted uppercase tracking-wider">Ringkasan Arus Kas</h3>
        <span class="text-[11px] text-content-muted font-medium">{{ basicData?.cash_flow?.period_label || 'Bulan Ini' }}</span>
      </div>

      <div class="grid grid-cols-2 gap-2.5 sm:gap-4">
        <div class="p-3 sm:p-4 bg-income-muted/50 rounded-xl border border-income-border/50 min-w-0 flex flex-col justify-between">
          <div class="text-[11px] text-income-default font-semibold flex items-center gap-1 min-w-0">
            <span class="w-1.5 h-1.5 rounded-full bg-income-default shrink-0"></span>
            <span class="truncate">Total {{ incomeTitle }}</span>
          </div>
          <div class="text-sm sm:text-base md:text-lg lg:text-xl font-bold sm:font-extrabold lg:font-black text-income-default mt-1 sm:mt-1.5 tabular-nums tracking-tight leading-tight">
            {{ walletStore.hideBalance ? '••••••••' : formatIDR(totalIncome) }}
          </div>
        </div>

        <div class="p-3 sm:p-4 bg-expense-muted/50 rounded-xl border border-expense-border/50 min-w-0 flex flex-col justify-between">
          <div class="text-[11px] text-expense-default font-semibold flex items-center gap-1 min-w-0">
            <span class="w-1.5 h-1.5 rounded-full bg-expense-default shrink-0"></span>
            <span class="truncate">Total {{ expenseTitle }}</span>
          </div>
          <div class="text-sm sm:text-base md:text-lg lg:text-xl font-bold sm:font-extrabold lg:font-black text-expense-default mt-1 sm:mt-1.5 tabular-nums tracking-tight leading-tight">
            {{ walletStore.hideBalance ? '••••••••' : formatIDR(totalExpenses) }}
          </div>
        </div>
      </div>

      <!-- Net Cash Flow & Savings Rate Row -->
      <div class="pt-3 border-t border-border-subtle grid grid-cols-1 sm:grid-cols-2 gap-3 items-center">
        <div class="flex items-center justify-between sm:justify-start sm:gap-3 text-xs">
          <span class="text-content-secondary font-medium">Arus Kas Bersih (Net Cash Flow):</span>
          <span :class="[
            'font-black tabular-nums',
            netCashFlow >= 0 ? 'text-income-default' : 'text-expense-default'
          ]">
            {{ walletStore.hideBalance ? '••••••••' : (netCashFlow >= 0 ? '+ ' : '') + formatIDR(netCashFlow) }}
          </span>
        </div>
        <div class="flex items-center justify-between sm:justify-end sm:gap-3 text-xs">
          <span class="text-content-secondary font-medium">Rasio Tabungan (Savings Rate):</span>
          <span class="text-sm font-black text-brand-default tabular-nums">
            {{ savingsRatePercent }}%
          </span>
        </div>
      </div>
    </div>

    <!-- Advanced Pro Analytics Section -->
    <div class="relative rounded-xl overflow-hidden border border-border-subtle shadow-card">
      <div class="p-5 sm:p-6 bg-surface-card space-y-5">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-bold text-content-primary uppercase tracking-wider flex items-center gap-2">
            <Sparkles class="w-4 h-4 text-brand-default" />
            <span>Kecerdasan Finansial & Proyeksi Akuntansi</span>
          </h3>
          <span class="text-xs text-brand-default font-extrabold flex items-center gap-1.5">
            <span v-if="isPro" class="w-1.5 h-1.5 rounded-full bg-brand-default"></span>
            <span v-else class="w-1.5 h-1.5 rounded-full bg-brand-default animate-pulse"></span>
            <span>FinRep Pro</span>
          </span>
        </div>

        <!-- 1. UNLOCKED PRO STATE -->
        <template v-if="isPro">
          <!-- Card 1 & 2: Health Score & Runway Projection -->
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
            <!-- Health Score Card -->
            <div class="p-4 bg-surface-subtle rounded-xl border border-border-subtle flex flex-col justify-between">
              <div>
                <div class="flex items-center justify-between">
                  <span class="text-xs text-content-secondary font-medium">Skor Kesehatan Finansial</span>
                  <span class="px-2 py-0.5 rounded text-[10px] font-extrabold bg-brand-default/15 text-brand-default">
                    {{ advancedData?.health_grade || (healthScore >= 80 ? 'Sangat Baik (A)' : 'Baik (B)') }}
                  </span>
                </div>
                <div class="text-3xl font-black text-brand-default mt-2 tabular-nums tracking-tight">
                  {{ healthScore }}<span class="text-sm font-normal text-content-muted">/100</span>
                </div>
              </div>
              <div class="pt-3 mt-2 border-t border-border-subtle/80 text-[11px] text-content-muted space-y-1">
                <div class="flex justify-between">
                  <span>Rasio Tabungan (Ideal &ge; 20%)</span>
                  <span class="font-bold text-content-primary">{{ savingsRatePercent }}%</span>
                </div>
                <div class="flex justify-between">
                  <span>Ketahanan Dana Darurat</span>
                  <span class="font-bold text-content-primary">{{ computedRunwayMonths }} Bulan</span>
                </div>
              </div>
            </div>

            <!-- Runway Forecast Card -->
            <div class="p-4 bg-surface-subtle rounded-xl border border-border-subtle flex flex-col justify-between">
              <div>
                <div class="flex items-center justify-between">
                  <span class="text-xs text-content-secondary font-medium">Proyeksi Runway Dana Darurat</span>
                  <span :class="[
                    'px-2 py-0.5 rounded text-[10px] font-extrabold flex items-center gap-1',
                    runwaySafetyColor
                  ]">
                    <span>{{ runwaySafetyLabel }}</span>
                  </span>
                </div>
                <div class="text-3xl font-black text-brand-default mt-2 tabular-nums tracking-tight">
                  {{ computedRunwayMonths }} <span class="text-sm font-normal text-content-muted">Bulan</span>
                </div>
              </div>

              <div class="space-y-1.5 pt-2">
                <div class="w-full bg-border-default h-2 rounded-full overflow-hidden">
                  <div
                    class="bg-brand-default h-full rounded-full transition-all duration-500"
                    :style="{ width: `${Math.min(100, Math.round((computedRunwayMonths / 12) * 100))}%` }"
                  ></div>
                </div>
                <div class="flex justify-between text-[11px] text-content-muted font-medium">
                  <span>Saldo: {{ walletStore.hideBalance ? '••••••' : formatIDR(totalBalance) }}</span>
                  <span>Target Ideal: 12 Bulan</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Card 3: Category Spending Breakdown (Distribusi Pengeluaran) -->
          <div class="p-4 sm:p-5 bg-surface-subtle rounded-xl border border-border-subtle space-y-3.5">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <PieChart class="w-4 h-4 text-brand-default" />
                <span class="text-xs font-bold text-content-primary uppercase tracking-wider">
                  Distribusi Pengeluaran per Kategori
                </span>
              </div>
              <span class="text-[11px] text-content-muted font-medium">Berdasarkan data riwayat kas</span>
            </div>

            <div v-if="categorySpendingList.length > 0" class="space-y-3 pt-1">
              <div
                v-for="cat in categorySpendingList"
                :key="cat.category_id || cat.category_name"
                class="space-y-1"
              >
                <div class="flex justify-between text-xs font-medium">
                  <div class="flex items-center gap-2 truncate max-w-[65%]">
                    <span
                      class="w-2.5 h-2.5 rounded-full shrink-0"
                      :style="{ backgroundColor: cat.category_color || '#10b981' }"
                    ></span>
                    <span class="truncate text-content-primary font-bold">{{ cat.category_name }}</span>
                  </div>
                  <div class="flex items-center gap-2 tabular-nums">
                    <span class="text-content-secondary font-semibold">{{ formatIDR(cat.total_amount?.[0] ?? cat.total_amount ?? 0) }}</span>
                    <span class="text-content-muted text-[11px] w-10 text-right">({{ cat.percentage }}%)</span>
                  </div>
                </div>
                <div class="w-full bg-border-default h-1.5 rounded-full overflow-hidden">
                  <div
                    class="h-full rounded-full transition-all duration-500"
                    :style="{
                      width: `${Math.min(100, Math.max(2, cat.percentage))}%`,
                      backgroundColor: cat.category_color || '#10b981'
                    }"
                  ></div>
                </div>
              </div>
            </div>
            <div v-else class="py-6 text-center text-xs text-content-muted">
              Belum ada catatan pengeluaran bulan ini. Catat transaksi untuk melihat visualisasi distribusi kategori.
            </div>
          </div>

          <!-- Card 4: 50/30/20 Rule Audit (Kaidah Akuntansi Finansial) -->
          <div class="p-4 sm:p-5 bg-surface-subtle rounded-xl border border-border-subtle space-y-3.5">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <ShieldCheck class="w-4 h-4 text-brand-default" />
                <span class="text-xs font-bold text-content-primary uppercase tracking-wider">
                  Audit Alokasi Pendapatan (Kaidah 50/30/20)
                </span>
              </div>
              <span class="text-[10px] font-bold px-2 py-0.5 rounded bg-brand-default/10 text-brand-default uppercase">
                Standar Akuntansi
              </span>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-3 gap-3 pt-1">
              <!-- Needs (50%) -->
              <div class="p-3 bg-surface-card rounded-xl border border-border-subtle space-y-1">
                <div class="text-[11px] text-content-secondary font-semibold">Kebutuhan Pokok (Needs)</div>
                <div class="text-sm font-black text-content-primary tabular-nums">
                  {{ rule503020.needs_percent }}%
                </div>
                <div class="text-[10px] text-content-muted">Target ideal: &le; 50% pendapatan</div>
              </div>

              <!-- Wants (30%) -->
              <div class="p-3 bg-surface-card rounded-xl border border-border-subtle space-y-1">
                <div class="text-[11px] text-content-secondary font-semibold">Keinginan & Lifestyle (Wants)</div>
                <div class="text-sm font-black text-content-primary tabular-nums">
                  {{ rule503020.wants_percent }}%
                </div>
                <div class="text-[10px] text-content-muted">Target ideal: &le; 30% pendapatan</div>
              </div>

              <!-- Savings (20%) -->
              <div class="p-3 bg-surface-card rounded-xl border border-border-subtle space-y-1">
                <div class="text-[11px] text-brand-default font-semibold">Tabungan & Investasi (Savings)</div>
                <div class="text-sm font-black text-brand-default tabular-nums">
                  {{ rule503020.savings_percent }}%
                </div>
                <div class="text-[10px] text-content-muted">Target ideal: &ge; 20% pendapatan</div>
              </div>
            </div>
          </div>

          <!-- Card 5: Personal Accounting Actions -->
          <div class="p-4 sm:p-5 bg-gradient-to-br from-brand-default/10 via-surface-subtle to-surface-subtle rounded-xl border border-brand-default/20 space-y-3">
            <div class="flex items-center justify-between">
              <div>
                <h4 class="text-xs font-bold text-content-primary uppercase tracking-wide flex items-center gap-1.5">
                  <FileSpreadsheet class="w-4 h-4 text-brand-default" />
                  <span>Aksi Akuntansi Pribadi</span>
                </h4>
                <p class="text-[11px] text-content-secondary mt-0.5">Rekonsiliasi mutasi perbankan & unduh rekapitulasi buku kas</p>
              </div>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2.5 pt-1">
              <button
                type="button"
                @click="showImportModal = true"
                class="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis text-white font-bold text-xs shadow-md shadow-brand-default/20 transition cursor-pointer"
              >
                <FileSpreadsheet class="w-4 h-4" />
                <span>Analisa E-Statement Bank</span>
              </button>

              <button
                type="button"
                @click="handleExportCSV"
                class="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl border border-border-default hover:bg-surface-card bg-surface-subtle text-content-primary font-bold text-xs transition cursor-pointer shadow-xs"
              >
                <Download class="w-4 h-4 text-brand-default" />
                <span>Ekspor Laporan (CSV)</span>
              </button>
            </div>
          </div>
        </template>

        <!-- 2. GATED SKELETON PREVIEW (FREE TIER) -->
        <template v-else>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 animate-pulse">
            <div class="p-4 bg-surface-subtle/80 rounded-xl border border-border-subtle space-y-2.5">
              <div class="flex items-center justify-between">
                <div class="text-xs text-content-secondary font-medium">Skor Kesehatan Finansial</div>
                <span class="text-[9px] font-bold px-1.5 py-0.5 rounded bg-brand-default/15 text-brand-default">Pro</span>
              </div>
              <div class="flex items-baseline gap-2">
                <div class="text-2xl font-black text-brand-default/60 tabular-nums">88<span class="text-sm font-normal text-content-muted">/100</span></div>
                <div class="h-3 w-16 bg-surface-card rounded"></div>
              </div>
              <div class="flex items-center gap-1.5 pt-0.5">
                <ShieldCheck class="w-3.5 h-3.5 text-income-default/50" />
                <span class="text-[11px] text-income-default/70 font-semibold">Audit Rasio Tabungan & Arus Kas</span>
              </div>
            </div>

            <div class="p-4 bg-surface-subtle/80 rounded-xl border border-border-subtle space-y-2.5">
              <div class="flex items-center justify-between">
                <div class="text-xs text-content-secondary font-medium">Proyeksi Runway Dana Darurat</div>
                <span class="text-[9px] font-bold px-1.5 py-0.5 rounded bg-brand-default/15 text-brand-default">Pro</span>
              </div>
              <div class="text-xl font-black text-brand-default/60">~6 - 12 Bulan</div>
              <div class="text-[11px] text-content-muted font-medium flex items-center gap-1">
                <span class="w-1.5 h-1.5 rounded-full bg-brand-default/40"></span>
                <span>Ketahanan aset terhadap pengeluaran</span>
              </div>
            </div>
          </div>

          <div class="p-4 bg-surface-subtle/80 rounded-xl border border-border-subtle space-y-2.5 animate-pulse">
            <div class="flex justify-between items-center text-xs font-bold text-content-primary">
              <span>Distribusi Pengeluaran & Audit Kaidah 50/30/20</span>
              <span class="text-brand-default font-black">FinRep Pro</span>
            </div>
            <div class="w-full bg-border-default/70 h-2.5 rounded-full overflow-hidden">
              <div class="bg-gradient-to-r from-brand-default to-brand-emphasis h-full w-[65%] rounded-full opacity-60"></div>
            </div>
            <div class="flex justify-between text-[11px] text-content-muted font-medium">
              <span>Breakdown alokasi kategori otomatis</span>
              <span>Personalized intelligence</span>
            </div>
          </div>
        </template>
      </div>

      <!-- Feature Lock Overlay for Free Tier -->
      <FeatureLockOverlay
        v-if="!isPro"
        feature-name="analytics.advanced"
        @open-upgrade="$emit('open-upgrade')"
      />
    </div>

    <!-- Bank Statement Import Modal Embedded -->
    <BankStatementImportModal
      v-if="showImportModal"
      @close="showImportModal = false"
      @success="handleImportSuccess"
    />
  </div>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { useWalletStore } from '@/stores/wallets'
import { useTransactionStore } from '@/stores/transactions'
import { useAuthStore } from '@/stores/auth'
import { useSubscriptionStore } from '@/stores/subscription'
import FeatureLockOverlay from '@/components/FeatureLockOverlay.vue'
import BankStatementImportModal from '@/components/BankStatementImportModal.vue'
import { exportTransactionsToCSV } from '@/utils/export'
import {
  Sparkles,
  ShieldCheck,
  Lock,
  PieChart,
  FileSpreadsheet,
  Download,
  AlertTriangle
} from 'lucide-vue-next'

const walletStore = useWalletStore()
const transactionStore = useTransactionStore()
const authStore = useAuthStore()
const subscriptionStore = useSubscriptionStore()

const props = defineProps({
  userTier: {
    type: String,
    default: 'free'
  }
})

defineEmits(['open-upgrade'])

const showImportModal = ref(false)
const basicData = ref(null)
const advancedData = ref(null)

const incomeTitle = computed(() => authStore.incomeTitle || 'Pemasukan')
const expenseTitle = computed(() => authStore.expenseTitle || 'Pengeluaran')

const isPro = computed(() => {
  return (
    props.userTier === 'premium' ||
    authStore.isPremium ||
    subscriptionStore.isPremium ||
    subscriptionStore.isTrialing
  )
})

const totalIncome = computed(() => basicData.value?.cash_flow?.total_income || 0)
const totalExpenses = computed(() => basicData.value?.cash_flow?.total_expenses || 0)
const netCashFlow = computed(() => totalIncome.value - totalExpenses.value)
const savingsRatePercent = computed(() => basicData.value?.savings_rate_percent || 0)
const totalBalance = computed(() => walletStore.totalBalance || advancedData.value?.total_balance?.[0] || advancedData.value?.total_balance || 0)

// Dynamic Runway Calculation
const computedRunwayMonths = computed(() => {
  if (advancedData.value?.runway_months !== undefined && advancedData.value?.runway_months !== null) {
    return Number(advancedData.value.runway_months).toFixed(1)
  }
  const exp = totalExpenses.value
  if (exp <= 0) return '12.0'
  const bal = totalBalance.value
  return (bal / exp).toFixed(1)
})

const runwaySafetyLabel = computed(() => {
  const m = parseFloat(computedRunwayMonths.value)
  if (m >= 6) return 'Zona Aman'
  if (m >= 3) return 'Zona Waspada'
  return 'Zona Kritis'
})

const runwaySafetyColor = computed(() => {
  const m = parseFloat(computedRunwayMonths.value)
  if (m >= 6) return 'bg-income-muted/60 text-income-default border border-income-border'
  if (m >= 3) return 'bg-amber-500/15 text-amber-500 border border-amber-500/30'
  return 'bg-expense-muted/60 text-expense-default border border-expense-border'
})

const healthScore = computed(() => {
  return advancedData.value?.financial_health_score || 85
})

const categorySpendingList = computed(() => {
  return advancedData.value?.category_spending || []
})

const rule503020 = computed(() => {
  if (advancedData.value?.rule_50_30_20) {
    return advancedData.value.rule_50_30_20
  }
  // Default estimate based on current numbers
  const exp = totalExpenses.value
  const inc = totalIncome.value || 1
  const needsPct = Math.min(100, Math.round(((exp * 0.65) / inc) * 100))
  const wantsPct = Math.min(100, Math.round(((exp * 0.35) / inc) * 100))
  const savingsPct = Math.max(0, savingsRatePercent.value)
  return {
    needs_percent: needsPct,
    wants_percent: wantsPct,
    savings_percent: savingsPct
  }
})

async function loadAnalytics() {
  try {
    const b = await api.getBasicAnalytics()
    basicData.value = b
  } catch (err) {
    console.error('Failed to load basic analytics', err)
  }

  if (isPro.value) {
    try {
      const a = await api.getAdvancedAnalytics()
      advancedData.value = a
    } catch (err) {
      console.warn('Advanced analytics access', err)
    }
  }

  // Ensure wallets and transactions are fetched for accurate balance and export
  try {
    if (!walletStore.accounts.length) {
      await walletStore.fetchAccounts()
    }
    if (!transactionStore.transactions.length) {
      await transactionStore.fetchTransactions(1)
    }
  } catch {}
}

function handleImportSuccess() {
  showImportModal.value = false
  loadAnalytics()
}

function handleExportCSV() {
  try {
    const txs = transactionStore.transactions || []
    if (!txs.length) {
      alert('Belum ada transaksi untuk diekspor. Tambahkan catatan transaksi terlebih dahulu.')
      return
    }
    exportTransactionsToCSV(txs)
  } catch (err) {
    alert(err.message || 'Gagal mengekspor laporan.')
  }
}

watch(
  () => isPro.value,
  (newVal) => {
    if (newVal) {
      loadAnalytics()
    }
  }
)

onMounted(() => {
  loadAnalytics()
})
</script>
