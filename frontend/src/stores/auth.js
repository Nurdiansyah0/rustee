import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useWalletStore } from './wallets'
import { useTransactionStore } from './transactions'
import { useCategoryStore } from './categories'
import { useAnalyticsStore } from './analytics'
import { useSubscriptionStore } from './subscription'

const CACHED_USER_KEY = 'invinite_auth_user'
const CACHED_PERSONALIZATION_KEY = 'invinite_user_personalization'

function loadCachedUser() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(CACHED_USER_KEY)
      return raw ? JSON.parse(raw) : null
    }
  } catch {
    // Ignore localStorage parse errors
  }
  return null
}

function saveCachedUser(userData) {
  try {
    if (typeof localStorage !== 'undefined') {
      if (userData) {
        localStorage.setItem(CACHED_USER_KEY, JSON.stringify(userData))
      } else {
        localStorage.removeItem(CACHED_USER_KEY)
      }
    }
  } catch {
    // Ignore storage quota or access errors
  }
}

function loadCachedPersonalization() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(CACHED_PERSONALIZATION_KEY)
      return raw ? JSON.parse(raw) : null
    }
  } catch {
    // Ignore parse error
  }
  return null
}

function saveCachedPersonalization(data) {
  try {
    if (typeof localStorage !== 'undefined') {
      if (data) {
        localStorage.setItem(CACHED_PERSONALIZATION_KEY, JSON.stringify(data))
      } else {
        localStorage.removeItem(CACHED_PERSONALIZATION_KEY)
      }
    }
  } catch {
    // Ignore storage errors
  }
}

