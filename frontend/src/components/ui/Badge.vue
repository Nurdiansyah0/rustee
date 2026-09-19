<template>
  <span :class="classes">
    <!-- Optional Dot Indicator -->
    <span
      v-if="dot"
      :class="['w-1.5 h-1.5 rounded-full shrink-0', dotClass]"
      :style="dotStyle"
      aria-hidden="true"
    />

    <!-- Optional Icon Slot -->
    <span v-if="$slots.icon" class="inline-flex shrink-0 items-center" aria-hidden="true">
      <slot name="icon" />
    </span>

    <!-- Text / Default Slot -->
    <span class="truncate">
      <slot />
    </span>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export type BadgeVariant =
  | 'default'
  | 'income'
  | 'surplus'
  | 'expense'
  | 'deficit'
  | 'pending'
  | 'active'
  | 'warning'
  | 'transfer'
  | 'brand'
  | 'free'
  | 'premium'
  | 'neutral'

export interface BadgeProps {
  variant?: BadgeVariant
  size?: 'sm' | 'md'
  dot?: boolean
  dotColor?: string
}

const props = withDefaults(defineProps<BadgeProps>(), {
  variant: 'default',
  size: 'md',
  dot: false,
  dotColor: undefined,
})

const normalizedVariant = computed(() => {
  switch (props.variant) {
    case 'surplus':
    case 'active':
      return 'income'
    case 'deficit':
      return 'expense'
    case 'pending':
      return 'warning'
    case 'free':
    case 'neutral':
      return 'default'
    default:
      return props.variant
  }
})

const classes = computed(() => {
  const base = [
    'inline-flex items-center font-semibold rounded-md border transition-colors select-none tabular-nums',
  ]

  // Size
  if (props.size === 'sm') {
    base.push('text-[10px] px-1.5 py-0.5 gap-1')
  } else {
    base.push('text-[11px] px-2 py-0.5 gap-1.5')
  }

  // Variant
  const variantMap: Record<string, string> = {
    income: 'bg-income-muted text-income-default border-income-border',
    expense: 'bg-expense-muted text-expense-default border-expense-border',
    warning: 'bg-warning-muted text-warning-default border-warning-border',
    transfer: 'bg-transfer-muted text-transfer-default border-transfer-border',
    brand: 'bg-brand-muted text-brand-default border-brand-border',
    premium: 'bg-brand-default/10 text-brand-default border-brand-border font-bold',
    default: 'bg-surface-subtle text-content-secondary border-border-subtle',
  }

  base.push(variantMap[normalizedVariant.value] || variantMap.default)

  return base.join(' ')
})

const dotClass = computed(() => {
  if (props.dotColor) return ''
  const dotMap: Record<string, string> = {
    income: 'bg-income-default',
    expense: 'bg-expense-default',
    warning: 'bg-warning-default',
    transfer: 'bg-transfer-default',
    brand: 'bg-brand-default',
    premium: 'bg-brand-default',
    default: 'bg-content-muted',
  }
  return dotMap[normalizedVariant.value] || dotMap.default
})

const dotStyle = computed(() => {
  return props.dotColor ? { backgroundColor: props.dotColor } : undefined
})
</script>
