import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useWalletStore } from './wallets'
import { useTransactionStore } from './transactions'
import { useCategoryStore } from './categories'
import { useAnalyticsStore } from './analytics'
import { useSubscriptionStore } from './subscription'

const CACHED_USER_KEY = 'invinite_auth_user'

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

export const useAuthStore = defineStore('auth', () => {
  // State
  const initialCached = loadCachedUser()
  const user = ref(initialCached)
  const permissions = ref(initialCached?.permissions || [])
  const loading = ref(false)
  const error = ref(null)
  const initialized = ref(Boolean(initialCached))

  // Getters
  const isAuthenticated = computed(() => Boolean(user.value))
  const isPremium = computed(() => {
    const tier = user.value?.subscription_tier || user.value?.tier
    return tier === 'premium'
  })
  const displayName = computed(() => {
    return user.value?.display_name || user.value?.name || user.value?.email?.split('@')[0] || 'Pengguna'
  })
  const initials = computed(() => {
    const name = displayName.value || 'U'
    return name.charAt(0).toUpperCase()
  })
  function hasPermission(permission) {
    return permissions.value.includes(permission)
  }

  // Actions
  async function checkAuth() {
    try {
      const me = await api.getMe()
      user.value = me
      permissions.value = me.permissions || []
      saveCachedUser(me)
      initialized.value = true
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
      await api.login(email, password)
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
    try {
      await api.register(name, email, password)
      const ok = await checkAuth()
      if (!ok) {
        throw new Error('Gagal menginisialisasi sesi setelah pendaftaran.')
      }
    } catch (err) {
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
    loading.value = false
    error.value = null
    initialized.value = true
    saveCachedUser(null)
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
      }
    })
  }

  return {
    // State
    user,
    permissions,
    loading,
    error,
    initialized,
    // Getters
    isAuthenticated,
    isPremium,
    displayName,
    initials,
    hasPermission,
    // Actions
    checkAuth,
    login,
    register,
    logout,
    $reset,
  }
})