export const useAuthStore = defineStore('auth', () => {
  // State
  const initialCached = loadCachedUser()
  const user = ref(initialCached)
  const permissions = ref(initialCached?.permissions || [])
  const personalization = ref(loadCachedPersonalization())
  const isJustRegistered = ref(false)
  const loading = ref(false)
  const error = ref(null)
  const initialized = ref(Boolean(initialCached))

  // Getters
  const isAuthenticated = computed(() => Boolean(user.value))
  const isPremium = computed(() => {
    const u = user.value?.user || user.value
    const tier = u?.subscription_tier || u?.tier
    let subStore = null
    try {
      subStore = useSubscriptionStore()
    } catch {}
    return (
      tier === 'premium' ||
      tier === 'trialing' ||
      Boolean(subStore?.isPremium) ||
      Boolean(subStore?.isTrialing)
    )
  })
  const displayName = computed(() => {
    const u = user.value?.user || user.value
    return (
      personalization.value?.display_name?.trim() ||
      u?.display_name ||
      u?.name ||
      u?.email?.split('@')[0] ||
      'Pengguna'
    )
  })
  const incomeTitle = computed(() => {
    return personalization.value?.income_title?.trim() || 'Pemasukan'
  })
  const expenseTitle = computed(() => {
    return personalization.value?.expense_title?.trim() || 'Pengeluaran'
  })
  const userGoals = computed(() => {
    return personalization.value?.financial_goals || personalization.value?.goals || []
  })
  const isOnboarded = computed(() => {
    return Boolean(personalization.value?.onboarding_completed)
  })
  const initials = computed(() => {
    const name = displayName.value || 'U'
    return name.charAt(0).toUpperCase()
  })
  function hasPermission(permission) {
    return permissions.value.includes(permission)
  }

  // Actions
  function setPersonalization(prefs) {
    if (!prefs) return
    personalization.value = {
      ...(personalization.value || {}),
      ...prefs
    }
    saveCachedPersonalization(personalization.value)
  }

  async function fetchPersonalization() {
    try {
      const prefs = await api.getPersonalization()
      if (prefs) {
        personalization.value = {
          ...(personalization.value || {}),
          ...prefs
        }
        saveCachedPersonalization(personalization.value)
      }
      return prefs
    } catch (err) {
      console.warn('Failed to fetch user personalization:', err)
      return null
    }
  }

  async function updatePersonalization(data) {
    try {
      const updated = await api.updatePersonalization(data)
      if (updated) {
        personalization.value = {
          ...(personalization.value || {}),
          ...updated
        }
        saveCachedPersonalization(personalization.value)
      }
      return updated
    } catch (err) {
      console.error('Failed to update personalization:', err)
      throw err
    }
  }

  async function checkAuth() {
    try {
      const me = await api.getMe()
      const userData = me?.user || me
      user.value = userData
      permissions.value = me?.permissions || userData?.permissions || []
      saveCachedUser(userData)
      initialized.value = true
      // Fetch user financial vocabulary & preferences
      fetchPersonalization().catch(() => {})
      return true
    } catch (err) {
      // 1. Explicit 401 Unauthorized from backend -> session is definitely expired
      if (err?.status === 401) {
        user.value = null
        permissions.value = []
        saveCachedUser(null)
        initialized.value = true
        return false
      }

      // 2. Offline / Temporary Network Failure (TypeError: Failed to fetch, offline navigator, no HTTP status)
      // DO NOT lock out the user if a valid session exists in cache
      const isNetworkIssue =
        (typeof navigator !== 'undefined' && !navigator.onLine) ||
        err instanceof TypeError ||
        !err?.status ||
        err?.status === 0;

      if (isNetworkIssue && user.value) {
        console.warn('[PWA] Network unavailable. Preserving authenticated session from local cache.');
        initialized.value = true
        return true
      }

      // 3. Fallback for unauthenticated terminal errors
      user.value = null
      permissions.value = []
      saveCachedUser(null)
      initialized.value = true
      return false
    }
  }

  async function login(email, password) {
    loading.value = true
    error.value = null
    try {
      const res = await api.login(email, password)
      if (res?.user) {
        user.value = res.user
        permissions.value = res.user.permissions || []
        saveCachedUser(res.user)
        initialized.value = true
        fetchPersonalization().catch(() => {})
        return true
      }
      const ok = await checkAuth()
      if (!ok) {
        throw new Error('Gagal mengambil sesi profil setelah masuk.')
      }
    } catch (err) {
      error.value = err.detail || err.message || 'Login gagal.'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function register(name, email, password) {
    loading.value = true
    error.value = null
    isJustRegistered.value = true
    try {
      const res = await api.register(name, email, password)
      if (res?.user) {
        user.value = res.user
        permissions.value = res.user.permissions || []
        personalization.value = {
          display_name: name,
          income_title: null,
          expense_title: null,
          financial_goals: [],
          onboarding_completed: false
        }
        saveCachedUser(res.user)
        saveCachedPersonalization(personalization.value)
        initialized.value = true
        fetchPersonalization().catch(() => {})
        return true
      }
      const ok = await checkAuth()
      if (!ok) {
        throw new Error('Gagal menginisialisasi sesi setelah pendaftaran.')
      }
    } catch (err) {
      isJustRegistered.value = false
      error.value = err.detail || err.message || 'Registrasi gagal.'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function logout() {
    try {
      await api.logout()
    } catch (err) {
      console.warn('Logout request warning:', err)
    } finally {
      $reset()
      // Cross-store reset
      try { useWalletStore().$reset() } catch {}
      try { useTransactionStore().$reset() } catch {}
      try { useCategoryStore().$reset() } catch {}
      try { useAnalyticsStore().$reset() } catch {}
      try { useSubscriptionStore().$reset() } catch {}
    }
  }

  function $reset() {
    user.value = null
    permissions.value = []
    personalization.value = null
    isJustRegistered.value = false
    loading.value = false
    error.value = null
    initialized.value = true
    saveCachedUser(null)
    saveCachedPersonalization(null)
  }

  // Synchronize authentication status across multiple browser tabs
  if (typeof window !== 'undefined') {
    window.addEventListener('storage', (event) => {
      if (event.key === CACHED_USER_KEY) {
        if (event.newValue) {
          try {
            const newUser = JSON.parse(event.newValue)
            user.value = newUser
            permissions.value = newUser.permissions || []
            initialized.value = true
          } catch {
            user.value = null
            permissions.value = []
          }
        } else {
          user.value = null
          permissions.value = []
        }
      } else if (event.key === CACHED_PERSONALIZATION_KEY) {
        if (event.newValue) {
          try {
            personalization.value = JSON.parse(event.newValue)
          } catch {
            personalization.value = null
          }
        } else {
          personalization.value = null
        }
      }
    })
  }

  return {
    // State
    user,
    permissions,
    personalization,
    isJustRegistered,
    loading,
    error,
    initialized,
    // Getters
    isAuthenticated,
    isPremium,
    displayName,
    incomeTitle,
    expenseTitle,
    userGoals,
    isOnboarded,
    initials,
    hasPermission,
    // Actions
    checkAuth,
    login,
    register,
    logout,
    setPersonalization,
    fetchPersonalization,
    updatePersonalization,
    $reset,
  }
})
