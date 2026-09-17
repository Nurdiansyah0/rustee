<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-4 bg-black/75 backdrop-blur-md select-none animate-in fade-in duration-300"
    role="dialog"
    aria-modal="true"
    aria-labelledby="onboarding-title"
  >
    <div
      class="relative w-full max-w-lg bg-surface-card rounded-2xl shadow-2xl border border-border-subtle overflow-hidden flex flex-col max-h-[94dvh] animate-in zoom-in-95 duration-200"
    >
      <!-- Top Progress Bar & Header -->
      <div class="p-5 sm:p-6 pb-4 border-b border-border-subtle bg-surface-sunken/60 shrink-0">
        <div class="flex items-center justify-between mb-3">
          <div class="flex items-center gap-2">
            <span class="w-6 h-6 rounded-full bg-brand-default text-white text-xs font-black flex items-center justify-center">
              {{ currentStep }}
            </span>
            <span class="text-xs font-bold text-content-secondary uppercase tracking-wider">
              Langkah {{ currentStep }} dari 4
            </span>
          </div>
          <span class="text-[11px] font-bold text-brand-default">
            {{ stepTitles[currentStep - 1] }}
          </span>
        </div>

        <!-- 4-Step Progress Indicator -->
        <div class="grid grid-cols-4 gap-1.5 h-1.5 w-full bg-surface-subtle rounded-full overflow-hidden">
          <div
            v-for="s in 4"
            :key="s"
            :class="[
              'h-full transition-all duration-300 rounded-full',
              s <= currentStep ? 'bg-brand-default' : 'bg-transparent'
            ]"
          ></div>
        </div>
      </div>

      <!-- Scrollable Step Body -->
      <div class="flex-1 overflow-y-auto p-5 sm:p-6 scroll-native space-y-5">
        <!-- ================= STEP 1: NAMA & TUJUAN FINANSIAL ================= -->
        <div v-if="currentStep === 1" class="space-y-4 animate-in fade-in duration-200">
          <div>
            <h3 id="onboarding-title" class="text-lg sm:text-xl font-black text-content-primary tracking-tight">
              Selamat datang! Siapa nama panggilan Anda?
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Personalisasikan dasbor finansial Anda untuk pengalaman pencatatan yang presisi dan relevan.
            </p>
          </div>

          <div>
            <label for="onboarding-name" class="block text-xs font-bold text-content-secondary mb-1.5">
              Nama Tampilan Preferensi
            </label>
            <input
              id="onboarding-name"
              v-model="userName"
              type="text"
              placeholder="Contoh: Nurdiansyah"
              class="w-full px-4 py-3 border border-border-default rounded-xl bg-surface-sunken text-content-primary text-sm font-medium focus:outline-none focus:ring-2 focus:ring-brand-default/40"
            />
          </div>

          <div>
            <label class="block text-xs font-bold text-content-secondary mb-2">
              Apa tujuan utama keuangan Anda saat ini? (Pilih satu atau lebih)
            </label>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
              <button
                v-for="goal in goalOptions"
                :key="goal.id"
                type="button"
                @click="toggleGoal(goal.id)"
                :class="[
                  'p-3 rounded-xl border text-left text-xs font-semibold transition cursor-pointer flex items-start gap-2.5',
                  selectedGoals.includes(goal.id)
                    ? 'border-brand-default bg-brand-muted/40 text-content-primary ring-1 ring-brand-default/30 shadow-xs'
                    : 'border-border-default bg-surface-sunken text-content-secondary hover:text-content-primary hover:bg-surface-subtle'
                ]"
              >
                <div
                  :class="[
                    'w-4 h-4 rounded-md border mt-0.5 flex items-center justify-center shrink-0 transition',
                    selectedGoals.includes(goal.id)
                      ? 'bg-brand-default border-brand-default text-white'
                      : 'border-border-default bg-surface-card'
                  ]"
                >
                  <Check v-if="selectedGoals.includes(goal.id)" class="w-3 h-3 stroke-[3]" />
                </div>
                <div>
                  <div class="font-bold text-content-primary">{{ goal.title }}</div>
                  <div class="text-[11px] text-content-muted font-normal mt-0.5">{{ goal.desc }}</div>
                </div>
              </button>
            </div>
          </div>
        </div>

        <!-- ================= STEP 2: KONFIGURASI MULTI-DOMPET ================= -->
        <div v-else-if="currentStep === 2" class="space-y-4 animate-in fade-in duration-200">
          <div>
            <h3 class="text-lg sm:text-xl font-black text-content-primary tracking-tight">
              Konfigurasi Dompet & Rekening
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Atur wadah keuangan Anda. Kelola rekening bank, dompet digital (e-wallet), dan uang tunai secara terpisah.
            </p>
          </div>

          <div class="space-y-3">
            <div
              v-for="(acc, index) in walletsConfig"
              :key="index"
              class="p-3.5 bg-surface-sunken rounded-xl border border-border-default space-y-2.5"
            >
              <div class="flex items-center justify-between">
                <span class="text-[11px] font-extrabold uppercase tracking-wider text-content-muted">
                  Dompet #{{ index + 1 }}
                </span>
                <button
                  v-if="walletsConfig.length > 1"
                  type="button"
                  @click="removeWallet(index)"
                  class="text-expense-default hover:text-expense-default/80 text-[11px] font-bold cursor-pointer"
                >
                  Hapus
                </button>
              </div>

              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <div>
                  <label class="block text-[11px] font-medium text-content-secondary mb-1">Nama Rekening / Dompet</label>
                  <input
                    v-model="acc.name"
                    type="text"
                    placeholder="Contoh: BCA Prioritas"
                    class="w-full px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
                  />
                </div>
                <div>
                  <label class="block text-[11px] font-medium text-content-secondary mb-1">Tipe Akun</label>
                  <select
                    v-model="acc.type"
                    class="w-full px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
                  >
                    <option value="checking">Rekening Bank (Checking)</option>
                    <option value="savings">Tabungan (Savings)</option>
                    <option value="e_wallet">Dompet Digital (GoPay/DANA/OVO)</option>
                    <option value="cash">Uang Tunai (Cash)</option>
                  </select>
                </div>
              </div>

              <div>
                <label class="block text-[11px] font-medium text-content-secondary mb-1">Saldo Awal (Rupiah)</label>
                <div class="relative">
                  <span class="absolute left-3 top-1/2 -translate-y-1/2 text-xs font-bold text-content-muted">Rp</span>
                  <input
                    v-model="acc.initial_balance"
                    type="number"
                    min="0"
                    step="1000"
                    placeholder="0"
                    class="w-full pl-9 pr-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary tabular-nums focus:outline-none focus:ring-2 focus:ring-brand-default/30"
                  />
                </div>
              </div>
            </div>

            <!-- Add Wallet Button -->
            <button
              type="button"
              @click="addWalletSlot"
              class="w-full py-2.5 px-3 border border-dashed border-border-default hover:border-brand-default hover:bg-surface-subtle text-content-secondary hover:text-brand-default font-bold text-xs rounded-xl transition cursor-pointer flex items-center justify-center gap-1.5"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Tambah Rekening / Dompet Lain</span>
            </button>
          </div>
        </div>

        <!-- ================= STEP 3: KUSTOMISASI KOSAKATA KATEGORI ================= -->
        <div v-else-if="currentStep === 3" class="space-y-4 animate-in fade-in duration-200">
          <div>
            <div class="flex items-center gap-1.5 text-brand-default text-xs font-bold mb-1">
              <Sparkles class="w-3.5 h-3.5" />
              <span>User-Owned Financial Vocabulary (§4)</span>
            </div>
            <h3 class="text-lg sm:text-xl font-black text-content-primary tracking-tight">
              Tentukan Istilah Kategori Anda
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Anda tidak dipaksa memakai istilah baku. Tentukan dan sesuaikan sebutan pemasukan & pengeluaran yang cocok dengan gaya hidup Anda.
            </p>
          </div>

          <!-- Vocabulary Tabs (Pemasukan vs Pengeluaran) -->
          <div class="flex p-1 bg-surface-subtle rounded-xl border border-border-subtle text-xs font-bold">
            <button
              type="button"
              @click="vocabTab = 'expense'"
              :class="[
                'flex-1 py-2 rounded-lg transition cursor-pointer',
                vocabTab === 'expense'
                  ? 'bg-surface-card text-expense-default shadow-xs border border-border-subtle'
                  : 'text-content-secondary hover:text-content-primary'
              ]"
            >
              Pengeluaran ({{ customExpenseCategories.length }})
            </button>
            <button
              type="button"
              @click="vocabTab = 'income'"
              :class="[
                'flex-1 py-2 rounded-lg transition cursor-pointer',
                vocabTab === 'income'
                  ? 'bg-surface-card text-income-default shadow-xs border border-border-subtle'
                  : 'text-content-secondary hover:text-content-primary'
              ]"
            >
              Pemasukan ({{ customIncomeCategories.length }})
            </button>
          </div>

          <!-- Active Categories List -->
          <div>
            <div class="text-[11px] font-bold text-content-muted uppercase tracking-wider mb-2">
              Kategori Terpilih (Klik untuk Hapus / Edit)
            </div>
            <div class="flex flex-wrap gap-2">
              <div
                v-for="(cat, idx) in activeVocabList"
                :key="cat"
                class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-surface-sunken border border-border-default text-content-primary"
              >
                <span>{{ cat }}</span>
                <button
                  type="button"
                  @click="removeCategory(cat)"
                  class="text-content-muted hover:text-expense-default cursor-pointer"
                  title="Hapus istilah ini"
                >
                  <X class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          </div>

          <!-- Add Custom Category Input -->
          <div class="p-3 bg-surface-sunken rounded-xl border border-border-default space-y-2">
            <label class="block text-xs font-bold text-content-secondary">
              Tambah Kategori Kustom Anda Sendiri
            </label>
            <div class="flex gap-2">
              <input
                v-model="newCategoryInput"
                type="text"
                @keyup.enter="addNewCategory"
                :placeholder="vocabTab === 'expense' ? 'Contoh: Kopi Santai, Bensin Motor' : 'Contoh: Honor Konsultasi, Dividen'"
                class="flex-1 px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
              />
              <button
                type="button"
                @click="addNewCategory"
                class="px-4 py-2 bg-brand-default hover:bg-brand-emphasis text-white font-bold text-xs rounded-lg transition cursor-pointer"
              >
                Tambah
              </button>
            </div>
          </div>

          <!-- Suggestion Chips -->
          <div>
            <div class="text-[11px] font-semibold text-content-muted mb-1.5">
              Rekomendasi Cepat (Klik untuk Tambahkan):
            </div>
            <div class="flex flex-wrap gap-1.5">
              <button
                v-for="sug in availableSuggestions"
                :key="sug"
                type="button"
                @click="addSuggestedCategory(sug)"
                class="px-2.5 py-1 text-[11px] font-medium rounded-full bg-surface-subtle hover:bg-border-default border border-border-subtle text-content-secondary cursor-pointer transition"
              >
                + {{ sug }}
              </button>
            </div>
          </div>
        </div>

        <!-- ================= STEP 4: PRO VALUE & 3-MONTH TRIAL ================= -->
        <div v-else-if="currentStep === 4" class="space-y-4 animate-in fade-in duration-200">
          <div class="text-center">
            <div class="inline-flex items-center justify-center w-12 h-12 mb-2 bg-gradient-to-br from-amber-500/20 to-emerald-500/20 rounded-xl text-amber-500 border border-amber-500/30">
              <Sparkles class="w-6 h-6" />
            </div>
            <h3 class="text-xl font-black text-content-primary tracking-tight">
              Buka Potensi Finansial Penuh
            </h3>
            <p class="text-xs text-content-secondary mt-1 max-w-sm mx-auto leading-relaxed">
              Sebagai pengguna baru, nikmati akses FinRep Pro selama 3 bulan penuh tanpa biaya di depan.
            </p>
          </div>

          <!-- Pro Value Cards -->
          <div class="p-4 rounded-xl bg-gradient-to-br from-amber-500/15 via-emerald-500/15 to-emerald-500/10 border border-amber-500/30 space-y-3">
            <div class="flex items-center justify-between">
              <span class="text-xs font-black text-amber-500 uppercase tracking-wider flex items-center gap-1.5">
                <Sparkles class="w-3.5 h-3.5" />
                Uji Coba Eksklusif
              </span>
              <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-emerald-500/25 text-emerald-400 border border-emerald-500/30">
                3 Bulan Gratis (90 Hari)
              </span>
            </div>

            <div class="space-y-2 text-xs text-content-secondary">
              <div class="flex items-start gap-2">
                <Check class="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                <span><strong class="text-content-primary">Analitik Runway Multi-Bulan</strong>: Prediksi ketahanan kas berdasarkan tren pengeluaran harian.</span>
              </div>
              <div class="flex items-start gap-2">
                <Check class="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                <span><strong class="text-content-primary">Skor Kesehatan Finansial</strong>: Evaluasi rasio tabungan dan likuiditas setara standar perbankan.</span>
              </div>
              <div class="flex items-start gap-2">
                <Check class="w-4 h-4 text-emerald-400 shrink-0 mt-0.5" />
                <span><strong class="text-content-primary">Ekspor Laporan Terenkripsi</strong>: Unduh buku kas rapi dalam format CSV dan PDF siap pajak.</span>
              </div>
            </div>

            <div class="pt-2 border-t border-amber-500/20 text-[11px] text-content-muted flex items-center justify-between">
              <span>Rp 0 di depan • Tanpa kartu kredit</span>
              <span class="text-amber-400 font-bold">Otomatis kembali ke Free Tier</span>
            </div>
          </div>

          <!-- Value Guarantees -->
          <div class="flex items-center gap-2 p-3 bg-surface-sunken rounded-xl border border-border-default text-xs text-content-secondary">
            <ShieldCheck class="w-5 h-5 text-brand-default shrink-0" />
            <div class="text-[11px] leading-snug">
              Seluruh data transaksi dan dompet Anda <strong>100% aman tersimpan</strong> selamanya, baik di Free Tier maupun Pro.
            </div>
          </div>
        </div>
      </div>

      <!-- Footer Action Buttons -->
      <div class="p-4 sm:p-5 border-t border-border-subtle bg-surface-sunken/40 flex items-center justify-between gap-3 shrink-0">
        <!-- Back Button (Steps 2, 3, 4) -->
        <button
          v-if="currentStep > 1"
          type="button"
          @click="currentStep--"
          :disabled="isSubmitting"
          class="px-4 py-2.5 border border-border-default hover:bg-surface-subtle text-content-secondary font-bold text-xs rounded-xl transition cursor-pointer disabled:opacity-50"
        >
          Kembali
        </button>
        <div v-else></div>

        <!-- Next / Finish CTAs -->
        <div class="flex items-center gap-2">
          <!-- Step 1, 2, 3: Continue -->
          <button
            v-if="currentStep < 4"
            type="button"
            @click="handleNextStep"
            class="px-5 py-2.5 bg-brand-default hover:bg-brand-emphasis active:scale-95 text-white font-bold text-xs rounded-xl shadow-md transition cursor-pointer flex items-center gap-1.5"
          >
            <span>Lanjutkan</span>
            <ChevronRight class="w-3.5 h-3.5" />
          </button>

          <!-- Step 4: Trial or Free Tier -->
          <template v-else>
            <button
              type="button"
              @click="finishOnboarding(false)"
              :disabled="isSubmitting"
              class="px-3.5 py-2.5 text-content-muted hover:text-content-primary font-bold text-xs rounded-xl transition cursor-pointer disabled:opacity-50"
            >
              Gunakan Free Tier
            </button>
            <button
              type="button"
              @click="finishOnboarding(true)"
              :disabled="isSubmitting"
              class="px-5 py-2.5 bg-gradient-to-r from-amber-500 to-emerald-600 hover:from-amber-600 hover:to-emerald-700 active:scale-95 text-white font-bold text-xs rounded-xl shadow-lg shadow-amber-500/20 transition cursor-pointer disabled:opacity-50 flex items-center gap-1.5"
            >
              <Sparkles class="w-3.5 h-3.5" />
              <span v-if="!isSubmitting">Aktifkan 3 Bulan Gratis</span>
              <span v-else>Menyiapkan Workspace...</span>
            </button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue'
