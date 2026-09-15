<template>
  <div class="min-h-screen grid grid-cols-1 lg:grid-cols-12 bg-surface-canvas font-sans selection:bg-brand-default selection:text-white">
    <!-- LEFT SIDE: Brand + Product Financial Visualization (Desktop only) -->
    <div class="hidden lg:flex lg:col-span-6 xl:col-span-7 bg-zinc-950 border-r border-zinc-800/80 p-10 xl:p-14 flex-col justify-between relative overflow-hidden text-white">
      <!-- Ambient Background Radial Glows -->
      <div class="absolute -top-32 -left-32 w-96 h-96 bg-emerald-500/15 rounded-full blur-3xl pointer-events-none"></div>
      <div class="absolute -bottom-32 -right-32 w-96 h-96 bg-teal-500/10 rounded-full blur-3xl pointer-events-none"></div>

      <!-- Top Brand Header -->
      <div class="relative z-10 flex items-center gap-3">
        <img
          src="/icons/Invinite_Logo.png"
          alt="Invinite"
          class="h-9 w-auto object-contain select-none brightness-110"
          loading="eager"
        />
        <div class="h-4 w-px bg-zinc-700"></div>
        <span class="text-xs font-semibold tracking-wider text-zinc-400 uppercase">
          Personal Finance
        </span>
      </div>

      <!-- Center Financial Product Preview Canvas -->
      <div class="relative z-10 space-y-6 max-w-lg my-auto py-8">
        <div>
          <h1 class="text-3xl xl:text-4xl font-black text-white tracking-tight leading-tight">
            Kejelasan finansial untuk setiap Rupiah Anda.
          </h1>
          <p class="text-sm text-zinc-400 mt-2.5 leading-relaxed">
            Satu aplikasi untuk memantau arus kas keuangan anda.
          </p>
        </div>

        <!-- Simulated Live Financial Dashboard Preview -->
        <div class="p-6 bg-zinc-900/90 rounded-2xl border border-zinc-800 shadow-2xl backdrop-blur-md space-y-5">
          <!-- Total Balance Widget -->
          <div class="flex items-center justify-between pb-4 border-b border-zinc-800/80">
            <div class="text-2xl xl:text-3xl font-black text-white tracking-tight tabular-nums">
              Rp 18.450.000
            </div>
            <div class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-xs font-bold tabular-nums">
              <TrendingUp class="w-3.5 h-3.5 stroke-[2.5]" />
              <span>+8.4% bulan ini</span>
            </div>
          </div>

          <!-- Simulated Monthly Cash Flow Trend (Vector SVG Sparkline) -->
          <div class="space-y-2">
            <div class="flex items-center justify-between text-xs">
              <span class="text-zinc-400 font-medium">Tren Arus Kas 30 Hari</span>
              <span class="text-emerald-400 font-bold tabular-nums">Surplus Rp 4.250.000</span>
            </div>
            <div class="h-16 w-full relative">
              <svg class="w-full h-full overflow-visible" viewBox="0 0 300 60" preserveAspectRatio="none">
                <defs>
                  <linearGradient id="emeraldGradient" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="#10b981" stop-opacity="0.35" />
                    <stop offset="100%" stop-color="#10b981" stop-opacity="0.0" />
                  </linearGradient>
                </defs>
                <path
                  d="M 0 45 Q 40 40 70 32 T 140 38 T 210 18 T 280 12 L 300 8 L 300 60 L 0 60 Z"
                  fill="url(#emeraldGradient)"
                />
                <path
                  d="M 0 45 Q 40 40 70 32 T 140 38 T 210 18 T 280 12 L 300 8"
                  fill="none"
                  stroke="#10b981"
                  stroke-width="2.5"
                  stroke-linecap="round"
                />
                <circle cx="300" cy="8" r="4" fill="#10b981" class="animate-pulse" />
              </svg>
            </div>
          </div>

        </div>
      </div>

      <!-- Bottom Trust Statement -->
      <div class="relative z-10 flex items-center justify-between text-xs text-zinc-400 pt-6 border-t border-zinc-800/80">
        <div class="flex items-center gap-2">
          <ShieldCheck class="w-4 h-4 text-emerald-400" />
          <span>Privasi data finansial Anda terlindungi penuh</span>
        </div>
        <span class="text-[11px] text-zinc-400">Invinite Digital Solutions</span>
      </div>
    </div>

    <!-- RIGHT SIDE: Focused, Frictionless Authentication Form -->
    <div class="col-span-12 lg:col-span-6 xl:col-span-5 flex flex-col justify-center px-6 sm:px-12 md:px-16 py-12 bg-surface-card">
      <div class="w-full max-w-md mx-auto space-y-6">
        <!-- Mobile Logo Header (Shown when left panel is hidden) -->
        <div class="lg:hidden flex items-center justify-between mb-4">
          <img
            src="/icons/Invinite_Logo.png"
            alt="Invinite"
            class="h-8 w-auto object-contain"
          />
          <span class="text-xs font-semibold text-content-muted">
            Personal Finance
          </span>
        </div>

        <!-- Form Heading -->
        <div>
          <h2 class="text-2xl sm:text-3xl font-black text-content-primary tracking-tight">
            {{ isRegister ? 'Buat Akun Finansial Anda' : 'Akses Kendali Finansial Anda' }}
          </h2>
          <p class="text-xs sm:text-sm text-content-secondary mt-1.5 leading-relaxed">
            {{ isRegister
              ? 'Kelola semua dompet, pantau arus kas, dan rencanakan masa depan.'
              : 'Kelola arus kas, aset, dan rencana masa depan di satu tempat.'
            }}
          </p>
        </div>

        <!-- Authentication Form -->
        <form @submit.prevent="handleSubmit" class="space-y-4">
          <div v-if="isRegister">
            <Input
              v-model="form.name"
              label="Nama Lengkap"
              placeholder="Contoh: Budi Santoso"
              autocomplete="name"
              required
            />
          </div>

          <div>
            <Input
              v-model="form.email"
              type="email"
              label="Alamat Email"
              placeholder="nama@domain.com"
              autocomplete="email"
              required
            />
          </div>

          <div>
            <Input
              v-model="form.password"
              type="password"
              label="Kata Sandi"
              placeholder="Min. 8 karakter kombinasi"
              autocomplete="current-password"
              showPasswordToggle
              required
            />
            <p v-if="isRegister" class="text-[11px] text-content-muted mt-1.5 pl-1">
              Minimal 8 karakter kombinasi huruf dan angka.
            </p>
          </div>

          <!-- Customer-Safe Error Alert -->
          <div
            v-if="errorMessage"
            class="p-3.5 bg-expense-muted border border-expense-border rounded-xl text-xs text-expense-default font-medium flex items-start gap-2.5 animate-in fade-in duration-150"
            role="alert"
          >
            <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
            <div class="leading-relaxed">{{ errorMessage }}</div>
          </div>

          <!-- Primary Submit Action -->
          <Button
            type="submit"
            variant="primary"
            size="lg"
            fullWidth
            :loading="loading"
            :loading-text="isRegister ? 'Membuat akun Anda...' : 'Memverifikasi...'"
          >
            {{ isRegister ? 'Buat Akun Gratis' : 'Masuk ke Dashboard' }}
          </Button>

          <!-- Toggle Auth Mode -->
          <div class="text-center pt-1">
            <button
              type="button"
              @click="toggleMode"
              class="text-xs font-semibold text-brand-default hover:underline focus-ring rounded p-1 cursor-pointer"
            >
              {{ isRegister
                ? 'Sudah terdaftar? Masuk ke akun Anda'
                : 'Belum terdaftar? Buat akun gratis'
              }}
            </button>
          </div>
        </form>

        <!-- Deliberate Demo Access (Subtle, Clean, Non-intrusive) -->
        <div class="pt-4 border-t border-border-subtle">
          <div class="flex items-center justify-between mb-2.5">
            <span class="text-xs font-semibold text-content-secondary">Coba account demo</span>
          </div>
          <div class="grid grid-cols-2 gap-2.5">
            <button
              type="button"
              @click="fillDemo('demo@example.com', 'Password123!')"
              class="px-3 py-2 rounded-xl bg-surface-subtle hover:bg-border-default border border-border-subtle text-left transition cursor-pointer flex flex-col justify-center"
            >
              <div class="text-xs font-bold text-content-primary">Personal account</div>
            </button>

            <button
              type="button"
              @click="fillDemo('premium@example.com', 'Password123!')"
              class="px-3 py-2 rounded-xl bg-surface-subtle hover:bg-border-default border border-border-subtle text-left transition cursor-pointer"
            >
              <div class="text-xs font-bold text-brand-default flex items-center gap-1">
                <span>Pro</span>
                <Sparkles class="w-3 h-3 text-amber-500" />
              </div>
              <div class="text-[10px] text-content-muted mt-0.5">Nikmati fitur penuh analisis keuangan</div>
            </button>
          </div>
        </div>

        <!-- Simple Trust Statement -->
        <div class="text-center pt-2">
          <p class="text-[11px] text-content-muted">
            Keamanan data terproteksi dengan enkripsi setara standar perbankan.
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue'
import { Input, Button } from '@/components/ui'
import { useAuthStore } from '@/stores/auth'
import {
  TrendingUp,
  ShieldCheck,
  AlertCircle,
  Sparkles
} from 'lucide-vue-next'

