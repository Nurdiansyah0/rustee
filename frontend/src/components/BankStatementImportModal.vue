<template>
  <div
    v-if="isOpen"
    @click.self="$emit('close')"
    class="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-4 bg-black/65 backdrop-blur-sm animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    aria-labelledby="import-modal-title"
  >
    <div
      ref="modalRef"
      class="relative w-full max-w-2xl max-h-[92dvh] flex flex-col bg-surface-card rounded-2xl shadow-2xl border border-border-subtle overflow-hidden animate-in zoom-in-95 duration-200"
    >
      <!-- Modal Header -->
      <div class="px-5 py-4 border-b border-border-subtle flex items-center justify-between shrink-0 bg-surface-sunken/40">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-brand-default/15 text-brand-default border border-brand-default/25 flex items-center justify-center shadow-xs">
            <FileSpreadsheet class="w-5 h-5 stroke-[2]" />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h3 id="import-modal-title" class="text-base sm:text-lg font-black text-content-primary tracking-tight">
                Accounting Pribadi: Impor E-Statement Bank
              </h3>
              <span class="text-[9px] font-extrabold px-2 py-0.5 rounded-full bg-brand-default text-white uppercase tracking-wider">
                FinRep Pro
              </span>
            </div>
            <p class="text-xs text-content-secondary mt-0.5">
              Ekstrak mutasi rekening koran, rekonsiliasi saldo, dan cocokkan kategori secara otomatis.
            </p>
          </div>
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

      <!-- Modal Body (Scrollable) -->
      <div class="flex-1 overflow-y-auto p-4 sm:p-6 space-y-5 scroll-native">
        <!-- Target Wallet Selection -->
        <div class="p-3.5 rounded-xl border border-border-default bg-surface-sunken/60 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div>
            <label class="block text-[11px] font-bold text-content-secondary uppercase tracking-wider">
              Rekening / Dompet Tujuan Rekonsiliasi
            </label>
            <div class="text-xs text-content-muted mt-0.5">
              Mutasi yang diimpor akan dicocokkan dan memperbarui saldo rekening ini.
            </div>
          </div>
          <div class="w-full sm:w-64 shrink-0">
            <select
              v-model="selectedAccountId"
              class="w-full px-3 py-2 bg-surface-card border border-border-default rounded-xl text-xs font-semibold text-content-primary focus:border-brand-default focus:ring-1 focus:ring-brand-default outline-none transition cursor-pointer"
            >
              <option v-for="acc in walletStore.wallets" :key="acc.id" :value="acc.id">
                {{ acc.name }} ({{ formatIDR(acc.current_balance || acc.balance || 0) }})
              </option>
            </select>
          </div>
        </div>

        <!-- Step 1: Input E-Statement Source (File / Text / Sample) -->
        <div v-if="parsedTransactions.length === 0" class="space-y-4">
          <!-- Source Mode Tabs -->
          <div class="flex border-b border-border-subtle">
            <button
              type="button"
              @click="activeInputTab = 'upload'"
              :class="[
                'pb-2.5 px-4 text-xs font-bold transition border-b-2 cursor-pointer',
                activeInputTab === 'upload'
                  ? 'border-brand-default text-brand-default'
                  : 'border-transparent text-content-muted hover:text-content-primary'
              ]"
            >
              Unggah File (CSV / E-Statement)
            </button>
            <button
              type="button"
              @click="activeInputTab = 'paste'"
              :class="[
                'pb-2.5 px-4 text-xs font-bold transition border-b-2 cursor-pointer',
                activeInputTab === 'paste'
                  ? 'border-brand-default text-brand-default'
                  : 'border-transparent text-content-muted hover:text-content-primary'
              ]"
            >
              Tempel Teks Mutasi / E-Banking
            </button>
          </div>

          <!-- Tab 1: Drag & Drop File Upload -->
          <div v-if="activeInputTab === 'upload'" class="space-y-3">
            <div
              @dragover.prevent="isDragging = true"
              @dragleave.prevent="isDragging = false"
              @drop.prevent="handleFileDrop"
              :class="[
                'p-8 rounded-2xl border-2 border-dashed text-center transition flex flex-col items-center justify-center cursor-pointer',
                isDragging
                  ? 'border-brand-default bg-brand-default/10 scale-[0.99]'
                  : 'border-border-default hover:border-brand-default/60 bg-surface-sunken/40'
              ]"
              @click="triggerFileInput"
            >
              <input
                ref="fileInputRef"
                type="file"
                accept=".csv,.txt,.pdf"
                class="hidden"
                @change="handleFileSelected"
              />
              <div class="w-12 h-12 rounded-xl bg-brand-default/10 text-brand-default flex items-center justify-center mb-3">
                <UploadCloud class="w-6 h-6 stroke-[2]" />
              </div>
              <div class="text-sm font-bold text-content-primary">
                Tarik & Lepas File E-Statement di Sini
              </div>
              <p class="text-xs text-content-secondary mt-1 max-w-sm leading-relaxed">
                Mendukung e-statement dari <strong>BCA, Mandiri, BRI, BNI, Bank Jago, Jenius</strong> dalam format .CSV atau .TXT
              </p>
              <button
                type="button"
                class="mt-3.5 px-4 py-2 bg-surface-card border border-border-default hover:border-brand-default text-content-primary text-xs font-semibold rounded-xl transition shadow-xs"
              >
                Pilih Dokumen dari Komputer
              </button>
            </div>
          </div>

          <!-- Tab 2: Paste Raw Mutation Text -->
          <div v-else class="space-y-2">
            <label class="block text-xs font-semibold text-content-secondary">
              Salin & Tempel Mutasi Rekening dari Internet Banking / E-Statement:
            </label>
            <textarea
              v-model="pastedText"
              rows="6"
              placeholder="Contoh format:&#10;15/09/2026 TRSF E-BANKING CR DARI BUDI 5.000.000,00&#10;16/09/2026 QRIS KOPI KENANGAN 28.000,00 DB&#10;18/09/2026 BIAYA ADM TABUNGAN 15.000,00 DB"
              class="w-full p-3 bg-surface-sunken/60 border border-border-default rounded-xl text-xs font-mono text-content-primary focus:border-brand-default focus:ring-1 focus:ring-brand-default outline-none resize-none transition"
            ></textarea>
            <div class="flex justify-end">
              <button
                type="button"
                @click="parsePastedText"
                :disabled="!pastedText.trim()"
                class="px-4 py-2 bg-brand-default hover:bg-brand-emphasis active:scale-95 text-white font-bold text-xs rounded-xl transition cursor-pointer disabled:opacity-50"
              >
                Ekstrak & Analisis Teks
              </button>
            </div>
          </div>

          <!-- Demo / Sample Generator for 1-Click Testing -->
          <div class="p-3.5 rounded-xl border border-dashed border-brand-default/30 bg-brand-default/5 flex items-center justify-between gap-3">
            <div class="flex items-center gap-2.5">
              <Sparkles class="w-4 h-4 text-brand-default shrink-0" />
              <div>
                <div class="text-xs font-bold text-content-primary">Ingin mencoba analisis akuntansi?</div>
                <div class="text-[11px] text-content-secondary">Muat contoh mutasi rekening koran BCA & Mandiri untuk menguji fitur accounting.</div>
              </div>
            </div>
            <button
              type="button"
              @click="loadSampleStatement"
              class="px-3 py-1.5 bg-surface-card border border-brand-default/30 hover:border-brand-default text-brand-default text-xs font-bold rounded-lg shadow-xs transition cursor-pointer shrink-0"
            >
              Coba Contoh Mutasi
            </button>
          </div>
        </div>

        <!-- Step 2: Reconciliation & Extracted Ledger Preview -->
        <div v-else class="space-y-4">
          <!-- Summary Cards -->
          <div class="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
            <div class="p-3 rounded-xl bg-surface-sunken border border-border-subtle">
              <div class="text-[10px] font-bold text-content-muted uppercase">Total Mutasi</div>
              <div class="text-base font-black text-content-primary mt-0.5 tabular-nums">
                {{ parsedTransactions.length }} Transaksi
              </div>
            </div>
            <div class="p-3 rounded-xl bg-income-muted/40 border border-income-border/50">
              <div class="text-[10px] font-bold text-income-default uppercase">Total Kredit (Masuk)</div>
              <div class="text-base font-black text-income-default mt-0.5 tabular-nums">
                +{{ formatIDR(totalExtractedIncome) }}
              </div>
            </div>
            <div class="p-3 rounded-xl bg-expense-muted/40 border border-expense-border/50">
              <div class="text-[10px] font-bold text-expense-default uppercase">Total Debit (Keluar)</div>
              <div class="text-base font-black text-expense-default mt-0.5 tabular-nums">
                -{{ formatIDR(totalExtractedExpense) }}
              </div>
            </div>
            <div class="p-3 rounded-xl bg-surface-sunken border border-border-subtle">
              <div class="text-[10px] font-bold text-content-muted uppercase">Selisih Bersih (Net)</div>
              <div
                :class="[
                  'text-base font-black mt-0.5 tabular-nums',
                  netExtractedFlow >= 0 ? 'text-income-default' : 'text-expense-default'
                ]"
              >
                {{ (netExtractedFlow >= 0 ? '+' : '-') + ' ' + formatIDR(Math.abs(netExtractedFlow)) }}
              </div>
            </div>
          </div>

          <!-- Action Bar: Select All / Clear -->
          <div class="flex items-center justify-between gap-2 pt-1">
            <div class="flex items-center gap-2">
              <label class="flex items-center gap-2 text-xs font-semibold text-content-secondary cursor-pointer">
                <input
                  type="checkbox"
                  :checked="areAllSelected"
                  @change="toggleSelectAll"
                  class="rounded text-brand-default focus:ring-brand-default cursor-pointer"
                />
                <span>Pilih Semua ({{ selectedCount }} dari {{ parsedTransactions.length }})</span>
              </label>
              <button
                v-if="duplicateCount > 0"
                type="button"
                @click="deselectDuplicates"
                class="text-[11px] font-bold text-amber-500 hover:underline cursor-pointer"
              >
                Hilangkan {{ duplicateCount }} Duplikat
              </button>
            </div>

            <button
              type="button"
              @click="resetParsed"
              class="text-xs font-semibold text-content-muted hover:text-content-primary cursor-pointer"
            >
              Ganti File E-Statement
            </button>
          </div>

          <!-- Transaction Candidate List -->
          <div class="border border-border-subtle rounded-xl divide-y divide-border-subtle overflow-hidden max-h-72 overflow-y-auto scroll-native bg-surface-card">
            <div
              v-for="(tx, idx) in parsedTransactions"
              :key="idx"
              :class="[
                'p-3 flex items-center justify-between gap-3 transition',
                tx.selected ? 'bg-surface-card hover:bg-surface-subtle/50' : 'bg-surface-sunken/40 opacity-60'
              ]"
            >
              <!-- Checkbox & Description -->
              <div class="flex items-center gap-3 min-w-0 flex-1">
                <input
                  type="checkbox"
                  v-model="tx.selected"
                  class="rounded text-brand-default focus:ring-brand-default cursor-pointer shrink-0"
                />
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <span class="text-xs font-bold text-content-primary truncate">{{ tx.description }}</span>
                    <span
                      v-if="tx.isDuplicate"
                      class="px-1.5 py-0.5 rounded text-[9px] font-bold bg-amber-500/15 text-amber-500 border border-amber-500/30 shrink-0"
                    >
                      Terdeteksi di Buku Kas
                    </span>
                    <span
                      v-else
                      class="px-1.5 py-0.5 rounded text-[9px] font-bold bg-income-muted text-income-default shrink-0"
                    >
                      Baru
                    </span>
                  </div>
                  <div class="flex items-center gap-2 mt-0.5 text-[10px] text-content-muted">
                    <span>{{ tx.dateFormatted }}</span>
                    <span>•</span>
                    <select
                      v-model="tx.categoryId"
                      class="bg-transparent border-b border-dashed border-border-default hover:border-brand-default text-content-secondary font-medium outline-none cursor-pointer py-0.5"
                    >
                      <option :value="null">-- Pilih Kategori --</option>
                      <option
                        v-for="cat in availableCategories(tx.transaction_type)"
                        :key="cat.id"
                        :value="cat.id"
                      >
                        {{ cat.display_name || cat.name }}
                      </option>
                    </select>
                  </div>
                </div>
              </div>

              <!-- Amount & Type Badge -->
              <div class="text-right shrink-0">
                <div
                  :class="[
                    'text-xs sm:text-sm font-bold tabular-nums',
                    tx.transaction_type === 'income' ? 'text-income-default' : 'text-expense-default'
                  ]"
                >
                  {{ (tx.transaction_type === 'income' ? '+' : '-') + ' ' + formatIDR(tx.amount) }}
                </div>
                <div class="text-[10px] uppercase font-bold text-content-muted mt-0.5">
                  {{ tx.transaction_type === 'income' ? 'Kredit (CR)' : 'Debit (DB)' }}
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Import Success State -->
        <div v-if="importSuccess" class="p-6 bg-brand-default/10 rounded-2xl border border-brand-default/25 text-center space-y-3">
          <div class="w-12 h-12 rounded-full bg-brand-default text-white flex items-center justify-center mx-auto shadow-md">
            <Check class="w-6 h-6 stroke-[3]" />
          </div>
          <div>
            <h4 class="text-base font-black text-content-primary">Rekonsiliasi Berhasil!</h4>
            <p class="text-xs text-content-secondary mt-1 max-w-sm mx-auto">
              Sebanyak <strong class="text-brand-default font-bold">{{ importedCount }} mutasi</strong> telah berhasil dimasukkan ke buku kas resmi dan saldo rekening telah disesuaikan.
            </p>
          </div>
          <button
            type="button"
            @click="handleFinish"
            class="px-6 py-2.5 bg-brand-default hover:bg-brand-emphasis text-white font-bold text-xs rounded-xl shadow-md transition cursor-pointer"
          >
            Kembali ke Buku Kas
          </button>
        </div>
      </div>

      <!-- Modal Footer -->
      <div v-if="!importSuccess" class="px-5 py-4 border-t border-border-subtle bg-surface-sunken/40 flex items-center justify-between gap-3 shrink-0">
        <button
          type="button"
          @click="$emit('close')"
          :disabled="isImporting"
          class="px-4 py-2.5 border border-border-default hover:bg-surface-subtle text-content-secondary font-bold text-xs rounded-xl transition cursor-pointer disabled:opacity-50"
        >
          Batal
        </button>

        <div class="flex items-center gap-2">
          <button
            v-if="parsedTransactions.length > 0"
            type="button"
            @click="commitImport"
            :disabled="isImporting || selectedCount === 0"
            class="px-5 py-2.5 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis active:scale-95 text-white font-bold text-xs rounded-xl shadow-lg shadow-brand-default/25 transition cursor-pointer disabled:opacity-50 flex items-center gap-2"
          >
            <span v-if="!isImporting">
              Rekonsiliasi & Masukkan ke Buku Kas ({{ selectedCount }} Mutasi)
            </span>
            <span v-else class="flex items-center gap-2">
              <span class="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
              <span>Memproses Rekonsiliasi...</span>
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch, onMounted } from 'vue'
import { api } from '@/services/api'
import { formatIDR } from '@/utils/currency'
import { formatFinancialDate } from '@/utils/datetime'
import { useWalletStore } from '@/stores/wallets'
import { useCategoryStore } from '@/stores/categories'
import { useTransactionStore } from '@/stores/transactions'
import { useAuthStore } from '@/stores/auth'
import {
  FileSpreadsheet,
  UploadCloud,
  X,
  Check,
  Sparkles,
  AlertTriangle
} from 'lucide-vue-next'

