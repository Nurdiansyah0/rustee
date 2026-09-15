<template>
  <div v-if="isOpen" class="fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-black/60 backdrop-blur-sm p-0 sm:p-4 select-none">
    <div class="relative w-full sm:max-w-md bg-surface-card rounded-t-3xl sm:rounded-3xl p-5 sm:p-6 shadow-2xl border border-border-subtle animate-in slide-in-from-bottom sm:zoom-in-95 duration-200">
      <!-- Header -->
      <div class="flex items-center justify-between mb-4">
        <div>
          <h3 class="text-base sm:text-lg font-black text-content-primary tracking-tight">Catat Transaksi</h3>
          <p class="text-[11px] text-content-secondary">Entri instan dengan presisi Rupiah</p>
        </div>
        <button
          type="button"
          @click="$emit('close')"
          class="p-2 text-content-muted hover:text-content-primary rounded-full hover:bg-surface-subtle transition cursor-pointer"
        >
          <X class="w-5 h-5 stroke-[2]" />
        </button>
      </div>

      <!-- Type Selector Tabs -->
      <div class="flex p-1 mb-4 bg-surface-subtle rounded-xl border border-border-subtle">
        <button
          v-for="t in [
            { id: 'expense', label: 'Pengeluaran' },
            { id: 'income', label: 'Pemasukan' },
            { id: 'transfer', label: 'Transfer' }
          ]"
          :key="t.id"
          type="button"
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
      <div class="mb-4 text-center py-2 bg-surface-subtle rounded-2xl border border-border-subtle">
        <div class="text-[11px] font-semibold text-content-muted uppercase tracking-wider mb-0.5">Nominal Mutasi</div>
        <div class="text-3xl sm:text-4xl font-black text-content-primary tracking-tight tabular-nums">
          {{ formattedAmount }}
        </div>
      </div>

      <!-- Quick increment chips -->
      <div class="flex justify-center gap-2 mb-4">
        <button
          v-for="inc in [50000, 100000, 500000]"
          :key="inc"
          type="button"
          @click="addAmount(inc)"
          class="px-3 py-1 bg-surface-subtle hover:bg-border-default border border-border-subtle text-xs font-semibold rounded-full text-content-secondary transition cursor-pointer"
        >
          +{{ inc >= 1000000 ? (inc/1000000) + 'jt' : (inc/1000) + 'rb' }}
        </button>
        <button
          type="button"
          @click="clearAmount"
          class="px-3 py-1 bg-expense-muted hover:bg-expense-muted/80 text-xs font-bold rounded-full text-expense-default transition cursor-pointer"
        >
          Reset
        </button>
      </div>

      <!-- Form Inputs: Account & Category -->
      <div class="space-y-3 mb-4 text-xs">
        <div>
          <label class="block font-semibold text-content-secondary mb-1">Dari Dompet / Rekening</label>
          <select
            v-model="selectedAccount"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          >
            <option v-for="acc in accounts" :key="acc.id" :value="acc.id">
              {{ acc.name }} ({{ formatIDR(acc.balance || acc.current_balance) }})
            </option>
          </select>
        </div>

        <div v-if="txType === 'transfer'">
          <label class="block font-semibold text-content-secondary mb-1">Ke Dompet / Rekening Tujuan</label>
          <select
            v-model="toAccount"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          >
            <option v-for="acc in accounts.filter(a => a.id !== selectedAccount)" :key="acc.id" :value="acc.id">
              {{ acc.name }}
            </option>
          </select>
        </div>

        <div v-if="txType !== 'transfer'">
          <label class="block font-semibold text-content-secondary mb-1">Kategori Transaksi</label>
          <select
            v-model="selectedCategory"
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          >
            <option value="">Tanpa Kategori</option>
            <option v-for="cat in categories.filter(c => c.category_type === txType)" :key="cat.id" :value="cat.id">
              {{ cat.name }}
            </option>
          </select>
        </div>

        <div>
          <label class="block font-semibold text-content-secondary mb-1">Keterangan / Catatan</label>
          <input
            v-model="description"
            type="text"
            placeholder="Contoh: Makan siang, langganan internet, dsb."
            class="w-full px-3.5 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary placeholder:text-content-muted focus:outline-none focus:ring-2 focus:ring-brand-default/30"
          />
        </div>
      </div>

      <!-- Rapid 4x3 POS Numeric Keypad -->
      <div class="grid grid-cols-3 gap-2 mb-4">
        <button
          v-for="digit in ['1', '2', '3', '4', '5', '6', '7', '8', '9', '000', '0']"
          :key="digit"
          type="button"
          @click="pressKey(digit)"
          class="h-11 flex items-center justify-center text-lg font-black rounded-xl bg-surface-subtle hover:bg-border-default active:bg-brand-muted active:text-brand-default border border-border-subtle transition cursor-pointer"
        >
          {{ digit }}
        </button>
        <button
          type="button"
          @click="pressKey('backspace')"
          class="h-11 flex items-center justify-center text-lg font-bold rounded-xl bg-surface-subtle hover:bg-border-default active:bg-expense-muted active:text-expense-default border border-border-subtle transition cursor-pointer"
          title="Hapus Digit Terakhir"
        >
          <Delete class="w-5 h-5 stroke-[2]" />
        </button>
      </div>

      <!-- Submit Button Primitive -->
      <button
        type="button"
        @click="submitTransaction"
        :disabled="loading || rawAmount === '0' || !selectedAccount"
        class="w-full py-3.5 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis disabled:opacity-50 text-white font-bold rounded-xl transition shadow-lg shadow-brand-default/30 cursor-pointer text-sm"
      >
        <span v-if="!loading">Simpan Transaksi</span>
        <span v-else>Memproses...</span>
      </button>

      <div v-if="errorMsg" class="mt-2 text-xs text-center text-expense-default font-medium">
        {{ errorMsg }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { X, Delete } from 'lucide-vue-next'

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

watch(() => props.accounts, (newAccs) => {
  if (newAccs.length > 0 && !selectedAccount.value) {
    selectedAccount.value = newAccs[0].id
  }
}, { immediate: true })

function vibrate() {
  if (typeof navigator !== 'undefined' && navigator.vibrate) {
    navigator.vibrate(10)
  }
}

function setType(type) {
  vibrate()
  txType.value = type
  selectedCategory.value = ''
}

function pressKey(key) {
  vibrate()
  if (key === 'backspace') {
    if (rawAmount.value.length > 1) {
      rawAmount.value = rawAmount.value.slice(0, -1)
    } else {
      rawAmount.value = '0'
    }
  } else if (key === '000') {
    if (rawAmount.value !== '0') {
      rawAmount.value += '000'
    }
  } else {
    if (rawAmount.value === '0') {
      rawAmount.value = key
    } else {
      rawAmount.value += key
    }
  }
}

function addAmount(delta) {
  vibrate()
  const cur = parseInt(rawAmount.value, 10) || 0
  rawAmount.value = String(cur + delta)
}

function clearAmount() {
  vibrate()
  rawAmount.value = '0'
}

async function submitTransaction() {
  loading.value = true
  errorMsg.value = ''

  const amount = parseInt(rawAmount.value, 10)
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
      amount: amount,
      date: new Date().toISOString(),
      description: description.value || (txType.value === 'transfer' ? 'Transfer Saldo' : 'Transaksi'),
    }

    await api.createTransaction(payload)
    emit('transaction-created')
    emit('close')
    // Reset inputs
    rawAmount.value = '0'
    description.value = ''
  } catch (err) {
    errorMsg.value = err.detail || 'Gagal menyimpan transaksi.'
  } finally {
    loading.value = false
  }
}
</script>
