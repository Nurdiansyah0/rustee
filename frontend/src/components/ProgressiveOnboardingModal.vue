<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-4 bg-black/75 backdrop-blur-md select-none animate-in fade-in duration-300"
    role="dialog"
    aria-modal="true"
    aria-labelledby="onboarding-title"
  >
    <div
      class="relative w-full max-w-xl bg-surface-card rounded-2xl shadow-2xl border border-border-subtle overflow-hidden flex flex-col max-h-[94dvh] animate-in zoom-in-95 duration-200"
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
        <!-- ================= STEP 1: PROFIL PENGELOLA & RUANG KERJA BISNIS ================= -->
        <div v-if="currentStep === 1" class="space-y-4 animate-in fade-in duration-200">
          <div>
            <div class="flex items-center gap-1.5 text-brand-default text-xs font-bold mb-1">
              <Building2 class="w-4 h-4" />
              <span>Invinite Business OS • Inisialisasi Ruang Kerja</span>
            </div>
            <h3 id="onboarding-title" class="text-lg sm:text-xl font-black text-content-primary tracking-tight">
              Selamat datang! Siapkan profil bisnis Anda
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Konfigurasikan ruang kerja operasional Anda. Sistem akan menyiapkan seluruh modul transaksi, stok, dan pembukuan yang sesuai.
            </p>
          </div>

          <div class="space-y-3.5 pt-1">
            <div>
              <label for="onboarding-name" class="block text-xs font-bold text-content-secondary mb-1.5">
                Nama Pemilik / Penanggung Jawab <span class="text-brand-default">*</span>
              </label>
              <input
                id="onboarding-name"
                v-model="userName"
                type="text"
                placeholder="Contoh: Budi Santoso"
                class="w-full px-4 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary text-sm font-medium focus:outline-none focus:ring-2 focus:ring-brand-default/40"
              />
            </div>

            <div>
              <label for="onboarding-business-name" class="block text-xs font-bold text-content-secondary mb-1.5">
                Nama Bisnis / Perusahaan / Usaha <span class="text-brand-default">*</span>
              </label>
              <input
                id="onboarding-business-name"
                v-model="businessName"
                type="text"
                placeholder="Contoh: CV Surya Konstruksi, Toko Sumber Rejeki, Kopi Kawan"
                class="w-full px-4 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary text-sm font-medium focus:outline-none focus:ring-2 focus:ring-brand-default/40"
              />
            </div>

            <div>
              <label for="onboarding-city" class="block text-xs font-bold text-content-secondary mb-1.5">
                Kota / Domisili Operasional Usaha
              </label>
              <input
                id="onboarding-city"
                v-model="businessCity"
                type="text"
                placeholder="Contoh: Jakarta Selatan, Surabaya, Bandung"
                class="w-full px-4 py-2.5 border border-border-default rounded-xl bg-surface-sunken text-content-primary text-sm font-medium focus:outline-none focus:ring-2 focus:ring-brand-default/40"
              />
            </div>
          </div>
        </div>

        <!-- ================= STEP 2: PILIH MODEL BISNIS (BUSINESS TYPE) ================= -->
        <div v-else-if="currentStep === 2" class="space-y-4 animate-in fade-in duration-200">
          <div>
            <div class="flex items-center gap-1.5 text-brand-default text-xs font-bold mb-1">
              <Sparkles class="w-3.5 h-3.5" />
              <span>Konfigurasi Kemampuan & Alur Kerja (PRD §2, §8, §69)</span>
            </div>
            <h3 class="text-lg sm:text-xl font-black text-content-primary tracking-tight">
              Pilih Jenis & Model Usaha Anda
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Pilih cara kerja bisnis Anda. Modul operasional (Faktur, Proyek, Kasir POS, Gudang, Akuntansi) akan otomatis diatur mengikuti kebutuhan Anda.
            </p>
          </div>

          <div class="grid grid-cols-1 gap-2.5 pt-1">
            <button
              v-for="b in businessTypeOptions"
              :key="b.id"
              type="button"
              @click="selectedBusinessType = b.id"
              :class="[
                'p-3.5 rounded-xl border text-left transition cursor-pointer flex items-start gap-3',
                selectedBusinessType === b.id
                  ? 'border-brand-default bg-brand-muted/40 text-content-primary ring-2 ring-brand-default/30 shadow-xs'
                  : 'border-border-default bg-surface-sunken text-content-secondary hover:text-content-primary hover:bg-surface-subtle'
              ]"
            >
              <div
                :class="[
                  'w-9 h-9 rounded-xl flex items-center justify-center shrink-0 transition mt-0.5',
                  selectedBusinessType === b.id
                    ? 'bg-brand-default text-white'
                    : 'bg-surface-card text-content-muted border border-border-subtle'
                ]"
              >
                <component :is="b.icon" class="w-5 h-5 stroke-[2]" />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center justify-between">
                  <div class="text-xs sm:text-sm font-bold text-content-primary">{{ b.name }}</div>
                  <span
                    v-if="selectedBusinessType === b.id"
                    class="text-[10px] font-black uppercase tracking-wider px-2 py-0.5 rounded-full bg-brand-default text-white"
                  >
                    Dipilih
                  </span>
                </div>
                <div class="text-xs text-content-secondary mt-0.5 leading-relaxed">{{ b.desc }}</div>
                <div class="text-[11px] font-bold text-brand-default mt-1.5 flex items-center gap-1.5">
                  <Check class="w-3 h-3 stroke-[3]" />
                  <span>{{ b.modules }}</span>
                </div>
              </div>
            </button>
          </div>
        </div>

        <!-- ================= STEP 3: KAS & BANK OPERASIONAL BISNIS ================= -->
        <div v-else-if="currentStep === 3" class="space-y-4 animate-in fade-in duration-200">
          <div>
            <div class="flex items-center gap-1.5 text-brand-default text-xs font-bold mb-1">
              <Wallet class="w-3.5 h-3.5" />
              <span>Sumber Finansial Usaha (PRD §11, §62)</span>
            </div>
            <h3 class="text-lg sm:text-xl font-black text-content-primary tracking-tight">
              Rekening Kas & Bank Operasional
            </h3>
            <p class="text-xs text-content-secondary mt-1 leading-relaxed">
              Tentukan akun penampung dana operasional bisnis Anda. Anda dapat mencatat modal awal usaha untuk rekonsiliasi kas dan neraca.
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
                  Akun Kas/Bank #{{ index + 1 }}
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
                  <label class="block text-[11px] font-medium text-content-secondary mb-1">Nama Rekening / Akun Kas</label>
                  <input
                    v-model="acc.name"
                    type="text"
                    placeholder="Contoh: BCA Operasional Bisnis"
                    class="w-full px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
                  />
                </div>
                <div>
                  <label class="block text-[11px] font-medium text-content-secondary mb-1">Tipe Akun</label>
                  <select
                    v-model="acc.type"
                    class="w-full px-3 py-2 border border-border-default rounded-lg bg-surface-card text-xs text-content-primary focus:outline-none focus:ring-2 focus:ring-brand-default/30"
                  >
                    <option value="checking">Rekening Bank Bisnis (Checking)</option>
                    <option value="savings">Rekening Tabungan Usaha (Savings)</option>
                    <option value="cash">Kas Tunai / Kas Kecil (Cash)</option>
                    <option value="e_wallet">Dompet Digital Bisnis (QRIS / E-Wallet)</option>
                  </select>
                </div>
              </div>

              <div>
                <label class="block text-[11px] font-medium text-content-secondary mb-1">Saldo Awal / Modal Awal (Rupiah)</label>
                <div class="relative">
                  <span class="absolute left-3 top-1/2 -translate-y-1/2 text-xs font-bold text-content-muted">Rp</span>
                  <input
                    :value="formatBalanceDisplay(acc.initial_balance)"
                    @input="handleBalanceInput($event, acc)"
                    type="text"
                    inputmode="numeric"
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
              <span>Tambah Rekening Kas / Bank Lainnya</span>
            </button>
          </div>
        </div>

        <!-- ================= STEP 4: PAKET LISENSI INVINITE BUSINESS OS ================= -->
        <div v-else-if="currentStep === 4" class="space-y-4 animate-in fade-in duration-200">
          <div class="text-center">
            <div class="inline-flex items-center justify-center w-12 h-12 mb-2 bg-gradient-to-br from-brand-default/20 to-brand-default/5 rounded-xl text-brand-default border border-brand-default/25 shadow-xs">
              <Sparkles class="w-6 h-6 stroke-[2]" />
            </div>
            <h3 class="text-xl font-black text-content-primary tracking-tight">
              Pilih Paket Invinite Business OS
            </h3>
            <p class="text-xs text-content-secondary mt-1 max-w-sm mx-auto leading-relaxed">
              Mulai dengan uji coba gratis 3 bulan penuh. Nikmati seluruh kemampuan sistem operasional bisnis tanpa komitmen awal.
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
                  ? 'border-brand-default bg-brand-muted/40 ring-2 ring-brand-default/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-brand-default/40 hover:bg-surface-subtle'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-brand-default/15 text-brand-default border border-brand-default/25">
                  Rekomendasi
                </span>
                <span class="text-xs font-black text-brand-default">Rp 0</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary flex items-center gap-1">
                  <span>3 Bulan Trial Pro</span>
                  <Sparkles class="w-3.5 h-3.5 text-brand-default" />
                </div>
                <p class="text-[11px] text-content-secondary mt-1 leading-relaxed">
                  Akses 100% modul Proyek, Stok Multi-Gudang, Faktur, Pembelian, & Pembukuan GL.
                </p>
              </div>
            </div>

            <!-- 2. Annual Plan -->
            <div
              @click="selectedPlanOption = 'annual'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'annual'
                  ? 'border-brand-default bg-brand-muted/40 ring-2 ring-brand-default/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-brand-default/40 hover:bg-surface-subtle'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-brand-default/15 text-brand-default border border-brand-default/25">
                  Hemat 1 Bulan
                </span>
                <span class="text-xs font-black text-brand-default">Rp 110.000 / thn</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary">Bisnis Pro Tahunan</div>
                <p class="text-[11px] text-content-secondary mt-1 leading-relaxed">
                  Operasional lancar tanpa jeda. Pembayaran terintegrasi via DANA SNAP.
                </p>
              </div>
            </div>

            <!-- 3. Monthly Plan -->
            <div
              @click="selectedPlanOption = 'monthly'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'monthly'
                  ? 'border-brand-default bg-brand-muted/40 ring-2 ring-brand-default/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-brand-default/40 hover:bg-surface-subtle'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-bold text-content-muted">Fleksibel</span>
                <span class="text-xs font-black text-brand-default">Rp 10.000 / bln</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary">Bisnis Pro Bulanan</div>
                <p class="text-[11px] text-content-secondary mt-1 leading-relaxed">
                  Langganan bulanan tanpa komitmen jangka panjang.
                </p>
              </div>
            </div>

            <!-- 4. Free Tier -->
            <div
              @click="selectedPlanOption = 'free'"
              :class="[
                'p-3.5 rounded-xl border text-left cursor-pointer transition relative flex flex-col justify-between',
                selectedPlanOption === 'free'
                  ? 'border-brand-default bg-brand-muted/40 ring-2 ring-brand-default/30 shadow-md'
                  : 'border-border-default bg-surface-sunken hover:border-brand-default/40 hover:bg-surface-subtle'
              ]"
            >
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] font-bold text-content-muted">Dasar</span>
                <span class="text-xs font-bold text-content-secondary">Gratis</span>
              </div>
              <div>
                <div class="font-extrabold text-xs text-content-primary">Starter Free</div>
                <p class="text-[11px] text-content-secondary mt-1 leading-relaxed">
                  Pencatatan dasar arus kas operasional tanpa fitur analitik lanjutan.
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Bottom Navigation Footer -->
      <div class="p-4 sm:p-5 border-t border-border-subtle bg-surface-sunken/60 flex items-center justify-between shrink-0">
        <!-- Back Button -->
        <button
          v-if="currentStep > 1"
          type="button"
          @click="currentStep--"
          class="px-4 py-2 border border-border-default hover:bg-surface-subtle text-content-secondary hover:text-content-primary font-bold text-xs rounded-xl transition cursor-pointer"
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

          <!-- Step 4: Finish CTA -->
          <template v-else>
            <button
              type="button"
              @click="finishOnboarding"
              :disabled="isSubmitting"
              :class="[
                'px-5 py-2.5 font-bold text-xs rounded-xl shadow-lg transition cursor-pointer disabled:opacity-50 flex items-center gap-1.5',
                selectedPlanOption === 'free'
                  ? 'bg-surface-sunken text-content-primary border border-border-default hover:bg-surface-subtle'
                  : 'bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis active:scale-95 text-white shadow-brand-default/25'
              ]"
            >
              <Sparkles v-if="selectedPlanOption === 'trial'" class="w-3.5 h-3.5" />
              <Check v-else-if="selectedPlanOption === 'free'" class="w-3.5 h-3.5" />
              <span v-if="!isSubmitting">
                {{
                  selectedPlanOption === 'trial'
                    ? 'Aktifkan 3 Bulan Gratis & Buka Ruang Kerja'
                    : selectedPlanOption === 'free'
                    ? 'Buka Ruang Kerja Starter'
                    : selectedPlanOption === 'annual'
                    ? 'Lanjutkan via DANA (Rp 110.000 / thn)'
                    : 'Lanjutkan via DANA (Rp 10.000 / bln)'
                }}
              </span>
              <span v-else>Menyiapkan Ruang Kerja Bisnis...</span>
            </button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, watch, markRaw } from 'vue'