const emit = defineEmits(['authenticated'])

const authStore = useAuthStore()
const isRegister = ref(false)
const loading = ref(false)
const errorMessage = ref('')

const form = ref({
  name: '',
  email: '',
  password: ''
})

function toggleMode() {
  isRegister.value = !isRegister.value
  errorMessage.value = ''
}

function fillDemo(email, password) {
  isRegister.value = false
  form.value.email = email
  form.value.password = password
  errorMessage.value = ''
}

// Map technical/backend errors to customer-safe messages
function sanitizeAuthError(err) {
  const code = err?.code || ''
  const raw = (err?.detail || err?.message || '').toLowerCase()

  if (code === 'PASSWORD_TOO_SHORT' || raw.includes('at least 8 characters')) {
    return 'Kata sandi minimal 8 karakter.'
  }
  if (code === 'PASSWORD_TOO_WEAK' || raw.includes('one letter and one number')) {
    return 'Kata sandi harus mengandung kombinasi huruf dan angka.'
  }
  if (code === 'PASSWORD_TOO_LONG' || raw.includes('exceeds maximum length')) {
    return 'Kata sandi maksimal 128 karakter.'
  }
  if (code === 'DISPLAY_NAME_REQUIRED' || raw.includes('display name cannot be blank')) {
    return 'Nama lengkap wajib diisi.'
  }
  if (code === 'INVALID_EMAIL_FORMAT' || raw.includes('valid email address')) {
    return 'Format alamat email tidak valid.'
  }
  if (code === 'EMAIL_ALREADY_EXISTS' || raw.includes('already exists') || raw.includes('email already')) {
    return 'Alamat email ini sudah terdaftar. Silakan masuk atau gunakan email lain.'
  }
  if (err?.status === 401 || code === 'INVALID_CREDENTIALS' || raw.includes('unauthorized') || raw.includes('invalid credential')) {
    return 'Alamat email atau kata sandi tidak cocok. Silakan periksa kembali.'
  }
  if (err?.status === 429 || code === 'RATE_LIMIT_EXCEEDED' || raw.includes('rate limit') || raw.includes('too many')) {
    return 'Terlalu banyak percobaan. Mohon tunggu beberapa saat sebelum mencoba lagi.'
  }
  if (err?.status === 422 || raw.includes('422') || raw.includes('validation')) {
    return 'Data pendaftaran tidak valid. Silakan periksa kembali nama, email, dan kata sandi.'
  }
  if (raw.includes('network') || raw.includes('failed to fetch')) {
    return 'Koneksi jaringan terputus. Silakan periksa koneksi internet Anda.'
  }
  return err?.detail || 'Terjadi kendala saat memproses permintaan Anda. Silakan coba beberapa saat lagi.'
}

async function handleSubmit() {
  errorMessage.value = ''

  if (isRegister.value) {
    if (!form.value.name.trim()) {
      errorMessage.value = 'Nama lengkap wajib diisi.'
      return
    }
    if (form.value.password.length < 8) {
      errorMessage.value = 'Kata sandi minimal 8 karakter.'
      return
    }
    const hasLetter = /[a-zA-Z]/.test(form.value.password)
    const hasDigit = /[0-9]/.test(form.value.password)
    if (!hasLetter || !hasDigit) {
      errorMessage.value = 'Kata sandi harus mengandung kombinasi huruf dan angka.'
      return
    }
  }

  loading.value = true
  try {
    if (isRegister.value) {
      await authStore.register(form.value.name, form.value.email, form.value.password)
    } else {
      await authStore.login(form.value.email, form.value.password)
    }
    emit('authenticated')
  } catch (err) {
    errorMessage.value = sanitizeAuthError(err)
  } finally {
    loading.value = false
  }
}
</script>
