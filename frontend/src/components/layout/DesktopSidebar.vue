<template>
  <aside class="hidden lg:flex w-64 xl:w-72 flex-col justify-between border-r border-border-subtle bg-surface-card p-3 xl:p-4 h-full min-h-0 z-20 select-none overflow-y-auto scroll-native">
    <!-- Top Cluster -->
    <div class="space-y-3.5 xl:space-y-4 shrink-0">
      <!-- FinRep Brand Logo -->
      <div class="flex items-center justify-between px-2 py-0.5">
        <FinrepIcon size="32" theme="auto" :show-typography="true" />
      </div>

      <!-- Workspace Selector Card & Dropdown -->
      <div class="relative" ref="sidebarSwitcherRef">
        <button
          type="button"
          @click="isDropdownOpen = !isDropdownOpen"
          class="w-full flex items-center justify-between p-2.5 rounded-xl bg-surface-subtle hover:bg-surface-sunken border border-border-subtle transition focus-ring cursor-pointer text-left group"
          :aria-expanded="isDropdownOpen"
          aria-haspopup="true"
        >
          <div class="flex items-center gap-2.5 min-w-0">
            <div
              :class="[
                'w-8 h-8 rounded-lg flex items-center justify-center shrink-0 font-bold',
                workspaceStore.isBusinessWorkspace ? 'bg-brand-default text-white' : 'bg-surface-sunken text-content-primary border border-border-subtle'
              ]"
            >
              <Building2 v-if="workspaceStore.isBusinessWorkspace" class="w-4 h-4 text-white" />
              <User v-else class="w-4 h-4 text-content-secondary" />
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-xs font-bold text-content-primary truncate">
                {{ workspaceStore.activeTenantName }}
              </div>
              <div class="text-[10px] text-content-muted flex items-center gap-1">
                <span class="capitalize">{{ workspaceStore.isBusinessWorkspace ? (workspaceStore.businessType || 'Bisnis') : 'Pribadi' }}</span>
                <span>•</span>
                <span class="capitalize">{{ workspaceStore.activeRole }}</span>
              </div>
            </div>
          </div>
          <ChevronsUpDown class="w-4 h-4 text-content-muted group-hover:text-content-primary shrink-0 ml-1.5" />
        </button>

        <!-- Dropdown Menu -->
        <div
          v-if="isDropdownOpen"
          class="absolute left-0 right-0 mt-2 max-h-80 overflow-y-auto scroll-native rounded-2xl bg-surface-card border border-border-default shadow-xl z-50 p-2 space-y-2 animate-in fade-in zoom-in-95 duration-150"
        >
          <div class="px-2 py-1 text-[11px] font-bold text-content-muted uppercase tracking-wider">
            Ganti Ruang Kerja
          </div>

          <!-- Personal Workspaces -->
          <div v-if="workspaceStore.personalWorkspaces.length > 0">
            <div class="px-2 py-0.5 text-[10px] font-semibold text-content-muted">Pribadi</div>
            <button
              v-for="ws in workspaceStore.personalWorkspaces"
              :key="ws.id"
              type="button"
              @click="handleSelectWorkspace(ws.id)"
              class="w-full flex items-center justify-between px-2.5 py-2 rounded-xl text-xs transition cursor-pointer text-left"
              :class="ws.id === workspaceStore.activeTenantId ? 'bg-brand-muted text-brand-default font-bold' : 'hover:bg-surface-subtle text-content-primary'"
            >
              <div class="flex items-center gap-2 min-w-0">
                <User class="w-4 h-4 text-content-muted shrink-0" />
                <span class="truncate">{{ ws.name }}</span>
              </div>
              <Check v-if="ws.id === workspaceStore.activeTenantId" class="w-4 h-4 text-brand-default shrink-0 ml-2" />
            </button>
          </div>

          <!-- Business Workspaces -->
          <div v-if="workspaceStore.businessWorkspaces.length > 0">
            <div class="px-2 py-0.5 text-[10px] font-semibold text-content-muted">Bisnis</div>
            <button
              v-for="ws in workspaceStore.businessWorkspaces"
              :key="ws.id"
              type="button"
              @click="handleSelectWorkspace(ws.id)"
              class="w-full flex items-center justify-between px-2.5 py-2 rounded-xl text-xs transition cursor-pointer text-left"
              :class="ws.id === workspaceStore.activeTenantId ? 'bg-brand-muted text-brand-default font-bold' : 'hover:bg-surface-subtle text-content-primary'"
            >
              <div class="flex items-center gap-2 min-w-0">
                <Building2 class="w-4 h-4 text-brand-default shrink-0" />
                <div class="min-w-0">
                  <div class="truncate">{{ ws.name }}</div>
                  <div class="text-[10px] text-content-muted capitalize">{{ ws.role || 'Member' }}</div>
                </div>
              </div>
              <Check v-if="ws.id === workspaceStore.activeTenantId" class="w-4 h-4 text-brand-default shrink-0 ml-2" />
            </button>
          </div>

          <!-- Divider & Action to Create New Business Workspace (PRD §5 & §69) -->
          <div class="pt-1.5 border-t border-border-subtle">
            <button
              type="button"
              @click="handleOpenCreateWorkspace"
              class="w-full flex items-center gap-2 px-2.5 py-2 rounded-xl text-xs font-bold text-brand-default hover:bg-brand-muted/50 transition cursor-pointer text-left focus-ring"
            >
              <Plus class="w-4 h-4 stroke-[2.5]" />
              <span>Buat Ruang Kerja Baru</span>
            </button>
          </div>
        </div>
      </div>

      <!-- Quick Action: Catat Transaksi (POS Keypad Trigger) -->
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

      <!-- Dynamic Capability-Driven Navigation Links -->
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
        class="w-full text-left p-2.5 rounded-xl bg-gradient-to-r from-brand-default/15 via-brand-default/10 to-brand-default/5 border border-brand-default/25 cursor-pointer hover:border-brand-default transition shadow-xs group focus-ring block"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-xs font-extrabold text-brand-default">
            <Clock class="w-3.5 h-3.5 animate-pulse text-brand-default" />
            <span>Masa Uji Coba</span>
          </div>
          <span class="text-[10px] font-extrabold text-brand-default bg-brand-default/20 border border-brand-default/30 px-1.5 py-0.5 rounded-full tabular-nums">
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
        class="w-full text-left p-2.5 rounded-xl bg-gradient-to-r from-brand-default/15 via-brand-default/10 to-surface-subtle border border-brand-border cursor-pointer hover:border-brand-default transition focus-ring block"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-xs font-bold text-brand-default">
            <Sparkles class="w-3.5 h-3.5" />
            <span>FinRep Pro</span>
          </div>
          <span class="text-[10px] font-extrabold text-brand-default bg-brand-default/15 border border-brand-default/25 px-1.5 py-0.5 rounded">
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
import { ref, computed, markRaw, onMounted, onUnmounted } from 'vue'
import { Button, Badge } from '@/components/ui'
import FinrepIcon from '@/components/FinrepIcon.vue'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useSubscriptionStore } from '@/stores/subscription'
import { useWorkspaceStore } from '@/stores/workspace'
import {
  LayoutDashboard,
  Receipt,
  PieChart,
  User,
  Plus,
  LogOut,
  Sparkles,
  Clock,
  Building2,
  ChevronsUpDown,
  Check,
  FileText,
  BookOpen
} from 'lucide-vue-next'

