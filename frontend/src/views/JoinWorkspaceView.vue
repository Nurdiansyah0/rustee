<template>
  <div class="min-h-screen bg-surface-base flex items-center justify-center px-4 py-12">

    <!-- Loading: verifying token -->
    <div v-if="state === 'loading'" class="text-center space-y-3">
      <div class="w-10 h-10 border-4 border-brand-default border-t-transparent rounded-full animate-spin mx-auto"></div>
      <p class="text-sm text-content-secondary">Memverifikasi undangan…</p>
    </div>

    <!-- Invalid / Expired token -->
    <div v-else-if="state === 'invalid'" class="max-w-sm w-full text-center space-y-4">
      <div class="w-14 h-14 rounded-2xl bg-red-100 flex items-center justify-center mx-auto">
        <XCircle class="w-7 h-7 text-red-500" />
      </div>
      <h1 class="text-xl font-bold text-content-primary">Link Tidak Valid</h1>
      <p class="text-sm text-content-secondary">{{ errorMessage }}</p>
      <a
        href="/"
        class="inline-block mt-2 px-5 py-2.5 rounded-xl bg-brand-default text-white text-sm font-semibold hover:bg-brand-emphasis transition"
      >
        Kembali ke Beranda
      </a>
    </div>

    <!-- Preview + Register / Accept form -->
    <div v-else-if="state === 'preview' || state === 'submitting'" class="max-w-sm w-full space-y-6">

      <!-- Workspace badge -->
      <div class="text-center space-y-2">
        <div class="w-14 h-14 rounded-2xl bg-brand-subtle flex items-center justify-center mx-auto">
          <Building2 class="w-7 h-7 text-brand-default" />
        </div>
        <p class="text-xs font-semibold uppercase tracking-wider text-content-muted">Undangan Bergabung</p>
        <h1 class="text-2xl font-extrabold text-content-primary">{{ preview.workspace_name }}</h1>
        <div class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-surface-card border border-border-subtle text-xs font-semibold text-content-secondary">
          <span>Role:</span>
          <span class="capitalize text-brand-default">{{ roleName }}</span>
        </div>
        <p class="text-xs text-content-muted">Berlaku hingga {{ formatExpiry(preview.expires_at) }}</p>
      </div>

      <!-- Email pre-filled (read-only) -->
      <div class="space-y-1">
        <label class="block text-xs font-semibold text-content-secondary">Email</label>
        <div class="w-full px-3.5 py-2.5 rounded-xl bg-surface-subtle border border-border-subtle text-sm text-content-muted font-mono select-all">
          {{ preview.invited_email }}
        </div>
      </div>

      <!-- If already logged in as ANOTHER user → show mismatch warning -->
      <div v-if="emailMismatch" class="p-3.5 rounded-xl bg-amber-50 border border-amber-200 text-xs text-amber-800 flex items-start gap-2">
        <AlertTriangle class="w-4 h-4 shrink-0 mt-0.5" />
        <span>Anda sedang login sebagai <strong>{{ currentUserEmail }}</strong>. Link ini untuk <strong>{{ preview.invited_email }}</strong>. Logout dulu atau buka di mode incognito.</span>
      </div>

      <!-- Register form (only when not already logged in with matching email) -->
      <form v-if="!emailMismatch" @submit.prevent="handleSubmit" class="space-y-4">

        <!-- Display Name -->
        <div class="space-y-1">
          <label class="block text-xs font-semibold text-content-secondary" for="display-name">
            Nama Lengkap <span class="text-red-500">*</span>
          </label>
          <input
            id="display-name"
            v-model="form.displayName"
            type="text"
            required
            autocomplete="name"
            placeholder="Nama Anda"
            class="w-full px-3.5 py-2.5 rounded-xl bg-surface-card border border-border-subtle text-sm text-content-primary placeholder-content-muted focus:outline-none focus:ring-2 focus:ring-brand-default focus:border-transparent transition"
            :disabled="state === 'submitting' || alreadyLoggedIn"
          />
        </div>

        <!-- Username (Optional / Recommended for easy login) -->
        <div v-if="!alreadyLoggedIn" class="space-y-1">
          <label class="block text-xs font-semibold text-content-secondary" for="username">
            Username Login <span class="text-xs text-content-muted font-normal">(opsional)</span>
          </label>
          <input
            id="username"
            v-model="form.username"
            type="text"
            autocomplete="username"
            placeholder="Contoh: budi_kasir atau kasir1"
            class="w-full px-3.5 py-2.5 rounded-xl bg-surface-card border border-border-subtle text-sm text-content-primary placeholder-content-muted focus:outline-none focus:ring-2 focus:ring-brand-default focus:border-transparent transition"
            :disabled="state === 'submitting'"
          />
          <p class="text-[11px] text-content-muted">Bisa digunakan untuk login ke aplikasi selain email.</p>
        </div>

        <!-- Phone Number (Optional) -->
        <div v-if="!alreadyLoggedIn" class="space-y-1">
          <label class="block text-xs font-semibold text-content-secondary" for="phone">
            No. WhatsApp / Handphone <span class="text-xs text-content-muted font-normal">(opsional)</span>
          </label>
          <input
            id="phone"
            v-model="form.phone"
            type="tel"
            autocomplete="tel"
            placeholder="Contoh: 08123456789"
            class="w-full px-3.5 py-2.5 rounded-xl bg-surface-card border border-border-subtle text-sm text-content-primary placeholder-content-muted focus:outline-none focus:ring-2 focus:ring-brand-default focus:border-transparent transition"
            :disabled="state === 'submitting'"
          />
        </div>

        <!-- Password (hidden when already logged in) -->
        <div v-if="!alreadyLoggedIn" class="space-y-1">
          <label class="block text-xs font-semibold text-content-secondary" for="password">
            Buat Password <span class="text-red-500">*</span>
          </label>
          <div class="relative">
            <input
              id="password"
              v-model="form.password"
              :type="showPassword ? 'text' : 'password'"
              required
              autocomplete="new-password"
              placeholder="Min. 8 karakter"
              class="w-full px-3.5 py-2.5 pr-11 rounded-xl bg-surface-card border border-border-subtle text-sm text-content-primary placeholder-content-muted focus:outline-none focus:ring-2 focus:ring-brand-default focus:border-transparent transition"
              :disabled="state === 'submitting'"
            />
            <button
              type="button"
              tabindex="-1"
              @click="showPassword = !showPassword"
              class="absolute inset-y-0 right-3 flex items-center text-content-muted hover:text-content-secondary"
            >
              <component :is="showPassword ? EyeOff : Eye" class="w-4 h-4" />
            </button>
          </div>
          <p class="text-[11px] text-content-muted">Minimal 8 karakter.</p>
        </div>

        <!-- Error -->
        <div v-if="formError" class="p-3 rounded-xl bg-red-50 border border-red-200 text-xs text-red-700 flex items-start gap-2">
          <AlertTriangle class="w-4 h-4 shrink-0 mt-0.5" />
          <span>{{ formError }}</span>
        </div>

        <!-- Submit -->
        <button
          type="submit"
          :disabled="state === 'submitting'"
          class="w-full min-h-[48px] rounded-xl bg-brand-default text-white text-sm font-bold hover:bg-brand-emphasis focus:outline-none focus:ring-2 focus:ring-brand-default transition flex items-center justify-center gap-2 disabled:opacity-60 disabled:cursor-not-allowed cursor-pointer"
        >
          <span v-if="state !== 'submitting'">
            {{ alreadyLoggedIn ? 'Bergabung ke Workspace' : 'Daftar & Bergabung' }}
          </span>
          <span v-else class="flex items-center gap-2">
            <span class="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"></span>
            Memproses…
          </span>
        </button>

        <p class="text-center text-[11px] text-content-muted">
          Dengan bergabung, kamu setuju dengan
          <a href="#" class="underline hover:text-content-secondary">Ketentuan Layanan</a>
          Invinite Business OS.
        </p>
      </form>
    </div>

    <!-- Success -->
    <div v-else-if="state === 'success'" class="max-w-sm w-full text-center space-y-4">
      <div class="w-14 h-14 rounded-2xl bg-emerald-100 flex items-center justify-center mx-auto">
        <CheckCircle2 class="w-7 h-7 text-emerald-500" />
      </div>
      <h1 class="text-xl font-bold text-content-primary">Berhasil Bergabung!</h1>
      <p class="text-sm text-content-secondary">
        Selamat datang di <strong>{{ successWorkspaceName }}</strong>.<br/>
        Kamu masuk sebagai <span class="capitalize font-semibold text-brand-default">{{ successRole }}</span>.
      </p>
      <p class="text-xs text-content-muted">Mengalihkan ke dashboard…</p>
    </div>

  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import {
  Building2, XCircle, CheckCircle2, AlertTriangle, Eye, EyeOff
} from 'lucide-vue-next'
import { api } from '@/services/api'
import { useAuthStore } from '@/stores/auth'

