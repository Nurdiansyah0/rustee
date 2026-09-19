<template>
  <div class="space-y-5 lg:space-y-6">
    <!-- User Profile Card -->
    <div class="p-5 sm:p-6 bg-surface-card rounded-xl border border-border-subtle shadow-card flex items-center gap-4">
      <div class="w-12 h-12 sm:w-14 sm:h-14 rounded-xl bg-brand-muted text-brand-default flex items-center justify-center font-bold text-lg sm:text-xl shrink-0 border border-brand-border">
        {{ computedUserInitials }}
      </div>
      <div class="min-w-0">
        <h2 class="text-base sm:text-lg font-bold text-content-primary truncate">{{ computedDisplayName }}</h2>
        <div class="text-xs text-content-secondary mt-0.5 truncate">{{ computedEmail }}</div>
        <div class="inline-flex items-center gap-1.5 mt-2 px-2 py-0.5 bg-surface-subtle border border-border-subtle rounded-md text-[10px] font-bold text-content-secondary">
          <Banknote class="w-3 h-3 text-brand-default" />
          <span>Mata Uang Utama: IDR (Rupiah)</span>
        </div>
      </div>
    </div>

    <!-- Subscription Status Card -->
    <div class="p-5 sm:p-6 bg-surface-card rounded-xl border border-border-subtle shadow-card space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-bold text-content-muted uppercase tracking-wider">Status Langganan</h3>
        <!-- Badge based on trialing vs active vs free -->
        <span v-if="subscriptionStore.isTrialing" class="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border bg-amber-500/15 text-amber-500 border-amber-500/30">
          <Clock class="w-3 h-3 text-amber-500 animate-pulse" />
          <span>Trial ({{ subscriptionStore.daysRemaining ?? 7 }} Hari)</span>
        </span>
        <span v-else-if="isPremiumActive" class="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border bg-gradient-to-r from-amber-500/10 to-emerald-500/10 text-brand-default border-brand-border">
          <Sparkles class="w-3 h-3 text-amber-500" />
          <span>FinRep Pro Aktif</span>
        </span>
        <span v-else-if="subscriptionStore.hasUsedTrial" class="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border bg-surface-subtle text-content-muted border-border-subtle">
          <span>Uji Coba Selesai</span>
        </span>
        <span v-else class="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider flex items-center gap-1 border bg-surface-subtle text-content-muted border-border-subtle">
          <span>Free Tier</span>
        </span>
      </div>

      <!-- State 1: Active Trial -->
      <div v-if="subscriptionStore.isTrialing" class="p-4 bg-amber-500/10 rounded-xl border border-amber-500/30 text-xs space-y-2.5">
        <div class="flex items-center justify-between">
          <div class="font-bold text-sm text-amber-500 flex items-center gap-1.5">
            <Clock class="w-4 h-4 animate-pulse" />
            <span>Masa Uji Coba Pro Aktif</span>
          </div>
          <span class="px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-400 font-extrabold text-[10px]">
            {{ subscriptionStore.daysRemaining ?? 90 }} Hari Tersisa
          </span>
        </div>
        <p class="text-content-secondary leading-relaxed">
          Anda sedang menikmati seluruh fitur FinRep Pro secara gratis. Jika masa uji coba berakhir, akun Anda otomatis kembali ke Free Tier <strong class="text-content-primary">tanpa kehilangan data apapun</strong>.
        </p>
        <button
          type="button"
          @click="$emit('open-upgrade')"
          class="w-full py-2.5 px-4 bg-gradient-to-r from-amber-500 to-emerald-600 hover:from-amber-600 hover:to-emerald-700 text-white font-bold rounded-xl shadow-md transition cursor-pointer text-center text-xs flex items-center justify-center gap-1.5 mt-1"
        >
          <Sparkles class="w-3.5 h-3.5" />
          <span>Kunci Akses Pro Tetap (Rp 10.000 / bln via DANA)</span>
        </button>
      </div>

      <!-- State 2: Paid Active Premium -->
      <div v-else-if="isPremiumActive" class="p-4 bg-income-muted/40 rounded-xl border border-income-border/60 text-xs">
        <div class="font-bold text-sm text-income-default flex items-center gap-1.5">
          <ShieldCheck class="w-4 h-4" />
          <span>Langganan Pro Aktif</span>
        </div>
        <div class="mt-1 text-content-secondary leading-relaxed">
          Semua fitur analitik lanjutan, proyeksi runway multi-bulan, dan pelaporan keuangan terbuka penuh.
        </div>
      </div>

      <!-- State 3: Trial Expired with Zero-Data-Loss Reassurance -->
      <div v-else-if="subscriptionStore.hasUsedTrial" class="p-5 bg-surface-subtle rounded-xl border border-border-subtle text-xs space-y-3">
        <div>
          <div class="font-bold text-sm text-content-primary flex items-center gap-1.5">
            <ShieldCheck class="w-4 h-4 text-brand-default" />
            <span>Masa Uji Coba Selesai — Data 100% Aman</span>
          </div>
          <div class="mt-1 text-content-secondary leading-relaxed">
            Seluruh data dompet, akun, dan catatan transaksi Anda tetap aman tersimpan tanpa risiko hilang. Lanjutkan akses analitik pro dengan berlangganan mulai Rp 10.000 / bulan.
          </div>
        </div>
        <button
          type="button"
          @click="$emit('open-upgrade')"
          class="w-full py-2.5 px-4 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis text-white font-bold rounded-xl shadow-md shadow-brand-default/20 transition cursor-pointer text-center"
        >
          Lanjutkan ke FinRep Pro (Mulai Rp 10.000 via DANA)
        </button>
      </div>

      <!-- State 4: Default Free Tier -->
      <div v-else class="p-5 bg-surface-subtle rounded-xl border border-border-subtle text-xs space-y-3">
        <div>
          <div class="font-bold text-sm text-content-primary">Buka Potensi Penuh dengan FinRep Pro</div>
          <div class="mt-1 text-content-secondary leading-relaxed">
            Mulai uji coba 3 bulan gratis atau berlangganan mulai Rp 10.000 / bulan untuk analitik presisi perbankan dan proyeksi masa depan.
          </div>
        </div>
        <button
          type="button"
          @click="$emit('open-upgrade')"
          class="w-full py-2.5 px-4 bg-brand-default hover:bg-brand-emphasis active:bg-brand-emphasis text-white font-bold rounded-xl shadow-md shadow-brand-default/20 transition cursor-pointer text-center"
        >
          Mulai Uji Coba 3 Bulan Gratis / Upgrade (Rp 10.000)
        </button>
      </div>
    </div>

    <!-- User-Owned Financial Vocabulary (§4, §6) -->
    <div class="p-5 sm:p-6 bg-surface-card rounded-xl border border-border-subtle shadow-card space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-bold text-content-muted uppercase tracking-wider">Kosakata Finansial Saya</h3>
        <button
          type="button"
          @click="$emit('open-onboarding')"
          class="text-[11px] font-bold px-2.5 py-1 rounded-lg bg-brand-default/10 text-brand-default hover:bg-brand-default hover:text-white border border-brand-default/20 transition cursor-pointer"
        >
          Ubah / Sesuaikan
        </button>
      </div>

      <div class="grid grid-cols-2 gap-3 pt-1">
        <div class="p-3 bg-surface-subtle rounded-xl border border-border-subtle">
          <div class="text-[10px] font-semibold text-income-default uppercase tracking-wider">Pemasukan Utama</div>
          <div class="text-sm font-bold text-content-primary mt-1">{{ authStore.incomeTitle }}</div>
          <div class="text-[10px] text-content-muted mt-0.5">Label arus kas masuk</div>
        </div>
        <div class="p-3 bg-surface-subtle rounded-xl border border-border-subtle">
          <div class="text-[10px] font-semibold text-expense-default uppercase tracking-wider">Pengeluaran Utama</div>
          <div class="text-sm font-bold text-content-primary mt-1">{{ authStore.expenseTitle }}</div>
          <div class="text-[10px] text-content-muted mt-0.5">Label arus kas keluar</div>
        </div>
      </div>
    </div>

    <!-- Security & Device Info -->
    <div class="p-5 sm:p-6 bg-surface-card rounded-xl border border-border-subtle shadow-card space-y-3">
      <h3 class="text-xs font-bold text-content-muted uppercase tracking-wider">Keamanan & Layanan</h3>

      <div class="flex items-center justify-between py-2 border-b border-border-subtle text-xs">
        <span class="text-content-secondary flex items-center gap-1.5">
          <Wifi class="w-3.5 h-3.5 text-content-muted" />
          <span>Status Jaringan</span>
        </span>
        <span class="font-bold flex items-center gap-1.5">
          <span :class="['w-2 h-2 rounded-full', isOnline ? 'bg-income-default' : 'bg-warning-default']" />
          <span :class="isOnline ? 'text-income-default' : 'text-warning-default'">
            {{ isOnline ? 'Terhubung (Online)' : 'Mode Offline' }}
          </span>
        </span>
      </div>

      <div class="flex items-center justify-between py-2 border-b border-border-subtle text-xs">
        <span class="text-content-secondary flex items-center gap-1.5">
          <ShieldCheck class="w-3.5 h-3.5 text-content-muted" />
          <span>Keamanan Sesi</span>
        </span>
        <span class="font-bold text-income-default">Terkontrol & Terenkripsi</span>
      </div>

      <div class="flex items-center justify-between py-2 text-xs">
        <span class="text-content-secondary flex items-center gap-1.5">
          <Smartphone class="w-3.5 h-3.5 text-content-muted" />
          <span>Aplikasi</span>
        </span>
        <span class="font-medium text-content-muted">FinRep PWA v0.1.0</span>
      </div>
    </div>

    <!-- Logout CTA -->
    <button
      type="button"
      @click="handleLogout"
      class="w-full py-3 px-4 bg-expense-muted/50 hover:bg-expense-muted text-expense-default border border-expense-border font-bold text-xs rounded-xl transition cursor-pointer flex items-center justify-center gap-2"
    >
      <LogOut class="w-4 h-4 stroke-[2]" />
      <span>Keluar dari Akun</span>
    </button>

    <!-- Brand Footnote & Platform Attribution -->
    <div class="pt-2 pb-1 text-center text-xs text-content-muted flex items-center justify-center gap-2 select-none">
      <span class="text-[11px] text-content-muted">FinRep by Invinite.id - Digital Creative Solutions</span>
      <img
        src="/icons/Invinite_Logo.png"
        alt="Invinite.id"
        class="h-3.5 w-auto max-h-3.5 object-contain opacity-70"
        loading="lazy"
      />
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '@/services/api'
import { useAuthStore } from '@/stores/auth'
import { useSubscriptionStore } from '@/stores/subscription'
import {
  Sparkles,
  ShieldCheck,
  Banknote,
  Wifi,
  Server,
  Smartphone,
  LogOut,
  Clock
} from 'lucide-vue-next'

