<template>
  <div v-if="workspaceStore.isBusinessWorkspace" class="p-5 sm:p-6 bg-surface-card rounded-xl border border-border-subtle shadow-card space-y-4">
    <!-- Header -->
    <div class="flex items-center justify-between gap-3">
      <div class="flex items-center gap-2.5">
        <div class="w-8 h-8 rounded-lg bg-brand-muted text-brand-default flex items-center justify-center shrink-0">
          <Users class="w-4 h-4 stroke-[2]" />
        </div>
        <div>
          <h3 class="text-sm font-bold text-content-primary">Tim & Akses Anggota</h3>
          <p class="text-[11px] text-content-muted">
            Kelola hak akses operasional untuk ruang kerja <strong>{{ workspaceStore.activeTenantName }}</strong>
          </p>
        </div>
      </div>

      <!-- Invite Member Trigger (Owner & Admin only) -->
      <button
        v-if="canManageMembers"
        type="button"
        @click="showInviteForm = !showInviteForm"
        class="min-h-[36px] px-3 py-1.5 rounded-lg bg-brand-default text-white hover:bg-brand-emphasis text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-xs"
      >
        <UserPlus class="w-3.5 h-3.5 stroke-[2.5]" />
        <span>Undang Staf</span>
      </button>
    </div>

    <!-- Non-Admin Notice for Staff -->
    <div v-if="!canManageMembers" class="p-3 bg-surface-subtle rounded-xl border border-border-subtle text-xs text-content-secondary flex items-center justify-between">
      <span>Peran Anda saat ini: <strong class="capitalize text-brand-default font-bold">{{ workspaceStore.activeRole }}</strong></span>
      <span class="text-[10px] text-content-muted">Akses pengelolaan tim dipegang oleh Owner/Admin</span>
    </div>

    <!-- Invite Form Drawer/Modal -->
    <div v-if="showInviteForm && canManageMembers" class="p-4 rounded-xl bg-surface-subtle border border-brand-border/40 space-y-3 animate-in fade-in duration-150">
      <div class="flex items-center justify-between">
        <h4 class="text-xs font-bold text-content-primary flex items-center gap-1.5">
          <Mail class="w-3.5 h-3.5 text-brand-default" />
          <span>Buat Link Undangan Anggota Baru</span>
        </h4>
        <button type="button" @click="showInviteForm = false" class="text-content-muted hover:text-content-primary">
          <X class="w-4 h-4" />
        </button>
      </div>

      <p class="text-[11px] text-content-muted leading-relaxed">
        Anggota yang diundang akan menerima tautan akses langsung tanpa perlu mengisi profil bisnis atau melakukan setup ruang kerja baru.
      </p>

      <div class="grid grid-cols-1 sm:grid-cols-12 gap-2.5">
        <div class="sm:col-span-7">
          <label class="block text-[10px] font-bold uppercase tracking-wider text-content-muted mb-1">Email Calon Anggota</label>
          <input
            v-model="inviteForm.email"
            type="email"
            placeholder="staf@email.com"
            class="w-full px-3 py-2 rounded-lg bg-surface-card border border-border-default text-xs text-content-primary placeholder-content-muted focus:ring-2 focus:ring-brand-default focus:outline-none"
            :disabled="isInviting"
          />
        </div>
        <div class="sm:col-span-5">
          <label class="block text-[10px] font-bold uppercase tracking-wider text-content-muted mb-1">Peran (Role RBAC)</label>
          <select
            v-model="inviteForm.role"
            class="w-full px-3 py-2 rounded-lg bg-surface-card border border-border-default text-xs text-content-primary focus:ring-2 focus:ring-brand-default focus:outline-none capitalize"
            :disabled="isInviting"
          >
            <option value="staff">Staf (Kasir/Faktur/Operasional)</option>
            <option value="manager">Manajer (Proyek & Operasional)</option>
            <option value="accountant">Akuntan (Buku Kas & Jurnal)</option>
            <option value="administrator">Administrator (Kelola Tim)</option>
          </select>
        </div>
      </div>

      <!-- Error / Success Alert -->
      <div v-if="inviteError" class="p-2.5 rounded-lg bg-red-50 border border-red-200 text-xs text-red-700 flex items-center gap-1.5">
        <AlertTriangle class="w-3.5 h-3.5 shrink-0" />
        <span>{{ inviteError }}</span>
      </div>

      <!-- Generated Link Box -->
      <div v-if="lastCreatedLink" class="p-3 rounded-lg bg-brand-subtle/50 border border-brand-default/30 space-y-2">
        <div class="flex items-center justify-between text-xs font-bold text-brand-default">
          <span>Link Undangan Siap Dibagikan:</span>
          <span class="text-[10px] text-content-muted">Berlaku 7 hari</span>
        </div>
        <div class="flex items-center gap-2">
          <input
            type="text"
            readonly
            :value="lastCreatedLink"
            class="flex-1 px-2.5 py-1.5 rounded-md bg-surface-card border border-border-subtle text-xs text-content-primary font-mono select-all"
          />
          <button
            type="button"
            @click="copyInviteLink(lastCreatedLink)"
            class="px-3 py-1.5 rounded-md bg-brand-default text-white hover:bg-brand-emphasis text-xs font-bold transition flex items-center gap-1 cursor-pointer shrink-0"
          >
            <component :is="isCopied ? Check : Copy" class="w-3.5 h-3.5" />
            <span>{{ isCopied ? 'Tersalin!' : 'Salin Link' }}</span>
          </button>
        </div>
      </div>

      <div class="flex justify-end gap-2 pt-1">
        <button
          type="button"
          @click="showInviteForm = false"
          class="px-3 py-1.5 rounded-lg border border-border-subtle text-xs font-semibold text-content-secondary hover:bg-surface-card"
        >
          Tutup
        </button>
        <button
          type="button"
          @click="handleCreateInvitation"
          :disabled="isInviting || !inviteForm.email.trim()"
          class="px-4 py-1.5 rounded-lg bg-brand-default text-white hover:bg-brand-emphasis text-xs font-bold transition flex items-center gap-1.5 disabled:opacity-50 cursor-pointer"
        >
          <span v-if="isInviting" class="w-3 h-3 border-2 border-white border-t-transparent rounded-full animate-spin"></span>
          <span>{{ isInviting ? 'Membuat link…' : 'Generate Link Undangan' }}</span>
        </button>
      </div>
    </div>

    <!-- Active Team Members List -->
    <div class="space-y-2">
      <div class="flex items-center justify-between text-xs font-bold text-content-muted uppercase tracking-wider px-1">
        <span>Anggota Aktif ({{ members.length }})</span>
        <button
          type="button"
          @click="loadTeamData"
          class="text-brand-default hover:underline normal-case font-semibold text-[11px] flex items-center gap-1"
        >
          <RefreshCw class="w-3 h-3" :class="{ 'animate-spin': isLoading }" />
          <span>Segarkan</span>
        </button>
      </div>

      <!-- Members Deck -->
      <div v-if="members.length > 0" class="divide-y divide-border-subtle border border-border-subtle rounded-xl bg-surface-card overflow-hidden">
        <div
          v-for="m in members"
          :key="m.user_id"
          class="p-3 sm:p-3.5 flex items-center justify-between gap-3 hover:bg-surface-subtle/50 transition"
        >
          <!-- User info -->
          <div class="flex items-center gap-3 min-w-0">
            <div class="w-8 h-8 rounded-lg bg-surface-subtle text-content-primary flex items-center justify-center font-bold text-xs shrink-0 border border-border-subtle">
              {{ (m.display_name || m.email || 'U')[0].toUpperCase() }}
            </div>
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-xs font-bold text-content-primary truncate">{{ m.display_name }}</span>
                <span
                  :class="[
                    'text-[9px] font-extrabold uppercase px-1.5 py-0.2 rounded-md tracking-wider',
                    m.role === 'owner' ? 'bg-amber-100 text-amber-800 border border-amber-300' :
                    m.role === 'administrator' ? 'bg-blue-100 text-blue-800 border border-blue-300' :
                    'bg-surface-subtle text-content-secondary border border-border-subtle'
                  ]"
                >
                  {{ roleLabel(m.role) }}
                </span>
              </div>
              <div class="text-[11px] text-content-muted truncate">{{ m.email }}</div>
            </div>
          </div>

          <!-- Actions (Owner only can demote/remove) -->
          <div v-if="canManageMembers && m.role !== 'owner'" class="flex items-center gap-1.5 shrink-0">
            <!-- Role Selector (Owner Only) -->
            <select
              v-if="isOwner"
              :value="m.role"
              @change="handleRoleChange(m.user_id, $event.target.value)"
              class="text-[11px] px-2 py-1 rounded-md bg-surface-subtle border border-border-subtle text-content-secondary focus:ring-1 focus:ring-brand-default capitalize cursor-pointer"
            >
              <option value="staff">Staf</option>
              <option value="manager">Manajer</option>
              <option value="accountant">Akuntan</option>
              <option value="administrator">Admin</option>
            </select>

            <!-- Remove Button -->
            <button
              type="button"
              @click="handleRemoveMember(m.user_id, m.display_name)"
              class="p-1.5 rounded-lg text-content-muted hover:text-red-500 hover:bg-red-50 transition cursor-pointer"
              title="Keluarkan dari workspace"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      <div v-else-if="!isLoading" class="p-4 text-center text-xs text-content-muted bg-surface-subtle rounded-xl border border-border-subtle">
        Belum ada anggota tim terdaftar.
      </div>
    </div>

    <!-- Pending Invitations Deck (Owner/Admin only) -->
    <div v-if="canManageMembers && pendingInvitations.length > 0" class="space-y-2 pt-2 border-t border-border-subtle">
      <div class="text-xs font-bold text-content-muted uppercase tracking-wider px-1">
        Undangan Menunggu Konfirmasi ({{ pendingInvitations.length }})
      </div>

      <div class="divide-y divide-border-subtle border border-border-subtle rounded-xl bg-surface-card overflow-hidden">
        <div
          v-for="inv in pendingInvitations"
          :key="inv.id"
          class="p-3 flex items-center justify-between gap-3 text-xs hover:bg-surface-subtle/50 transition"
        >
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-medium text-content-primary truncate">{{ inv.email }}</span>
              <span class="text-[9px] font-bold uppercase px-1.5 py-0.2 rounded-md bg-amber-50 text-amber-700 border border-amber-200">
                {{ roleLabel(inv.role) }}
              </span>
            </div>
            <div class="text-[10px] text-content-muted mt-0.5">
              Kedaluwarsa: {{ formatExpiry(inv.expires_at) }}
            </div>
          </div>

          <div class="flex items-center gap-1.5 shrink-0">
            <button
              type="button"
              @click="copyInviteLink(getInviteUrl(inv.token))"
              class="px-2.5 py-1 rounded-md bg-surface-subtle hover:bg-surface-card border border-border-subtle text-[11px] font-semibold text-content-secondary flex items-center gap-1 cursor-pointer"
            >
              <Copy class="w-3 h-3" />
              <span>Salin</span>
            </button>
            <button
              type="button"
              @click="handleRevokeInvitation(inv.id)"
              class="p-1 rounded-md text-content-muted hover:text-red-500 hover:bg-red-50 transition cursor-pointer"
              title="Batalkan undangan"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import {
  Users, UserPlus, Mail, X, Check, Copy, AlertTriangle, Trash2, RefreshCw
} from 'lucide-vue-next'
import { api } from '@/services/api'
import { useWorkspaceStore } from '@/stores/workspace'