import { api } from '@/services/api'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useCategoryStore } from '@/stores/categories'
import { useAnalyticsStore } from '@/stores/analytics'
import { useSubscriptionStore } from '@/stores/subscription'
import { Check, Plus, X, Sparkles, ShieldCheck, ChevronRight } from 'lucide-vue-next'

const props = defineProps({
  isOpen: Boolean,
  initialName: {
    type: String,
    default: ''
  }
})

const emit = defineEmits(['completed', 'close'])

const authStore = useAuthStore()
const walletStore = useWalletStore()
const categoryStore = useCategoryStore()
const analyticsStore = useAnalyticsStore()
const subscriptionStore = useSubscriptionStore()

const currentStep = ref(1)
const isSubmitting = ref(false)

const stepTitles = [
  'Profil & Tujuan Finansial',
  'Konfigurasi Multi-Dompet',
  'Kustomisasi Kosakata Kategori',
  'Aktivasi Fitur & Pro Trial'
]

// ================= STEP 1 STATE =================
const userName = ref(props.initialName || authStore.user?.display_name || authStore.user?.name || '')
const goalOptions = [
  { id: 'emergency', title: 'Dana Darurat 6 Bulan', desc: 'Membangun cadangan dana darurat aman' },
  { id: 'debt_free', title: 'Bebas Hutang & Cicilan', desc: 'Melunasi kewajiban finansial secara disiplin' },
  { id: 'invest', title: 'Investasi Rutin Bulanan', desc: 'Mengalokasikan dana ke reksa dana / saham' },
  { id: 'daily_control', title: 'Kontrol Belanja Harian', desc: 'Menghindari kebocoran halus dan impulsive buying' },
  { id: 'property', title: 'Tabungan Rumah / Properti', desc: 'Menyiapkan uang muka (DP) hunian masa depan' },
  { id: 'pension', title: 'Dana Pensiun Mandiri', desc: 'Menyiapkan kebebasan finansial jangka panjang' }
]
const selectedGoals = ref(['emergency', 'daily_control'])

