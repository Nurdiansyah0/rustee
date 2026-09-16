<template>
  <div class="min-h-dvh lg:h-screen lg:max-h-screen lg:overflow-hidden w-full grid grid-cols-1 lg:grid-cols-12 bg-surface-canvas font-sans selection:bg-brand-default selection:text-white">
    <!-- LEFT SIDE: Brand + Product Financial Visualization (Desktop only) -->
    <div class="hidden lg:flex lg:col-span-6 xl:col-span-7 bg-zinc-950 border-r border-zinc-800/80 p-5 xl:p-8 2xl:p-10 flex-col justify-between relative text-white lg:h-full lg:overflow-y-auto scroll-native">
      <!-- Ambient Background Radial Glows -->
      <div class="absolute -top-32 -left-32 w-96 h-96 bg-emerald-500/15 rounded-full blur-3xl pointer-events-none"></div>
      <div class="absolute -bottom-32 -right-32 w-96 h-96 bg-teal-500/10 rounded-full blur-3xl pointer-events-none"></div>

      <!-- Center Financial Product Preview Canvas -->
      <div class="relative z-10 space-y-3 xl:space-y-4 max-w-md xl:max-w-lg my-auto py-2 w-full">
        <div>
          <div class="mb-2">
            <FinrepIcon size="44" theme="dark" />
          </div>
          <h1 class="text-xl xl:text-2xl 2xl:text-3xl font-black text-white tracking-tight leading-snug">
            Satu Dasbor Finansial, Keputusan Lebih Presisi.
          </h1>
        </div>

        <!-- Simulated Live Financial Dashboard Preview -->
        <LiveDashboardPreview />
      </div>

      <!-- Bottom Footer -->
      <div class="relative z-10 flex items-center justify-end gap-2 text-xs text-zinc-400 pt-3 border-t border-zinc-800/80 shrink-0">
        <span class="text-[11px] text-zinc-400">by Invinite.id - Digital Creative Solutions</span>
        <img
          src="/icons/Invinite_Logo.png"
          alt="Invinite.id - Digital Creative Solutions"
          class="h-4 w-auto max-h-4 object-contain select-none"
          loading="eager"
        />
      </div>
    </div>

    <!-- RIGHT SIDE: Focused, Frictionless Authentication Form -->
    <div class="col-span-12 lg:col-span-6 xl:col-span-5 flex flex-col justify-start lg:justify-center px-5 sm:px-8 md:px-10 lg:px-8 xl:px-12 py-4 lg:py-3.5 bg-surface-card min-h-dvh lg:h-full lg:overflow-y-auto scroll-native">
      <div class="w-full max-w-md mx-auto space-y-3 my-auto">
        <!-- Mobile Logo Header (Shown when left panel is hidden) -->
        <div class="lg:hidden flex items-start mb-2">
          <FinrepIcon size="44" theme="auto" />
        </div>

        <!-- Form Heading (Penyambutan Hangat & Tenang) -->
        <div>
          <h2 class="text-lg sm:text-xl font-black text-content-primary tracking-tight leading-snug">
            <template v-if="isForgot">
              {{ recoverySubmitted ? 'Periksa Kotak Masuk Email Anda' : 'Pemulihan Akses Akun FinRep' }}
            </template>
            <template v-else-if="isRegister">
              Mulai langkah pertamamu. Bangun finansial yang lebih tertata.
            </template>
            <template v-else>
              Selamat datang kembali. Mari lihat perkembangan finansialmu hari ini.
            </template>
          </h2>
          <p class="text-xs text-content-secondary mt-0.5 leading-normal">
            <template v-if="isForgot">
              {{ recoverySubmitted
                ? 'Langkah pemulihan keamanan telah dikirimkan ke email Anda.'
                : 'Masukkan alamat email yang terdaftar untuk menerima tautan pemulihan kata sandi.'
              }}
            </template>
            <template v-else>
              Satu tempat tenang untuk mengamati arus kas, aset, dan rencanamu.
            </template>
          </p>
        </div>

        <!-- Mobile Live Dashboard Preview (Hidden in Forgot view to keep recovery frictionless) -->
        <div v-if="!isForgot" class="lg:hidden">
          <LiveDashboardPreview />
        </div>

        <!-- ============================================================ -->
        <!-- FLOW 1: "LUPA AKUN" (ACCOUNT RECOVERY) VIEW -->
        <!-- ============================================================ -->
        <div v-if="isForgot" class="space-y-3">
          <!-- State A: Recovery Request Form -->
          <form v-if="!recoverySubmitted" @submit.prevent="handleRecoverySubmit" novalidate class="space-y-2.5">
            <div>
              <Input
                ref="recoveryInputRef"
                v-model="recoveryEmail"
                type="email"
                label="Email yang terdaftar pada akunmu"
                placeholder="contoh: budi@perusahaan.com"
                autocomplete="email"
                required
                autofocus
              />
            </div>

            <!-- Customer-Safe Error Alert -->
            <div
              v-if="recoveryError"
              class="p-2.5 bg-expense-muted border border-expense-border rounded-xl text-xs text-expense-default font-medium flex items-start gap-2 animate-in fade-in duration-150"
              role="alert"
            >
              <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
              <div class="leading-relaxed">{{ recoveryError }}</div>
            </div>

            <!-- Primary Submit Action -->
            <Button
              type="submit"
              variant="primary"
              size="md"
              fullWidth
              :loading="recoveryLoading"
              loading-text="Mengirim tautan pemulihan..."
            >
              Kirim Tautan Pemulihan
            </Button>

            <!-- Seamless Return Link to Login -->
            <div class="text-center pt-0.5">
              <button
                type="button"
                @click="setMode('login')"
                class="inline-flex items-center gap-1.5 text-xs font-semibold text-brand-default hover:underline focus-ring rounded p-1 cursor-pointer"
              >
                <ArrowLeft class="w-3.5 h-3.5" />
                <span>Kembali ke Masuk ke Akun Saya</span>
              </button>
            </div>
          </form>

          <!-- State B: Confirmation Feedback State -->
          <div
            v-else
            ref="confirmationCardRef"
            tabindex="-1"
            role="status"
            aria-live="polite"
            class="space-y-3 animate-in fade-in zoom-in-95 duration-200 focus:outline-none"
          >
            <div class="p-3.5 rounded-2xl bg-surface-subtle border border-border-subtle text-left space-y-2.5">
              <div class="flex items-center gap-2.5">
                <div class="w-8 h-8 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-emerald-500 shrink-0">
                  <CheckCircle2 class="w-4 h-4 stroke-[2.5]" />
                </div>
                <div>
                  <div class="text-[10px] font-bold text-emerald-500 uppercase tracking-wider">Email Terkirim</div>
                  <h3 class="text-xs font-bold text-content-primary">Periksa Kotak Masuk Anda</h3>
                </div>
              </div>

              <div class="p-2.5 bg-surface-card rounded-xl border border-border-default text-xs leading-relaxed space-y-1">
                <p class="font-semibold text-emerald-600 dark:text-emerald-400 flex items-center gap-1.5">
                  <MailCheck class="w-3.5 h-3.5 shrink-0" />
                  <span>Tautan pemulihan akun telah dikirimkan ke email Anda</span>
                </p>
                <p class="text-content-secondary text-[11px] break-words">
                  Instruksi pemulihan telah dikirimkan ke <strong class="text-content-primary break-all">{{ recoveryEmail }}</strong>. Klik tautan di dalamnya untuk mengatur ulang kata sandi.
                </p>
                <p class="text-[10px] text-content-muted">
                  Belum muncul? Pastikan untuk memeriksa folder Spam atau Promosi.
                </p>
              </div>
            </div>

            <!-- Resend success status -->
            <div
              v-if="resendSuccessMessage"
              class="p-2 bg-emerald-500/10 border border-emerald-500/20 rounded-xl text-xs text-emerald-600 dark:text-emerald-400 font-medium flex items-center gap-2 animate-in fade-in duration-150"
              role="status"
              aria-live="polite"
            >
              <CheckCircle2 class="w-3.5 h-3.5 shrink-0" />
              <span>{{ resendSuccessMessage }}</span>
            </div>

            <!-- Primary Return Action -->
            <Button
              type="button"
              variant="primary"
              size="md"
              fullWidth
              @click="setMode('login')"
            >
              Masuk ke Akun Saya
            </Button>

            <!-- Option to Resend -->
            <div class="text-center pt-0.5">
              <button
                type="button"
                @click="handleResendRecovery"
                :disabled="resendLoading || resendCooldown > 0"
                class="text-xs font-semibold text-brand-default hover:underline focus-ring rounded p-1 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <span v-if="resendLoading">Mengirim ulang...</span>
                <span v-else-if="resendCooldown > 0">Kirim ulang tautan dalam {{ resendCooldown }} detik</span>
                <span v-else>Belum menerima email? Kirim ulang</span>
              </button>
            </div>
          </div>
        </div>

        <!-- ============================================================ -->
        <!-- FLOW 2: LOGIN & REGISTER AUTHENTICATION FORM -->
        <!-- ============================================================ -->
        <form v-else @submit.prevent="handleSubmit" novalidate class="space-y-2.5">
          <div v-if="isRegister">
            <Input
              ref="registerNameInputRef"
              v-model="form.name"
              label="Nama Lengkap"
              placeholder="Contoh: Budi Santoso"
              autocomplete="name"
              autofocus
              required
            />
          </div>

          <div>
            <Input
              ref="loginEmailInputRef"
              v-model="form.email"
              type="email"
              label="Email yang biasa kamu gunakan"
              placeholder="contoh: budi@perusahaan.com"
              autocomplete="email"
              required
            />
          </div>

          <div>
            <Input
              v-model="form.password"
              type="password"
              label="Kata Sandi"
              placeholder="Masukkan kata sandi"
              autocomplete="current-password"
              showPasswordToggle
              required
            >
              <template #labelExtra>
                <button
                  v-if="!isRegister"
                  type="button"
                  @click="openForgotPassword"
                  class="text-[11px] font-medium text-brand-default hover:underline focus-ring rounded p-0.5 cursor-pointer"
                >
                  Lupa Akun?
                </button>
              </template>
            </Input>
            <p v-if="isRegister" class="text-[10px] sm:text-[11px] text-content-muted mt-0.5 pl-0.5 leading-tight">
              Gunakan minimal 8 karakter agar akunmu tetap aman dan terlindungi.
            </p>
          </div>

          <!-- Customer-Safe Error Alert -->
          <div
            v-if="errorMessage"
            class="p-2.5 bg-expense-muted border border-expense-border rounded-xl text-xs text-expense-default font-medium flex items-start gap-2 animate-in fade-in duration-150"
            role="alert"
          >
            <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
            <div class="leading-relaxed">{{ errorMessage }}</div>
          </div>

          <!-- Tombol Utama (CTA) -->
          <Button
            type="submit"
            variant="primary"
            size="md"
            fullWidth
            :loading="loading"
            :loading-text="isRegister ? 'Menyiapkan akunmu...' : 'Memverifikasi...'"
          >
            {{ isRegister ? 'Mulai Langkah Pertama Gratis' : 'Masuk ke Akun Saya' }}
          </Button>

          <!-- Navigasi Sekunder (Toggle Auth Mode) -->
          <div class="text-center pt-0.5">
            <button
              type="button"
              @click="toggleMode"
              class="text-xs font-semibold text-brand-default hover:underline focus-ring rounded p-1 cursor-pointer"
            >
              {{ isRegister
                ? 'Sudah punya akun? Masuk ke Akun Saya'
                : 'Baru di FinRep? Mulai langkah pertamamu gratis'
              }}
            </button>
          </div>
        </form>

        <!-- Pilihan Akun & Paket Demo (Bukan Sekadar Label Kering) -->
        <div v-if="!isForgot" class="pt-2 border-t border-border-subtle space-y-1.5">
          <div class="flex items-center justify-between">
            <span class="text-xs font-semibold text-content-secondary">
              Ingin lihat-lihat dulu? Coba akun demo tanpa daftar
            </span>
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <!-- Akun Pribadi -->
            <button
              type="button"
              @click="fillDemo('demo@example.com', 'Password123!')"
              class="p-2 rounded-xl bg-surface-subtle hover:bg-border-default border border-border-subtle text-left transition cursor-pointer flex flex-col justify-between group"
            >
              <div>
                <div class="text-xs font-bold text-content-primary group-hover:text-brand-default transition">
                  Akun Pribadi
                </div>
                <div class="text-[10px] text-content-muted mt-0.5 leading-snug">
                  Untuk kamu yang ingin keuangan harian lebih tertata rapi.
                </div>
              </div>
              <div class="text-[10px] font-semibold text-brand-default mt-1">Coba Demo Pribadi &rarr;</div>
            </button>

            <!-- FinRep Pro -->
            <button
              type="button"
              @click="fillDemo('premium@example.com', 'Password123!')"
              class="p-2 rounded-xl bg-surface-subtle hover:bg-border-default border border-border-subtle text-left transition cursor-pointer flex flex-col justify-between group"
            >
              <div>
                <div class="text-xs font-bold text-brand-default flex items-center gap-1">
                  <span>FinRep Pro</span>
                  <Sparkles class="w-3 h-3 text-amber-500" />
                </div>
                <div class="text-[10px] text-content-muted mt-0.5 leading-snug">
                  Dirancang untuk pemilik bisnis dan profesional yang butuh proyeksi laba lebih presisi.
                </div>
              </div>
              <div class="text-[10px] font-semibold text-brand-default mt-1">Coba Demo Pro &rarr;</div>
            </button>
          </div>
        </div>

        <!-- Mobile Footer (by Invinite.id) -->
        <div class="lg:hidden flex items-center justify-center gap-2 pt-2.5 text-xs text-content-muted border-t border-border-subtle">
          <span class="text-[11px]">by Invinite.id - Digital Creative Solutions</span>
          <img
            src="/icons/Invinite_Logo.png"
            alt="Invinite.id - Digital Creative Solutions"
            class="h-4 w-auto max-h-4 object-contain select-none"
            loading="eager"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { Input, Button } from '@/components/ui'