import { api } from '@/services/api'
import { useAuthStore } from '@/stores/auth'
import { useWalletStore } from '@/stores/wallets'
import { useCategoryStore } from '@/stores/categories'
import { useAnalyticsStore } from '@/stores/analytics'
import { useSubscriptionStore } from '@/stores/subscription'
import { useWorkspaceStore } from '@/stores/workspace'
import { formatIntegerRupiah, parseIDR } from '@/utils/currency'
import {
  Check,
  Plus,
  Sparkles,
  ChevronRight,
  Building2,
  FolderKanban,
  Store,
  Utensils,
  KeyRound,
  Wallet
} from 'lucide-vue-next'

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
const workspaceStore = useWorkspaceStore()

const currentStep = ref(1)
const isSubmitting = ref(false)

const stepTitles = [
  'Profil & Ruang Kerja Bisnis',
  'Pilih Model & Jenis Usaha',
  'Kas & Bank Operasional',
  'Aktivasi Lisensi Bisnis'
]

// ================= STEP 1 STATE =================
const userName = ref(props.initialName || authStore.user?.display_name || authStore.user?.name || '')
const businessName = ref('')
const businessCity = ref('')

watch(
  () => props.isOpen,
  (open) => {
    if (open) {
      currentStep.value = 1
      if (props.initialName || authStore.displayName) {
        userName.value = props.initialName || authStore.displayName || ''
      }
      if (!businessName.value && authStore.displayName) {
        businessName.value = `${authStore.displayName}'s Business`
      }
    }
  }
)

