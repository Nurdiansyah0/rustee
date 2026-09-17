<template>
  <aside class="hidden lg:flex w-64 xl:w-72 flex-col justify-between border-r border-border-subtle bg-surface-card p-3 xl:p-4 h-full min-h-0 z-20 select-none overflow-y-auto scroll-native">
    <!-- Top Cluster -->
    <div class="space-y-3.5 xl:space-y-4 shrink-0">
      <!-- FinRep Brand Logo -->
      <div class="flex items-center justify-between px-2 py-0.5">
        <FinrepIcon size="32" theme="auto" :show-typography="true" />
      </div>

      <!-- Quick Action: Catat Transaksi -->
      <Button
        variant="primary"
        size="md"
        fullWidth
        @click="$emit('open-add')"
      >
        <template #prefix>
          <Plus class="w-4 h-4 mr-1.5 stroke-[2.5]" aria-hidden="true" />
        </template>
        Catat Transaksi
      </Button>

      <!-- Navigation Links -->
      <nav class="space-y-1">
        <button
          v-for="item in navItems"
          :key="item.id"
          type="button"
          @click="$emit('select-tab', item.id)"
          :class="[
            'w-full h-9 xl:h-10 px-3 rounded-xl flex items-center gap-3 text-xs font-semibold transition text-left focus-ring cursor-pointer',
            activeTab === item.id
              ? 'bg-brand-muted text-brand-default font-bold shadow-xs'
              : 'text-content-secondary hover:text-content-primary hover:bg-surface-subtle'
          ]"
        >
          <component
            :is="item.icon"
            :class="[
              'w-4 h-4 stroke-[2]',
              activeTab === item.id ? 'text-brand-default' : 'text-content-muted'
            ]"
            aria-hidden="true"
          />
          <span>{{ item.label }}</span>
        </button>
      </nav>
    </div>

    <!-- Bottom Cluster -->
    <div class="space-y-2 pt-3 border-t border-border-subtle shrink-0">
      <!-- Trial Status Card (when trialing) -->
      <button
        v-if="subscriptionStore.isTrialing"
        type="button"
        @click="$emit('open-upgrade')"
        class="w-full text-left p-2.5 rounded-xl bg-gradient-to-r from-amber-500/15 via-amber-500/10 to-amber-600/15 border border-amber-500/30 cursor-pointer hover:border-amber-500 transition shadow-xs group focus-ring block"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-xs font-extrabold text-amber-500">
            <Clock class="w-3.5 h-3.5 animate-pulse text-amber-500" />
            <span>Masa Uji Coba</span>
          </div>
          <span class="text-[10px] font-extrabold text-amber-500 bg-amber-500/20 border border-amber-500/30 px-1.5 py-0.5 rounded-full tabular-nums">
            {{ subscriptionStore.daysRemaining ?? 90 }} Hari
          </span>
        </div>
        <p class="text-[10px] text-content-secondary mt-1 leading-snug">
          Fitur Pro aktif. Kunci langganan Rp10k/bln sebelum masa uji coba berakhir.
        </p>
      </button>

      <!-- Upgrade Banner (for Free Tier) -->
      <button
        v-else-if="!authStore.isPremium"
        type="button"
        @click="$emit('open-upgrade')"
        class="w-full text-left p-2.5 rounded-xl bg-gradient-to-r from-amber-500/10 via-emerald-500/10 to-teal-500/10 border border-brand-border cursor-pointer hover:border-brand-default transition focus-ring block"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-xs font-bold text-brand-default">
            <Sparkles class="w-3.5 h-3.5" />
            <span>FinRep Pro</span>
          </div>
          <span class="text-[10px] font-extrabold text-amber-600 bg-amber-50 dark:bg-amber-950/40 px-1.5 py-0.5 rounded">
            Rp10k/bln
          </span>
        </div>
        <p class="text-[10px] text-content-secondary mt-0.5 leading-snug">
          Buka analitik runway & proyeksi keuangan.
        </p>
      </button>

      <!-- User Profile Card -->
      <div class="p-1.5 flex items-center justify-between rounded-xl hover:bg-surface-subtle transition shrink-0">
        <div class="flex items-center gap-2 overflow-hidden min-w-0">
          <div class="w-8 h-8 rounded-full bg-brand-muted text-brand-default font-extrabold text-xs flex items-center justify-center shrink-0">
            {{ authStore.initials }}
          </div>
          <div class="overflow-hidden min-w-0">
            <div class="text-xs font-bold text-content-primary truncate flex items-center gap-1.5">
              <span class="truncate">{{ authStore.displayName }}</span>
              <Badge v-if="subscriptionStore.isTrialing" variant="warning" size="sm">Trial</Badge>
              <Badge v-else-if="authStore.isPremium" variant="premium" size="sm">Pro</Badge>
            </div>
            <div class="text-[10px] text-content-muted truncate">{{ authStore.user?.email }}</div>
          </div>
        </div>

        <button
          type="button"
          @click="$emit('logout')"
          class="min-w-[44px] min-h-[44px] flex items-center justify-center text-content-muted hover:text-expense-default rounded-lg transition cursor-pointer shrink-0 ml-1 focus-ring"
          title="Keluar (Logout)"
          aria-label="Keluar dari akun FinRep"
        >
          <LogOut class="w-4 h-4 stroke-[2]" aria-hidden="true" />
        </button>
      </div>
    </div>
  </aside>
</template>

<script setup>
import { markRaw } from 'vue'
import { Button, Badge } from '@/components/ui'
import FinrepIcon from '@/components/FinrepIcon.vue'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useSubscriptionStore } from '@/stores/subscription'
import {
  LayoutDashboard,
  Receipt,
  PieChart,
  User,
  Plus,
  LogOut,
  Sparkles,
  Clock
} from 'lucide-vue-next'

defineProps({
  activeTab: {
    type: String,
    default: 'home'
  }
})

defineEmits(['select-tab', 'open-add', 'open-upgrade', 'logout'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const subscriptionStore = useSubscriptionStore()

const navItems = [
  { id: 'home', label: 'Dashboard', icon: markRaw(LayoutDashboard) },
  { id: 'transactions', label: 'Riwayat Transaksi', icon: markRaw(Receipt) },
  { id: 'analytics', label: 'Analisis & Proyeksi', icon: markRaw(PieChart) },
  { id: 'profile', label: 'Profil & Akun', icon: markRaw(User) }
]
</script>
