<template>
  <div
    v-if="isOpen"
    @click.self="$emit('close')"
    class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    aria-labelledby="upgrade-modal-title"
    aria-describedby="upgrade-modal-desc"
  >
    <div
      ref="modalRef"
      class="relative w-full max-w-md p-4 sm:p-6 max-h-[90dvh] overflow-y-auto bg-surface-card rounded-2xl shadow-2xl border border-border-subtle animate-in fade-in zoom-in-95 duration-200 scroll-native"
    >
      <!-- Close button with focus ref -->
      <button
        ref="closeButtonRef"
        type="button"
        @click="$emit('close')"
        class="absolute top-3 sm:top-4 right-3 sm:right-4 p-2 text-content-muted hover:text-content-primary rounded-full hover:bg-surface-subtle transition cursor-pointer z-10"
        aria-label="Tutup modal"
      >
        <X class="w-5 h-5 stroke-[2]" />
      </button>

      <!-- Header with compact vertical spacing -->
      <div class="text-center mb-4">
        <div class="inline-flex items-center justify-center w-12 h-12 mb-2 bg-gradient-to-br from-amber-500/20 to-emerald-500/20 rounded-xl text-amber-500 border border-amber-500/30 shadow-xs">
          <Sparkles class="w-6 h-6 stroke-[2]" />
        </div>
        <h2 id="upgrade-modal-title" class="text-2xl font-black text-content-primary tracking-tight">
          Upgrade ke FinRep Pro
        </h2>
        <p id="upgrade-modal-desc" class="text-xs text-content-secondary mt-1 max-w-sm mx-auto leading-relaxed">
          Buka analitik presisi perbankan, proyeksi runway, dan automasi finansial cerdas
        </p>
      </div>

      <!-- Active Trial Alert Banner -->
      <div v-if="isTrialing" class="mb-4 p-3.5 rounded-xl bg-amber-500/10 border border-amber-500/30 text-xs">
        <div class="flex items-center gap-2 text-amber-500 font-bold mb-1">
          <Clock class="w-4 h-4 shrink-0 animate-pulse" />
          <span>Masa Uji Coba Pro Aktif</span>
        </div>
        <p class="text-content-secondary text-[11px] leading-relaxed">
          Tersisa <strong class="text-amber-400 font-bold">{{ daysRemaining ?? 90 }} hari</strong>. Kunci akses tanpa jeda dengan berlangganan mulai Rp 10.000 / bulan. Data keuangan Anda tetap tersimpan di akun Anda dan tidak terhapus saat masa trial berakhir.
        </p>
      </div>

      <!-- Trial Expired Alert Banner (Verifiable data retention reassurance) -->
      <div v-else-if="hasUsedTrial" class="mb-4 p-3.5 rounded-xl bg-surface-subtle border border-border-default text-xs">
        <div class="flex items-center justify-between mb-1.5">
          <div class="flex items-center gap-2 text-content-primary font-bold">
            <ShieldCheck class="w-4 h-4 text-brand-default shrink-0" />
            <span>Masa Uji Coba Selesai</span>
          </div>
          <span class="text-[10px] font-bold px-2 py-0.5 rounded-full bg-surface-card border border-border-subtle text-content-muted">
            Data Tersimpan di Akun
          </span>
        </div>
        <p class="text-content-secondary text-[11px] leading-relaxed">
          Masa uji coba 3 bulan telah berakhir. Seluruh data transaksi Anda tetap tersimpan di akun Free Tier tanpa terhapus. Aktifkan FinRep Pro mulai Rp 10.000 / bulan untuk membuka kembali analitik runway & laporan penuh.
        </p>
      </div>

      <!-- 3-Month Free Trial Promotion Card (Only shown if NOT trialing and NOT already used trial) -->
      <div
        v-else-if="!isPremium"
        class="mb-4 p-4 rounded-xl bg-gradient-to-br from-amber-500/15 via-emerald-500/15 to-emerald-500/10 border border-amber-500/30 relative overflow-hidden"
      >
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs font-black text-amber-500 uppercase tracking-wider flex items-center gap-1.5">
            <Sparkles class="w-3.5 h-3.5" />
            Coba Dulu, Bayar Nanti
          </span>
          <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full bg-emerald-500/25 text-emerald-400 border border-emerald-500/30">
            3 Bulan Gratis (90 Hari)
          </span>
        </div>
        <p class="text-xs text-content-secondary leading-relaxed mb-3.5">
          Nikmati seluruh analitik lanjutan & proyeksi runway selama 3 bulan penuh tanpa biaya di depan. Tanpa kartu kredit, aktif instan.
        </p>
        <button
          type="button"
          @click="handleActivateTrial"
          :disabled="trialLoading || loading"
          class="w-full py-3 px-4 bg-gradient-to-r from-amber-500 to-emerald-600 hover:from-amber-600 hover:to-emerald-700 active:scale-[0.98] text-white font-bold rounded-xl shadow-md shadow-amber-500/20 transition disabled:opacity-50 cursor-pointer text-xs flex items-center justify-center gap-2"
        >
          <Sparkles class="w-4 h-4" />
          <span v-if="!trialLoading">Mulai Uji Coba 3 Bulan Gratis (Rp 0)</span>
          <span v-else>Mengaktifkan Akses Pro...</span>
        </button>
      </div>

      <!-- Plan Duration Selector with Semantic ARIA Radiogroup -->
      <div class="mb-4">
        <label class="block text-[11px] font-bold text-content-secondary uppercase tracking-wide mb-2">
          Pilihan Paket Berlangganan
        </label>
        <div
          role="radiogroup"
          aria-label="Pilihan Paket Berlangganan"
          class="grid grid-cols-2 gap-2"
        >
          <!-- Monthly Plan -->
          <button
            type="button"
            role="radio"
            :aria-checked="selectedPlan === 'premium_monthly'"
            :aria-pressed="selectedPlan === 'premium_monthly'"
            @click="selectedPlan = 'premium_monthly'"
            :class="[
              'p-3 rounded-xl border text-left transition cursor-pointer relative',
              selectedPlan === 'premium_monthly'
                ? 'border-brand-default bg-brand-muted/40 ring-2 ring-brand-default/30'
                : 'border-border-default hover:bg-surface-subtle bg-surface-sunken'
            ]"
          >
            <div class="text-[10px] font-bold text-content-muted uppercase">Paket Bulanan</div>
            <div class="text-sm font-black text-content-primary mt-0.5 tabular-nums">
              Rp 10.000 <span class="text-[10px] font-normal text-content-secondary">/ bln</span>
            </div>
            <div class="text-[10px] text-content-muted mt-0.5">Fleksibel setiap bulan</div>
          </button>

          <!-- Annual Plan -->
          <button
            type="button"
            role="radio"
            :aria-checked="selectedPlan === 'premium_annual'"
            :aria-pressed="selectedPlan === 'premium_annual'"
            @click="selectedPlan = 'premium_annual'"
            :class="[
              'p-3 rounded-xl border text-left transition cursor-pointer relative overflow-hidden',
              selectedPlan === 'premium_annual'
                ? 'border-brand-default bg-brand-muted/40 ring-2 ring-brand-default/30'
                : 'border-border-default hover:bg-surface-subtle bg-surface-sunken'
            ]"
          >
            <span class="absolute top-1.5 right-1.5 text-[8px] font-black px-1.5 py-0.5 rounded bg-amber-500 text-black uppercase">
              Hemat 8%
            </span>
            <div class="text-[10px] font-bold text-content-muted uppercase">Paket Tahunan</div>
            <div class="text-sm font-black text-content-primary mt-0.5 tabular-nums">
              Rp 110.000 <span class="text-[10px] font-normal text-content-secondary">/ thn</span>
            </div>
            <div class="text-[10px] text-emerald-500 font-semibold mt-0.5">Setara ~Rp 9.166/bln</div>
          </button>
        </div>
      </div>

      <!-- Feature list -->
      <div class="space-y-2 mb-4 text-xs text-content-secondary">
        <div class="flex items-center gap-2.5">
          <div class="w-5 h-5 rounded-full bg-income-muted text-income-default flex items-center justify-center shrink-0">
            <Check class="w-3.5 h-3.5 stroke-[2.5]" />
          </div>
          <span class="text-content-primary font-medium">Analitik pengeluaran mendalam & proyeksi runway multi-bulan</span>
        </div>
        <div class="flex items-center gap-2.5">
          <div class="w-5 h-5 rounded-full bg-income-muted text-income-default flex items-center justify-center shrink-0">
            <Check class="w-3.5 h-3.5 stroke-[2.5]" />
          </div>
          <span class="text-content-primary font-medium">Skor kesehatan finansial & pemantauan rasio tabungan</span>
        </div>
        <div class="flex items-center gap-2.5">
          <div class="w-5 h-5 rounded-full bg-income-muted text-income-default flex items-center justify-center shrink-0">
            <Check class="w-3.5 h-3.5 stroke-[2.5]" />
          </div>
          <span class="text-content-primary font-medium">Budgeting per kategori fleksibel & peringatan overbudget</span>
        </div>
        <div class="flex items-center gap-2.5">
          <div class="w-5 h-5 rounded-full bg-income-muted text-income-default flex items-center justify-center shrink-0">
            <Check class="w-3.5 h-3.5 stroke-[2.5]" />
          </div>
          <span class="text-content-primary font-medium">Ekspor laporan buku kas terenkripsi (CSV & PDF)</span>
        </div>
      </div>

      <!-- Divider -->
      <div class="relative flex py-2 items-center mb-3">
        <div class="flex-grow border-t border-border-subtle"></div>
        <span class="flex-shrink mx-3 text-[10px] font-bold text-content-muted uppercase tracking-wider">
          Metode Pembayaran Resmi
        </span>
        <div class="flex-grow border-t border-border-subtle"></div>
      </div>

      <!-- Payment Provider Information (Informative, non-pseudo-radio card) -->
      <div class="mb-4">
        <div class="flex items-center justify-between mb-1.5">
          <span class="text-[11px] font-bold text-content-secondary uppercase tracking-wide">
            Metode Pembayaran Tersedia
          </span>
          <span class="text-[10px] font-bold text-brand-default tabular-nums">
            {{ selectedPlan === 'premium_annual' ? 'Rp 110.000 / thn' : 'Rp 10.000 / bln' }}
          </span>
        </div>
        <div class="p-3 rounded-xl border border-[#118EEA]/30 bg-[#118EEA]/5 flex items-center justify-between">
          <div class="flex items-center gap-2.5">
            <div class="w-8 h-8 rounded-lg bg-[#118EEA] text-white font-extrabold text-xs flex items-center justify-center shrink-0">
              DANA
            </div>
            <div>
              <div class="font-extrabold text-xs text-content-primary flex items-center gap-1.5">
                <span>DANA Direct & SNAP</span>
                <span class="text-[9px] font-bold px-1.5 py-0.2 rounded bg-[#118EEA]/20 text-[#118EEA] border border-[#118EEA]/30">Terverifikasi</span>
              </div>
              <div class="text-[10px] text-content-muted mt-0.5">Saldo DANA & QRIS • Verifikasi Server Asimetris</div>
            </div>
          </div>
          <span class="text-[10px] font-semibold text-content-muted bg-surface-subtle px-2 py-0.5 rounded border border-border-subtle">
            Otomatis
          </span>
        </div>
        <div class="mt-2 text-[10px] text-content-muted flex items-center gap-1.5 px-1">
          <ShieldCheck class="w-3.5 h-3.5 text-brand-default shrink-0" />
          <span>Transaksi diverifikasi server secara otomatis melalui webhook resmi DANA.</span>
        </div>
      </div>

      <!-- Submit CTA -->
      <button
        type="button"
        @click="handleCheckout"
        :disabled="loading || trialLoading"
        class="w-full py-3.5 px-4 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis text-white font-bold rounded-xl shadow-lg shadow-brand-default/30 transition disabled:opacity-50 cursor-pointer text-sm flex items-center justify-center gap-2"
      >
        <span v-if="!loading">
          Lanjutkan via DANA ({{ selectedPlan === 'premium_annual' ? 'Rp 110.000' : 'Rp 10.000' }})
        </span>
        <span v-else class="flex items-center gap-2">
          <span class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
          <span>Menghubungkan ke Payment Gateway...</span>
        </span>
      </button>

      <div v-if="error" class="mt-3 text-xs text-center text-expense-default font-medium">
        {{ error }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch, onUnmounted, nextTick } from 'vue'