watch(
  () => props.initialName,
  (name) => {
    if (name) {
      userName.value = name
      if (!businessName.value) {
        businessName.value = `${name}'s Business`
      }
    }
  }
)

// ================= STEP 2 STATE: BUSINESS TYPES =================
const businessTypeOptions = [
  {
    id: 'contractor',
    name: 'Kontraktor & Jasa Proyek',
    desc: 'Manajemen proyek, tahapan milestone progress billing, BoQ pekerjaan, dan job costing (material & upah).',
    modules: 'Proyek • Milestone • Faktur Progres • Biaya Pokok',
    icon: markRaw(FolderKanban)
  },
  {
    id: 'retail',
    name: 'Toko Retail & Grosir',
    desc: 'Kasir POS cepat, inventaris stok barang multi-gudang, pesanan pembelian supplier, dan faktur penjualan.',
    modules: 'Kasir POS • Stok Gudang • Pembelian • Faktur',
    icon: markRaw(Store)
  },
  {
    id: 'fnb',
    name: 'F&B, Kafe & Restoran',
    desc: 'Kasir POS pesanan, manajemen meja, dapur/kitchen, dan pemakaian bahan baku/resep otomatis.',
    modules: 'Kasir POS • Meja & Dapur • Bahan Baku • Akuntansi',
    icon: markRaw(Utensils)
  },
  {
    id: 'rental',
    name: 'Rental & Penyewaan',
    desc: 'Katalog unit rental, jadwal reservasi/booking, serah terima pengembalian, dan penagihan sewa.',
    modules: 'Aset Rental • Booking • Tagihan Sewa • Akuntansi',
    icon: markRaw(KeyRound)
  },
  {
    id: 'general',
    name: 'Dagang & Jasa Umum',
    desc: 'Faktur komersial terpadu, piutang usaha, pembelian & stok, kas/bank, dan laporan keuangan laba rugi.',
    modules: 'Faktur • Piutang • Stok • Buku Kas & Jurnal',
    icon: markRaw(Building2)
  }
]

