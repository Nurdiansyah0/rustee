<template>
  <div class="min-h-dvh w-full bg-surface-canvas text-content-primary selection:bg-brand-default selection:text-white flex flex-col antialiased">
    <!-- Unauthenticated State: Split-Screen Auth -->
    <SplitScreenAuth
      v-if="!authStore.isAuthenticated"
      @authenticated="handleAuthenticated"
    />

    <!-- Authenticated State: Responsive Dual-Mode Shell -->
    <div v-else class="flex-1 flex flex-col lg:flex-row min-w-0 min-h-0 h-dvh max-h-dvh overflow-hidden">
      <!-- Desktop Sidebar (Hidden on screens <1024px) -->
      <DesktopSidebar
        :active-tab="currentTab"
        @select-tab="handleSelectTab"
        @open-add="showAddModal = true"
        @open-upgrade="subscriptionStore.openUpgradeModal()"
        @logout="handleLogout"
      />

      <!-- Main View Stream -->
      <div class="flex-1 flex flex-col min-w-0 min-h-0 h-full overflow-hidden">
        <!-- Top App Header (Pinned with Notch / Safe Area support) -->
        <AppHeader
          :title="currentTabTitle"
          :is-online="isOnline"
          :current-tab="currentTab"
          @open-upgrade="subscriptionStore.openUpgradeModal()"
        />

        <!-- View Content Canvas (Isolated Native Momentum Scroll Container) -->
        <main
          ref="mainScrollRef"
          class="flex-1 min-h-0 scroll-native px-4 sm:px-6 md:px-8 pt-4 sm:pt-6 pb-[calc(4.75rem+env(safe-area-inset-bottom,0px))] lg:pb-6"
        >
          <div class="w-full max-w-7xl mx-auto space-y-5 lg:space-y-6">
            <Transition :name="transitionName" mode="out-in">
              <HomeView
                v-if="currentTab === 'home'"
                key="home"
                :dashboard="analyticsStore.dashboardData || {}"
                @nav="handleSelectTab"
                @open-add="showAddModal = true"
              />

              <TransactionsView
                v-else-if="currentTab === 'transactions'"
                key="transactions"
                ref="transactionsViewRef"
                @open-add="showAddModal = true"
                @refresh="handleTransactionCreated"
              />

              <AnalyticsView
                v-else-if="currentTab === 'analytics'"
                key="analytics"
                :user-tier="authStore.isPremium ? 'premium' : 'free'"
                @open-upgrade="subscriptionStore.openUpgradeModal()"
              />

              <ProfileView
                v-else-if="currentTab === 'profile'"
                key="profile"
                :user="authStore.user"
                :user-tier="authStore.isPremium ? 'premium' : 'free'"
                @open-upgrade="subscriptionStore.openUpgradeModal()"
                @logout="handleLogout"
              />
            </Transition>

            <!-- Desktop App Footnote (Hidden on mobile PWA stream) -->
            <footer class="hidden lg:flex pt-3 pb-2 text-center text-xs text-content-muted items-center justify-center gap-2 border-t border-border-subtle/60">
              <span class="text-[11px] text-content-muted">FinRep by Invinite.id - Digital Creative Solutions</span>
              <img
                src="/icons/Invinite_Logo.png"
                alt="Invinite.id"
                class="h-3.5 w-auto max-h-3.5 object-contain opacity-75 select-none"
                loading="lazy"
              />
            </footer>
          </div>
        </main>
      </div>

      <!-- Mobile Bottom Navigation (Hidden on desktop ≥768px) -->
      <MobileBottomNav
        :active-tab="currentTab"
        @select-tab="handleSelectTab"
        @open-add="showAddModal = true"
      />
    </div>

    <!-- Modals -->
    <AddTransactionModal
      :is-open="showAddModal"
      :accounts="walletStore.wallets"
      :categories="categoryStore.categories"
      @close="showAddModal = false"
      @transaction-created="handleTransactionCreated"
    />

    <UpgradeModal
      :is-open="subscriptionStore.isUpgradeModalOpen"
      @close="subscriptionStore.closeUpgradeModal()"
      @success="handleUpgradeSuccess"
    />

    <!-- Progressive Personalization Onboarding Modal (§3, §4, §6) -->
    <ProgressiveOnboardingModal
      :is-open="showOnboardingModal"
      :initial-name="onboardingInitialName"
      @completed="handleOnboardingCompleted"
      @close="showOnboardingModal = false"
    />
  </div>
</template>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useCategoryStore } from '@/stores/categories'
import { useTransactionStore } from '@/stores/transactions'
import { useAnalyticsStore } from '@/stores/analytics'
import { useSubscriptionStore } from '@/stores/subscription'
import { useRealtimeStore } from '@/stores/realtime'
import { syncService } from '@/services/sync'

import DesktopSidebar from '@/components/layout/DesktopSidebar.vue'
import MobileBottomNav from '@/components/layout/MobileBottomNav.vue'
import AppHeader from '@/components/layout/AppHeader.vue'
import SplitScreenAuth from '@/components/auth/SplitScreenAuth.vue'

import HomeView from '@/views/HomeView.vue'
import TransactionsView from '@/views/TransactionsView.vue'
import AnalyticsView from '@/views/AnalyticsView.vue'
import ProfileView from '@/views/ProfileView.vue'
import AddTransactionModal from '@/components/AddTransactionModal.vue'
import UpgradeModal from '@/components/UpgradeModal.vue'
import ProgressiveOnboardingModal from '@/components/ProgressiveOnboardingModal.vue'