function toggleGoal(goalId) {
  if (selectedGoals.value.includes(goalId)) {
    selectedGoals.value = selectedGoals.value.filter(g => g !== goalId)
  } else {
    selectedGoals.value.push(goalId)
  }
}

// ================= STEP 2 STATE =================
const walletsConfig = ref([
  { name: 'Rekening Utama (BCA)', type: 'checking', initial_balance: 1000000 },
  { name: 'E-Wallet (GoPay/DANA)', type: 'e_wallet', initial_balance: 250000 },
  { name: 'Uang Tunai (Cash)', type: 'cash', initial_balance: 100000 }
])

function addWalletSlot() {
  walletsConfig.value.push({
    name: 'Dompet Baru',
    type: 'checking',
    initial_balance: 0
  })
}

function removeWallet(index) {
  if (walletsConfig.value.length > 1) {
    walletsConfig.value.splice(index, 1)
  }
}

// ================= STEP 3 STATE =================
const vocabTab = ref('expense')
const customExpenseCategories = ref([
  'Makan & Minum',
  'Transportasi & Bensin',
  'Kebutuhan Rumah',
  'Hiburan & Santai',
  'Belanja & Lifestyle'
])
const customIncomeCategories = ref([
  'Gaji Pokok',
  'Freelance & Proyek',
  'Bisnis & Penjualan',
  'Investasi & Dividen'
])
const newCategoryInput = ref('')