const selectedBusinessType = ref('contractor')

// ================= STEP 3 STATE: CASH & BANK =================
const walletsConfig = ref([
  { name: 'Rekening Operasional Bisnis (BCA)', type: 'checking', initial_balance: 5000000 },
  { name: 'Kas Kasir / Kas Tunai Operasional', type: 'cash', initial_balance: 500000 }
])

function formatBalanceDisplay(val) {
  if (val === undefined || val === null || val === '') return '0'
  const num = parseIDR(val)
  return formatIntegerRupiah(num)
}

function handleBalanceInput(e, acc) {
  const input = e.target
  const cursorStart = input.selectionStart || 0
  const prevLen = input.value.length

  const raw = input.value
  const num = parseIDR(raw)
  acc.initial_balance = num

  const formatted = num === 0 && (raw === '' || raw === '0') ? raw : formatIntegerRupiah(num)
  input.value = formatted

  const newLen = formatted.length
  const newPos = Math.max(0, cursorStart + (newLen - prevLen))
  input.setSelectionRange(newPos, newPos)
}

function addWalletSlot() {
  walletsConfig.value.push({
    name: 'Rekening Kas / Bank Baru',
    type: 'checking',
    initial_balance: 0
  })
}

function removeWallet(index) {
  if (walletsConfig.value.length > 1) {
    walletsConfig.value.splice(index, 1)
  }
}

