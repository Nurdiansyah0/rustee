<template>
  <div
    v-if="isOpen"
    @click.self="$emit('close')"
    class="fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-black/60 backdrop-blur-sm p-0 sm:p-4 select-none animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    aria-labelledby="modal-tx-title"
  >
    <div
      class="relative w-full sm:max-w-md max-h-[92dvh] overflow-y-auto scroll-native bg-surface-card rounded-t-2xl sm:rounded-2xl p-5 sm:p-6 shadow-2xl border border-border-subtle animate-in slide-in-from-bottom sm:zoom-in-95 duration-200"
    >
      <!-- Header -->
      <div class="flex items-center justify-between mb-4">
        <div>
          <h3 id="modal-tx-title" class="text-base sm:text-lg font-bold text-content-primary tracking-tight">
            Catat Transaksi
          </h3>
          <p class="text-[11px] text-content-secondary">Entri instan dengan presisi Rupiah & POS Keypad</p>
        </div>
        <button
          type="button"
          @click="$emit('close')"
          class="p-2 text-content-muted hover:text-content-primary rounded-full hover:bg-surface-subtle transition cursor-pointer"
          aria-label="Tutup modal"
        >
          <X class="w-5 h-5 stroke-[2]" />
        </button>
      </div>

      <!-- Type Selector Tabs -->
      <div class="flex p-1 mb-4 bg-surface-subtle rounded-xl border border-border-subtle" role="tablist">
        <button
          v-for="t in [
            { id: 'expense', label: 'Pengeluaran' },
            { id: 'income', label: 'Pemasukan' },
            { id: 'transfer', label: 'Transfer' }
          ]"
          :key="t.id"
          type="button"
          role="tab"
          :aria-selected="txType === t.id"
          @click="setType(t.id)"
          :class="[
            'flex-1 py-2 text-xs font-bold rounded-lg transition cursor-pointer',
            txType === t.id
              ? 'bg-surface-card text-brand-default shadow-xs border border-border-subtle'
              : 'text-content-secondary hover:text-content-primary'
          ]"
        >
          {{ t.label }}
        </button>
      </div>

      <!-- Display Amount Input -->
      <div class="mb-3 text-center py-2.5 px-3 bg-surface-subtle rounded-xl border border-border-subtle">
        <div class="text-[11px] font-semibold text-content-muted uppercase tracking-wider mb-0.5">
          Nominal Mutasi
        </div>
        <div
          class="text-3xl sm:text-4xl font-black tracking-tight tabular-nums transition-colors duration-150"
          :class="txType === 'expense' ? 'text-expense-default' : txType === 'income' ? 'text-income-default' : 'text-brand-default'"
        >
          {{ formattedAmount }}
        </div>
      </div>

      <!-- Quick increment chips (+50rb, +100rb, +500rb) & Reset Button -->
      <div class="flex items-center justify-center gap-2 mb-4 flex-wrap">
        <button
          v-for="inc in [50000, 100000, 500000]"
          :key="inc"
          type="button"
          @click="addAmount(inc)"
          class="px-3 py-1 bg-surface-subtle hover:bg-border-default active:scale-95 border border-border-subtle text-xs font-semibold rounded-full text-content-secondary transition cursor-pointer"
        >
          +{{ inc >= 1000000 ? (inc/1000000) + 'jt' : (inc/1000) + 'rb' }}
        </button>
        <button
          type="button"
          @click="clearAmount"
          class="px-3 py-1 bg-expense-muted hover:bg-expense-muted/80 active:scale-95 text-xs font-bold rounded-full text-expense-default transition cursor-pointer"
          title="Reset nominal ke 0"
        >
          Reset
        </button>
      </div>

      <!-- Form Inputs: Account & Category -->
      <div class="space-y-3 mb-4 text-xs">
        <div>
          <label for="from-account-pos" class="block font-semibold text-content-secondary mb-1">
            Dari Dompet / Rekening
          </label>
          <select
            id="from-account-pos"
            v-model="selectedAccount"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          >
            <option v-for="acc in accounts" :key="acc.id" :value="acc.id">
              {{ acc.name || acc.display_name }} ({{ walletStore.hideBalance ? '••••' : formatIDR(acc.balance ?? acc.current_balance ?? 0) }})
            </option>
          </select>
        </div>

        <div v-if="txType === 'transfer'">
          <label for="to-account-pos" class="block font-semibold text-content-secondary mb-1">
            Ke Dompet / Rekening Tujuan
          </label>
          <select
            id="to-account-pos"
            v-model="toAccount"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          >
            <option v-for="acc in availableDestinationAccounts" :key="acc.id" :value="acc.id">
              {{ acc.name || acc.display_name }}
            </option>
          </select>
        </div>

        <div v-if="txType !== 'transfer'">
          <label for="category-select-pos" class="block font-semibold text-content-secondary mb-1">
            Kategori Transaksi
          </label>
          <select
            id="category-select-pos"
            v-model="selectedCategory"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          >
            <option value="">Tanpa Kategori</option>
            <option
              v-for="cat in filteredCategories"
              :key="cat.id"
              :value="cat.id"
            >
              {{ cat.name || cat.display_name }}
            </option>
          </select>
        </div>

        <div>
          <label for="tx-description-pos" class="block font-semibold text-content-secondary mb-1">
            Keterangan / Catatan
          </label>
          <input
            id="tx-description-pos"
            v-model="description"
            type="text"
            placeholder="Contoh: Makan siang, kopi santai, transport kantor"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary placeholder:text-content-muted focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          />
        </div>
      </div>

      <!-- Rapid 4x3 POS Numeric Keypad (1-9, 000, 0, backspace) -->
      <div class="grid grid-cols-3 gap-2 mb-4" role="group" aria-label="Keypad Numerik POS">
        <button
          v-for="digit in ['1', '2', '3', '4', '5', '6', '7', '8', '9', '000', '0']"
          :key="digit"
          type="button"
          @click="pressKey(digit)"
          class="h-12 flex items-center justify-center text-lg font-black rounded-xl bg-surface-subtle hover:bg-border-default active:scale-95 active:bg-brand-muted active:text-brand-default border border-border-subtle transition cursor-pointer tabular-nums select-none"
        >
          {{ digit }}
        </button>
        <button
          type="button"
          @click="pressKey('backspace')"
          class="h-12 flex items-center justify-center text-lg font-bold rounded-xl bg-surface-subtle hover:bg-border-default active:scale-95 active:bg-expense-muted active:text-expense-default border border-border-subtle transition cursor-pointer select-none"
          title="Hapus Digit Terakhir"
          aria-label="Hapus digit terakhir"
        >
          <Delete class="w-5 h-5 stroke-[2]" />
        </button>
      </div>

      <!-- Submit Button -->
      <button
        type="button"
        @click="submitTransaction"
        :disabled="loading || rawAmount === '0' || !selectedAccount"
        class="w-full py-3.5 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis active:scale-[0.99] disabled:opacity-50 text-white font-bold rounded-xl transition shadow-lg shadow-brand-default/30 cursor-pointer text-sm flex items-center justify-center gap-2"
      >
        <span v-if="!loading">Simpan Transaksi</span>
        <span v-else class="flex items-center gap-2">
          <span class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
          <span>Menyimpan ke Buku Kas...</span>
        </span>
      </button>

      <div v-if="errorMsg" class="mt-2 text-xs text-center text-expense-default font-medium">
        {{ errorMsg }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch, onUnmounted } from 'vue'