import FinrepIcon from '@/components/FinrepIcon.vue'
import LiveDashboardPreview from './LiveDashboardPreview.vue'
import { useAuthStore } from '@/stores/auth'
import {
  AlertCircle,
  Sparkles,
  ArrowLeft,
  CheckCircle2,
  MailCheck
} from 'lucide-vue-next'

const emit = defineEmits(['authenticated'])

const authStore = useAuthStore()
const authMode = ref('login') // 'login' | 'register' | 'forgot'
const isRegister = computed(() => authMode.value === 'register')
const isForgot = computed(() => authMode.value === 'forgot')

const loading = ref(false)
const errorMessage = ref('')

// Focus management refs
const recoveryInputRef = ref(null)
const loginEmailInputRef = ref(null)
const registerNameInputRef = ref(null)
const confirmationCardRef = ref(null)

let isMounted = false

const form = ref({
  name: '',
  email: '',
  password: ''
})

// Account Recovery ("Lupa Akun") state
const recoveryEmail = ref('')
const recoveryLoading = ref(false)
const recoveryError = ref('')
const recoverySubmitted = ref(false)
const resendLoading = ref(false)
const resendSuccessMessage = ref('')
const resendCooldown = ref(0)
let resendTimer = null

function updateAuthUrl(mode, push = false) {
  try {
    if (typeof window !== 'undefined' && window.history) {
      const url = new URL(window.location.href)
      const hadOtherParams = url.searchParams.has('mode') || url.searchParams.has('view')
      if (mode === 'forgot') {
        url.searchParams.set('auth', 'forgot')
        url.searchParams.delete('mode')
        url.searchParams.delete('view')
      } else if (mode === 'register') {
        url.searchParams.set('auth', 'register')
        url.searchParams.delete('mode')
        url.searchParams.delete('view')
      } else {
        url.searchParams.delete('auth')
        url.searchParams.delete('mode')
        url.searchParams.delete('view')
      }
      const newUrl = url.pathname + (url.search ? url.search : '') + url.hash
      const currentParam = new URLSearchParams(window.location.search).get('auth') || 'login'
      const targetParam = mode === 'forgot' ? 'forgot' : mode === 'register' ? 'register' : 'login'

      if (push && currentParam !== targetParam) {
        window.history.pushState({ authMode: mode }, '', newUrl)
      } else if (currentParam !== targetParam || hadOtherParams) {
        window.history.replaceState({ authMode: mode }, '', newUrl)
      }
    }
  } catch {
    // Ignore URL manipulation failures in restricted iframe/sandbox
  }
}

