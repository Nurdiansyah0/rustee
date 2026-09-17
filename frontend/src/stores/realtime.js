import { defineStore } from 'pinia'
import { ref } from 'vue'
import { websocketService } from '@/services/websocket'
import { useWalletStore } from './wallets'
import { useAnalyticsStore } from './analytics'
import { useTransactionStore } from './transactions'
import { syncService } from '@/services/sync'

export const useRealtimeStore = defineStore('realtime', () => {
  const status = ref('disconnected') // 'connecting' | 'connected' | 'disconnected' | 'backgrounded'
  const isConnected = ref(false)
  const lastEvent = ref(null)
  const lastEventTimestamp = ref(null)
  const eventHistory = ref([])

  let unsubStatus = null
  let unsubTx = null
  let unsubBal = null
  let unsubSync = null

  function initRealtime() {
    if (typeof window === 'undefined') return

    websocketService.init()

    // Status listener
    unsubStatus = websocketService.onStatusChange((newStatus) => {
      status.value = newStatus
      isConnected.value = newStatus === 'connected'
    })

    // 1. TransactionCreated event listener (§25)
    unsubTx = websocketService.subscribe('TransactionCreated', async (data) => {
      recordEvent('TransactionCreated', data)
      const walletStore = useWalletStore()
      const analyticsStore = useAnalyticsStore()
      const txStore = useTransactionStore()

      // Dynamically reconcile affected wallet & dashboard
      await Promise.allSettled([
        walletStore.fetchWallets(),
        analyticsStore.fetchDashboard(),
        txStore.fetchTransactions ? txStore.fetchTransactions() : Promise.resolve()
      ])
    })

    // 2. BalanceChanged event listener (§25)
    unsubBal = websocketService.subscribe('BalanceChanged', (data) => {
      recordEvent('BalanceChanged', data)
      const walletStore = useWalletStore()
      if (data?.account_id && data?.new_balance !== undefined) {
        const wallet = walletStore.wallets.find(w => w.id === data.account_id)
        if (wallet) {
          wallet.balance = data.new_balance
          wallet.current_balance = data.new_balance
        }
      } else {
        walletStore.fetchWallets()
      }
    })

    // 3. SyncHint event listener (§25)
    unsubSync = websocketService.subscribe('SyncHint', async (data) => {
      recordEvent('SyncHint', data)
      if (syncService?.syncWithCursor) {
        await syncService.syncWithCursor(data?.new_cursor)
      }
    })
  }

  function recordEvent(type, data) {
    lastEvent.value = { type, data }
    lastEventTimestamp.value = new Date().toISOString()
    eventHistory.value.unshift({
      type,
      data,
      timestamp: lastEventTimestamp.value
    })
    if (eventHistory.value.length > 50) {
      eventHistory.value.pop()
    }
  }

  function cleanupRealtime() {
    if (unsubStatus) unsubStatus()
    if (unsubTx) unsubTx()
    if (unsubBal) unsubBal()
    if (unsubSync) unsubSync()
    websocketService.disconnect(false, 'Cleanup store')
  }

  return {
    status,
    isConnected,
    lastEvent,
    lastEventTimestamp,
    eventHistory,
    initRealtime,
    cleanupRealtime,
  }
})