const props = defineProps({
  token: {
    type: String,
    default: ''
  }
})

const emit = defineEmits(['accepted', 'close'])

// --- State ---
const authStore = useAuthStore()

const state = ref('loading') // 'loading' | 'invalid' | 'preview' | 'submitting' | 'success'
const errorMessage = ref('')
const formError = ref('')
const preview = ref(null)
const showPassword = ref(false)

const form = ref({
  displayName: '',
  username: '',
  phone: '',
  password: '',
})

const successWorkspaceName = ref('')
const successRole = ref('')

const resolvedToken = computed(() => {
  if (props.token) return props.token
  if (typeof window !== 'undefined') {
    const url = new URL(window.location.href)
    if (url.searchParams.get('invite')) return url.searchParams.get('invite')
    const match = window.location.pathname.match(/\/join\/([^/?#]+)/)
    if (match) return match[1]
  }
  return ''
})

// --- Computed ---
const alreadyLoggedIn = computed(() =>
  authStore.isAuthenticated &&
  authStore.user?.email?.toLowerCase() === preview.value?.invited_email?.toLowerCase()
)

const currentUserEmail = computed(() => authStore.user?.email || '')

const emailMismatch = computed(() =>
  authStore.isAuthenticated &&
  preview.value?.invited_email &&
  authStore.user?.email?.toLowerCase() !== preview.value.invited_email.toLowerCase()
)

const roleName = computed(() => {
  const roleMap = {
    administrator: 'Administrator',
    manager: 'Manajer',
    staff: 'Staf',
    accountant: 'Akuntan',
  }
  return roleMap[preview.value?.role] || preview.value?.role || '-'
})

// --- Helpers ---
function formatExpiry(iso) {
  if (!iso) return '-'
  return new Date(iso).toLocaleDateString('id-ID', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
  })
}

// --- Lifecycle ---
onMounted(async () => {
  if (!resolvedToken.value) {
    errorMessage.value = 'Token undangan tidak ditemukan pada tautan.'
    state.value = 'invalid'
    return
  }
  try {
    const res = await api.previewInvitation(resolvedToken.value)
    preview.value = res
    // Pre-fill name if already logged in
    if (authStore.isAuthenticated && authStore.user?.display_name) {
      form.value.displayName = authStore.user.display_name
    }
    state.value = 'preview'
  } catch (err) {
    const msg = err?.message || ''
    if (msg.includes('expired') || msg.includes('INVITATION_EXPIRED')) {
      errorMessage.value = 'Link undangan sudah kedaluwarsa. Minta Owner untuk mengirim ulang.'
    } else if (msg.includes('used') || msg.includes('INVITATION_INVALID')) {
      errorMessage.value = 'Link ini sudah digunakan sebelumnya.'
    } else {
      errorMessage.value = 'Link undangan tidak valid atau tidak ditemukan.'
    }
    state.value = 'invalid'
  }
})

// --- Actions ---
async function handleSubmit() {
  formError.value = ''

  if (!form.value.displayName.trim()) {
    formError.value = 'Nama lengkap harus diisi.'
    return
  }

  if (!alreadyLoggedIn.value && form.value.password.length < 8) {
    formError.value = 'Password minimal 8 karakter.'
    return
  }

  state.value = 'submitting'

  try {
    const res = await api.acceptInvitation(resolvedToken.value, {
      display_name: form.value.displayName.trim(),
      username: form.value.username.trim() || null,
      phone: form.value.phone.trim() || null,
      password: alreadyLoggedIn.value ? '__existing_user__' : form.value.password,
    })

    successWorkspaceName.value = res.workspace_name
    successRole.value = res.role

    // Initialize authenticated user in session
    if (!alreadyLoggedIn.value) {
      const staffUser = {
        id: res.user_id,
        email: res.email,
        display_name: res.display_name,
        account_type: res.account_type || 'staff',
        role: 'user',
        subscription_tier: 'free',
      }
      authStore.user = staffUser
      authStore.permissions = res.permissions || []
      try {
        localStorage.setItem('invinite_auth_user', JSON.stringify(staffUser))
        localStorage.setItem('invinite_user_personalization', JSON.stringify({
          display_name: res.display_name,
          onboarding_completed: true, // Staff accounts are fully active immediately
        }))
      } catch {}
    }

    state.value = 'success'

    // Automatically switch to the invited workspace and transition to main app
    setTimeout(async () => {
      try {
        if (res.workspace_id) {
          localStorage.setItem('invinite_active_tenant_id', res.workspace_id)
          await api.switchWorkspace(res.workspace_id)
        }
      } catch (_) {}
      emit('accepted', res)
      window.location.href = '/'
    }, 1500)
  } catch (err) {
    const msg = err?.message || ''
    if (msg.includes('ALREADY_MEMBER')) {
      formError.value = 'Kamu sudah menjadi anggota workspace ini.'
    } else if (msg.includes('EMAIL_ALREADY_EXISTS')) {
      formError.value = 'Email sudah terdaftar. Silakan login terlebih dahulu, lalu buka link ini kembali.'
    } else {
      formError.value = err?.message || 'Gagal bergabung. Coba lagi atau hubungi Owner.'
    }
    state.value = 'preview'
  }
}
</script>