async function setMode(mode, pushHistory = true) {
  authMode.value = mode
  errorMessage.value = ''
  recoveryError.value = ''
  resendSuccessMessage.value = ''
  if (mode === 'forgot') {
    recoverySubmitted.value = false
    const trimmed = (form.value.email || '').trim()
    if (trimmed) {
      recoveryEmail.value = trimmed
    }
  } else {
    const trimmed = (recoveryEmail.value || '').trim()
    if (trimmed) {
      form.value.email = trimmed
    }
  }
  updateAuthUrl(mode, pushHistory)
  await nextTick()
  if (authMode.value === 'forgot') {
    recoveryInputRef.value?.focus?.()
  } else if (authMode.value === 'register') {
    registerNameInputRef.value?.focus?.()
  } else if (authMode.value === 'login') {
    loginEmailInputRef.value?.focus?.()
  }
}

function toggleMode() {
  if (authMode.value === 'register') {
    setMode('login', true)
  } else {
    setMode('register', true)
  }
}

function openForgotPassword() {
  setMode('forgot', true)
}

function fillDemo(email, password) {
  setMode('login', false)
  form.value.email = email
  form.value.password = password
  errorMessage.value = ''
}

function validateEmail(email) {
  if (typeof email !== 'string') return false
  const trimmed = email.trim()
  if (!trimmed || trimmed.length > 254) return false
  const re = /^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*\.[a-zA-Z]{2,}$/
  return re.test(trimmed)
}

