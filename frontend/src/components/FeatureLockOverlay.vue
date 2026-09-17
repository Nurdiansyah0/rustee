<template>
  <div
    class="absolute inset-0 bg-surface-card/85 backdrop-blur-md flex flex-col items-center justify-center p-6 text-center z-20 select-none rounded-xl animate-in fade-in duration-200"
    role="region"
    :aria-label="title || 'Fitur Terkunci'"
  >
    <div
      class="w-12 h-12 rounded-xl bg-amber-500/10 border border-amber-500/20 flex items-center justify-center text-amber-500 mb-3 shadow-xs"
    >
      <Lock class="w-5 h-5 stroke-[2]" />
    </div>

    <div
      v-if="badgeText"
      class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-amber-500/15 border border-amber-500/30 text-amber-500 text-[10px] font-extrabold uppercase tracking-wider mb-2"
    >
      <Sparkles class="w-3 h-3" />
      <span>{{ badgeText }}</span>
    </div>

    <h4 class="text-base font-black text-content-primary tracking-tight">
      {{ title }}
    </h4>

    <p class="text-xs text-content-secondary max-w-xs mt-1.5 mb-4 leading-relaxed">
      {{ description }}
    </p>

    <button
      type="button"
      @click="handleUpgradeClick"
      class="inline-flex items-center gap-1.5 px-5 py-2.5 bg-brand-default hover:bg-brand-emphasis active:scale-95 text-white font-bold text-xs rounded-xl shadow-lg shadow-brand-default/30 transition cursor-pointer"
    >
      <Sparkles class="w-3.5 h-3.5 text-amber-300" />
      <span>{{ buttonText }}</span>
    </button>

    <div class="mt-2.5 text-[11px] text-content-muted">
      <span>Tersedia uji coba 3 bulan gratis • Tanpa komitmen</span>
    </div>
  </div>
</template>

<script setup>
import { useSubscriptionStore } from '@/stores/subscription'
import { Lock, Sparkles } from 'lucide-vue-next'

const subscriptionStore = useSubscriptionStore()

const props = defineProps({
  title: {
    type: String,
    default: 'Fitur Eksklusif FinRep Pro'
  },
  description: {
    type: String,
    default: 'Buka skor kesehatan finansial perbankan, proyeksi runway multi-bulan, dan optimasi anggaran cerdas.'
  },
  featureName: {
    type: String,
    default: 'analytics.advanced'
  },
  badgeText: {
    type: String,
    default: 'FinRep Pro'
  },
  buttonText: {
    type: String,
    default: 'Buka Fitur Pro (Rp 10.000 / bln)'
  }
})

const emit = defineEmits(['upgrade', 'open-upgrade'])

function handleUpgradeClick() {
  emit('upgrade', props.featureName)
  emit('open-upgrade', props.featureName)
  subscriptionStore.openUpgradeModal(props.featureName)
}
</script>
