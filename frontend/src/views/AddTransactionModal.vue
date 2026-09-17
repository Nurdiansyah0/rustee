<template>
  <AddTransactionModal
    :is-open="isOpen"
    :accounts="accounts"
    :categories="categories"
    @close="$emit('close')"
    @transaction-created="$emit('transaction-created', $event)"
  />
</template>

<script setup>
import AddTransactionModal from '@/components/AddTransactionModal.vue'

defineProps({
  isOpen: Boolean,
  accounts: {
    type: Array,
    default: () => []
  },
  categories: {
    type: Array,
    default: () => []
  }
})

defineEmits(['close', 'transaction-created'])
</script>
