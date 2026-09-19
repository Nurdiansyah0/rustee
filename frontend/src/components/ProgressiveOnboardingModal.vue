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
              Tentukan Sebutan Finansial Anda
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Anda memegang kendali penuh atas istilah keuangan Anda. Tentukan sebutan pemasukan utama, pengeluaran utama, serta sesuaikan daftar kategori sesuai kebutuhan.
            </p>
          </div>

          <!-- Primary Income & Expense Title Inputs (§4) -->
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 p-3.5 bg-surface-sunken rounded-xl border border-border-default">
            <div>
              <label class="block text-xs font-bold text-income-default mb-1">
                Sebutan Pemasukan Utama
              </label>
              <input
                v-model="primaryIncomeTitle"
                type="text"
                placeholder="Contoh: Gaji, Freelance"
                class="w-full px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-income-default/30"
              />
              <div class="flex flex-wrap gap-1 mt-1.5">
                <button
                  v-for="sug in ['Gaji', 'Freelance', 'Bisnis', 'Honor']"
                  :key="sug"
                  type="button"
                  @click="primaryIncomeTitle = sug"
                  class="px-2 py-0.5 text-[10px] font-semibold rounded bg-surface-subtle hover:bg-border-default text-content-secondary cursor-pointer transition"
                >
                  {{ sug }}
                </button>
              </div>
            </div>

            <div>
              <label class="block text-xs font-bold text-expense-default mb-1">
                Sebutan Pengeluaran Utama
              </label>
              <input
                v-model="primaryExpenseTitle"
                type="text"
                placeholder="Contoh: Makan, Belanja"
                class="w-full px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-expense-default/30"
              />
              <div class="flex flex-wrap gap-1 mt-1.5">
                <button
                  v-for="sug in ['Makan & Jajan', 'Kebutuhan Rumah', 'Operasional', 'Belanja']"
                  :key="sug"
                  type="button"
                  @click="primaryExpenseTitle = sug"
                  class="px-2 py-0.5 text-[10px] font-semibold rounded bg-surface-subtle hover:bg-border-default text-content-secondary cursor-pointer transition"
                >
                  {{ sug }}
                </button>
              </div>
            </div>
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
              Kategori Pengeluaran ({{ customExpenseCategories.length }})
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
              Kategori Pemasukan ({{ customIncomeCategories.length }})
            </button>
          </div>

          <!-- Active Categories List -->
          <div>
            <div class="text-[11px] font-bold text-content-muted uppercase tracking-wider mb-2">
              Kategori Aktif (Klik X untuk Hapus)
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

        <!-- ================= STEP 4: PILIHAN PAKET BERLANGGANAN (R2 §6, §16, §17) ================= -->
        <div v-else-if="currentStep === 4" class="space-y-4 animate-in fade-in duration-200">
          <div class="text-center">
            <div class="inline-flex items-center justify-center w-12 h-12 mb-2 bg-gradient-to-br from-amber-500/20 to-emerald-500/20 rounded-xl text-amber-500 border border-amber-500/30">
              <Sparkles class="w-6 h-6" />
            </div>
            <h3 class="text-xl font-black text-content-primary tracking-tight">
              Pilih Paket Berlangganan Anda
            </h3>
            <p class="text-xs text-content-secondary mt-1 max-w-sm mx-auto leading-relaxed">
              Tentukan paket yang sesuai. Anda selalu dapat mengubah atau meng-upgrade paket kapan saja.
            </p>
          </div>

          <!-- 4 Subscription Plan Options (§6, §16) -->
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <!-- 1. 3-Month Free Trial -->
            <div
              @click="selectedPlanOption = 'trial'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'trial'
                  ? 'border-amber-500 bg-amber-500/10 ring-2 ring-amber-500/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-amber-500/50'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-500 border border-amber-500/30">
                  Rekomendasi
                </span>
                <span class="text-xs font-black text-emerald-500">Rp 0</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary flex items-center gap-1">
                  <span>3 Bulan Trial Pro</span>
                  <Sparkles class="w-3.5 h-3.5 text-amber-500" />
                </div>
                <div class="text-[11px] text-content-muted mt-0.5">
                  90 hari fitur analitik & proyeksi penuh tanpa biaya di depan.
                </div>
              </div>
              <div class="mt-2 text-[10px] text-amber-500 font-bold">
                Otomatis ke Free Tier setelah 90 hari
              </div>
            </div>

            <!-- 2. Continue with Free Tier -->
            <div
              @click="selectedPlanOption = 'free'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'free'
                  ? 'border-brand-default bg-brand-default/10 ring-2 ring-brand-default/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-brand-default/50'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-bold px-2 py-0.5 rounded-full bg-surface-subtle text-content-secondary border border-border-subtle">
                  Dasar
                </span>
                <span class="text-xs font-black text-content-primary">Rp 0</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary">
                  Continue with Free
                </div>
                <div class="text-[11px] text-content-muted mt-0.5">
                  Pencatatan kas harian, multi-dompet, dan rekonsiliasi data tanpa batas.
                </div>
              </div>
              <div class="mt-2 text-[10px] text-content-secondary font-bold">
                Gratis selamanya
              </div>
            </div>

            <!-- 3. Monthly Premium (Rp 10.000 / bln) -->
            <div
              @click="selectedPlanOption = 'monthly'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'monthly'
                  ? 'border-[#118EEA] bg-[#118EEA]/10 ring-2 ring-[#118EEA]/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-[#118EEA]/50'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-bold px-2 py-0.5 rounded-full bg-[#118EEA]/20 text-[#118EEA]">
                  Bulanan
                </span>
                <span class="text-xs font-black text-content-primary">Rp 10.000 / bln</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary flex items-center gap-1">
                  <span>Pro Bulanan via DANA</span>
                </div>
                <div class="text-[11px] text-content-muted mt-0.5">
                  Akses analitik runway, skor kesehatan & integrasi DANA instan.
                </div>
              </div>
              <div class="mt-2 text-[10px] text-[#118EEA] font-bold">
                Pembayaran instan via DANA
              </div>
            </div>

            <!-- 4. Annual Premium (Rp 110.000 / thn) -->
            <div
              @click="selectedPlanOption = 'annual'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'annual'
                  ? 'border-[#118EEA] bg-[#118EEA]/10 ring-2 ring-[#118EEA]/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-[#118EEA]/50'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-500">
                  Hemat Rp 10.000
                </span>
                <span class="text-xs font-black text-content-primary">Rp 110.000 / thn</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary flex items-center gap-1">
                  <span>Pro Tahunan via DANA</span>
                </div>
                <div class="text-[11px] text-content-muted mt-0.5">
                  12 bulan penuh dengan harga 11 bulan. Ekspor laporan tak terbatas.
                </div>
              </div>
              <div class="mt-2 text-[10px] text-[#118EEA] font-bold">
                Pembayaran instan via DANA
              </div>
            </div>
          </div>

          <!-- Guarantee & Single Gateway Notice -->
          <div class="flex items-center gap-2 p-3 bg-surface-sunken rounded-xl border border-border-default text-xs text-content-secondary">
            <ShieldCheck class="w-5 h-5 text-[#118EEA] shrink-0" />
            <div class="text-[11px] leading-snug">
              Pembayaran online diproses secara aman menggunakan <strong>DANA Open API & SNAP</strong> resmi dengan enkripsi RSA-SHA256.
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

          <!-- Step 4: Selected Plan CTA -->
          <template v-else>
            <button
              type="button"
              @click="finishOnboarding"
              :disabled="isSubmitting"
              :class="[
                'px-5 py-2.5 font-bold text-xs rounded-xl shadow-lg transition cursor-pointer disabled:opacity-50 flex items-center gap-1.5',
                selectedPlanOption === 'trial'
                  ? 'bg-gradient-to-r from-amber-500 to-emerald-600 hover:from-amber-600 hover:to-emerald-700 text-white shadow-amber-500/20'
                  : selectedPlanOption === 'free'
                  ? 'bg-surface-sunken text-content-primary border border-border-default hover:bg-surface-subtle'
                  : 'bg-[#118EEA] hover:bg-[#0c7acb] text-white shadow-[#118EEA]/20'
              ]"
            >
              <Sparkles v-if="selectedPlanOption === 'trial'" class="w-3.5 h-3.5" />
              <Check v-else-if="selectedPlanOption === 'free'" class="w-3.5 h-3.5" />
              <span v-if="!isSubmitting">
                {{
                  selectedPlanOption === 'trial'
                    ? 'Aktifkan 3 Bulan Gratis (Rp 0)'
                    : selectedPlanOption === 'free'
                    ? 'Lanjutkan dengan Free Tier'
                    : selectedPlanOption === 'annual'
                    ? 'Lanjutkan via DANA (Rp 110.000 / thn)'
                    : 'Lanjutkan via DANA (Rp 10.000 / bln)'
                }}
              </span>
              <span v-else>Menyiapkan Workspace...</span>
            </button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
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

