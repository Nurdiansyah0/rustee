<template>
  <div class="space-y-4 pb-24 md:pb-8">
    <!-- Header & Action -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-black text-content-primary tracking-tight">Daftar Transaksi</h2>
        <p class="text-xs text-content-secondary mt-0.5">Semua mutasi pemasukan, pengeluaran, dan transfer</p>
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

    <!-- Filter chips -->
    <div class="flex gap-2 overflow-x-auto pb-1 scrollbar-none select-none">
      <button
        v-for="tab in [
          { id: '', label: 'Semua Mutasi' },
          { id: 'income', label: 'Pemasukan' },
          { id: 'expense', label: 'Pengeluaran' },
          { id: 'transfer', label: 'Transfer' }
        ]"
        :key="tab.id"
        type="button"
        @click="setFilter(tab.id)"
        :class="[
          'px-3.5 py-1.5 rounded-full text-xs font-semibold whitespace-nowrap transition cursor-pointer',
          selectedType === tab.id
            ? 'bg-brand-default text-white shadow-xs font-bold'
            : 'bg-surface-card text-content-secondary border border-border-subtle hover:bg-surface-subtle'
        ]"
      >
        {{ tab.label }}
      </button>
    </div>

    <!-- Transaction List -->
    <div v-if="loading" class="text-center py-12 text-content-muted text-xs flex flex-col items-center gap-2">
      <div class="w-5 h-5 border-2 border-brand-default border-t-transparent rounded-full animate-spin"></div>
      <span>Memuat riwayat transaksi...</span>
    </div>

    <div v-else-if="transactions.length" class="space-y-2.5">
      <div
        v-for="tx in transactions"
        :key="tx.id"
        class="flex items-center justify-between p-3.5 sm:p-4 bg-surface-card rounded-2xl border border-border-subtle shadow-card hover:border-border-default transition"
      >
        <div class="flex items-center gap-3.5 min-w-0">
          <div :class="[
            'w-10 h-10 rounded-xl flex items-center justify-center font-bold text-sm shrink-0',
            tx.transaction_type === 'income' ? 'bg-income-muted text-income-default' :
            tx.transaction_type === 'expense' ? 'bg-expense-muted text-expense-default' : 'bg-transfer-muted text-transfer-default'
          ]">
            <component
              :is="tx.transaction_type === 'income' ? ArrowDownLeft : tx.transaction_type === 'expense' ? ArrowUpRight : ArrowLeftRight"
              class="w-5 h-5 stroke-[2]"
            />
          </div>
          <div class="min-w-0">
            <div class="text-xs sm:text-sm font-bold text-content-primary truncate">{{ tx.description || 'Transaksi' }}</div>
            <div class="text-[11px] text-content-muted mt-0.5">{{ formatDate(tx.date) }}</div>
          </div>
        </div>

        <div class="flex items-center gap-3 shrink-0 ml-3">
          <div :class="[
            'text-xs sm:text-sm font-black tabular-nums',
            tx.transaction_type === 'income' ? 'text-income-default' :
            tx.transaction_type === 'expense' ? 'text-expense-default' : 'text-content-primary'
          ]">
            {{ tx.transaction_type === 'income' ? '+' : tx.transaction_type === 'expense' ? '-' : '' }}
            {{ formatIDR(tx.amount) }}
          </div>

          <button
            type="button"
            @click="handleDelete(tx.id)"
            class="text-content-muted hover:text-expense-default p-1.5 rounded-lg transition hover:bg-expense-muted/30 cursor-pointer"
            title="Hapus transaksi"
          >
            <Trash2 class="w-4 h-4 stroke-[1.75]" />
          </button>
        </div>
      </div>

      <!-- Pagination -->
      <div v-if="meta && meta.total_pages > 1" class="flex justify-center items-center gap-3 pt-4">
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

    <div v-else class="text-center py-12 bg-surface-card rounded-2xl border border-dashed border-border-default">
      <Receipt class="w-8 h-8 text-content-muted mx-auto mb-2 opacity-50" />
      <div class="text-content-secondary text-sm font-medium">Tidak ada transaksi ditemukan</div>
      <button
        type="button"
        @click="$emit('open-add')"
        class="mt-3 inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-brand-default text-white text-xs font-bold hover:bg-brand-emphasis transition cursor-pointer"
      >
        <Plus class="w-3.5 h-3.5" />
        <span>Catat Transaksi Sekarang</span>
      </button>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { Button } from '@/components/ui'
import {
  Plus,
  Trash2,
  Receipt,
  ArrowDownLeft,
  ArrowUpRight,
  ArrowLeftRight
} from 'lucide-vue-next'

const emit = defineEmits(['open-add', 'refresh'])

const transactions = ref([])
const meta = ref(null)
const page = ref(1)
const selectedType = ref('')
const loading = ref(false)

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

function formatDate(isoStr) {
  if (!isoStr) return ''
  try {
    const d = new Date(isoStr)
    return d.toLocaleDateString('id-ID', {
      day: 'numeric',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit'
    })
  } catch {
    return isoStr
  }
}

onMounted(() => {
  loadTransactions()
})

defineExpose({
  reload: loadTransactions
})
</script>
