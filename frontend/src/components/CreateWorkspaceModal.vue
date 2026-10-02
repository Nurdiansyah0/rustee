<template>
  <div
    v-if="isOpen"
    @click.self="$emit('close')"
    class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    aria-labelledby="create-workspace-modal-title"
  >
    <div
      ref="modalRef"
      class="relative w-full max-w-lg p-5 sm:p-6 max-h-[90dvh] overflow-y-auto bg-surface-card rounded-2xl shadow-2xl border border-border-subtle animate-in fade-in zoom-in-95 duration-200 scroll-native space-y-5"
    >
      <!-- Close button -->
      <button
        type="button"
        @click="$emit('close')"
        class="absolute top-4 right-4 p-2 text-content-muted hover:text-content-primary rounded-full hover:bg-surface-subtle transition cursor-pointer z-10"
        aria-label="Tutup modal"
      >
        <X class="w-5 h-5 stroke-[2]" />
      </button>

      <!-- Modal Header -->
      <div class="flex items-center gap-3">
        <div class="w-11 h-11 rounded-xl bg-brand-muted text-brand-default flex items-center justify-center shrink-0 border border-brand-border">
          <Building2 class="w-6 h-6 stroke-[2]" />
        </div>
        <div>
          <h2 id="create-workspace-modal-title" class="text-lg font-black text-content-primary tracking-tight">
            Buat Ruang Kerja Bisnis Baru
          </h2>
          <p class="text-xs text-content-secondary mt-0.5">
            Kelola operasional dan keuangan usaha Anda dalam workspace terpisah (PRD §5).
          </p>
        </div>
      </div>

      <!-- Error Alert -->
      <div
        v-if="errorMessage"
        class="p-3 rounded-xl bg-expense-muted/60 border border-expense-border text-xs text-expense-default flex items-center gap-2"
      >
        <AlertCircle class="w-4 h-4 shrink-0" />
        <span>{{ errorMessage }}</span>
      </div>

      <!-- Form -->
      <form @submit.prevent="handleSubmit" class="space-y-4">
        <!-- 1. Workspace Name -->
        <div class="space-y-1.5">
          <label for="ws-name" class="block text-xs font-bold text-content-primary">
            Nama Usaha / Ruang Kerja <span class="text-brand-default">*</span>
          </label>
          <input
            id="ws-name"
            v-model="form.name"
            type="text"
            required
            placeholder="Contoh: Toko Berkah, Bengkel Maju, CV Mandiri"
            class="w-full h-10 px-3 rounded-xl bg-surface-subtle border border-border-subtle hover:border-border-default focus:border-brand-default focus:bg-surface-card text-xs text-content-primary placeholder:text-content-muted transition outline-hidden focus-ring"
          />
        </div>

        <!-- 2. Business Type Selector (PRD §2 & §8) -->
        <div class="space-y-2">
          <label class="block text-xs font-bold text-content-primary">
            Jenis Usaha (Business Type) <span class="text-brand-default">*</span>
          </label>
          <p class="text-[11px] text-content-muted leading-tight">
            Pilih cara kerja bisnis Anda. Modul dan alur kerja akan otomatis disesuaikan.
          </p>

          <div class="grid grid-cols-2 gap-2 pt-1">
            <button
              v-for="b in businessTypes"
              :key="b.id"
              type="button"
              @click="form.business_type = b.id"
              :class="[
                'p-2.5 rounded-xl border text-left transition cursor-pointer flex flex-col justify-between focus-ring',
                form.business_type === b.id
                  ? 'border-brand-default bg-brand-muted/40 shadow-xs'
                  : 'border-border-subtle bg-surface-subtle hover:bg-surface-sunken hover:border-border-default'
              ]"
            >
              <div class="flex items-center justify-between w-full mb-1">
                <component :is="b.icon" class="w-4 h-4 text-brand-default shrink-0" />
                <span
                  v-if="form.business_type === b.id"
                  class="w-3.5 h-3.5 rounded-full bg-brand-default text-white flex items-center justify-center text-[9px] font-bold"
                >
                  ✓
                </span>
              </div>
              <div>
                <div class="text-xs font-bold text-content-primary">{{ b.name }}</div>
                <div class="text-[10px] text-content-muted mt-0.5 line-clamp-1">{{ b.desc }}</div>
              </div>
            </button>
          </div>
        </div>

        <!-- 3. Timezone Selector (PRD §7) -->
        <div class="space-y-1.5">
          <label for="ws-timezone" class="block text-xs font-bold text-content-primary">
            Zona Waktu Operasional
          </label>
          <select
            id="ws-timezone"
            v-model="form.timezone"
            class="w-full h-10 px-3 rounded-xl bg-surface-subtle border border-border-subtle hover:border-border-default focus:border-brand-default focus:bg-surface-card text-xs text-content-primary transition outline-hidden focus-ring cursor-pointer"
          >
            <option value="Asia/Jakarta">WIB — Asia/Jakarta (Waktu Indonesia Barat)</option>
            <option value="Asia/Makassar">WITA — Asia/Makassar (Waktu Indonesia Tengah)</option>
            <option value="Asia/Jayapura">WIT — Asia/Jayapura (Waktu Indonesia Timur)</option>
          </select>
        </div>

        <!-- Bottom Notice -->
        <div class="p-3 rounded-xl bg-surface-subtle border border-border-subtle text-[11px] text-content-secondary space-y-1">
          <div class="font-semibold text-content-primary flex items-center gap-1.5">
            <ShieldCheck class="w-3.5 h-3.5 text-brand-default" />
            <span>Multi-Tenant & Double-Entry Otomatis</span>
          </div>
          <p>
            Mata uang default: <strong>IDR (Rupiah)</strong>. Bagan Akun (CoA) 8 akun standar dan jurnal pembukuan akan disiapkan otomatis untuk ruang kerja ini.
          </p>
        </div>

        <!-- Actions -->
        <div class="flex items-center justify-end gap-2.5 pt-2">
          <Button
            type="button"
            variant="ghost"
            size="md"
            @click="$emit('close')"
            :disabled="isSubmitting"
          >
            Batal
          </Button>

          <Button
            type="submit"
            variant="primary"
            size="md"
            :loading="isSubmitting"
            :disabled="!form.name.trim() || isSubmitting"
          >
            <template #prefix>
              <Plus class="w-4 h-4 mr-1 stroke-[2.5]" />
            </template>
            Buat Ruang Kerja
          </Button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, markRaw } from 'vue'
