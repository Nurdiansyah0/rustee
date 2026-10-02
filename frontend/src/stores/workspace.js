import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useWalletStore } from './wallets'
import { useTransactionStore } from './transactions'
import { useCategoryStore } from './categories'
import { useAnalyticsStore } from './analytics'

const CACHED_WORKSPACE_KEY = 'invinite_active_workspace'
const CACHED_WORKSPACES_KEY = 'invinite_user_workspaces'
const CACHED_CAPABILITIES_KEY = 'invinite_workspace_capabilities'
const CACHED_TENANT_ID_KEY = 'invinite_active_tenant_id'

// Fallback capability matrix based on PRD §8, §35 & test_tier1.py F29
const CAPABILITY_MATRIX = {
  general: ['invoicing', 'accounting', 'receivables', 'reports'],
  retail: ['pos', 'inventory', 'invoicing', 'accounting', 'receivables', 'reports'],
  fnb: ['pos', 'tables', 'kitchen', 'inventory', 'accounting', 'reports'],
  rental: ['inventory', 'bookings', 'invoicing', 'receivables', 'accounting'],
  contractor: ['projects', 'milestones', 'invoicing', 'receivables', 'accounting'],
  personal: ['accounts', 'transactions', 'budgets', 'analytics']
}

function loadCachedWorkspace() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(CACHED_WORKSPACE_KEY)
      return raw ? JSON.parse(raw) : null
    }
  } catch {
    // Ignore storage parse error
  }
  return null
}

function saveCachedWorkspace(data) {
  try {
    if (typeof localStorage !== 'undefined') {
      if (data) {
        localStorage.setItem(CACHED_WORKSPACE_KEY, JSON.stringify(data))
        if (data.id) {
          localStorage.setItem(CACHED_TENANT_ID_KEY, data.id)
        }
      } else {
        localStorage.removeItem(CACHED_WORKSPACE_KEY)
        localStorage.removeItem(CACHED_TENANT_ID_KEY)
      }
    }
  } catch {
    // Ignore storage error
  }
}

function loadCachedWorkspaces() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(CACHED_WORKSPACES_KEY)
      return raw ? JSON.parse(raw) : []
    }
  } catch {
    // Ignore storage parse error
  }
  return []
}

function saveCachedWorkspaces(list) {
  try {
    if (typeof localStorage !== 'undefined') {
      if (list && list.length > 0) {
        localStorage.setItem(CACHED_WORKSPACES_KEY, JSON.stringify(list))
      } else {
        localStorage.removeItem(CACHED_WORKSPACES_KEY)
      }
    }
  } catch {
    // Ignore storage error
  }
}

function loadCachedCapabilities() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(CACHED_CAPABILITIES_KEY)
      return raw ? JSON.parse(raw) : []
    }
  } catch {
    // Ignore storage parse error
  }
  return []
}

function saveCachedCapabilities(caps) {
  try {
    if (typeof localStorage !== 'undefined') {
      if (caps && caps.length > 0) {
        localStorage.setItem(CACHED_CAPABILITIES_KEY, JSON.stringify(caps))
      } else {
        localStorage.removeItem(CACHED_CAPABILITIES_KEY)
      }
    }
  } catch {
    // Ignore storage error
  }
}

