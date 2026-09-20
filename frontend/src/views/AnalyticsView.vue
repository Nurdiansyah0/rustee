<template>
  <div class="space-y-5 lg:space-y-6">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold text-content-primary tracking-tight">Analisis & Proyeksi</h2>
        <p class="text-xs text-content-secondary mt-0.5">Metrik kesehatan finansial dan estimasi ketahanan aset</p>
      </div>
      <div :class="[
        'px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border',
        isPro
          ? 'bg-brand-default/10 text-brand-default border-brand-border'
          : 'bg-surface-subtle text-content-muted border-border-subtle'
      ]">
        <Sparkles v-if="isPro" class="w-3 h-3 text-brand-default" />
        <span>{{ isPro ? (subscriptionStore.isTrialing ? 'Pro Trial (90 Hari)' : 'Premium') : 'Free Tier' }}</span>
      </div>
    </div>

    <!-- Basic Metrics Card -->
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
            {{ walletStore.hideBalance ? '••••••••' : formatIDR(basicData?.cash_flow?.total_income || 0) }}
          </div>
        </div>

        <div class="p-3 sm:p-4 bg-expense-muted/50 rounded-xl border border-expense-border/50 min-w-0 flex flex-col justify-between">
          <div class="text-[11px] text-expense-default font-semibold flex items-center gap-1 min-w-0">
            <span class="w-1.5 h-1.5 rounded-full bg-expense-default shrink-0"></span>
            <span class="truncate">Total {{ expenseTitle }}</span>
          </div>
          <div class="text-sm sm:text-base md:text-lg lg:text-xl font-bold sm:font-extrabold lg:font-black text-expense-default mt-1 sm:mt-1.5 tabular-nums tracking-tight leading-tight">
            {{ walletStore.hideBalance ? '••••••••' : formatIDR(basicData?.cash_flow?.total_expenses || 0) }}
          </div>
        </div>
      </div>

      <div class="pt-3 border-t border-border-subtle flex items-center justify-between">
        <div class="text-xs text-content-secondary font-medium">Rasio Tabungan (Savings Rate)</div>
        <div class="text-sm font-black text-brand-default tabular-nums">
          {{ basicData?.savings_rate_percent || 0 }}%
        </div>
      </div>
    </div>

    <!-- Advanced Analytics (Gated Feature Section) -->
    <div class="relative rounded-xl overflow-hidden border border-border-subtle shadow-card">
      <!-- Section Content: Pro Real Data (when premium) vs Pro Skeleton (when free) -->
      <div class="p-5 sm:p-6 bg-surface-card space-y-4">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-bold text-content-primary uppercase tracking-wider flex items-center gap-2">
            <Sparkles class="w-4 h-4 text-brand-default" />
            <span>Analisis Presisi & Proyeksi Runway</span>
          </h3>
          <span class="text-xs text-brand-default font-extrabold flex items-center gap-1.5">
            <span v-if="isPro" class="w-1.5 h-1.5 rounded-full bg-brand-default"></span>
            <span v-else class="w-1.5 h-1.5 rounded-full bg-brand-default animate-pulse"></span>
            <span>FinRep Pro</span>
          </span>
        </div>

        <!-- Premium Real Data State -->
        <template v-if="isPro">
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div class="p-4 bg-surface-subtle rounded-xl border border-border-subtle">
              <div class="text-xs text-content-secondary font-medium">Skor Kesehatan Finansial</div>
              <div class="text-2xl font-black text-brand-default mt-1 tabular-nums">
                {{ advancedData?.financial_health_score || 85 }}/100
              </div>
              <div class="text-[11px] text-income-default mt-1 font-semibold flex items-center gap-1">
                <ShieldCheck class="w-3.5 h-3.5" />
                <span>Sangat Baik</span>
              </div>
            </div>

            <div class="p-4 bg-surface-subtle rounded-xl border border-border-subtle">
              <div class="text-xs text-content-secondary font-medium">Tren Arus Kas</div>
              <div class="text-xl font-black text-brand-default mt-1 capitalize">
                {{ advancedData?.monthly_trend || 'Positif' }}
              </div>
              <div class="text-[11px] text-content-muted mt-1 font-medium">Stabil 30 hari terakhir</div>
            </div>
          </div>

          <div class="p-4 bg-surface-subtle rounded-xl border border-border-subtle space-y-2">
            <div class="flex justify-between text-xs font-bold text-content-primary">
              <span>Proyeksi Ketahanan Dana Darurat (Runway)</span>
              <span class="text-brand-default tabular-nums">6.2 Bulan</span>
            </div>
            <div class="w-full bg-border-default h-2 rounded-full overflow-hidden">
              <div class="bg-brand-default h-full w-[65%] rounded-full transition-all duration-500"></div>
            </div>
            <div class="flex justify-between text-[11px] text-content-muted font-medium">
              <span>Cadangan saat ini: 6.2 Bulan</span>
              <span>Target ideal: 12 Bulan</span>
            </div>
          </div>
        </template>

        <!-- Free Tier: Pro Skeleton Preview -->
        <template v-else>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 animate-pulse">
            <!-- Skeleton Card 1: Health Score -->
            <div class="p-4 bg-surface-subtle/80 rounded-xl border border-border-subtle space-y-2.5">
              <div class="flex items-center justify-between">
                <div class="text-xs text-content-secondary font-medium">Skor Kesehatan Finansial</div>
                <span class="text-[9px] font-bold px-1.5 py-0.5 rounded bg-brand-default/15 text-brand-default">Pro</span>
              </div>
              <div class="flex items-baseline gap-2">
                <div class="text-2xl font-black text-brand-default/60 tabular-nums">85<span class="text-sm font-normal text-content-muted">/100</span></div>
                <div class="h-3 w-16 bg-surface-card rounded"></div>
              </div>
              <div class="flex items-center gap-1.5 pt-0.5">
                <ShieldCheck class="w-3.5 h-3.5 text-income-default/50" />
                <span class="text-[11px] text-income-default/70 font-semibold">Analisis Rasio Tabungan</span>
              </div>
            </div>

            <!-- Skeleton Card 2: Cash Flow Trend -->
            <div class="p-4 bg-surface-subtle/80 rounded-xl border border-border-subtle space-y-2.5">
              <div class="flex items-center justify-between">
                <div class="text-xs text-content-secondary font-medium">Tren Arus Kas & AI</div>
                <span class="text-[9px] font-bold px-1.5 py-0.5 rounded bg-brand-default/15 text-brand-default">Pro</span>
              </div>
              <div class="text-xl font-black text-brand-default/60">Positif & Terukur</div>
              <div class="text-[11px] text-content-muted font-medium flex items-center gap-1">
                <span class="w-1.5 h-1.5 rounded-full bg-brand-default/40"></span>
                <span>Proyeksi prediktif 30–90 hari</span>
              </div>
            </div>
          </div>

          <!-- Skeleton Card 3: Emergency Runway Meter -->
          <div class="p-4 bg-surface-subtle/80 rounded-xl border border-border-subtle space-y-2.5 animate-pulse">
            <div class="flex justify-between items-center text-xs font-bold text-content-primary">
              <span>Proyeksi Ketahanan Dana Darurat (Runway)</span>
              <span class="text-brand-default font-black tabular-nums">~6 - 12 Bulan</span>
            </div>
            <div class="w-full bg-border-default/70 h-2.5 rounded-full overflow-hidden">
              <div class="bg-gradient-to-r from-brand-default to-brand-emphasis h-full w-[65%] rounded-full opacity-60"></div>
            </div>
            <div class="flex justify-between text-[11px] text-content-muted font-medium">
              <span>Simulasi runway berbasis pengeluaran</span>
              <span>Target ideal: 12 Bulan</span>
            </div>
          </div>

          <!-- Skeleton Card 4: Category Smart Budgeting Preview -->
          <div class="p-3.5 bg-surface-subtle/60 rounded-xl border border-border-subtle flex items-center justify-between animate-pulse">
            <div class="flex items-center gap-2.5 min-w-0">
              <div class="w-8 h-8 rounded-lg bg-brand-default/10 text-brand-default flex items-center justify-center shrink-0">
                <Sparkles class="w-4 h-4" />
              </div>
              <div class="min-w-0">
                <div class="text-xs font-bold text-content-primary truncate">Alokasi Budget & Early Warning Overbudget</div>
                <div class="text-[10px] text-content-muted truncate">Notifikasi pintar sebelum pengeluaran bulanan terlampaui</div>
              </div>
            </div>
            <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-brand-default/15 text-brand-default shrink-0 ml-2">
              FinRep Pro
            </span>
          </div>
        </template>
      </div>

      <!-- Frosted Lock Overlay for Free Tier Users (Visually Revealing the Pro Skeleton) -->
      <FeatureLockOverlay
        v-if="!isPro"
        feature-name="analytics.advanced"
        @open-upgrade="$emit('open-upgrade')"
      />
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { useWalletStore } from '@/stores/wallets'
import { useAuthStore } from '@/stores/auth'
import { useSubscriptionStore } from '@/stores/subscription'
import FeatureLockOverlay from '@/components/FeatureLockOverlay.vue'
import { Sparkles, ShieldCheck, Lock } from 'lucide-vue-next'

const walletStore = useWalletStore()
const authStore = useAuthStore()
const subscriptionStore = useSubscriptionStore()

const incomeTitle = computed(() => authStore.incomeTitle || 'Pemasukan')
const expenseTitle = computed(() => authStore.expenseTitle || 'Pengeluaran')

const props = defineProps({
  userTier: {
    type: String,
    default: 'free'
  }
})

const isPro = computed(() => {
  return (
    props.userTier === 'premium' ||
    authStore.isPremium ||
    subscriptionStore.isPremium ||
    subscriptionStore.isTrialing
  )
})

defineEmits(['open-upgrade'])

const basicData = ref(null)
const advancedData = ref(null)

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
