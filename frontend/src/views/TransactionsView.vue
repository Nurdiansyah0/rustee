<template>
  <div class="space-y-4">
    <!-- Header & Action -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold text-content-primary tracking-tight">Riwayat Transaksi</h2>
        <p class="text-xs text-content-secondary mt-0.5">Catatan mutasi pemasukan, pengeluaran, dan transfer</p>
      </div>
      <Button
        variant="primary"
        size="sm"
        @click="$emit('open-add')"
      >
        <template #prefix>
          <Plus class="w-3.5 h-3.5 mr-1" />
        </template>
        Catat
      </Button>
    </div>

    <!-- Search & Filter Controls -->
    <div class="flex flex-col sm:flex-row gap-2.5 items-stretch sm:items-center justify-between">
      <!-- Instant Search Input -->
      <div class="relative flex-1">
        <Search class="w-4 h-4 text-content-muted absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Cari transaksi..."
          aria-label="Cari riwayat transaksi"
          class="w-full pl-9 pr-8 py-2 bg-surface-card border border-border-subtle hover:border-border-default focus:border-brand-default focus:ring-1 focus:ring-brand-default rounded-xl text-xs text-content-primary placeholder:text-content-muted transition outline-none"
        />
        <button
          v-if="searchQuery"
          type="button"
          @click="searchQuery = ''"
          class="absolute right-2.5 top-1/2 -translate-y-1/2 p-0.5 text-content-muted hover:text-content-primary rounded-full cursor-pointer"
          aria-label="Bersihkan pencarian"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- Compact Filter Chips -->
      <div class="flex gap-1.5 overflow-x-auto pb-0.5 scrollbar-none select-none shrink-0">
        <button
          v-for="tab in filterTabs"
          :key="tab.id"
          type="button"
          @click="setFilter(tab.id)"
          :class="[
            'px-3 py-1.5 rounded-lg text-xs font-semibold whitespace-nowrap transition cursor-pointer',
            selectedType === tab.id
              ? 'bg-brand-default text-white shadow-xs font-bold'
              : 'bg-surface-card text-content-secondary border border-border-subtle hover:bg-surface-subtle'
          ]"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <!-- Transaction List Loading State -->
    <div v-if="loading" class="text-center py-10 text-content-muted text-xs flex flex-col items-center gap-2">
      <div class="w-5 h-5 border-2 border-brand-default border-t-transparent rounded-full animate-spin"></div>
      <span>Memuat data transaksi...</span>
    </div>

    <!-- Transaction Ledger Table (Single Surface, Continuous Hairline Dividers) -->
    <div v-else-if="displayedTransactions.length" class="space-y-3">
      <div class="bg-surface-card rounded-xl border border-border-subtle shadow-card divide-y divide-border-subtle overflow-hidden">
        <div
          v-for="tx in displayedTransactions"
          :key="tx.id"
          class="flex items-center justify-between px-3.5 sm:px-4 py-2.5 sm:py-3 hover:bg-surface-subtle/50 transition min-w-0"
        >
          <div class="flex items-center gap-3 min-w-0">
            <div :class="[
              'w-8 h-8 rounded-lg flex items-center justify-center font-bold text-xs shrink-0',
              tx.transaction_type === 'income' ? 'bg-income-muted text-income-default' :
              tx.transaction_type === 'expense' ? 'bg-expense-muted text-expense-default' : 'bg-transfer-muted text-transfer-default'
            ]">
              <component
                :is="tx.transaction_type === 'income' ? ArrowDownLeft : tx.transaction_type === 'expense' ? ArrowUpRight : ArrowLeftRight"
                class="w-4 h-4 stroke-[2]"
              />
            </div>
            <div class="min-w-0">
              <div class="text-xs sm:text-sm font-medium text-content-primary truncate">{{ tx.description || 'Transaksi' }}</div>
              <div class="text-[10px] text-content-muted mt-0.5">{{ formatDate(tx) }}</div>
            </div>
          </div>

          <div class="flex items-center gap-2.5 shrink-0 ml-3">
            <div :class="[
              'text-xs sm:text-sm font-bold tabular-nums',
              tx.transaction_type === 'income' ? 'text-income-default' :
              tx.transaction_type === 'expense' ? 'text-expense-default' : 'text-content-primary'
            ]">
              {{ walletStore.hideBalance ? '••••••••' : (tx.transaction_type === 'income' ? '+' : tx.transaction_type === 'expense' ? '-' : '') + ' ' + formatIDR(tx.amount) }}
            </div>

            <button
              type="button"
              @click="handleDelete(tx.id)"
              class="min-w-[36px] min-h-[36px] flex items-center justify-center text-content-muted hover:text-expense-default rounded-lg transition hover:bg-expense-muted/30 cursor-pointer focus-ring"
              title="Hapus transaksi"
              aria-label="Hapus transaksi"
            >
              <Trash2 class="w-4 h-4 stroke-[1.75]" />
            </button>
          </div>
        </div>
      </div>

      <!-- Pagination -->
      <div v-if="meta && meta.total_pages > 1 && !searchQuery" class="flex justify-center items-center gap-3 pt-2">
        <button
          type="button"
          :disabled="page <= 1"
          @click="changePage(page - 1)"
          class="px-3 py-1.5 bg-surface-card border border-border-subtle rounded-lg text-xs font-semibold disabled:opacity-40 hover:bg-surface-subtle transition cursor-pointer"
        >
          Sebelumnya
        </button>
        <span class="text-xs text-content-secondary font-medium tabular-nums">{{ page }} / {{ meta.total_pages }}</span>
        <button
          type="button"
          :disabled="page >= meta.total_pages"
          @click="changePage(page + 1)"
          class="px-3 py-1.5 bg-surface-card border border-border-subtle rounded-lg text-xs font-semibold disabled:opacity-40 hover:bg-surface-subtle transition cursor-pointer"
        >
          Selanjutnya
        </button>
      </div>
    </div>

    <!-- Empty Search State -->
    <div v-else-if="transactions.length && !displayedTransactions.length" class="text-center py-8 bg-surface-card rounded-xl border border-dashed border-border-default">
      <Receipt class="w-6 h-6 text-content-muted mx-auto mb-1.5 opacity-40" />
      <div class="text-content-secondary text-xs font-medium">Tidak ada transaksi yang cocok</div>
      <button
        type="button"
        @click="searchQuery = ''"
        class="mt-2 text-xs font-semibold text-brand-default hover:underline cursor-pointer"
      >
        Reset pencarian
      </button>
    </div>

    <!-- Empty Global State -->
    <div v-else class="text-center py-10 bg-surface-card rounded-xl border border-dashed border-border-default">
      <Receipt class="w-7 h-7 text-content-muted mx-auto mb-2 opacity-50" />
      <div class="text-content-secondary text-xs font-medium">Belum ada transaksi</div>
      <button
        type="button"
        @click="$emit('open-add')"
        class="mt-3 inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-brand-default text-white text-xs font-bold hover:bg-brand-emphasis transition cursor-pointer"
      >
        <Plus class="w-3.5 h-3.5" />
        <span>Catat transaksi</span>
      </button>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { formatFinancialDate } from '@/utils/datetime'