async function handleRecoverySubmit() {
  if (recoveryLoading.value) return
  recoveryError.value = ''
  resendSuccessMessage.value = ''

  const email = recoveryEmail.value.trim()
  if (!email) {
    recoveryError.value = 'Mohon masukkan alamat email yang terdaftar.'
    return
  }
  if (!validateEmail(email)) {
    recoveryError.value = 'Format alamat email tidak valid (contoh: nama@domain.com).'
    return
  }

  recoveryEmail.value = email
  recoveryLoading.value = true
  try {
    // Simulated customer-safe recovery dispatch
    await new Promise((resolve) => setTimeout(resolve, 450))
    if (!isMounted) return
    recoverySubmitted.value = true
    startResendCooldown(30)
    await nextTick()
    confirmationCardRef.value?.focus?.()
  } catch (err) {
    if (isMounted) {
      recoveryError.value = 'Gagal memproses permintaan pemulihan. Silakan coba sesaat lagi.'
    }
  } finally {
    if (isMounted) {
      recoveryLoading.value = false
    }
  }
}

async function handleResendRecovery() {
  if (resendCooldown.value > 0 || resendLoading.value) return
  resendLoading.value = true
  resendSuccessMessage.value = ''
  recoveryError.value = ''

  try {
    await new Promise((resolve) => setTimeout(resolve, 350))
    if (!isMounted) return
    resendSuccessMessage.value = 'Tautan pemulihan baru telah dikirimkan ulang!'
    startResendCooldown(30)
  } catch (err) {
    if (isMounted) {
      recoveryError.value = 'Gagal mengirim ulang email. Silakan coba beberapa saat lagi.'
    }
  } finally {
    if (isMounted) {
      resendLoading.value = false
    }
  }
}

