<template>
  <div class="space-y-6 pb-24 md:pb-8">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-black text-content-primary tracking-tight">Analisis & Proyeksi</h2>
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
    <div class="p-6 bg-surface-card rounded-2xl border border-border-subtle shadow-card space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-bold text-content-muted uppercase tracking-wider">Ringkasan Arus Kas</h3>
        <span class="text-[11px] text-content-muted font-medium">Bulan Ini</span>
      </div>

      <div class="grid grid-cols-2 gap-4">
        <div class="p-4 bg-income-muted/50 rounded-xl border border-income-border/50">
          <div class="text-[11px] text-income-default font-semibold flex items-center gap-1">
            <span class="w-1.5 h-1.5 rounded-full bg-income-default"></span>
            <span>Total Pemasukan</span>
          </div>
          <div class="text-lg sm:text-xl font-black text-income-default mt-1.5 tabular-nums">
            {{ formatIDR(basicData?.cash_flow?.total_income || 0) }}
          </div>
        </div>

        <div class="p-4 bg-expense-muted/50 rounded-xl border border-expense-border/50">
          <div class="text-[11px] text-expense-default font-semibold flex items-center gap-1">
            <span class="w-1.5 h-1.5 rounded-full bg-expense-default"></span>
            <span>Total Pengeluaran</span>
          </div>
          <div class="text-lg sm:text-xl font-black text-expense-default mt-1.5 tabular-nums">
            {{ formatIDR(basicData?.cash_flow?.total_expenses || 0) }}
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
    <div class="relative rounded-2xl overflow-hidden border border-border-subtle shadow-card">
      <!-- Section Content -->
      <div class="p-6 bg-surface-card space-y-4">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-bold text-content-primary uppercase tracking-wider flex items-center gap-2">
            <Sparkles class="w-4 h-4 text-amber-500" />
            <span>Analisis Presisi & Proyeksi Runway</span>
          </h3>
          <span class="text-xs text-amber-600 font-extrabold">Invinite Pro</span>
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
      <div
        v-if="userTier !== 'premium'"
        class="absolute inset-0 bg-surface-card/85 backdrop-blur-md flex flex-col items-center justify-center p-6 text-center z-10 select-none"
      >
        <div class="w-12 h-12 rounded-2xl bg-amber-500/10 border border-amber-500/20 flex items-center justify-center text-amber-500 mb-3 shadow-xs">
          <Lock class="w-5 h-5 stroke-[2]" />
        </div>
        <h4 class="text-base font-black text-content-primary tracking-tight">Fitur Eksklusif Invinite Pro</h4>
        <p class="text-xs text-content-secondary max-w-xs mt-1 mb-4 leading-relaxed">
          Dapatkan skor kesehatan finansial perbankan, proyeksi runway multi-bulan, dan optimasi anggaran cerdas.
        </p>
        <button
          type="button"
          @click="$emit('open-upgrade')"
          class="inline-flex items-center gap-1.5 px-5 py-2.5 bg-brand-default hover:bg-brand-emphasis text-white font-bold text-xs rounded-xl shadow-lg shadow-brand-default/30 transition cursor-pointer active:scale-95"
        >
          <Sparkles class="w-3.5 h-3.5 text-amber-300" />
          <span>Buka Fitur Pro (Rp 5.000 / bln)</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { Sparkles, ShieldCheck, Lock } from 'lucide-vue-next'

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