import { useSubscriptionStore } from '@/stores/subscription'
import { X, Sparkles, Check, Clock, ShieldCheck } from 'lucide-vue-next'

const props = defineProps({
  isOpen: Boolean
})

const emit = defineEmits(['close', 'success'])

const modalRef = ref(null)
const closeButtonRef = ref(null)
let previousActiveElement = null

function getFocusableElements() {
  if (!modalRef.value) return []
  return Array.from(
    modalRef.value.querySelectorAll(
      'button:not([disabled]), [href]:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )
  )
}

function handleKeyDown(e) {
  if (!props.isOpen) return

  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
    return
  }

  if (e.key === 'Tab') {
    const focusable = getFocusableElements()
    if (focusable.length === 0) return

    const first = focusable[0]
    const last = focusable[focusable.length - 1]

    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault()
        last.focus()
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }
}

watch(
  () => props.isOpen,
  async (open) => {
    if (open) {
      previousActiveElement = document.activeElement
      window.addEventListener('keydown', handleKeyDown)
      await nextTick()
      if (closeButtonRef.value?.focus) {
        closeButtonRef.value.focus()
      }
    } else {
      window.removeEventListener('keydown', handleKeyDown)
      if (previousActiveElement && typeof previousActiveElement.focus === 'function') {
        previousActiveElement.focus()
        previousActiveElement = null
      }
    }
  },
  { immediate: true }
)

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeyDown)
  if (previousActiveElement && typeof previousActiveElement.focus === 'function') {
    previousActiveElement.focus()
  }
})

