<template>
  <div class="space-y-5 lg:space-y-6">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold text-content-primary tracking-tight">Analisis & Proyeksi</h2>
        <p class="text-xs text-content-secondary mt-0.5">Metrik kesehatan finansial dan estimasi ketahanan aset</p>
      </div>
      <div :class="[
        'px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border',
        userTier === 'premium'
          ? 'bg-gradient-to-r from-amber-500/10 to-emerald-500/10 text-brand-default border-brand-border'
          : 'bg-surface-subtle text-content-muted border-border-subtle'
      ]">
        <Sparkles v-if="userTier === 'premium'" class="w-3 h-3 text-amber-500" />
        <span>{{ userTier === 'premium' ? 'Premium' : 'Free Tier' }}</span>
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
      <!-- Section Content -->
      <div class="p-6 bg-surface-card space-y-4">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-bold text-content-primary uppercase tracking-wider flex items-center gap-2">
            <Sparkles class="w-4 h-4 text-amber-500" />
            <span>Analisis Presisi & Proyeksi Runway</span>
          </h3>
          <span class="text-xs text-amber-600 font-extrabold">FinRep Pro</span>
        </div>

        <div class="grid grid-cols-2 gap-3">
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
      </div>

      <!-- Frosted Lock Overlay for Free Tier Users -->
      <FeatureLockOverlay
        v-if="userTier !== 'premium'"
        feature-name="analytics.advanced"
        title="Fitur Eksklusif FinRep Pro"
        description="Dapatkan skor kesehatan finansial perbankan, proyeksi runway multi-bulan, dan optimasi anggaran cerdas."
        button-text="Buka Fitur Pro (Rp 10.000 / bln)"
        @open-upgrade="$emit('open-upgrade')"
      />
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { useWalletStore } from '@/stores/wallets'
import { useAuthStore } from '@/stores/auth'
import FeatureLockOverlay from '@/components/FeatureLockOverlay.vue'
import { Sparkles, ShieldCheck, Lock } from 'lucide-vue-next'

const walletStore = useWalletStore()
const authStore = useAuthStore()

const incomeTitle = computed(() => authStore.incomeTitle || 'Pemasukan')
const expenseTitle = computed(() => authStore.expenseTitle || 'Pengeluaran')

const props = defineProps({
  userTier: {
    type: String,
    default: 'free'
  }
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

  if (props.userTier === 'premium') {
    try {
      const a = await api.getAdvancedAnalytics()
      advancedData.value = a
    } catch (err) {
      console.warn('Advanced analytics access', err)
    }
  }
}

onMounted(() => {
  loadAnalytics()
})
</script>