const props = defineProps({
  isOpen: Boolean
})

const emit = defineEmits(['close', 'imported'])

const walletStore = useWalletStore()
const categoryStore = useCategoryStore()
const transactionStore = useTransactionStore()
const authStore = useAuthStore()

const modalRef = ref(null)
const fileInputRef = ref(null)
const isDragging = ref(false)
const activeInputTab = ref('upload')
const pastedText = ref('')
const selectedAccountId = ref('')
const parsedTransactions = ref([])
const isImporting = ref(false)
const importSuccess = ref(false)
const importedCount = ref(0)

// Initialize default wallet
watch(
  () => walletStore.wallets,
  (wallets) => {
    if (wallets?.length > 0 && !selectedAccountId.value) {
      selectedAccountId.value = wallets[0].id
    }
  },
  { immediate: true }
)

function availableCategories(type) {
  return (categoryStore.categories || []).filter(
    (c) => !type || c.category_type === type
  )
}

function triggerFileInput() {
  fileInputRef.value?.click()
}

function handleFileSelected(e) {
  const file = e.target.files?.[0]
  if (file) {
    readFile(file)
  }
}

function handleFileDrop(e) {
  isDragging.value = false
  const file = e.dataTransfer?.files?.[0]
  if (file) {
    readFile(file)
  }
}

