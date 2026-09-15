<template>
  <header class="sticky top-0 z-30 bg-surface-card/90 backdrop-blur-md border-b border-border-subtle px-4 sm:px-6 md:px-8 py-3.5 flex items-center justify-between select-none">
    <!-- Left Section: Title / Mobile Logo -->
    <div class="flex items-center gap-3">
      <!-- Mobile Logo (Shown only on small screens) -->
      <div class="md:hidden flex items-center gap-2">
        <img
          src="/icons/Invinite_Logo.png"
          alt="Invinite"
          class="h-7 w-auto object-contain"
        />
      </div>

      <!-- Desktop Page Title & Breadcrumb -->
      <div class="hidden md:block">
        <h1 class="text-base font-extrabold text-content-primary tracking-tight">
          {{ title }}
        </h1>
      </div>
    </div>

    <!-- Right Section: Quick Balance, Status, Upgrade Pill -->
    <div class="flex items-center gap-2.5 sm:gap-3">
      <!-- Quick Balance Pill (Header peek) -->
      <div class="hidden sm:flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-surface-subtle border border-border-subtle text-xs">
        <span class="text-content-muted">Saldo:</span>
        <span class="font-extrabold text-content-primary tabular-nums">
          {{ walletStore.displayTotalBalance }}
        </span>
        <button
          type="button"
          @click="walletStore.toggleHideBalance()"
          class="text-content-muted hover:text-content-primary p-0.5 rounded transition cursor-pointer"
          :title="walletStore.hideBalance ? 'Tampilkan Saldo' : 'Sembunyikan Saldo'"
        >
          <component :is="walletStore.hideBalance ? Eye : EyeOff" class="w-3 h-3 stroke-[2]" />
        </button>
      </div>

      <!-- Trial Countdown Badge (when trialing) -->
      <button
        v-if="subscriptionStore.isTrialing"
        type="button"
        @click="$emit('open-upgrade')"
        class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-amber-500/15 border border-amber-500/30 text-amber-500 hover:bg-amber-500/25 active:scale-95 text-[11px] font-extrabold transition cursor-pointer shadow-sm shadow-amber-500/10"
        :title="`Masa Uji Coba: ${subscriptionStore.daysRemaining ?? 7} hari tersisa. Klik untuk berlangganan.`"
      >
        <Clock class="w-3 h-3 text-amber-500 animate-pulse" />
        <span>Trial ({{ subscriptionStore.daysRemaining ?? 7 }} hari)</span>
      </button>

      <!-- Premium Active Badge -->
      <div
        v-else-if="authStore.isPremium"
        class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-brand-muted text-brand-default border border-brand-border text-[11px] font-extrabold"
      >
        <Sparkles class="w-3 h-3 text-amber-500" />
        <span>Premium</span>
      </div>

      <!-- Upgrade CTA (Free tier) -->
      <button
        v-else
        type="button"
        @click="$emit('open-upgrade')"
        class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-gradient-to-r from-amber-500/15 to-emerald-500/15 border border-brand-border text-brand-default hover:bg-brand-muted text-[11px] font-extrabold transition cursor-pointer"
      >
        <Sparkles class="w-3 h-3 text-amber-500" />
        <span>Upgrade Rp5k</span>
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
import { Eye, EyeOff, Sparkles, Clock } from 'lucide-vue-next'

defineProps({
  title: {
    type: String,
    default: 'Dashboard'
  },
  isOnline: {
    type: Boolean,
    default: true
  }
})

defineEmits(['open-upgrade'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const subscriptionStore = useSubscriptionStore()
</script>
