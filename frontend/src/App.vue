<template>
  <div class="min-h-dvh w-full bg-surface-canvas text-content-primary selection:bg-brand-default selection:text-white flex flex-col antialiased">
    <!-- Unauthenticated State: Split-Screen Auth -->
    <SplitScreenAuth
      v-if="!authStore.isAuthenticated"
      @authenticated="handleAuthenticated"
    />

    <!-- Authenticated State: Responsive Dual-Mode Shell -->
    <div v-else class="flex-1 flex flex-col md:flex-row min-w-0 h-dvh max-h-dvh overflow-hidden">
      <!-- Desktop Sidebar (Hidden on mobile <768px) -->
      <DesktopSidebar
        :active-tab="currentTab"
        @select-tab="handleSelectTab"
        @open-add="showAddModal = true"
        @open-upgrade="subscriptionStore.openUpgradeModal()"
        @logout="handleLogout"
      />

      <!-- Main View Stream -->
      <div class="flex-1 flex flex-col min-w-0 h-full overflow-hidden">
        <!-- Top App Header (Pinned with Notch / Safe Area support) -->
        <AppHeader
          :title="currentTabTitle"
          :is-online="isOnline"
          @open-upgrade="subscriptionStore.openUpgradeModal()"
        />

        <!-- View Content Canvas (Isolated Native Momentum Scroll Container) -->
        <main
          ref="mainScrollRef"
          class="flex-1 scroll-native px-4 sm:px-6 md:px-8 py-4 sm:py-6 md:py-8 pb-safe-nav md:pb-8"
        >
          <div class="w-full max-w-7xl mx-auto space-y-6">
            <Transition :name="transitionName" mode="out-in">
              <HomeView
                v-if="currentTab === 'home'"
                key="home"
                :dashboard="analyticsStore.dashboardData || {}"
                @nav="handleSelectTab"
                @open-add="showAddModal = true"
                @open-accounts="handleSelectTab('profile')"
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

import DesktopSidebar from '@/components/layout/DesktopSidebar.vue'
import MobileBottomNav from '@/components/layout/MobileBottomNav.vue'
import AppHeader from '@/components/layout/AppHeader.vue'
import SplitScreenAuth from '@/components/auth/SplitScreenAuth.vue'

import HomeView from '@/views/HomeView.vue'
import TransactionsView from '@/views/TransactionsView.vue'
import AnalyticsView from '@/views/AnalyticsView.vue'
import ProfileView from '@/views/ProfileView.vue'
import AddTransactionModal from '@/views/AddTransactionModal.vue'
import UpgradeModal from '@/components/UpgradeModal.vue'

const authStore = useAuthStore()
const walletStore = useWalletStore()
const categoryStore = useCategoryStore()
const transactionStore = useTransactionStore()
const analyticsStore = useAnalyticsStore()
const subscriptionStore = useSubscriptionStore()

const currentTab = ref('home')
const showAddModal = ref(false)
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

async function handleAuthenticated() {
  cleanupAuthUrlParams()
  await Promise.allSettled([
    walletStore.fetchWallets(),
    categoryStore.fetchCategories(),
    analyticsStore.fetchDashboard(),
    subscriptionStore.fetchSubscriptionStatus()
  ])
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
})
</script>