defineProps({
  activeTab: {
    type: String,
    default: 'home'
  }
})

const emit = defineEmits(['select-tab', 'open-add', 'open-upgrade', 'logout', 'open-create-workspace'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const subscriptionStore = useSubscriptionStore()
const workspaceStore = useWorkspaceStore()

const isDropdownOpen = ref(false)
const sidebarSwitcherRef = ref(null)

function handleSelectWorkspace(id) {
  isDropdownOpen.value = false
  workspaceStore.switchWorkspace(id)
}

function handleOpenCreateWorkspace() {
  isDropdownOpen.value = false
  emit('open-create-workspace')
}

function handleClickOutside(event) {
  if (sidebarSwitcherRef.value && !sidebarSwitcherRef.value.contains(event.target)) {
    isDropdownOpen.value = false
  }
}

onMounted(() => {
  if (typeof document !== 'undefined') {
    document.addEventListener('click', handleClickOutside)
  }
})

onUnmounted(() => {
  if (typeof document !== 'undefined') {
    document.removeEventListener('click', handleClickOutside)
  }
})

// Dynamic Capability-Driven Navigation Links
const navItems = computed(() => {
  const items = [
    { id: 'home', label: 'Dashboard', icon: markRaw(LayoutDashboard) },
    { id: 'transactions', label: 'Riwayat Transaksi', icon: markRaw(Receipt) }
  ]

  // Capability modules for business workspaces
  if (workspaceStore.hasCapability('invoicing')) {
    items.push({ id: 'invoices', label: 'Faktur & Tagihan', icon: markRaw(FileText) })
  }
  if (workspaceStore.hasCapability('accounting')) {
    items.push({ id: 'accounting', label: 'Buku Kas & Jurnal', icon: markRaw(BookOpen) })
  }

  items.push(
    { id: 'analytics', label: 'Analisis & Proyeksi', icon: markRaw(PieChart) },
    { id: 'profile', label: 'Profil & Akun', icon: markRaw(User) }
  )

  return items
})
</script>