function readFile(file) {
  const reader = new FileReader()
  reader.onload = (event) => {
    const content = event.target?.result || ''
    processRawStatement(String(content))
  }
  reader.readAsText(file)
}

function parsePastedText() {
  if (pastedText.value.trim()) {
    processRawStatement(pastedText.value)
  }
}

/**
 * Intelligent Indonesian Bank E-Statement Extractor.
 * Parses lines from BCA, Mandiri, BRI, BNI, Jago, Jenius, and generic CSV.
 */
function processRawStatement(text) {
  const lines = text.split(/\r?\n/).filter((l) => l.trim().length > 0)
  const results = []

  for (const rawLine of lines) {
    const line = rawLine.trim()
    // Skip table headers and empty metadata lines
    if (
      line.toLowerCase().includes('tanggal') &&
      (line.toLowerCase().includes('keterangan') || line.toLowerCase().includes('uraian') || line.toLowerCase().includes('debet'))
    ) {
      continue
    }
    if (line.toLowerCase().startsWith('saldo awal') || line.toLowerCase().startsWith('saldo akhir')) {
      continue
    }

    const tx = extractLineData(line)
    if (tx) {
      results.push(tx)
    }
  }

  if (results.length > 0) {
    // Check against existing transactions to detect duplicates
    checkDuplicates(results)
    parsedTransactions.value = results
  } else {
    alert('Tidak ditemukan baris transaksi yang valid dalam file/teks. Pastikan format mengandung tanggal dan nominal.')
  }
}

