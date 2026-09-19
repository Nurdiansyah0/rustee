import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useAuthStore } from './auth'

export const useSubscriptionStore = defineStore('subscription', () => {
  // State
  const tier = ref('free')
  const status = ref('free')
  const hasUsedTrialState = ref(false)
  const priceMonthly = ref(10000)
  const priceAnnual = ref(110000)
  const features = ref([])
  const isUpgradeModalOpen = ref(false)
  const targetLockedFeature = ref(null)
  const checkoutLoading = ref(false)
  const checkoutError = ref(null)
  const daysRemaining = ref(null)
  const currentPeriodEnd = ref(null)

  // Getters
  const isPremium = computed(() => tier.value === 'premium')
  const isTrialing = computed(() => status.value === 'trialing')
  const hasUsedTrial = computed(() => hasUsedTrialState.value || status.value === 'trial_expired')

  function canAccess(feature) {
    if (isPremium.value) return true
    return features.value.includes(feature)
  }

  const formattedPrice = computed(() => 'Rp 10.000 / bulan')
  const formattedAnnualPrice = computed(() => 'Rp 110.000 / tahun')

  // Actions
  async function fetchSubscriptionStatus() {
    try {
      const res = await api.getSubscriptionStatus()
      tier.value = res?.tier || 'free'
      status.value = res?.status || 'free'
      hasUsedTrialState.value = Boolean(res?.has_used_trial || res?.status === 'trial_expired')
      priceMonthly.value = res?.price_monthly ?? (res?.tier === 'premium' && res?.status !== 'trialing' ? 10000 : 0)
      priceAnnual.value = res?.price_annual ?? 110000
      features.value = res?.features || []
      daysRemaining.value = res?.days_remaining ?? null
      currentPeriodEnd.value = res?.current_period_end ?? null
      return res
    } catch (err) {
      console.error('Failed to fetch subscription status', err)
      throw err
    }
  }

  function openUpgradeModal(featureName = null) {
    targetLockedFeature.value = featureName
    isUpgradeModalOpen.value = true
    checkoutError.value = null
  }

  function closeUpgradeModal() {
    isUpgradeModalOpen.value = false
    targetLockedFeature.value = null
    checkoutError.value = null
  }

  async function initiateCheckout(provider = 'dana', plan = 'premium_monthly') {
    checkoutLoading.value = true
    checkoutError.value = null
    try {
      const res = await fetch('/api/v1/subscriptions/checkout', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        credentials: 'include',
        body: JSON.stringify({
          provider: provider || 'dana',
          plan: plan || 'premium_monthly',
          plan_id: plan || 'premium_monthly',
        }),
      })

      if (!res.ok) {
        let errBody = {}
        try {
          errBody = await res.json()
        } catch {}
        throw new Error(errBody.detail || errBody.message || 'Gagal memulai proses pembayaran.')
      }

      const data = await res.json()
      if (data?.checkout_url) {
        if (typeof window !== 'undefined') {
          const opened = window.open(data.checkout_url, '_blank')
          if (!opened && provider === 'dana') {
            window.location.href = data.checkout_url
          }
        }
        closeUpgradeModal()
        // Refresh subscription and auth status
        setTimeout(async () => {
          await fetchSubscriptionStatus()
          try {
            const authStore = useAuthStore()
            if (typeof authStore.fetchUser === 'function') {
              await authStore.fetchUser()
            }
            if (typeof authStore.checkAuth === 'function') {
              await authStore.checkAuth()
            }
          } catch {}
        }, 1000)
      }
      return data
    } catch (err) {
      checkoutError.value = err.message || 'Gagal memulai proses pembayaran.'
      throw err
    } finally {
      checkoutLoading.value = false
    }
  }

  async function activateTrial() {
    checkoutLoading.value = true
    checkoutError.value = null
    try {
      // Try /api/v1/subscriptions/trial/activate first (per v3.1.0 contract)
      let res = await fetch('/api/v1/subscriptions/trial/activate', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        credentials: 'include',
        body: JSON.stringify({}),
      })

      // Fallback to /api/v1/subscriptions/trial or /api/v1/subscription/trial if 404
      if (res.status === 404) {
        res = await fetch('/api/v1/subscriptions/trial', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          credentials: 'include',
        })
      }

      if (res.status === 404) {
        res = await fetch('/api/v1/subscription/trial', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          credentials: 'include',
        })
      }

      if (!res.ok) {
        let errBody = {}
        try {
          errBody = await res.json()
        } catch {}
        throw new Error(errBody.detail || errBody.message || 'Gagal mengaktifkan masa uji coba.')
      }

      const data = await res.json()
      tier.value = data?.tier || 'premium'
      status.value = data?.status || 'trialing'
      hasUsedTrialState.value = true
      daysRemaining.value = data?.days_remaining ?? 90

      await fetchSubscriptionStatus()

      try {
        const authStore = useAuthStore()
        if (typeof authStore.fetchUser === 'function') {
          await authStore.fetchUser()
        }
        if (typeof authStore.checkAuth === 'function') {
          await authStore.checkAuth()
        }
      } catch {}

      closeUpgradeModal()
      return data
    } catch (err) {
      checkoutError.value = err.message || 'Gagal mengaktifkan masa uji coba.'
      throw err
    } finally {
      checkoutLoading.value = false
    }
  }

  function $reset() {
    tier.value = 'free'
    status.value = 'free'
    hasUsedTrialState.value = false
    priceMonthly.value = 10000
    priceAnnual.value = 110000
    features.value = []
    isUpgradeModalOpen.value = false
    targetLockedFeature.value = null
    checkoutLoading.value = false
    checkoutError.value = null
    daysRemaining.value = null
    currentPeriodEnd.value = null
  }

  return {
    // State
    tier,
    status,
    priceMonthly,
    priceAnnual,
    features,
    isUpgradeModalOpen,
    targetLockedFeature,
    checkoutLoading,
    checkoutError,
    daysRemaining,
    currentPeriodEnd,
    // Getters
    isPremium,
    isTrialing,
    hasUsedTrial,
    canAccess,
    formattedPrice,
    formattedAnnualPrice,
    // Actions
    fetchSubscriptionStatus,
    openUpgradeModal,
    closeUpgradeModal,
    initiateCheckout,
    activateTrial,
    $reset,
  }
})