// ================= STEP 4 STATE: PLANS =================
const selectedPlanOption = ref('trial') // 'trial' | 'free' | 'monthly' | 'annual'

// ================= NAVIGATION =================
function handleNextStep() {
  if (currentStep.value === 1) {
    if (!userName.value.trim()) {
      userName.value = authStore.displayName || 'Pemilik Usaha'
    }
    if (!businessName.value.trim()) {
      businessName.value = `${userName.value}'s Business`
    }
  }
  if (currentStep.value < 4) {
    currentStep.value++
  }
}

/**
 * Commits Pure Business OS setup:
 * 1. Creates Business Workspace (Tenant) with selected Business Type
 * 2. Switches to Business Workspace
 * 3. Seeds initial business operational accounts
 * 4. Activates Trial if selected
 */
async function finishOnboarding() {
  isSubmitting.value = true
  try {
    const finalBusinessName = businessName.value.trim() || `${userName.value.trim() || 'Usaha'}'s Business`
    const isTrial = selectedPlanOption.value === 'trial'

    // 1. Create Business Workspace (PRD §5, §8, §69)
    let createdWs = null
    try {
      createdWs = await api.createWorkspace({
        name: finalBusinessName,
        business_type: selectedBusinessType.value,
        address: businessCity.value.trim() || undefined
      })
    } catch (wsErr) {
      console.warn('Workspace creation notice during onboarding:', wsErr)
    }

    // 2. Fetch all workspaces and switch to the business workspace
    try {
      await workspaceStore.fetchWorkspaces()
      if (createdWs?.id) {
        await workspaceStore.switchWorkspace(createdWs.id)
      } else if (workspaceStore.businessWorkspaces.length > 0) {
        await workspaceStore.switchWorkspace(workspaceStore.businessWorkspaces[0].id)
      }
    } catch (switchErr) {
      console.warn('Switch workspace warning:', switchErr)
    }

    // 3. Submit personalization payload for backward compatibility
    try {
      await api.submitOnboarding({
        display_name: userName.value || finalBusinessName,
        income_title: 'Penjualan Usaha',
        expense_title: 'Beban Operasional',
        financial_goals: ['business_growth', 'profitability'],
        wallets: walletsConfig.value.map((wallet) => ({
          name: wallet.name?.trim() || 'Kas Operasional',
          account_type: wallet.type || 'checking',
          initial_balance: parseIDR(wallet.initial_balance)
        })),
        categories: [
          { name: 'Penjualan Produk', display_name: 'Penjualan Produk', category_type: 'income' },
          { name: 'Pendapatan Jasa / Proyek', display_name: 'Pendapatan Jasa / Proyek', category_type: 'income' },
          { name: 'Beban Pokok Penjualan', display_name: 'Beban Pokok Penjualan', category_type: 'expense' },
          { name: 'Beban Operasional & Gaji', display_name: 'Beban Operasional & Gaji', category_type: 'expense' },
          { name: 'Beban Perlengkapan & Utilitas', display_name: 'Beban Perlengkapan & Utilitas', category_type: 'expense' }
        ],
        activate_trial: isTrial
      })
    } catch (onbErr) {
      console.warn('Submit onboarding API notice:', onbErr)
    }

    // 4. Activate Trial if requested
    if (isTrial) {
      try {
        await subscriptionStore.activateTrial()
      } catch (trialErr) {
        console.warn('Trial activation notice:', trialErr)
      }
    }

    // 5. If user chose Monthly or Annual, initiate checkout in background
    if (selectedPlanOption.value === 'monthly' || selectedPlanOption.value === 'annual') {
      try {
        const planId = selectedPlanOption.value === 'annual' ? 'premium_annual' : 'premium_monthly'
        await subscriptionStore.initiateCheckout('dana', planId)
      } catch (danaErr) {
        console.warn('Checkout notice during onboarding:', danaErr)
      }
    }

    // 6. Persist local completion flag
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('invinite_onboarding_completed', 'true')
      localStorage.setItem(
        'invinite_user_personalization',
        JSON.stringify({
          display_name: userName.value || finalBusinessName,
          business_name: finalBusinessName,
          business_type: selectedBusinessType.value,
          income_title: 'Penjualan Usaha',
          expense_title: 'Beban Operasional',
          plan: selectedPlanOption.value,
          onboarding_completed: true,
          onboarded_at: new Date().toISOString()
        })
      )
    }

    authStore.setPersonalization({
      display_name: userName.value || finalBusinessName,
      income_title: 'Penjualan Usaha',
      expense_title: 'Beban Operasional',
      onboarding_completed: true
    })

    // 7. Refresh all dependent stores
    await Promise.allSettled([
      workspaceStore.fetchWorkspaces(),
      walletStore.fetchWallets(),
      categoryStore.fetchCategories(),
      analyticsStore.fetchDashboard(),
      subscriptionStore.fetchSubscriptionStatus(),
      authStore.fetchPersonalization(),
      authStore.checkAuth()
    ])

    emit('completed')
  } catch (err) {
    console.error('Failed to finalize business onboarding', err)
    emit('completed')
  } finally {
    isSubmitting.value = false
  }
}
</script>