function extractLineData(line) {
  // Regex to match Indonesian date formats (DD/MM, DD/MM/YYYY, YYYY-MM-DD, DD-MM-YYYY)
  const dateMatch = line.match(/(\d{1,2}[\/\-]\d{1,2}(?:[\/\-]\d{2,4})?|\d{4}-\d{2}-\d{2})/)
  if (!dateMatch) return null

  const rawDate = dateMatch[0]
  const isoDate = normalizeDate(rawDate)

  // Detect Type: CR (Credit/Income) vs DB (Debit/Expense)
  let txType = 'expense'
  const isCR = /\b(cr|kredit|credit|masuk|\+)\b/i.test(line)
  const isDB = /\b(db|debet|debit|keluar|\-)\b/i.test(line)
  if (isCR && !isDB) {
    txType = 'income'
  } else if (line.includes(',') && !isCR && !isDB) {
    // CSV with separate Debit and Credit columns
    const cols = line.split(/[;,]/).map((c) => c.trim())
    if (cols.length >= 4) {
      const col3 = parseAmountString(cols[2])
      const col4 = parseAmountString(cols[3])
      if (col4 > 0 && col3 === 0) txType = 'income'
    }
  }

  // Extract Amount (Rupiah integer)
  const amount = extractAmount(line)
  if (amount <= 0) return null

  // Extract Clean Description
  const description = extractCleanDescription(line, rawDate)

  // Smart Categorize based on keywords
  const matchedCategoryId = matchCategory(description, txType)

  return {
    date: isoDate,
    dateFormatted: formatFinancialDate(isoDate, { monthFormat: 'short' }),
    description,
    amount,
    transaction_type: txType,
    categoryId: matchedCategoryId,
    selected: true,
    isDuplicate: false
  }
}