const expenseSuggestions = [
  'Kopi & Jajan',
  'Listrik & Air',
  'Internet & Pulsa',
  'Kesehatan & Obat',
  'Pendidikan',
  'Zakat & Sedekah',
  'Cicilan',
  'Peliharaan'
]

const incomeSuggestions = [
  'Bonus & THR',
  'Komisi Affiliate',
  'Hadiah / Hibah',
  'Rental & Sewa',
  'Refund'
]

const activeVocabList = computed(() => {
  return vocabTab.value === 'expense' ? customExpenseCategories.value : customIncomeCategories.value
})

const availableSuggestions = computed(() => {
  const current = activeVocabList.value
  const pool = vocabTab.value === 'expense' ? expenseSuggestions : incomeSuggestions
  return pool.filter(s => !current.includes(s))
})

function removeCategory(cat) {
  if (vocabTab.value === 'expense') {
    customExpenseCategories.value = customExpenseCategories.value.filter(c => c !== cat)
  } else {
    customIncomeCategories.value = customIncomeCategories.value.filter(c => c !== cat)
  }
}

function addNewCategory() {
  const term = newCategoryInput.value.trim()
  if (!term) return

  if (vocabTab.value === 'expense') {
    if (!customExpenseCategories.value.includes(term)) {
      customExpenseCategories.value.push(term)
    }
  } else {
    if (!customIncomeCategories.value.includes(term)) {
      customIncomeCategories.value.push(term)
    }
  }
  newCategoryInput.value = ''
}