import { Button } from '@/components/ui'
import { useWorkspaceStore } from '@/stores/workspace'
import {
  X,
  Building2,
  AlertCircle,
  Plus,
  ShieldCheck,
  ShoppingBag,
  Wrench,
  Car,
  Hammer,
  Coffee,
  Briefcase
} from 'lucide-vue-next'

const props = defineProps({
  isOpen: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['close', 'created'])

const workspaceStore = useWorkspaceStore()
const isSubmitting = ref(false)
const errorMessage = ref('')

const form = reactive({
  name: '',
  business_type: 'general',
  timezone: 'Asia/Jakarta',
  currency: 'IDR'
})

const businessTypes = [
  { id: 'general', name: 'Bisnis Umum / Jasa', desc: 'Faktur, Piutang, Kas & Laporan', icon: markRaw(Briefcase) },
  { id: 'retail', name: 'Ritel / Toko / Warung', desc: 'Kasir POS, Stok, Penjualan', icon: markRaw(ShoppingBag) },
  { id: 'bengkel', name: 'Bengkel / Servis', desc: 'Jasa servis, suku cadang & invoice', icon: markRaw(Wrench) },
  { id: 'rental', name: 'Rental & Sewa', desc: 'Booking, jadwal rental & invoice', icon: markRaw(Car) },
  { id: 'contractor', name: 'Kontraktor / Proyek', desc: 'Progress, milestone & piutang', icon: markRaw(Hammer) },
  { id: 'fnb', name: 'Kafe & Restoran', desc: 'Meja, pesanan & buku kas harian', icon: markRaw(Coffee) }
]

async function handleSubmit() {
  if (!form.name.trim()) return

  isSubmitting.value = true
  errorMessage.value = ''

  try {
    const payload = {
      name: form.name.trim(),
      business_type: form.business_type,
      timezone: form.timezone,
      currency: form.currency
    }

    const created = await workspaceStore.createWorkspace(payload)
    
    // Reset form
    form.name = ''
    form.business_type = 'general'
    form.timezone = 'Asia/Jakarta'

    emit('created', created)
    emit('close')
  } catch (err) {
    errorMessage.value = err.detail || err.message || 'Gagal membuat ruang kerja baru.'
  } finally {
    isSubmitting.value = false
  }
}
</script>
