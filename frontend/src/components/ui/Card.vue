<template>
  <component
    :is="as"
    :class="classes"
    :tabindex="interactive ? 0 : undefined"
    :role="interactive ? 'button' : undefined"
    @click="handleClick"
    @keydown.enter.prevent="handleKeydown"
    @keydown.space.prevent="handleKeydown"
  >
    <!-- Card Header Slot -->
    <header
      v-if="$slots.header"
      :class="[
        'border-b border-border-subtle flex items-center justify-between',
        headerPaddingClass,
      ]"
    >
      <slot name="header" />
    </header>

    <!-- Main Card Body Slot -->
    <div :class="bodyPaddingClass">
      <slot />
    </div>

    <!-- Card Footer Slot -->
    <footer
      v-if="$slots.footer"
      :class="[
        'border-t border-border-subtle bg-surface-subtle/50 flex items-center justify-between',
        footerPaddingClass,
      ]"
    >
      <slot name="footer" />
    </footer>
  </component>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export interface CardProps {
  as?: string
  padding?: 'none' | 'sm' | 'md' | 'lg'
  interactive?: boolean
  variant?: 'default' | 'elevated' | 'subtle' | 'sunken' | 'bordered'
}

const props = withDefaults(defineProps<CardProps>(), {
  as: 'div',
  padding: 'md',
  interactive: false,
  variant: 'default',
})

const emit = defineEmits<{
  (e: 'click', event: MouseEvent | KeyboardEvent): void
}>()

function handleClick(event: MouseEvent) {
  if (props.interactive) {
    emit('click', event)
  }
}

function handleKeydown(event: KeyboardEvent) {
  if (props.interactive) {
    emit('click', event)
  }
}

const classes = computed(() => {
  const base = [
    'rounded-xl border transition-all duration-150 relative overflow-hidden',
  ]

  // Surface and border variants
  const variantMap: Record<string, string> = {
    default: 'bg-surface-card border-border-subtle shadow-card',
    elevated: 'bg-surface-elevated border-border-subtle shadow-dropdown',
    subtle: 'bg-surface-subtle border-border-subtle shadow-subtle',
    sunken: 'bg-surface-sunken border-border-subtle shadow-inner',
    bordered: 'bg-surface-card border-border-default shadow-card',
  }
  base.push(variantMap[props.variant] || variantMap.default)

  if (props.interactive) {
    base.push(
      'cursor-pointer hover:border-border-default hover:shadow-subtle active:scale-[0.99] focus-ring select-none'
    )
  }

  return base.join(' ')
})

const bodyPaddingClass = computed(() => {
  const map: Record<string, string> = {
    none: 'p-0',
    sm: 'p-3.5 sm:p-4',
    md: 'p-4 sm:p-5',
    lg: 'p-5 sm:p-6',
  }
  return map[props.padding] || map.md
})

const headerPaddingClass = computed(() => {
  const map: Record<string, string> = {
    none: 'p-0',
    sm: 'px-3.5 py-2.5 sm:px-4 sm:py-3',
    md: 'px-4 py-3 sm:px-5 sm:py-3.5',
    lg: 'px-5 py-3.5 sm:px-6 sm:py-4',
  }
  return map[props.padding] || map.md
})

const footerPaddingClass = computed(() => {
  const map: Record<string, string> = {
    none: 'p-0',
    sm: 'px-3.5 py-2.5 sm:px-4 sm:py-3',
    md: 'px-4 py-3 sm:px-5 sm:py-3.5',
    lg: 'px-5 py-3.5 sm:px-6 sm:py-4',
  }
  return map[props.padding] || map.md
})
</script>