function startResendCooldown(seconds = 30) {
  resendCooldown.value = seconds
  if (resendTimer) clearInterval(resendTimer)
  resendTimer = setInterval(() => {
    if (!isMounted) {
      clearInterval(resendTimer)
      resendTimer = null
      return
    }
    if (resendCooldown.value > 0) {
      resendCooldown.value--
    } else {
      clearInterval(resendTimer)
      resendTimer = null
    }
  }, 1000)
}

async function syncModeFromUrl() {
  try {
    const params = new URLSearchParams(window.location.search)
    const param = params.get('auth') || params.get('mode') || params.get('view')
    errorMessage.value = ''
    recoveryError.value = ''
    resendSuccessMessage.value = ''
    let targetMode = 'login'
    if (param === 'forgot') {
      targetMode = 'forgot'
      recoverySubmitted.value = false
      const trimmed = (form.value.email || '').trim()
      if (trimmed) {
        recoveryEmail.value = trimmed
      }
    } else if (param === 'register') {
      targetMode = 'register'
      const trimmed = (recoveryEmail.value || '').trim()
      if (trimmed) {
        form.value.email = trimmed
      }
    } else {
      targetMode = 'login'
      const trimmed = (recoveryEmail.value || '').trim()
      if (trimmed) {
        form.value.email = trimmed
      }
    }
    authMode.value = targetMode
    updateAuthUrl(targetMode, false)
    await nextTick()
    if (authMode.value === 'forgot') {
      recoveryInputRef.value?.focus?.()
    } else if (authMode.value === 'register') {
      registerNameInputRef.value?.focus?.()
    } else if (authMode.value === 'login') {
      loginEmailInputRef.value?.focus?.()
    }
  } catch {}
}

