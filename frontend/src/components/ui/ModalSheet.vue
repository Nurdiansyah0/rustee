<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition-opacity duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="active"
        class="fixed inset-0 z-50 flex items-end sm:items-center justify-center p-0 sm:p-4 overflow-hidden"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="title ? titleId : undefined"
        :aria-describedby="description ? descId : undefined"
      >
        <!-- Backdrop Scrim -->
        <div
          class="fixed inset-0 bg-black/60 backdrop-blur-sm transition-opacity"
          aria-hidden="true"
          @click="handleBackdropClick"
        />

        <!-- Panel Container (Bottom Sheet on Mobile, Centered Dialog on Desktop) -->
        <div
          ref="modalPanelRef"
          :class="[
            'relative w-full z-10 flex flex-col bg-surface-card border border-border-subtle shadow-modal',
            'transition-all transform max-h-[92vh] sm:max-h-[85vh]',
            // Mobile: Bottom Sheet
            'rounded-t-2xl sm:rounded-2xl',
            'pb-safe',
            // Desktop: Sizing
            sizeClass,
          ]"
          :style="sheetTranslateStyle"
          tabindex="-1"
          @keydown="handleKeydown"
        >
          <!-- Mobile Drag Handle Bar -->
          <div
            class="sm:hidden pt-3 pb-1 cursor-grab active:cursor-grabbing flex flex-col items-center select-none touch-none"
            @touchstart="handleTouchStart"
            @touchmove="handleTouchMove"
            @touchend="handleTouchEnd"
          >
            <div class="w-12 h-1.5 bg-border-strong rounded-full shrink-0" />
          </div>

          <!-- Header Section -->
          <div
            v-if="title || $slots.header || showCloseButton"
            class="px-5 py-4 border-b border-border-subtle flex items-start justify-between shrink-0"
          >
            <slot name="header">
              <div class="flex flex-col pr-6">
                <h3
                  v-if="title"
                  :id="titleId"
                  class="text-base sm:text-lg font-bold text-content-primary tracking-tight"
                >
                  {{ title }}
                </h3>
                <p
                  v-if="description"
                  :id="descId"
                  class="text-xs sm:text-sm text-content-secondary mt-0.5"
                >
                  {{ description }}
                </p>
              </div>
            </slot>

            <!-- Close Button -->
            <button
              v-if="showCloseButton"
              type="button"
              class="p-1.5 -mr-1.5 -mt-1 text-content-muted hover:text-content-primary rounded-lg focus-ring shrink-0"
              aria-label="Tutup dialog"
              title="Tutup dialog"
              @click="closeModal"
            >
              <X class="w-5 h-5 stroke-2" aria-hidden="true" />
            </button>
          </div>

          <!-- Scrollable Body Content -->
          <div class="flex-1 overflow-y-auto px-5 py-4 scrollbar-none">
            <slot />
          </div>

          <!-- Footer Actions -->
          <div
            v-if="$slots.footer"
            class="px-5 py-3.5 bg-surface-subtle/60 border-t border-border-subtle flex items-center justify-end gap-2.5 shrink-0 rounded-b-2xl"
          >
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { X } from 'lucide-vue-next'

export interface ModalSheetProps {
  modelValue?: boolean
  isOpen?: boolean
  title?: string
  description?: string
  size?: 'sm' | 'md' | 'lg' | 'full'
  showCloseButton?: boolean
  closeOnBackdrop?: boolean
  closeOnEscape?: boolean
}

const props = withDefaults(defineProps<ModalSheetProps>(), {
  modelValue: undefined,
  isOpen: undefined,
  title: undefined,
  description: undefined,
  size: 'md',
  showCloseButton: true,
  closeOnBackdrop: true,
  closeOnEscape: true,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'update:isOpen', value: boolean): void
  (e: 'close'): void
}>()

const modalPanelRef = ref<HTMLElement | null>(null)
let previouslyFocusedElement: HTMLElement | null = null

// Responsive touch drag tracking
const touchStartY = ref(0)
const currentDeltaY = ref(0)
const isDragging = ref(false)

const active = computed(() => {
  if (props.isOpen !== undefined) return props.isOpen
  if (props.modelValue !== undefined) return props.modelValue
  return false
})

const titleId = `modal-title-${Math.random().toString(36).substring(2, 9)}`
const descId = `modal-desc-${Math.random().toString(36).substring(2, 9)}`

const sizeClass = computed(() => {
  const map: Record<string, string> = {
    sm: 'sm:max-w-sm',
    md: 'sm:max-w-md',
    lg: 'sm:max-w-lg',
    full: 'sm:max-w-2xl',
  }
  return map[props.size] || map.md
})

const sheetTranslateStyle = computed(() => {
  if (isDragging.value && currentDeltaY.value > 0) {
    return {
      transform: `translateY(${currentDeltaY.value}px)`,
      transition: 'none',
    }
  }
  return undefined
})

function closeModal() {
  emit('update:modelValue', false)
  emit('update:isOpen', false)
  emit('close')
}

function handleBackdropClick() {
  if (props.closeOnBackdrop) {
    closeModal()
  }
}

// Touch swipe-down handler
function handleTouchStart(e: TouchEvent) {
  touchStartY.value = e.touches[0].clientY
  isDragging.value = true
  currentDeltaY.value = 0
}

function handleTouchMove(e: TouchEvent) {
  if (!isDragging.value) return
  const currentY = e.touches[0].clientY
  const delta = currentY - touchStartY.value
  if (delta > 0) {
    currentDeltaY.value = delta
  }
}

function handleTouchEnd() {
  if (!isDragging.value) return
  if (currentDeltaY.value > 75) {
    closeModal()
  }
  isDragging.value = false
  currentDeltaY.value = 0
}

// Escape key handling & focus trapping
function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && props.closeOnEscape) {
    closeModal()
    return
  }

  if (event.key === 'Tab' && modalPanelRef.value) {
    const focusable = modalPanelRef.value.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
    if (focusable.length === 0) return

    const first = focusable[0]
    const last = focusable[focusable.length - 1]

    if (event.shiftKey && document.activeElement === first) {
      last.focus()
      event.preventDefault()
    } else if (!event.shiftKey && document.activeElement === last) {
      first.focus()
      event.preventDefault()
    }
  }
}

function onGlobalKeydown(e: KeyboardEvent) {
  if (active.value && e.key === 'Escape' && props.closeOnEscape) {
    closeModal()
  }
}

// Scroll lock & focus management
watch(
  active,
  async (isOpen) => {
    if (typeof document === 'undefined') return

    if (isOpen) {
      previouslyFocusedElement = document.activeElement as HTMLElement
      document.body.style.overflow = 'hidden'
      window.addEventListener('keydown', onGlobalKeydown)

      await nextTick()
      if (modalPanelRef.value) {
        // Focus first input or button inside panel, or panel itself
        const firstInput = modalPanelRef.value.querySelector<HTMLElement>(
          'input:not([disabled]), button:not([disabled]), [tabindex="0"]'
        )
        if (firstInput) {
          firstInput.focus()
        } else {
          modalPanelRef.value.focus()
        }
      }
    } else {
      document.body.style.overflow = ''
      window.removeEventListener('keydown', onGlobalKeydown)
      if (previouslyFocusedElement) {
        previouslyFocusedElement.focus()
        previouslyFocusedElement = null
      }
    }
  },
  { immediate: true }
)

onUnmounted(() => {
  if (typeof document !== 'undefined') {
    document.body.style.overflow = ''
    window.removeEventListener('keydown', onGlobalKeydown)
  }
})
</script>
