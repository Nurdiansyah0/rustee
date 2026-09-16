<template>
  <aside class="hidden md:flex w-64 lg:w-72 flex-col justify-between border-r border-border-subtle bg-surface-card p-4 h-full sticky top-0 z-20 select-none">
    <!-- Top Cluster -->
    <div class="space-y-6">
      <!-- FinRep Brand Logo -->
      <div class="flex items-center justify-between px-2 py-1">
        <FinrepIcon size="36" theme="auto" :show-typography="true" />
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
            'w-full h-11 px-3.5 rounded-xl flex items-center gap-3 text-xs font-semibold transition text-left focus-ring cursor-pointer',
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
    <div class="space-y-3 pt-4 border-t border-border-subtle">
      <!-- Total Saldo Mini Widget -->
      <div class="p-3 bg-surface-subtle rounded-xl border border-border-subtle">
        <div class="flex items-center justify-between text-[11px] font-medium text-content-muted">
          <span>Total Saldo</span>
          <button
            type="button"
            @click="walletStore.toggleHideBalance()"
            class="hover:text-content-primary p-0.5 rounded transition cursor-pointer"
            :title="walletStore.hideBalance ? 'Tampilkan Saldo' : 'Sembunyikan Saldo'"
          >
            <component :is="walletStore.hideBalance ? Eye : EyeOff" class="w-3.5 h-3.5 stroke-[2]" />
          </button>
        </div>
        <div class="text-base font-black text-content-primary tabular-nums tracking-tight mt-1">
          {{ walletStore.displayTotalBalance }}
        </div>
      </div>

      <!-- Trial Status Card (when trialing) -->
      <div
        v-if="subscriptionStore.isTrialing"
        @click="$emit('open-upgrade')"
        class="p-3 rounded-xl bg-gradient-to-r from-amber-500/15 via-amber-500/10 to-amber-600/15 border border-amber-500/30 cursor-pointer hover:border-amber-500 transition shadow-xs group"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-xs font-extrabold text-amber-500">
            <Clock class="w-3.5 h-3.5 animate-pulse text-amber-500" />
            <span>Masa Uji Coba</span>
          </div>
          <span class="text-[10px] font-extrabold text-amber-500 bg-amber-500/20 border border-amber-500/30 px-1.5 py-0.5 rounded-full tabular-nums">
            {{ subscriptionStore.daysRemaining ?? 7 }} Hari
          </span>
        </div>
        <p class="text-[10px] text-content-secondary mt-1 leading-snug">
          Fitur Pro aktif. Kunci langganan Rp5k/bln sebelum masa uji coba berakhir.
        </p>
      </div>

      <!-- Upgrade Banner (for Free Tier) -->
      <div
        v-else-if="!authStore.isPremium"
        @click="$emit('open-upgrade')"
        class="p-3 rounded-xl bg-gradient-to-r from-amber-500/10 via-emerald-500/10 to-teal-500/10 border border-brand-border cursor-pointer hover:border-brand-default transition"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-xs font-bold text-brand-default">
            <Sparkles class="w-3.5 h-3.5" />
            <span>FinRep Pro</span>
          </div>
          <span class="text-[10px] font-extrabold text-amber-600 bg-amber-50 dark:bg-amber-950/40 px-1.5 py-0.5 rounded">
            Rp5k/bln
          </span>
        </div>
        <p class="text-[10px] text-content-secondary mt-1">
          Buka analitik runway & proyeksi keuangan tanpa batas.
        </p>
      </div>

      <!-- User Profile Card -->
      <div class="p-2 flex items-center justify-between rounded-xl hover:bg-surface-subtle transition">
        <div class="flex items-center gap-2.5 overflow-hidden">
          <div class="w-8 h-8 rounded-full bg-brand-muted text-brand-default font-extrabold text-xs flex items-center justify-center shrink-0">
            {{ authStore.initials }}
          </div>
          <div class="overflow-hidden">
            <div class="text-xs font-bold text-content-primary truncate flex items-center gap-1.5">
              <span>{{ authStore.displayName }}</span>
              <Badge v-if="subscriptionStore.isTrialing" variant="warning" size="sm">Trial</Badge>
              <Badge v-else-if="authStore.isPremium" variant="premium" size="sm">Pro</Badge>
            </div>
            <div class="text-[10px] text-content-muted truncate">{{ authStore.user?.email }}</div>
          </div>
        </div>

        <button
          type="button"
          @click="$emit('logout')"
          class="p-1.5 text-content-muted hover:text-expense-default rounded-lg transition cursor-pointer"
          title="Keluar (Logout)"
        >
          <LogOut class="w-4 h-4 stroke-[2]" />
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
  Eye,
  EyeOff,
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

const navItems = [
  { id: 'home', label: 'Dashboard', icon: markRaw(LayoutDashboard) },
  { id: 'transactions', label: 'Riwayat Transaksi', icon: markRaw(Receipt) },
  { id: 'analytics', label: 'Analisis & Proyeksi', icon: markRaw(PieChart) },
  { id: 'profile', label: 'Profil & Akun', icon: markRaw(User) }
]
</script>