export const useWorkspaceStore = defineStore('workspace', () => {
  // State
  const initialWorkspace = loadCachedWorkspace()
  const currentWorkspace = ref(initialWorkspace)
  const workspaces = ref(loadCachedWorkspaces())
  const capabilities = ref(loadCachedCapabilities())
  const isLoading = ref(false)
  const loading = isLoading // Alias for cross-store consistency
  const error = ref(null)
  const initialized = ref(Boolean(initialWorkspace))

  // Getters
  const activeTenantId = computed(() => currentWorkspace.value?.id || null)
  const activeTenantName = computed(() => currentWorkspace.value?.name || currentWorkspace.value?.business_name || 'Ruang Kerja')
  const activeTenantSlug = computed(() => currentWorkspace.value?.slug || '')

  const isPersonalWorkspace = computed(() => {
    const ws = currentWorkspace.value
    if (!ws) return true
    return Boolean(
      ws.is_personal === true ||
      ws.is_personal === 1 ||
      ws.is_default === true ||
      ws.is_default === 1 ||
      ws.business_type === 'personal' ||
      ws.type === 'personal'
    )
  })

  const isBusinessWorkspace = computed(() => !isPersonalWorkspace.value)
  const activeRole = computed(() => currentWorkspace.value?.role || 'owner')

  const businessType = computed(() => {
    if (currentWorkspace.value?.business_type) return currentWorkspace.value.business_type
    return isPersonalWorkspace.value ? 'personal' : 'general'
  })

  const hasCapability = (cap) => {
    if (!cap) return false
    return capabilities.value.includes(cap)
  }

  const personalWorkspaces = computed(() => {
    return workspaces.value.filter(
      (w) => w.is_personal === true || w.is_personal === 1 || w.is_default === true || w.business_type === 'personal'
    )
  })

  const businessWorkspaces = computed(() => {
    return workspaces.value.filter(
      (w) => !(w.is_personal === true || w.is_personal === 1 || w.is_default === true || w.business_type === 'personal')
    )
  })

  // Actions
  async function fetchWorkspaces() {
    isLoading.value = true
    error.value = null
    try {
      const res = await api.getWorkspaces()
      const list = Array.isArray(res) ? res : (res?.tenants || res?.workspaces || [])
      workspaces.value = list
      saveCachedWorkspaces(list)

      if (list.length > 0) {
        // Resolve active workspace
        let active = null
        if (currentWorkspace.value?.id) {
          active = list.find((w) => w.id === currentWorkspace.value.id)
        }
        if (!active) {
          const cached = loadCachedWorkspace()
          if (cached?.id) {
            active = list.find((w) => w.id === cached.id)
          }
        }
        if (!active) {
          active = list.find((w) => w.is_personal || w.is_default) || list[0]
        }

        currentWorkspace.value = active
        saveCachedWorkspace(active)
        await fetchCapabilities(active.id)
      }

      initialized.value = true
      return workspaces.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat daftar ruang kerja'
      if (currentWorkspace.value?.id && capabilities.value.length === 0) {
        const bType = currentWorkspace.value.business_type || (isPersonalWorkspace.value ? 'personal' : 'general')
        capabilities.value = CAPABILITY_MATRIX[bType] || CAPABILITY_MATRIX.general
      }
      throw err
    } finally {
      isLoading.value = false
    }
  }

  async function fetchCapabilities(tenantId) {
    if (!tenantId) {
      capabilities.value = CAPABILITY_MATRIX.personal
      return capabilities.value
    }

    try {
      const res = await api.getTenantCapabilities(tenantId)
      if (res?.capabilities && Array.isArray(res.capabilities)) {
        capabilities.value = res.capabilities
        if (res.role && currentWorkspace.value) {
          currentWorkspace.value.role = res.role
        }
        saveCachedCapabilities(res.capabilities)
        return res.capabilities
      }
    } catch (err) {
      console.warn('Tenant capabilities fetch warning, using matrix fallback:', err)
    }

    // Fallback based on business type
    const bType = currentWorkspace.value?.business_type || (isPersonalWorkspace.value ? 'personal' : 'general')
    const fallbackCaps = CAPABILITY_MATRIX[bType] || CAPABILITY_MATRIX.general
    capabilities.value = fallbackCaps
    saveCachedCapabilities(fallbackCaps)
    return fallbackCaps
  }

  async function switchWorkspace(tenantId) {
    if (!tenantId) return
    isLoading.value = true
    error.value = null

    try {
      let switchResult = null
      try {
        switchResult = await api.switchWorkspace(tenantId)
      } catch (err) {
        if (err.status === 404 || err.status === 403) {
          error.value = err.detail || 'Ruang kerja tidak ditemukan atau akses ditolak'
          throw err
        }
        console.warn('Switch workspace API warning:', err)
      }

      let target = workspaces.value.find((w) => w.id === tenantId)
      if (!target && switchResult) {
        target = {
          id: switchResult.active_tenant_id || tenantId,
          name: switchResult.name,
          slug: switchResult.slug,
          role: switchResult.role,
          status: switchResult.status || 'ACTIVE',
          is_personal: false
        }
        workspaces.value.push(target)
        saveCachedWorkspaces(workspaces.value)
      }

      if (target) {
        if (switchResult?.role) target.role = switchResult.role
        if (switchResult?.name) target.name = switchResult.name
        currentWorkspace.value = target
        saveCachedWorkspace(target)
      }

      await fetchCapabilities(tenantId)
      refreshDependentStores()

      return currentWorkspace.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal beralih ruang kerja'
      throw err
    } finally {
      isLoading.value = false
    }
  }

  function refreshDependentStores() {
    try { useWalletStore().fetchWallets?.() } catch {}
    try { useTransactionStore().fetchTransactions?.() } catch {}
    try { useCategoryStore().fetchCategories?.() } catch {}
    try { useAnalyticsStore().fetchDashboard?.() } catch {}
  }

  async function createWorkspace(data) {
    isLoading.value = true
    error.value = null
    try {
      const res = await api.createWorkspace(data)
      await fetchWorkspaces()
      if (res?.id) {
        await switchWorkspace(res.id)
      }
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membuat ruang kerja'
      throw err
    } finally {
      isLoading.value = false
    }
  }

  async function initWorkspace() {
    if (!initialized.value || !currentWorkspace.value) {
      return fetchWorkspaces().catch(() => {})
    }
  }

  function $reset() {
    currentWorkspace.value = null
    workspaces.value = []
    capabilities.value = []
    isLoading.value = false
    error.value = null
    initialized.value = false
    saveCachedWorkspace(null)
    saveCachedWorkspaces(null)
    saveCachedCapabilities(null)
  }

  // Synchronize workspace changes across browser tabs
  if (typeof window !== 'undefined') {
    window.addEventListener('storage', (event) => {
      if (event.key === CACHED_WORKSPACE_KEY) {
        if (event.newValue) {
          try {
            const newWs = JSON.parse(event.newValue)
            currentWorkspace.value = newWs
            if (newWs?.id) {
              fetchCapabilities(newWs.id).catch(() => {})
              refreshDependentStores()
            }
          } catch {
            currentWorkspace.value = null
          }
        } else {
          $reset()
        }
      }
    })
  }

  return {
    // State
    currentWorkspace,
    workspaces,
    capabilities,
    isLoading,
    loading,
    error,
    initialized,
    // Getters
    activeTenantId,
    activeTenantName,
    activeTenantSlug,
    isPersonalWorkspace,
    isBusinessWorkspace,
    activeRole,
    businessType,
    hasCapability,
    personalWorkspaces,
    businessWorkspaces,
    // Actions
    fetchWorkspaces,
    switchWorkspace,
    fetchCapabilities,
    createWorkspace,
    initWorkspace,
    refreshDependentStores,
    $reset
  }
})