import { api } from '@/services/api'
import { formatIDR, parseIDR } from '@/utils/currency'
import { useWalletStore } from '@/stores/wallets'
import { X, Delete } from 'lucide-vue-next'

const walletStore = useWalletStore()

const props = defineProps({
  isOpen: Boolean,
  accounts: {
    type: Array,
    default: () => []
  },
  categories: {
    type: Array,
    default: () => []
  }
})

const emit = defineEmits(['close', 'transaction-created'])

function handleKeyDown(e) {
  if (e.key === 'Escape' && props.isOpen) {
    e.stopPropagation()
    emit('close')
  }
}

watch(
  () => props.isOpen,
  (open) => {
    if (open) {
      window.addEventListener('keydown', handleKeyDown)
    } else {
      window.removeEventListener('keydown', handleKeyDown)
    }
  },
  { immediate: true }
)

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeyDown)
})

const txType = ref('expense')
const rawAmount = ref('0')
const selectedAccount = ref('')
const toAccount = ref('')
const selectedCategory = ref('')
const description = ref('')
const loading = ref(false)
const errorMsg = ref('')

const formattedAmount = computed(() => {
  const num = parseInt(rawAmount.value, 10) || 0
  return formatIDR(num)
})

const availableDestinationAccounts = computed(() => {
  return props.accounts.filter(a => a.id !== selectedAccount.value)
})

