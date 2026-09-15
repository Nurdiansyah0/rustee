import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'

export const useWalletStore = defineStore('wallets', () => {
  // State
  const wallets = ref([])
  const selectedWalletId = ref(null)
  const hideBalance = ref(
    typeof localStorage !== 'undefined'
      ? localStorage.getItem('invinite_hide_balance') === 'true'
      : false
  )
  const loading = ref(false)
  const error = ref(null)

  // Getters
  const totalBalance = computed(() => {
    return wallets.value.reduce((sum, w) => {
      const bal = w.current_balance !== undefined ? w.current_balance : (w.balance || 0)
      return sum + (typeof bal === 'number' ? Math.round(bal) : (parseInt(bal, 10) || 0))
    }, 0)
  })

  const selectedWallet = computed(() => {
    if (!selectedWalletId.value) return null
    return wallets.value.find((w) => w.id === selectedWalletId.value) || null
  })

  const activeWallets = computed(() => {
    return wallets.value.filter((w) => !w.is_archived)
  })

  const displayTotalBalance = computed(() => {
    return hideBalance.value ? 'Rp ••••••••' : formatIDR(totalBalance.value)
  })

  const walletsByType = computed(() => {
    const grouped = {
      cash: [],
      savings: [],
      credit: [],
      'e-wallet': []
    }
    for (const w of activeWallets.value) {
      const type = w.account_type || 'cash'
      if (grouped[type]) {
        grouped[type].push(w)
      } else {
        grouped.cash.push(w)
      }
    }
    return grouped
  })

  // Actions
  async function fetchWallets() {
    loading.value = true
    error.value = null
    try {
      const data = await api.getAccounts()
      wallets.value = Array.isArray(data) ? data : (data?.data || [])
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat akun dompet.'
      console.error('Failed to fetch wallets', err)
    } finally {
      loading.value = false
    }
  }

  function selectWallet(id) {
    selectedWalletId.value = id
  }

  function toggleHideBalance() {
    hideBalance.value = !hideBalance.value
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('invinite_hide_balance', String(hideBalance.value))
    }
  }

  async function createWallet(payload) {
    loading.value = true
    error.value = null
    try {
      const res = await api.createAccount(payload)
      await fetchWallets()
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membuat dompet baru.'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function archiveWallet(id) {
    loading.value = true
    error.value = null
    try {
      await api.archiveAccount(id)
      const found = wallets.value.find((w) => w.id === id)
      if (found) {
        found.is_archived = true
      }
      if (selectedWalletId.value === id) {
        selectedWalletId.value = null
      }
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal mengarsipkan dompet.'
      throw err
    } finally {
      loading.value = false
    }
  }

  function adjustBalanceLocally(accountId, delta) {
    const target = wallets.value.find((w) => w.id === accountId)
    if (target) {
      const current = target.current_balance !== undefined ? target.current_balance : (target.balance || 0)
      target.current_balance = current + delta
    }
  }

  function $reset() {
    wallets.value = []
    selectedWalletId.value = null
    loading.value = false
    error.value = null
  }

  return {
    // State
    wallets,
    selectedWalletId,
    hideBalance,
    loading,
    error,
    // Getters
    totalBalance,
    selectedWallet,
    activeWallets,
    displayTotalBalance,
    walletsByType,
    // Actions
    fetchWallets,
    selectWallet,
    toggleHideBalance,
    createWallet,
    archiveWallet,
    adjustBalanceLocally,
    $reset,
  }
})