const authStore = useAuthStore()
const walletStore = useWalletStore()
const categoryStore = useCategoryStore()
const transactionStore = useTransactionStore()
const analyticsStore = useAnalyticsStore()
const subscriptionStore = useSubscriptionStore()
const realtimeStore = useRealtimeStore()

const currentTab = ref('home')
const showAddModal = ref(false)
const showOnboardingModal = ref(false)
const onboardingInitialName = ref('')
const transactionsViewRef = ref(null)
const mainScrollRef = ref(null)
const isOnline = ref(typeof navigator !== 'undefined' ? navigator.onLine : true)

// Dynamic Navigation Transition (Compose Navigation Feel)
const tabOrder = ['home', 'transactions', 'analytics', 'profile']
const transitionName = ref('slide-left')

const currentTabTitle = computed(() => {
  const titles = {
    home: 'Dashboard Utama',
    transactions: 'Riwayat Transaksi',
    analytics: 'Analisis & Proyeksi',
    profile: 'Profil & Pengaturan Akun'
  }
  return titles[currentTab.value] || 'FinRep'
})

function handleSelectTab(tab) {
  if (tab === currentTab.value) {
    // Tap on active tab in native apps smoothly scrolls to top
    if (mainScrollRef.value) {
      mainScrollRef.value.scrollTo({ top: 0, behavior: 'smooth' })
    }
    return
  }

  const oldIndex = tabOrder.indexOf(currentTab.value)
  const newIndex = tabOrder.indexOf(tab)
  transitionName.value = newIndex >= oldIndex ? 'slide-left' : 'slide-right'

  // Seamless View Transitions API with instant fallback
  if (typeof document !== 'undefined' && 'startViewTransition' in document) {
    document.startViewTransition(() => {
      currentTab.value = tab
      if (mainScrollRef.value) {
        mainScrollRef.value.scrollTop = 0
      }
    })
  } else {
    currentTab.value = tab
    if (mainScrollRef.value) {
      mainScrollRef.value.scrollTop = 0
    }
  }
}

function cleanupAuthUrlParams() {
  if (typeof window === 'undefined') return
  try {
    const url = new URL(window.location.href)
    if (url.searchParams.has('auth') || url.searchParams.has('mode') || url.searchParams.has('view')) {
      url.searchParams.delete('auth')
      url.searchParams.delete('mode')
      url.searchParams.delete('view')
      window.history.replaceState({}, '', url.pathname + (url.search ? url.search : '') + url.hash)
    }
  } catch {}
}

async function handleAuthenticated(authEvent = {}) {
  cleanupAuthUrlParams()

  // Initialize Foreground WebSocket connection (§25, §27)
  realtimeStore.initRealtime()

  // Check if onboarding flow should be presented for newly registered user (§3, §4, §6)
  if (authEvent?.isNewUser) {
    onboardingInitialName.value = authEvent.name || authStore.user?.display_name || authStore.user?.name || ''
    showOnboardingModal.value = true
  }

  await Promise.allSettled([
    walletStore.fetchWallets(),
    categoryStore.fetchCategories(),
    analyticsStore.fetchDashboard(),
    subscriptionStore.fetchSubscriptionStatus(),
    syncService.reconcileOnReconnect()
  ])
}

async function handleOnboardingCompleted() {
  showOnboardingModal.value = false
  await handleAuthenticated()
}

// Reactively respond to cross-tab multi-session login or logout
watch(
  () => authStore.isAuthenticated,
  (isAuth, wasAuth) => {
    if (isAuth && !wasAuth) {
      cleanupAuthUrlParams()
      handleAuthenticated()
    } else if (!isAuth && wasAuth) {
      currentTab.value = 'home'
      showOnboardingModal.value = false
      realtimeStore.cleanupRealtime()
    }
  }
)

async function handleTransactionCreated() {
  await Promise.allSettled([
    analyticsStore.fetchDashboard(),
    walletStore.fetchWallets()
  ])
  if (transactionsViewRef.value?.reload) {
    transactionsViewRef.value.reload()
  }
}

async function handleUpgradeSuccess() {
  await Promise.allSettled([
    authStore.checkAuth(),
    subscriptionStore.fetchSubscriptionStatus(),
    analyticsStore.fetchDashboard()
  ])
}

function handleLogout() {
  realtimeStore.cleanupRealtime()
  showOnboardingModal.value = false
  authStore.logout()
  currentTab.value = 'home'
}

function updateOnlineStatus() {
  isOnline.value = navigator.onLine
}

async function handleOnline() {
  updateOnlineStatus()
  if (authStore.isAuthenticated) {
    const ok = await authStore.checkAuth()
    if (ok) {
      // Reconcile pending offline mutations and cursor deltas on reconnect (§25, §26)
      await syncService.reconcileOnReconnect()
      await handleAuthenticated()
    }
  }
}

onMounted(async () => {
  window.addEventListener('online', handleOnline)
  window.addEventListener('offline', updateOnlineStatus)
  if (authStore.isAuthenticated) {
    cleanupAuthUrlParams()
  }
  const ok = await authStore.checkAuth()
  if (ok) {
    await handleAuthenticated()
  }
})

onUnmounted(() => {
  window.removeEventListener('online', handleOnline)
  window.removeEventListener('offline', updateOnlineStatus)
  realtimeStore.cleanupRealtime()
})
</script>
