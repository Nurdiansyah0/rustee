<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :aria-busy="loading ? 'true' : undefined"
    :aria-label="ariaLabel"
    :class="classes"
    @click="handleClick"
  >
    <!-- Loading Spinner -->
    <svg
      v-if="loading"
      class="animate-spin shrink-0 -ml-0.5 mr-2 h-4 w-4"
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
      aria-hidden="true"
    >
      <circle
        class="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        stroke-width="4"
      />
      <path
        class="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>

    <!-- Prefix Icon Slot (when not loading or icon should remain) -->
    <span v-if="$slots.prefix && !loading" class="inline-flex shrink-0 items-center">
      <slot name="prefix" />
    </span>

    <!-- Text / Default Slot Content -->
    <span v-if="loading && loadingText" class="truncate">
      {{ loadingText }}
    </span>
    <span v-else class="truncate inline-flex items-center gap-1.5">
      <slot />
    </span>

    <!-- Suffix Icon Slot -->
    <span v-if="$slots.suffix" class="inline-flex shrink-0 items-center">
      <slot name="suffix" />
    </span>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export interface ButtonProps {
  variant?: 'primary' | 'secondary' | 'outline' | 'ghost' | 'destructive'
  size?: 'sm' | 'md' | 'lg' | 'icon'
  type?: 'button' | 'submit' | 'reset'
  disabled?: boolean
  loading?: boolean
  loadingText?: string
  fullWidth?: boolean
  mobileFullWidth?: boolean
  ariaLabel?: string
}

const props = withDefaults(defineProps<ButtonProps>(), {
  variant: 'primary',
  size: 'md',
  type: 'button',
  disabled: false,
  loading: false,
  loadingText: undefined,
  fullWidth: false,
  mobileFullWidth: false,
  ariaLabel: undefined,
})

const emit = defineEmits<{
  (e: 'click', event: MouseEvent): void
}>()

function handleClick(event: MouseEvent) {
  if (!props.disabled && !props.loading) {
    emit('click', event)
  }
}

const classes = computed(() => {
  const base = [
    'inline-flex items-center justify-center font-semibold rounded-lg',
    'transition-all duration-150 select-none cursor-pointer',
    'focus-ring active:scale-[0.98] motion-reduce:transform-none',
    'disabled:opacity-50 disabled:cursor-not-allowed disabled:pointer-events-none disabled:active:scale-100',
  ]

  // Sizing
  const sizeMap: Record<string, string> = {
    sm: 'h-9 px-3 text-xs gap-1.5 min-h-[36px]',
    md: 'h-11 px-4 text-sm gap-2 min-h-[44px]',
    lg: 'h-13 px-5 text-base gap-2.5 min-h-[52px]',
    icon: 'w-11 h-11 p-2.5 min-w-[44px] min-h-[44px]',
  }
  base.push(sizeMap[props.size] || sizeMap.md)

  // Variants
  const variantMap: Record<string, string> = {
    primary:
      'bg-brand-default text-content-inverse hover:bg-brand-emphasis active:bg-brand-emphasis shadow-sm',
    secondary:
      'bg-surface-subtle text-content-primary hover:bg-border-default active:bg-border-strong',
    outline:
      'bg-surface-card text-content-primary border border-border-default hover:bg-surface-subtle active:bg-surface-subtle',
    ghost:
      'bg-transparent text-content-secondary hover:text-content-primary hover:bg-surface-subtle active:bg-surface-subtle',
    destructive:
      'bg-expense-default text-content-inverse hover:bg-expense-emphasis active:bg-expense-emphasis shadow-sm',
  }
  base.push(variantMap[props.variant] || variantMap.primary)

  // Width
  if (props.fullWidth) {
    base.push('w-full')
  } else if (props.mobileFullWidth) {
    base.push('w-full sm:w-auto')
  }

  return base.join(' ')
})
</script>