onMounted(() => {
  isMounted = true
  syncModeFromUrl()
  if (typeof window !== 'undefined') {
    window.addEventListener('popstate', syncModeFromUrl)
  }
})

onUnmounted(() => {
  isMounted = false
  if (resendTimer) {
    clearInterval(resendTimer)
    resendTimer = null
  }
  if (typeof window !== 'undefined') {
    window.removeEventListener('popstate', syncModeFromUrl)
  }
})

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
  if (loading.value) return
  errorMessage.value = ''

  const email = (form.value.email || '').trim()
  const password = form.value.password || ''

  if (!isRegister.value) {
    if (!email) {
      errorMessage.value = 'Mohon masukkan alamat email Anda.'
      return
    }
    if (!validateEmail(email)) {
      errorMessage.value = 'Format alamat email tidak valid (contoh: nama@domain.com).'
      return
    }
    if (!password) {
      errorMessage.value = 'Mohon masukkan kata sandi Anda.'
      return
    }
  } else {
    const name = (form.value.name || '').trim()
    if (!name) {
      errorMessage.value = 'Nama lengkap wajib diisi.'
      return
    }
    if (!email) {
      errorMessage.value = 'Mohon masukkan alamat email Anda.'
      return
    }
    if (!validateEmail(email)) {
      errorMessage.value = 'Format alamat email tidak valid (contoh: nama@domain.com).'
      return
    }
    if (password.length < 8) {
      errorMessage.value = 'Kata sandi minimal 8 karakter.'
      return
    }
    const hasLetter = /[a-zA-Z]/.test(password)
    const hasDigit = /[0-9]/.test(password)
    if (!hasLetter || !hasDigit) {
      errorMessage.value = 'Kata sandi harus mengandung kombinasi huruf dan angka.'
      return
    }
  }

  form.value.email = email
  loading.value = true
  try {
    if (isRegister.value) {
      await authStore.register((form.value.name || '').trim(), email, password)
    } else {
      await authStore.login(email, password)
    }
    emit('authenticated')
  } catch (err) {
    errorMessage.value = sanitizeAuthError(err)
  } finally {
    loading.value = false
  }
}
</script>