const workspaceStore = useWorkspaceStore()

// --- State ---
const members = ref([])
const pendingInvitations = ref([])
const isLoading = ref(false)
const isInviting = ref(false)
const showInviteForm = ref(false)
const inviteError = ref('')
const lastCreatedLink = ref('')
const isCopied = ref(false)

const inviteForm = ref({
  email: '',
  role: 'staff',
})

// --- Computed RBAC Gates ---
const isOwner = computed(() => workspaceStore.activeRole === 'owner')
const canManageMembers = computed(() =>
  workspaceStore.activeRole === 'owner' || workspaceStore.activeRole === 'administrator'
)

function roleLabel(role) {
  const map = {
    owner: 'Owner (Pemilik)',
    administrator: 'Administrator',
    manager: 'Manajer',
    staff: 'Staf',
    accountant: 'Akuntan',
  }
  return map[role] || role || 'Anggota'
}

function formatExpiry(iso) {
  if (!iso) return '-'
  return new Date(iso).toLocaleDateString('id-ID', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  })
}

function getInviteUrl(token) {
  if (typeof window === 'undefined') return `/join/${token}`
  return `${window.location.origin}/join/${token}`
}

async function loadTeamData() {
  const wsId = workspaceStore.activeTenantId
  if (!wsId) return
  isLoading.value = true
  try {
    const [memberList, inviteList] = await Promise.allSettled([
      api.getWorkspaceMembers(wsId),
      canManageMembers.value ? api.listWorkspaceInvitations(wsId) : Promise.resolve([])
    ])
    if (memberList.status === 'fulfilled') {
      members.value = memberList.value || []
    }
    if (inviteList.status === 'fulfilled') {
      pendingInvitations.value = (inviteList.value || []).filter((inv) => inv.status === 'pending')
    }
  } catch (err) {
    console.warn('Failed to load team data:', err)
  } finally {
    isLoading.value = false
  }
}