watch(
  () => props.isOpen,
  (open) => {
    if (open) {
      currentStep.value = 1
      if (props.initialName || authStore.displayName) {
        userName.value = props.initialName || authStore.displayName || ''
      }
    }
  }
)

watch(
  () => props.initialName,
  (name) => {
    if (name) {
      userName.value = name
    }
  }
)
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
const primaryIncomeTitle = ref('Gaji')
const primaryExpenseTitle = ref('Makan & Jajan')
const selectedPlanOption = ref('trial') // 'trial' | 'free' | 'monthly' | 'annual'

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
 */
async function finishOnboarding() {
  isSubmitting.value = true
  try {
    const isTrial = selectedPlanOption.value === 'trial'

    // 1. Submit atomic onboarding payload to backend (§4, §5, §6)
    try {
      const res = await api.submitOnboarding({
        display_name: userName.value || 'Sobat FinRep',
        income_title: primaryIncomeTitle.value || 'Gaji',
        expense_title: primaryExpenseTitle.value || 'Makan & Jajan',
        financial_goals: selectedGoals.value,
        wallets: walletsConfig.value.map((wallet) => ({
          name: wallet.name?.trim() || 'Dompet',
          account_type: wallet.type || 'checking',
          initial_balance: parseInt(wallet.initial_balance, 10) || 0
        })),
        categories: [
          ...customExpenseCategories.value.map((catName) => ({
            name: catName.trim(),
            display_name: catName.trim(),
            category_type: 'expense'
          })),
          ...customIncomeCategories.value.map((catName) => ({
            name: catName.trim(),
            display_name: catName.trim(),
            category_type: 'income'
          }))
        ],
        activate_trial: isTrial
      })

      if (res?.personalization) {
        authStore.setPersonalization(res.personalization)
      } else {
        authStore.setPersonalization({
          display_name: userName.value || 'Sobat FinRep',
          income_title: primaryIncomeTitle.value || 'Gaji',
          expense_title: primaryExpenseTitle.value || 'Makan & Jajan',
          financial_goals: selectedGoals.value,
          onboarding_completed: true
        })
      }
    } catch (e) {
      console.warn('Backend onboarding API fallback, creating directly', e)
      authStore.setPersonalization({
        display_name: userName.value || 'Sobat FinRep',
        income_title: primaryIncomeTitle.value || 'Gaji',
        expense_title: primaryExpenseTitle.value || 'Makan & Jajan',
        financial_goals: selectedGoals.value,
        onboarding_completed: true
      })
      // Fallback: sync accounts & categories directly
      for (const wallet of walletsConfig.value) {
        if (wallet.name?.trim()) {
          try {
            await api.createAccount({
              name: wallet.name.trim(),
              account_type: wallet.type || 'checking',
              initial_balance: parseInt(wallet.initial_balance, 10) || 0,
              currency: 'IDR'
            })
          } catch {}
        }
      }
      for (const catName of customExpenseCategories.value) {
        if (catName?.trim()) {
          try {
            await api.createCategory({
              name: catName.trim(),
              display_name: catName.trim(),
              category_type: 'expense',
              icon: 'Tag'
            })
          } catch {}
        }
      }
      for (const catName of customIncomeCategories.value) {
        if (catName?.trim()) {
          try {
            await api.createCategory({
              name: catName.trim(),
              display_name: catName.trim(),
              category_type: 'income',
              icon: 'DollarSign'
            })
          } catch {}
        }
      }
      if (isTrial) {
        try {
          await subscriptionStore.activateTrial()
        } catch {}
      }
    }

    // 2. If user chose Monthly or Annual paid subscription, initiate DANA checkout
    if (selectedPlanOption.value === 'monthly' || selectedPlanOption.value === 'annual') {
      try {
        const planId = selectedPlanOption.value === 'annual' ? 'premium_annual' : 'premium_monthly'
        const checkoutRes = await subscriptionStore.initiateCheckout('dana', planId)
        if (checkoutRes?.checkout_url && typeof window !== 'undefined') {
          window.location.href = checkoutRes.checkout_url
          return
        }
      } catch (err) {
        console.warn('DANA checkout error during onboarding', err)
      }
    }

    // 3. Mark onboarding completed in local storage
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('invinite_onboarding_completed', 'true')
      localStorage.setItem(
        'invinite_user_personalization',
        JSON.stringify({
          display_name: userName.value || 'Sobat FinRep',
          income_title: primaryIncomeTitle.value || 'Gaji',
          expense_title: primaryExpenseTitle.value || 'Makan & Jajan',
          goals: selectedGoals.value,
          financial_goals: selectedGoals.value,
          plan: selectedPlanOption.value,
          onboarding_completed: true,
          onboarded_at: new Date().toISOString()
        })
      )
    }

    // 4. Refresh Pinia stores
    await Promise.allSettled([
      walletStore.fetchWallets(),
      categoryStore.fetchCategories(),
      analyticsStore.fetchDashboard(),
      subscriptionStore.fetchSubscriptionStatus(),
      authStore.fetchPersonalization(),
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