const filteredCategories = computed(() => {
  return props.categories.filter(c => {
    const type = c.category_type || c.type || 'expense'
    return type === txType.value
  })
})

watch(
  () => props.accounts,
  (newAccs) => {
    if (newAccs.length > 0 && !selectedAccount.value) {
      selectedAccount.value = newAccs[0].id
    }
  },
  { immediate: true }
)

/**
 * Tactile Haptic Feedback
 * Dispatches both to Android JS Capability Bridge and native navigator.vibrate
 */
function triggerHaptic(style = 'light') {
  // 1. Android Native Shell Bridge (window.InviniteBridge)
  if (typeof window !== 'undefined' && window.InviniteBridge?.postMessage) {
    try {
      window.InviniteBridge.postMessage(
        JSON.stringify({
          action: 'haptic_feedback',
          style
        })
      )
    } catch {}
  }

  // 2. Browser / PWA standard Vibration API
  if (typeof navigator !== 'undefined' && navigator.vibrate) {
    try {
      navigator.vibrate(style === 'medium' ? 20 : 10)
    } catch {}
  }
}

function setType(type) {
  triggerHaptic('medium')
  txType.value = type
  selectedCategory.value = ''
}

function pressKey(key) {
  triggerHaptic('light')
  if (key === 'backspace') {
    if (rawAmount.value.length > 1) {
      rawAmount.value = rawAmount.value.slice(0, -1)
    } else {
      rawAmount.value = '0'
    }
  } else if (key === '000') {
    if (rawAmount.value !== '0' && rawAmount.value.length <= 11) {
      rawAmount.value += '000'
    }
  } else {
    // Avoid overflow (max IDR 999 milyar)
    if (rawAmount.value.length > 13) return
    if (rawAmount.value === '0') {
      rawAmount.value = key
    } else {
      rawAmount.value += key
    }
  }
}

function addAmount(delta) {
  triggerHaptic('light')
  const cur = parseInt(rawAmount.value, 10) || 0
  rawAmount.value = String(cur + delta)
}

function clearAmount() {
  triggerHaptic('medium')
  rawAmount.value = '0'
}

async function submitTransaction() {
  loading.value = true
  errorMsg.value = ''

  const amount = parseIDR(rawAmount.value)
  if (!amount || amount <= 0) {
    errorMsg.value = 'Nominal harus lebih besar dari 0'
    loading.value = false
    return
  }

  if (txType.value === 'transfer' && !toAccount.value) {
    errorMsg.value = 'Pilih rekening tujuan transfer'
    loading.value = false
    return
  }

  try {
    const payload = {
      account_id: selectedAccount.value,
      to_account_id: txType.value === 'transfer' ? toAccount.value : null,
      category_id: selectedCategory.value || null,
      transaction_type: txType.value,
      amount,
      transaction_date: new Date().toISOString(),
      date: new Date().toISOString(),
      description: description.value || (txType.value === 'transfer' ? 'Transfer Saldo' : 'Transaksi Harian'),
    }

    await api.createTransaction(payload)
    triggerHaptic('medium')
    emit('transaction-created')
    emit('close')

    // Reset state for next entry
    rawAmount.value = '0'
    description.value = ''
  } catch (err) {
    errorMsg.value = err.detail || err.message || 'Gagal menyimpan transaksi.'
  } finally {
    loading.value = false
  }
}
</script>
