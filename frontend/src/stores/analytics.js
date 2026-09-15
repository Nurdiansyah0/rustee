import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useWalletStore } from './wallets'

export const useAnalyticsStore = defineStore('analytics', () => {
  // State
  const dashboardData = ref(null)
  const basicAnalytics = ref(null)
  const advancedAnalytics = ref(null)
  const isFeatureLocked = ref(false)
  const loading = ref(false)
  const error = ref(null)

  // Getters
  const cashFlow = computed(() => {
    return (
      dashboardData.value?.cash_flow || {
        total_income: 0,
        total_expenses: 0,
        net_cash_flow: 0,
      }
    )
  })

  const netCashFlowStatus = computed(() => {
    const net = cashFlow.value?.net_cash_flow || 0
    if (net > 0) return 'surplus'
    if (net < 0) return 'deficit'
    return 'neutral'
  })

  const savingsRatePercent = computed(() => {
    return basicAnalytics.value?.savings_rate_percent || 0
  })

  const healthScore = computed(() => {
    return advancedAnalytics.value?.financial_health_score || null
  })

  const monthlyTrend = computed(() => {
    return advancedAnalytics.value?.monthly_trend || 'Positif'
  })

  const runwayMonths = computed(() => {
    const walletStore = useWalletStore()
    const balance = walletStore.totalBalance || dashboardData.value?.total_balance || 0
    const expenses = cashFlow.value?.total_expenses || 0
    if (expenses <= 0) return 12 // Default positive runway if no expenses
    const months = (balance / expenses).toFixed(1)
    return parseFloat(months)
  })

  // Actions
  async function fetchDashboard() {
    loading.value = true
    error.value = null
    try {
      const data = await api.getDashboard()
      dashboardData.value = data
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat ringkasan dashboard.'
      console.error('Failed to fetch dashboard', err)
    } finally {
      loading.value = false
    }
  }

  async function fetchBasicAnalytics() {
    try {
      const data = await api.getBasicAnalytics()
      basicAnalytics.value = data
    } catch (err) {
      console.error('Failed to fetch basic analytics', err)
    }
  }

  async function fetchAdvancedAnalytics() {
    try {
      const data = await api.getAdvancedAnalytics()
      advancedAnalytics.value = data
      isFeatureLocked.value = false
    } catch (err) {
      if (err.status === 403 || err.code === 'FEATURE_LOCKED') {
        isFeatureLocked.value = true
        advancedAnalytics.value = null
      } else {
        console.warn('Advanced analytics access', err)
      }
    }
  }

  function $reset() {
    dashboardData.value = null
    basicAnalytics.value = null
    advancedAnalytics.value = null
    isFeatureLocked.value = false
    loading.value = false
    error.value = null
  }

  return {
    // State
    dashboardData,
    basicAnalytics,
    advancedAnalytics,
    isFeatureLocked,
    loading,
    error,
    // Getters
    cashFlow,
    netCashFlowStatus,
    savingsRatePercent,
    healthScore,
    monthlyTrend,
    runwayMonths,
    // Actions
    fetchDashboard,
    fetchBasicAnalytics,
    fetchAdvancedAnalytics,
    $reset,
  }
})
