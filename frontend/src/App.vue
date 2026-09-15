<template>
  <div class="min-h-screen bg-surface-canvas text-content-primary selection:bg-brand-default selection:text-white flex flex-col antialiased">
    <!-- Unauthenticated State: Split-Screen Auth -->
    <SplitScreenAuth
      v-if="!authStore.isAuthenticated"
      @authenticated="handleAuthenticated"
    />

    <!-- Authenticated State: Responsive Dual-Mode Shell -->
    <div v-else class="flex-1 flex flex-col md:flex-row min-w-0">
      <!-- Desktop Sidebar (Hidden on mobile <768px) -->
      <DesktopSidebar
        :active-tab="currentTab"
        @select-tab="handleSelectTab"
        @open-add="showAddModal = true"
        @open-upgrade="subscriptionStore.openUpgradeModal()"
        @logout="handleLogout"
      />

      <!-- Main View Stream -->
      <div class="flex-1 flex flex-col min-w-0 overflow-x-hidden">
        <!-- Top App Header -->
        <AppHeader
          :title="currentTabTitle"
          :is-online="isOnline"
          @open-upgrade="subscriptionStore.openUpgradeModal()"
        />

        <!-- View Content Canvas (Responsive Max-Width Container) -->
        <main class="flex-1 w-full max-w-7xl mx-auto p-4 sm:p-6 md:p-8 space-y-6">
          <HomeView
            v-if="currentTab === 'home'"
            :dashboard="analyticsStore.dashboardData || {}"
            @nav="handleSelectTab"
            @open-add="showAddModal = true"
            @open-accounts="handleSelectTab('profile')"
          />

          <TransactionsView
            v-else-if="currentTab === 'transactions'"
            ref="transactionsViewRef"
            @open-add="showAddModal = true"
            @refresh="handleTransactionCreated"
          />

          <AnalyticsView
            v-else-if="currentTab === 'analytics'"
            :user-tier="authStore.isPremium ? 'premium' : 'free'"
            @open-upgrade="subscriptionStore.openUpgradeModal()"
          />

          <ProfileView
            v-else-if="currentTab === 'profile'"
            :user="authStore.user"
            :user-tier="authStore.isPremium ? 'premium' : 'free'"
            @open-upgrade="subscriptionStore.openUpgradeModal()"
            @logout="handleLogout"
          />
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
import { ref, computed, onMounted, onUnmounted } from 'vue'
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
const isOnline = ref(typeof navigator !== 'undefined' ? navigator.onLine : true)

const currentTabTitle = computed(() => {
  const titles = {
    home: 'Dashboard Utama',
    transactions: 'Riwayat Transaksi',
    analytics: 'Analisis & Proyeksi',
    profile: 'Profil & Pengaturan Akun'
  }
  return titles[currentTab.value] || 'Invinite Finance'
})

function handleSelectTab(tab) {
  currentTab.value = tab
}

async function handleAuthenticated() {
  await Promise.allSettled([
    walletStore.fetchWallets(),
    categoryStore.fetchCategories(),
    analyticsStore.fetchDashboard(),
    subscriptionStore.fetchSubscriptionStatus()
  ])
}

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