import { useWalletStore } from '@/stores/wallets'
import { useAuthStore } from '@/stores/auth'
import { Button } from '@/components/ui'
import {
  Plus,
  Trash2,
  Receipt,
  Search,
  X,
  ArrowDownLeft,
  ArrowUpRight,
  ArrowLeftRight
} from 'lucide-vue-next'

const emit = defineEmits(['open-add', 'refresh'])

const walletStore = useWalletStore()
const authStore = useAuthStore()

const incomeTitle = computed(() => authStore.incomeTitle || 'Masuk')
const expenseTitle = computed(() => authStore.expenseTitle || 'Keluar')

const filterTabs = computed(() => [
  { id: '', label: 'Semua' },
  { id: 'income', label: incomeTitle.value },
  { id: 'expense', label: expenseTitle.value },
  { id: 'transfer', label: 'Transfer' }
])

const transactions = ref([])
const meta = ref(null)
const page = ref(1)
const selectedType = ref('')
const searchQuery = ref('')
const loading = ref(false)

const displayedTransactions = computed(() => {
  if (!searchQuery.value.trim()) {
    return transactions.value
  }
  const q = searchQuery.value.toLowerCase().trim()
  return transactions.value.filter((tx) => {
    const desc = (tx.description || '').toLowerCase()
    const amountStr = String(tx.amount || '')
    return desc.includes(q) || amountStr.includes(q)
  })
})

async function loadTransactions() {
  loading.value = true
  try {
    const res = await api.getTransactions({
      page: page.value,
      per_page: 15,
      transaction_type: selectedType.value || undefined
    })
    transactions.value = res.data || []
    meta.value = res.meta || null
  } catch (err) {
    console.error('Failed to load transactions', err)
  } finally {
    loading.value = false
  }
}

function setFilter(type) {
  selectedType.value = type
  page.value = 1
  loadTransactions()
}

function changePage(newPage) {
  page.value = newPage
  loadTransactions()
}

async function handleDelete(id) {
  if (!confirm('Hapus transaksi ini? Saldo dompet akan disesuaikan secara otomatis.')) return
  try {
    await api.deleteTransaction(id)
    transactions.value = transactions.value.filter((t) => t.id !== id)
    emit('refresh')
  } catch (err) {
    alert(err.detail || 'Gagal menghapus transaksi.')
  }
}

function formatDate(input) {
  return formatFinancialDate(input, {
    includeTime: true,
    monthFormat: 'short'
  })
}

onMounted(() => {
  loadTransactions()
})

defineExpose({
  reload: loadTransactions
})
</script>