function addSuggestedCategory(term) {
  if (vocabTab.value === 'expense') {
    if (!customExpenseCategories.value.includes(term)) {
      customExpenseCategories.value.push(term)
    }
  } else {
    if (!customIncomeCategories.value.includes(term)) {
      customIncomeCategories.value.push(term)
    }
  }
}

// ================= NAVIGATION & PERSISTENCE =================
function handleNextStep() {
  if (currentStep.value < 4) {
    currentStep.value++
  }
}

/**
 * Commits personalized configuration to backend and initializes personalized workspace.
 * 
 * @param {boolean} activateProTrial - Whether user elected to start 3-month free trial
 */
async function finishOnboarding(activateProTrial = false) {
  isSubmitting.value = true
  try {
    // 1. Sync User-Configured Wallets / Accounts (/api/v1/accounts)
    for (const wallet of walletsConfig.value) {
      if (wallet.name?.trim()) {
        try {
          await api.createAccount({
            name: wallet.name.trim(),
            account_type: wallet.type || 'checking',
            initial_balance: parseInt(wallet.initial_balance, 10) || 0,
            currency: 'IDR'
          })
        } catch (e) {
          console.warn('Onboarding account creation fallback', e)
        }
      }
    }

    // 2. Sync Custom Vocabulary Categories (/api/v1/categories)
    // Expense categories
    for (const catName of customExpenseCategories.value) {
      if (catName?.trim()) {
        try {
          await api.createCategory({
            name: catName.trim(),
            display_name: catName.trim(),
            category_type: 'expense',
            icon: 'Tag'
          })
        } catch (e) {
          console.warn('Onboarding category creation fallback', e)
        }
      }
    }

    // Income categories
    for (const catName of customIncomeCategories.value) {
      if (catName?.trim()) {
        try {
          await api.createCategory({
            name: catName.trim(),
            display_name: catName.trim(),
            category_type: 'income',
            icon: 'DollarSign'
          })
        } catch (e) {
          console.warn('Onboarding category creation fallback', e)
        }
      }
    }

    // 3. If User Opted for 3-Month Pro Trial: Call /api/v1/subscriptions/trial/activate
    if (activateProTrial) {
      try {
        await subscriptionStore.activateTrial()
      } catch (err) {
        console.warn('Trial activation error during onboarding', err)
      }
    }

    // 4. Mark onboarding finished in local storage & memory
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('invinite_onboarding_completed', 'true')
      localStorage.setItem(
        'invinite_user_personalization',
        JSON.stringify({
          display_name: userName.value || 'Sobat FinRep',
          goals: selectedGoals.value,
          onboarded_at: new Date().toISOString()
        })
      )
    }

    // 5. Refresh all reactive Pinia stores
    await Promise.allSettled([
      walletStore.fetchWallets(),
      categoryStore.fetchCategories(),
      analyticsStore.fetchDashboard(),
      subscriptionStore.fetchSubscriptionStatus(),
      authStore.checkAuth()
    ])

    emit('completed')
  } catch (err) {
    console.error('Failed to finalize personalization onboarding', err)
    emit('completed')
  } finally {
    isSubmitting.value = false
  }
}
</script>
