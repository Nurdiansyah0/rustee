<template>
  <nav class="lg:hidden fixed bottom-0 inset-x-0 z-40 bg-surface-card/95 backdrop-blur-lg border-t border-border-subtle px-3 pt-1.5 pb-[max(env(safe-area-inset-bottom),0.5rem)] shadow-floating select-none">
    <div class="flex items-center justify-around max-w-lg mx-auto">
      <!-- Slot 1: Home Tab -->
      <button
        type="button"
        @click="$emit('select-tab', 'home')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition active:scale-90 focus-ring cursor-pointer',
          activeTab === 'home' ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <LayoutDashboard class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">Home</span>
      </button>

      <!-- Slot 2: Transactions Tab -->
      <button
        type="button"
        @click="$emit('select-tab', 'transactions')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition active:scale-90 focus-ring cursor-pointer',
          activeTab === 'transactions' ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <Receipt class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">Transaksi</span>
      </button>

      <!-- Slot 3: STRICTLY PRESERVED Rapid POS Keypad Action Button -->
      <!-- Emits open-add to trigger AddTransactionModal.vue (Rapid 4x3 POS Numeric Keypad) -->
      <button
        type="button"
        @click="$emit('open-add')"
        class="-mt-5 w-12 h-12 rounded-2xl bg-brand-default text-content-inverse flex items-center justify-center shadow-lg shadow-brand-default/40 active:scale-90 transition focus-ring cursor-pointer hover:bg-brand-emphasis"
        aria-label="Catat Transaksi Baru"
        title="Catat Transaksi Baru (POS Keypad)"
      >
        <Plus class="w-6 h-6 stroke-[2.75]" aria-hidden="true" />
      </button>

      <!-- Slot 4: Capability-Driven Dynamic Tab (Invoices / Accounting / Analytics) -->
      <button
        type="button"
        @click="$emit('select-tab', slot4Tab.id)"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition active:scale-90 focus-ring cursor-pointer',
          activeTab === slot4Tab.id ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <component :is="slot4Tab.icon" class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">{{ slot4Tab.label }}</span>
      </button>

      <!-- Slot 5: Profile Tab -->
      <button
        type="button"
        @click="$emit('select-tab', 'profile')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition active:scale-90 focus-ring cursor-pointer',
          activeTab === 'profile' ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <User class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">Profil</span>
      </button>
    </div>
  </nav>
</template>

<script setup>
import { computed } from 'vue'
import { useWorkspaceStore } from '@/stores/workspace'
import {
  LayoutDashboard,
  Receipt,
  Plus,
  PieChart,
  User,
  FileText,
  BookOpen
} from 'lucide-vue-next'

const props = defineProps({
  activeTab: {
    type: String,
    default: 'home'
  }
})

defineEmits(['select-tab', 'open-add'])

const workspaceStore = useWorkspaceStore()

// Slot 4 dynamically adapts: Invoicing/Faktur for business, Analitik for personal
const slot4Tab = computed(() => {
  if (props.activeTab === 'analytics') {
    return { id: 'analytics', label: 'Analitik', icon: PieChart }
  }
  if (workspaceStore.hasCapability('invoicing')) {
    return { id: 'invoices', label: 'Faktur', icon: FileText }
  }
  if (workspaceStore.hasCapability('accounting')) {
    return { id: 'accounting', label: 'Buku Kas', icon: BookOpen }
  }
  return { id: 'analytics', label: 'Analitik', icon: PieChart }
})
</script>
