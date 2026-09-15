import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useWalletStore } from './wallets'
import { useAnalyticsStore } from './analytics'

function generateUUID() {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    const v = c === 'x' ? r : (r & 0x3) | 0x8
    return v.toString(16)
  })
}

export const useTransactionStore = defineStore('transactions', () => {
  // State
  const transactions = ref([])
  const recentTransactions = ref([])
  const meta = ref({ page: 1, per_page: 15, total: 0, total_pages: 1 })
  const filters = ref({
    type: '',
    accountId: null,
    categoryId: null,
    dateFrom: null,
    dateTo: null,
    searchKeyword: '',
  })
  const loading = ref(false)
  const submitting = ref(false)
  const error = ref(null)

  // Getters
  const filteredTransactions = computed(() => {
    const query = filters.value.searchKeyword?.trim().toLowerCase()
    if (!query) return transactions.value
    return transactions.value.filter((t) => {
      const desc = (t.description || '').toLowerCase()
      const notes = (t.notes || '').toLowerCase()
      return desc.includes(query) || notes.includes(query)
    })
  })

  const hasActiveFilters = computed(() => {
    const f = filters.value
    return Boolean(f.type || f.accountId || f.categoryId || f.dateFrom || f.dateTo || f.searchKeyword)
  })

  const isEmpty = computed(() => {
    return transactions.value.length === 0 && !loading.value
  })

  // Actions
  async function fetchTransactions(page = 1) {
    loading.value = true
    error.value = null
    try {
      const params = {
        page,
        per_page: meta.value?.per_page || 15,
      }
      if (filters.value.type) params.transaction_type = filters.value.type
      if (filters.value.accountId) params.account_id = filters.value.accountId
      if (filters.value.categoryId) params.category_id = filters.value.categoryId

      const res = await api.getTransactions(params)
      transactions.value = res?.data || []
      meta.value = res?.meta || {
        page,
        per_page: 15,
        total: transactions.value.length,
        total_pages: 1,
      }
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat transaksi.'
      console.error('Failed to fetch transactions', err)
    } finally {
      loading.value = false
    }
  }

  async function fetchRecentTransactions() {
    try {
      const res = await api.getTransactions({ page: 1, per_page: 5 })
      recentTransactions.value = res?.data || []
    } catch (err) {
      console.error('Failed to fetch recent transactions', err)
    }
  }

  function setFilter(key, value) {
    filters.value[key] = value
    fetchTransactions(1)
  }

  function resetFilters() {
    filters.value = {
      type: '',
      accountId: null,
      categoryId: null,
      dateFrom: null,
      dateTo: null,
      searchKeyword: '',
    }
    fetchTransactions(1)
  }

  async function createTransaction(payload) {
    submitting.value = true
    error.value = null
    try {
      const idempotencyKey = payload.idempotency_key || generateUUID()
      const txPayload = {
        ...payload,
        idempotency_key: idempotencyKey,
      }

      const res = await api.createTransaction(txPayload)

      // Prepend to current lists
      transactions.value.unshift(res)
      recentTransactions.value.unshift(res)
      if (recentTransactions.value.length > 5) {
        recentTransactions.value.pop()
      }

      // Synchronize with related stores
      try {
        const walletStore = useWalletStore()
        await walletStore.fetchWallets()
      } catch {}
      try {
        const analyticsStore = useAnalyticsStore()
        await analyticsStore.fetchDashboard()
      } catch {}

      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal menyimpan transaksi.'
      throw err
    } finally {
      submitting.value = false
    }
  }

  async function deleteTransaction(id) {
    loading.value = true
    error.value = null
    try {
      await api.deleteTransaction(id)
      transactions.value = transactions.value.filter((t) => t.id !== id)
      recentTransactions.value = recentTransactions.value.filter((t) => t.id !== id)

      // Synchronize balances
      try {
        const walletStore = useWalletStore()
        await walletStore.fetchWallets()
      } catch {}
      try {
        const analyticsStore = useAnalyticsStore()
        await analyticsStore.fetchDashboard()
      } catch {}
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal menghapus transaksi.'
      throw err
    } finally {
      loading.value = false
    }
  }

  function $reset() {
    transactions.value = []
    recentTransactions.value = []
    meta.value = { page: 1, per_page: 15, total: 0, total_pages: 1 }
    filters.value = {
      type: '',
      accountId: null,
      categoryId: null,
      dateFrom: null,
      dateTo: null,
      searchKeyword: '',
    }
    loading.value = false
    submitting.value = false
    error.value = null
  }

  return {
    // State
    transactions,
    recentTransactions,
    meta,
    filters,
    loading,
    submitting,
    error,
    // Getters
    filteredTransactions,
    hasActiveFilters,
    isEmpty,
    // Actions
    fetchTransactions,
    fetchRecentTransactions,
    setFilter,
    resetFilters,
    createTransaction,
    deleteTransaction,
    $reset,
  }
})