function extractAmount(line) {
  // Matches numbers formatted with dots/commas: 1.500.000,00 or 1,500,000.00 or 50000
  const matches = line.match(/(?:Rp\s*)?([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?|[0-9]+)/gi)
  if (!matches) return 0

  let maxVal = 0
  for (const m of matches) {
    const val = parseAmountString(m)
    // Avoid picking up year 2026 or small day numbers
    if (val > 1000 && val > maxVal && val !== 2026) {
      maxVal = val
    }
  }
  return maxVal
}

function parseAmountString(str) {
  if (!str) return 0
  let clean = str.replace(/[^\d.,]/g, '')
  if (clean.includes(',') && clean.includes('.')) {
    if (clean.lastIndexOf(',') > clean.lastIndexOf('.')) {
      // Indonesian format: 1.500.000,00
      clean = clean.replace(/\./g, '').replace(',', '.')
    } else {
      // US format: 1,500,000.00
      clean = clean.replace(/,/g, '')
    }
  } else if (clean.includes(',')) {
    clean = clean.replace(',', '.')
  }
  return Math.round(parseFloat(clean) || 0)
}

function normalizeDate(raw) {
  const now = new Date()
  const currentYear = now.getFullYear()

  if (raw.includes('-') && raw.length === 10 && raw.indexOf('-') === 4) {
    return raw // YYYY-MM-DD
  }

  const parts = raw.split(/[\/\-]/)
  if (parts.length >= 2) {
    const day = parts[0].padStart(2, '0')
    const month = parts[1].padStart(2, '0')
    let year = parts[2] ? parts[2] : String(currentYear)
    if (year.length === 2) year = '20' + year
    return `${year}-${month}-${day}`
  }
  return now.toISOString().split('T')[0]
}

function extractCleanDescription(line, dateStr) {
  let text = line.replace(dateStr, '')
  // Remove amounts and technical banking terms
  text = text.replace(/(?:Rp\s*)?[0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?/gi, '')
  text = text.replace(/\b(cr|db|debet|kredit|cbg|ws\d+|ftscy|bi-fast|qris|trsf|e-banking|m-banking)\b/gi, '')
  text = text.replace(/[,;|\/\\]+/g, ' ')
  text = text.replace(/\s+/g, ' ').trim()
  return text.length > 2 ? text : 'Mutasi Bank'
}

function matchCategory(desc, type) {
  const d = desc.toLowerCase()
  const cats = categoryStore.categories || []

  // Keyword lookup mapping
  if (type === 'income') {
    if (d.includes('gaji') || d.includes('payroll') || d.includes('salary') || d.includes('honor')) {
      const match = cats.find((c) => c.category_type === 'income' && /gaji|salary|pendapatan/i.test(c.name))
      if (match) return match.id
    }
    const defaultIncome = cats.find((c) => c.category_type === 'income')
    return defaultIncome?.id || null
  } else {
    if (d.includes('kopi') || d.includes('makan') || d.includes('resto') || d.includes('food') || d.includes('indomaret') || d.includes('alfamart')) {
      const match = cats.find((c) => c.category_type === 'expense' && /makan|jajan|food/i.test(c.name))
      if (match) return match.id
    }
    if (d.includes('pln') || d.includes('listrik') || d.includes('pdam') || d.includes('wifi') || d.includes('pulsa') || d.includes('telkom')) {
      const match = cats.find((c) => c.category_type === 'expense' && /tagihan|utilitas|bill/i.test(c.name))
      if (match) return match.id
    }
    if (d.includes('shopee') || d.includes('tokopedia') || d.includes('tiktok') || d.includes('lazada') || d.includes('mall')) {
      const match = cats.find((c) => c.category_type === 'expense' && /belanja|shopping/i.test(c.name))
      if (match) return match.id
    }
    const defaultExpense = cats.find((c) => c.category_type === 'expense')
    return defaultExpense?.id || null
  }
}

function checkDuplicates(items) {
  const existing = transactionStore.transactions || []
  for (const item of items) {
    const dup = existing.some(
      (tx) =>
        tx.amount === item.amount &&
        tx.transaction_type === item.transaction_type &&
        tx.date?.substring(0, 10) === item.date
    )
    if (dup) {
      item.isDuplicate = true
      item.selected = false // Uncheck duplicate by default
    }
  }
}

// Computed stats
const selectedCount = computed(() => parsedTransactions.value.filter((t) => t.selected).length)
const duplicateCount = computed(() => parsedTransactions.value.filter((t) => t.isDuplicate).length)
const areAllSelected = computed(() => parsedTransactions.value.length > 0 && selectedCount.value === parsedTransactions.value.length)

const totalExtractedIncome = computed(() =>
  parsedTransactions.value
    .filter((t) => t.selected && t.transaction_type === 'income')
    .reduce((acc, t) => acc + t.amount, 0)
)

const totalExtractedExpense = computed(() =>
  parsedTransactions.value
    .filter((t) => t.selected && t.transaction_type === 'expense')
    .reduce((acc, t) => acc + t.amount, 0)
)

const netExtractedFlow = computed(() => totalExtractedIncome.value - totalExtractedExpense.value)

function toggleSelectAll() {
  const target = !areAllSelected.value
  parsedTransactions.value.forEach((t) => {
    t.selected = target
  })
}

function deselectDuplicates() {
  parsedTransactions.value.forEach((t) => {
    if (t.isDuplicate) t.selected = false
  })
}

function resetParsed() {
  parsedTransactions.value = []
  pastedText.value = ''
  importSuccess.value = false
}

// Load realistic sample Indonesian statement
function loadSampleStatement() {
  const sample = `
15/09/2026,TRSF E-BANKING CR DARI PT INVINITE KREATIF SOLUSI,0,15000000,28500000
16/09/2026,QRIS KOPI KENANGAN SENAYAN CITY,28000,0,28472000
17/09/2026,PEMBAYARAN TOKOPEDIA TRANSAKSI BELANJA,350000,0,28122000
18/09/2026,TAGIHAN LISTRIK PLN PREPAID,250000,0,27872000
19/09/2026,BIAYA ADM REKENING TABUNGAN,15000,0,27857000
`
  processRawStatement(sample)
}

// Commit selected mutations to ledger atomically
async function commitImport() {
  if (!selectedAccountId.value) {
    alert('Silakan pilih rekening tujuan terlebih dahulu.')
    return
  }

  const itemsToImport = parsedTransactions.value.filter((t) => t.selected)
  if (itemsToImport.length === 0) return

  isImporting.value = true
  let successCount = 0

  try {
    for (const item of itemsToImport) {
      await api.createTransaction({
        account_id: selectedAccountId.value,
        category_id: item.categoryId || undefined,
        transaction_type: item.transaction_type,
        amount: item.amount,
        date: item.date,
        description: item.description,
        notes: 'Impor E-Statement Bank (FinRep Pro Personal Accounting)'
      })
      successCount++
    }

    importedCount.value = successCount
    importSuccess.value = true
    await walletStore.fetchWallets()
    emit('imported', successCount)
  } catch (err) {
    alert(err.detail || err.message || 'Sebagian transaksi gagal dimasukkan. Silakan periksa koneksi.')
  } finally {
    isImporting.value = false
  }
}

function handleFinish() {
  emit('close')
}
</script>