const subscriptionStore = useSubscriptionStore()

const selectedPlan = ref('premium_monthly')
const selectedProvider = ref('dana')
const loading = ref(false)
const trialLoading = ref(false)
const error = ref('')

const isPremium = computed(() => subscriptionStore.isPremium)
const isTrialing = computed(() => subscriptionStore.isTrialing)
const daysRemaining = computed(() => subscriptionStore.daysRemaining)
const hasUsedTrial = computed(() => subscriptionStore.hasUsedTrial)

async function handleActivateTrial() {
  if (trialLoading.value || loading.value) return

  trialLoading.value = true
  error.value = ''
  try {
    const res = await subscriptionStore.activateTrial()
    emit('success', res)
    emit('close')
  } catch (err) {
    error.value = err.detail || err.message || 'Gagal mengaktifkan masa uji coba.'
  } finally {
    trialLoading.value = false
  }
}

async function handleCheckout() {
  if (loading.value || trialLoading.value) return

  loading.value = true
  error.value = ''
  try {
    const res = await subscriptionStore.initiateCheckout(selectedProvider.value, selectedPlan.value)
    if (!res?.checkout_url) {
      throw new Error('Payment gateway tidak mengembalikan URL checkout.')
    }
    emit('success', res)
    emit('close')
  } catch (err) {
    error.value = err.detail || err.message || 'Gagal memulai checkout. Silakan coba lagi.'
  } finally {
    loading.value = false
  }
}
</script>
