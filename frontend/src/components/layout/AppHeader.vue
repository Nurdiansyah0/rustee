<template>
  <header class="sticky top-0 z-30 shrink-0 bg-surface-card/90 backdrop-blur-md border-b border-border-subtle px-4 sm:px-6 md:px-8 pt-[max(env(safe-area-inset-top),0.875rem)] pb-3 flex items-center justify-between select-none">
    <!-- Left Section: Title / Mobile Logo -->
    <div class="flex items-center gap-3">
      <!-- Mobile / Tablet Header (Shown on screens <1024px) -->
      <div class="lg:hidden flex items-center gap-2.5">
        <FinrepIcon size="30" theme="auto" :show-typography="false" />
        <h1 class="text-sm sm:text-base font-extrabold text-content-primary tracking-tight">
          {{ title }}
        </h1>
      </div>

      <!-- Desktop Page Title & Breadcrumb (Shown on desktop >=1024px) -->
      <div class="hidden lg:block">
        <h1 class="text-base font-extrabold text-content-primary tracking-tight">
          {{ title }}
        </h1>
      </div>
    </div>

    <!-- Right Section: Quick Balance, Status, Upgrade Pill -->
    <div class="flex items-center gap-2.5 sm:gap-3">
      <!-- Quick Balance Pill (Contextual: shown on secondary tabs, quiet on Dashboard) -->
      <div v-if="currentTab !== 'home'" class="hidden sm:flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-surface-subtle border border-border-subtle text-xs select-none">
        <span class="text-content-muted font-medium">Saldo:</span>
        <span class="font-bold text-content-primary tabular-nums">
          {{ walletStore.displayTotalBalance }}
        </span>
      </div>

      <!-- Trial Countdown Badge (when trialing) -->
      <button
        v-if="subscriptionStore.isTrialing"
        type="button"
        @click="$emit('open-upgrade')"
        class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-brand-default/15 border border-brand-default/30 text-brand-default hover:bg-brand-default/25 active:scale-95 text-[11px] font-extrabold transition cursor-pointer shadow-sm shadow-brand-default/10"
        :title="`Masa Uji Coba: ${subscriptionStore.daysRemaining ?? 90} hari tersisa. Klik untuk berlangganan.`"
      >
        <Clock class="w-3 h-3 text-brand-default animate-pulse" />
        <span>Trial ({{ subscriptionStore.daysRemaining ?? 90 }} hari)</span>
      </button>

      <!-- Premium Active Badge -->
      <div
        v-else-if="authStore.isPremium"
        class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-brand-muted text-brand-default border border-brand-border text-[11px] font-extrabold"
      >
        <Sparkles class="w-3 h-3 text-brand-default" />
        <span>Premium</span>
      </div>

      <!-- Upgrade CTA (Free tier) -->
      <button
        v-else
        type="button"
        @click="$emit('open-upgrade')"
        class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-brand-default/10 border border-brand-border text-brand-default hover:bg-brand-default hover:text-white text-[11px] font-extrabold transition cursor-pointer"
      >
        <Sparkles class="w-3 h-3 text-brand-default" />
        <span>Upgrade Pro</span>
      </button>

      <!-- Online / Offline Status Dot -->
      <div
        class="flex items-center gap-1 px-2 py-1 rounded-md bg-surface-subtle border border-border-subtle text-[11px] text-content-muted"
        :title="isOnline ? 'Terhubung ke server' : 'Mode Offline'"
      >
        <span
          :class="[
            'w-2 h-2 rounded-full',
            isOnline ? 'bg-income-default ring-2 ring-income-muted' : 'bg-warning-default ring-2 ring-warning-muted'
          ]"
        />
        <span class="hidden sm:inline text-[10px] font-medium">{{ isOnline ? 'Online' : 'Offline' }}</span>
      </div>
    </div>
  </header>
</template>

<script setup>
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useSubscriptionStore } from '@/stores/subscription'
import FinrepIcon from '@/components/FinrepIcon.vue'
import { Sparkles, Clock } from 'lucide-vue-next'

defineProps({
  title: {
    type: String,
    default: 'Dashboard'
  },
  isOnline: {
    type: Boolean,
    default: true
  },
  currentTab: {
    type: String,
    default: 'home'
  }
})

defineEmits(['open-upgrade'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const subscriptionStore = useSubscriptionStore()
</script>
