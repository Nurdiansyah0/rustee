<template>
  <header class="sticky top-0 z-30 shrink-0 bg-surface-card/90 backdrop-blur-md border-b border-border-subtle px-4 sm:px-6 md:px-8 pt-[max(env(safe-area-inset-top),0.875rem)] pb-3 flex items-center justify-between select-none">
    <!-- Left Section: Title & Workspace Switcher -->
    <div class="flex items-center gap-3 sm:gap-4 min-w-0">
      <!-- Mobile / Tablet Header (Shown on screens <1024px) -->
      <div class="lg:hidden flex items-center gap-2">
        <FinrepIcon size="30" theme="auto" :show-typography="false" />
        <h1 class="text-sm font-extrabold text-content-primary tracking-tight truncate max-w-[100px] sm:max-w-none">
          {{ title }}
        </h1>
      </div>

      <!-- Desktop Page Title & Breadcrumb (Shown on desktop >=1024px) -->
      <div class="hidden lg:block shrink-0">
        <h1 class="text-base font-extrabold text-content-primary tracking-tight">
          {{ title }}
        </h1>
      </div>

      <!-- Workspace Switcher Pill & Dropdown -->
      <div class="relative" ref="switcherRef">
        <button
          type="button"
          @click="isDropdownOpen = !isDropdownOpen"
          class="inline-flex items-center gap-1.5 px-2.5 py-1 sm:px-3 sm:py-1.5 rounded-xl bg-surface-subtle hover:bg-surface-sunken border border-border-subtle text-xs font-semibold text-content-primary transition focus-ring cursor-pointer max-w-[160px] sm:max-w-[220px]"
          :aria-expanded="isDropdownOpen"
          aria-haspopup="true"
          title="Ganti Ruang Kerja"
        >
          <Building2 v-if="workspaceStore.isBusinessWorkspace" class="w-3.5 h-3.5 text-brand-default shrink-0" />
          <User v-else class="w-3.5 h-3.5 text-content-secondary shrink-0" />
          <span class="truncate font-bold">
            {{ workspaceStore.activeTenantName }}
          </span>
          <span
            :class="[
              'hidden sm:inline-block px-1.5 py-0.2 text-[9px] font-extrabold rounded-md uppercase tracking-wider',
              workspaceStore.isBusinessWorkspace
                ? 'bg-brand-muted text-brand-default border border-brand-border'
                : 'bg-surface-card text-content-muted border border-border-subtle'
            ]"
          >
            {{ workspaceStore.isBusinessWorkspace ? 'Bisnis' : 'Pribadi' }}
          </span>
          <ChevronDown
            class="w-3.5 h-3.5 text-content-muted shrink-0 transition-transform duration-200"
            :class="{ 'rotate-180': isDropdownOpen }"
          />
        </button>

        <!-- Dropdown Menu -->
        <div
          v-if="isDropdownOpen"
          class="absolute left-0 mt-2 w-72 max-h-96 overflow-y-auto scroll-native rounded-2xl bg-surface-card border border-border-default shadow-xl z-50 p-2 space-y-2 animate-in fade-in zoom-in-95 duration-150"
        >
          <div class="px-2 py-1 text-[11px] font-bold text-content-muted uppercase tracking-wider">
            Pilih Ruang Kerja
          </div>

          <!-- Personal Workspaces -->
          <div v-if="workspaceStore.personalWorkspaces.length > 0">
            <div class="px-2 py-0.5 text-[10px] font-semibold text-content-muted">Ruang Kerja Pribadi</div>
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
            <div class="px-2 py-0.5 text-[10px] font-semibold text-content-muted">Ruang Kerja Bisnis</div>
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
    </div>

    <!-- Right Section: Quick Balance, Status, Upgrade Pill -->
    <div class="flex items-center gap-2.5 sm:gap-3 shrink-0">
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
import { ref, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useSubscriptionStore } from '@/stores/subscription'
import { useWorkspaceStore } from '@/stores/workspace'
import FinrepIcon from '@/components/FinrepIcon.vue'
import { Sparkles, Clock, Building2, User, ChevronDown, Check, Plus } from 'lucide-vue-next'

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

const emit = defineEmits(['open-upgrade', 'open-create-workspace'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const subscriptionStore = useSubscriptionStore()
const workspaceStore = useWorkspaceStore()

const isDropdownOpen = ref(false)
const switcherRef = ref(null)

function handleSelectWorkspace(id) {
  isDropdownOpen.value = false
  workspaceStore.switchWorkspace(id)
}

function handleOpenCreateWorkspace() {
  isDropdownOpen.value = false
  emit('open-create-workspace')
}

function handleClickOutside(event) {
  if (switcherRef.value && !switcherRef.value.contains(event.target)) {
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
</script>