async function handleCreateInvitation() {
  inviteError.value = ''
  lastCreatedLink.value = ''
  if (!inviteForm.value.email.trim()) {
    inviteError.value = 'Email calon anggota wajib diisi.'
    return
  }
  isInviting.value = true
  try {
    const wsId = workspaceStore.activeTenantId
    const res = await api.createWorkspaceInvitation(wsId, {
      email: inviteForm.value.email.trim(),
      role: inviteForm.value.role,
    })
    lastCreatedLink.value = getInviteUrl(res.token)
    inviteForm.value.email = ''
    await loadTeamData()
  } catch (err) {
    inviteError.value = err?.detail || err?.message || 'Gagal membuat undangan.'
  } finally {
    isInviting.value = false
  }
}

async function handleRevokeInvitation(invitationId) {
  if (!confirm('Batalkan link undangan ini?')) return
  try {
    const wsId = workspaceStore.activeTenantId
    await api.revokeWorkspaceInvitation(wsId, invitationId)
    await loadTeamData()
  } catch (err) {
    alert(err?.detail || err?.message || 'Gagal membatalkan undangan.')
  }
}

async function handleRoleChange(userId, newRole) {
  try {
    const wsId = workspaceStore.activeTenantId
    await api.updateWorkspaceMemberRole(wsId, userId, newRole)
    await loadTeamData()
  } catch (err) {
    alert(err?.detail || err?.message || 'Gagal mengubah peran anggota.')
  }
}

async function handleRemoveMember(userId, name) {
  if (!confirm(`Keluarkan ${name || 'anggota'} dari workspace ini?`)) return
  try {
    const wsId = workspaceStore.activeTenantId
    await api.removeWorkspaceMember(wsId, userId)
    await loadTeamData()
  } catch (err) {
    alert(err?.detail || err?.message || 'Gagal mengeluarkan anggota.')
  }
}

async function copyInviteLink(link) {
  try {
    if (navigator?.clipboard?.writeText) {
      await navigator.clipboard.writeText(link)
    } else {
      const el = document.createElement('textarea')
      el.value = link
      document.body.appendChild(el)
      el.select()
      document.execCommand('copy')
      document.body.removeChild(el)
    }
    isCopied.value = true
    setTimeout(() => {
      isCopied.value = false
    }, 2000)
  } catch {}
}

onMounted(() => {
  loadTeamData()
})
</script>
