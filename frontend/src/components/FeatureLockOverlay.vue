<template>
  <div
    class="absolute inset-0 bg-surface-card/65 hover:bg-surface-card/55 backdrop-blur-[2px] flex flex-col items-center justify-center p-5 sm:p-6 text-center z-20 select-none rounded-xl transition duration-200 cursor-pointer"
    role="region"
    :aria-label="computedTitle"
    @click="handleUpgradeClick"
  >
    <div
      class="w-12 h-12 rounded-xl bg-brand-default/15 border border-brand-default/25 flex items-center justify-center text-brand-default mb-2.5 shadow-xs transition group-hover:scale-105"
    >
      <Sparkles v-if="!hasUsedTrial" class="w-6 h-6 stroke-[2]" />
      <Lock v-else class="w-6 h-6 stroke-[2]" />
    </div>

    <div
      class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-brand-default/15 border border-brand-default/25 text-brand-default text-[10px] font-extrabold uppercase tracking-wider mb-2"
    >
      <Sparkles class="w-3 h-3" />
      <span>{{ computedBadge }}</span>
    </div>

    <h4 class="text-base sm:text-lg font-black text-content-primary tracking-tight">
      {{ computedTitle }}
    </h4>

    <p class="text-xs text-content-secondary max-w-sm mt-1 mb-3.5 leading-relaxed">
      {{ computedDescription }}
    </p>

    <button
      type="button"
      @click.stop="handleUpgradeClick"
      class="inline-flex items-center gap-1.5 px-5 py-2.5 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis active:scale-95 text-white font-bold text-xs rounded-xl shadow-lg shadow-brand-default/25 transition cursor-pointer"
    >
      <Sparkles class="w-3.5 h-3.5" />
      <span>{{ computedButtonText }}</span>
    </button>

    <div class="mt-2 text-[10px] text-content-muted">
      <span>{{ hasUsedTrial ? 'Pembayaran instan & resmi via DANA' : 'Tersedia uji coba 3 bulan gratis • Tanpa komitmen' }}</span>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { useSubscriptionStore } from '@/stores/subscription'
import { Lock, Sparkles } from 'lucide-vue-next'

const subscriptionStore = useSubscriptionStore()

const props = defineProps({
  title: {
    type: String,
    default: ''
  },
  description: {
    type: String,
    default: ''
  },
  featureName: {
    type: String,
    default: 'analytics.advanced'
  },
  badgeText: {
    type: String,
    default: ''
  },
  buttonText: {
    type: String,
    default: ''
  }
})

const emit = defineEmits(['upgrade', 'open-upgrade'])

const hasUsedTrial = computed(() => subscriptionStore.hasUsedTrial)

const computedBadge = computed(() => {
  if (props.badgeText) return props.badgeText
  return hasUsedTrial.value ? 'Masa Coba Selesai' : '3 Bulan Gratis (90 Hari)'
})

const computedTitle = computed(() => {
  if (props.title) return props.title
  return hasUsedTrial.value ? 'Lanjutkan Akses FinRep Pro' : 'Fitur Eksklusif FinRep Pro'
})

const computedDescription = computed(() => {
  if (props.description) return props.description
  return hasUsedTrial.value
    ? 'Masa uji coba 3 bulan Anda telah selesai. Aktifkan FinRep Pro mulai Rp 10.000 / bulan via DANA untuk membuka skor kesehatan & runway lengkap.'
    : 'Buka skor kesehatan finansial perbankan, proyeksi runway multi-bulan, dan optimasi anggaran cerdas.'
})

const computedButtonText = computed(() => {
  if (props.buttonText) return props.buttonText
  return hasUsedTrial.value
    ? 'Berlangganan Pro via DANA (Rp 10.000 / bln)'
    : 'Mulai Coba 3 Bulan Gratis (Rp 0)'
})

function handleUpgradeClick() {
  emit('upgrade', props.featureName)
  emit('open-upgrade', props.featureName)
  subscriptionStore.openUpgradeModal(props.featureName)
}
</script>
