<template>
  <nav class="md:hidden fixed bottom-0 inset-x-0 z-40 bg-surface-card/95 backdrop-blur-lg border-t border-border-subtle px-3 pt-1.5 pb-safe shadow-floating select-none">
    <div class="flex items-center justify-around max-w-lg mx-auto">
      <!-- Tab 1: Home -->
      <button
        type="button"
        @click="$emit('select-tab', 'home')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition focus-ring cursor-pointer',
          activeTab === 'home' ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <LayoutDashboard class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">Home</span>
      </button>

      <!-- Tab 2: Transactions -->
      <button
        type="button"
        @click="$emit('select-tab', 'transactions')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition focus-ring cursor-pointer',
          activeTab === 'transactions' ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <Receipt class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">Transaksi</span>
      </button>

      <!-- Center Floating Action: Plus -->
      <button
        type="button"
        @click="$emit('open-add')"
        class="-mt-5 w-12 h-12 rounded-2xl bg-brand-default text-content-inverse flex items-center justify-center shadow-lg shadow-brand-default/40 active:scale-95 transition focus-ring cursor-pointer hover:bg-brand-emphasis"
        aria-label="Catat Transaksi Baru"
        title="Catat Transaksi Baru"
      >
        <Plus class="w-6 h-6 stroke-[2.75]" aria-hidden="true" />
      </button>

      <!-- Tab 4: Analytics -->
      <button
        type="button"
        @click="$emit('select-tab', 'analytics')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition focus-ring cursor-pointer',
          activeTab === 'analytics' ? 'text-brand-default font-bold' : 'text-content-muted hover:text-content-primary'
        ]"
      >
        <PieChart class="w-5 h-5 stroke-[2]" aria-hidden="true" />
        <span class="text-[10px] mt-0.5 font-medium">Analitik</span>
      </button>

      <!-- Tab 5: Profile -->
      <button
        type="button"
        @click="$emit('select-tab', 'profile')"
        :class="[
          'flex flex-col items-center justify-center py-1 px-2.5 rounded-xl min-w-[56px] min-h-[44px] transition focus-ring cursor-pointer',
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
import {
  LayoutDashboard,
  Receipt,
  Plus,
  PieChart,
  User
} from 'lucide-vue-next'

defineProps({
  activeTab: {
    type: String,
    default: 'home'
  }
})

defineEmits(['select-tab', 'open-add'])
</script>