const props = defineProps({
  user: Object,
  userTier: String
})

const emit = defineEmits(['open-upgrade', 'open-onboarding', 'logout'])

const authStore = useAuthStore()
const subscriptionStore = useSubscriptionStore()
const isOnline = ref(typeof navigator !== 'undefined' ? navigator.onLine : true)

const computedDisplayName = computed(() => {
  return authStore.user?.display_name || props.user?.display_name || 'Pengguna Invinite'
})

const computedEmail = computed(() => {
  return authStore.user?.email || props.user?.email || 'user@example.com'
})

const computedUserInitials = computed(() => {
  return (computedDisplayName.value || 'U')[0].toUpperCase()
})

const isPremiumActive = computed(() => {
  return authStore.isPremium || props.userTier === 'premium'
})

function updateOnlineStatus() {
  isOnline.value = navigator.onLine
}

onMounted(() => {
  window.addEventListener('online', updateOnlineStatus)
  window.addEventListener('offline', updateOnlineStatus)
})

onUnmounted(() => {
  window.removeEventListener('online', updateOnlineStatus)
  window.removeEventListener('offline', updateOnlineStatus)
})

async function handleLogout() {
  try {
    await api.logout()
  } catch (err) {
    console.warn('Logout error', err)
  }
  emit('logout')
}
</script>
