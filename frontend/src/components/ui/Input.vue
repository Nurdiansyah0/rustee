<template>
  <div class="w-full flex flex-col gap-1.5">
    <!-- Optional Accessible Label -->
    <label
      v-if="label"
      :for="inputId"
      class="text-xs font-semibold text-content-secondary select-none flex items-center justify-between"
    >
      <span>
        {{ label }}
        <span v-if="required" class="text-expense-default ml-0.5" aria-hidden="true">*</span>
      </span>
      <span v-if="$slots.labelExtra" class="font-normal text-content-muted">
        <slot name="labelExtra" />
      </span>
    </label>

    <!-- Input Group Container -->
    <div
      :class="[
        'group relative flex items-center w-full rounded-lg transition-all duration-150',
        'bg-surface-sunken border',
        errorMessage
          ? 'border-expense-default focus-within:ring-2 focus-within:ring-expense-default/30 focus-within:border-expense-default'
          : 'border-border-default focus-within:ring-2 focus-within:ring-brand-default/40 focus-within:border-brand-default',
        disabled ? 'opacity-50 cursor-not-allowed bg-surface-subtle' : '',
      ]"
    >
      <!-- Leading Icon / Slot -->
      <div v-if="$slots.leading" class="pl-3 flex items-center pointer-events-none text-content-muted">
        <slot name="leading" />
      </div>

      <!-- Prefix / IDR Adornment -->
      <div
        v-if="prefix || $slots.prefix"
        class="px-3 py-2 bg-surface-subtle border-r border-border-subtle text-content-secondary font-bold text-xs select-none flex items-center shrink-0 rounded-l-lg self-stretch"
      >
        <slot name="prefix">{{ prefix }}</slot>
      </div>

      <!-- Main HTML Input -->
      <input
        :id="inputId"
        ref="inputRef"
        :name="name"
        :type="actualType"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :readonly="readonly"
        :required="required"
        :autofocus="autofocus"
        :autocomplete="autocomplete"
        :spellcheck="spellcheck"
        :inputmode="inputmode"
        :min="min"
        :max="max"
        :step="step"
        :aria-invalid="errorMessage ? 'true' : undefined"
        :aria-describedby="describedByIds"
        class="w-full bg-transparent px-3.5 py-2.5 text-sm text-content-primary placeholder:text-content-muted focus:outline-none min-h-[44px] tabular-nums"
        @input="handleInput"
        @focus="emit('focus', $event)"
        @blur="emit('blur', $event)"
      />

      <!-- Clear Button -->
      <button
        v-if="clearable && (modelValue !== '' && modelValue !== null && modelValue !== undefined) && !disabled && !readonly"
        type="button"
        class="p-1.5 mr-1 text-content-muted hover:text-content-primary rounded-md focus-ring"
        aria-label="Hapus teks"
        title="Hapus teks"
        @click="handleClear"
      >
        <X class="w-4 h-4 stroke-2" aria-hidden="true" />
      </button>

      <!-- Password Visibility Toggle -->
      <button
        v-if="showPasswordToggle && type === 'password' && !disabled"
        type="button"
        class="p-1.5 mr-1.5 text-content-muted hover:text-content-primary rounded-md focus-ring"
        :aria-label="isPasswordVisible ? 'Sembunyikan kata sandi' : 'Tampilkan kata sandi'"
        :title="isPasswordVisible ? 'Sembunyikan kata sandi' : 'Tampilkan kata sandi'"
        @click="isPasswordVisible = !isPasswordVisible"
      >
        <EyeOff v-if="isPasswordVisible" class="w-4 h-4 stroke-2" aria-hidden="true" />
        <Eye v-else class="w-4 h-4 stroke-2" aria-hidden="true" />
      </button>

      <!-- Suffix / Currency Code Adornment -->
      <div
        v-if="suffix || $slots.suffix"
        class="px-3 py-2 bg-surface-subtle border-l border-border-subtle text-content-muted font-medium text-xs select-none flex items-center shrink-0 rounded-r-lg self-stretch"
      >
        <slot name="suffix">{{ suffix }}</slot>
      </div>

      <!-- Trailing Icon / Slot -->
      <div v-if="$slots.trailing" class="pr-3 flex items-center text-content-muted">
        <slot name="trailing" />
      </div>
    </div>

    <!-- Error Message -->
    <p
      v-if="errorMessage"
      :id="errorId"
      class="text-xs text-expense-default flex items-center gap-1 mt-0.5"
      aria-live="polite"
    >
      <AlertCircle class="w-3.5 h-3.5 stroke-2 shrink-0" aria-hidden="true" />
      <span>{{ errorMessage }}</span>
    </p>

    <!-- Helper Text (when no error) -->
    <p
      v-else-if="helperText"
      :id="helperId"
      class="text-xs text-content-muted mt-0.5"
    >
      {{ helperText }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Eye, EyeOff, X, AlertCircle } from 'lucide-vue-next'

export interface InputProps {
  modelValue?: string | number
  id?: string
  name?: string
  type?: string
  label?: string
  placeholder?: string
  helperText?: string
  errorMessage?: string
  prefix?: string
  suffix?: string
  clearable?: boolean
  showPasswordToggle?: boolean
  disabled?: boolean
  readonly?: boolean
  required?: boolean
  autofocus?: boolean
  autocomplete?: string
  spellcheck?: boolean | 'false' | 'true'
  inputmode?: 'none' | 'text' | 'decimal' | 'numeric' | 'tel' | 'search' | 'email' | 'url'
  min?: number | string
  max?: number | string
  step?: number | string
}

const props = withDefaults(defineProps<InputProps>(), {
  modelValue: '',
  id: undefined,
  name: undefined,
  type: 'text',
  label: undefined,
  placeholder: undefined,
  helperText: undefined,
  errorMessage: undefined,
  prefix: undefined,
  suffix: undefined,
  clearable: false,
  showPasswordToggle: false,
  disabled: false,
  readonly: false,
  required: false,
  autofocus: false,
  autocomplete: undefined,
  spellcheck: false,
  inputmode: undefined,
  min: undefined,
  max: undefined,
  step: undefined,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: string | number): void
  (e: 'focus', event: FocusEvent): void
  (e: 'blur', event: FocusEvent): void
  (e: 'clear'): void
}>()

const inputRef = ref<HTMLInputElement | null>(null)
const isPasswordVisible = ref(false)

onMounted(() => {
  if (props.autofocus) {
    inputRef.value?.focus()
  }
})

// Generate deterministic IDs for accessibility
const uniqueId = `input-${Math.random().toString(36).substring(2, 9)}`
const inputId = computed(() => props.id || uniqueId)
const errorId = computed(() => `${inputId.value}-error`)
const helperId = computed(() => `${inputId.value}-helper`)

const describedByIds = computed(() => {
  if (props.errorMessage) return errorId.value
  if (props.helperText) return helperId.value
  return undefined
})

const actualType = computed(() => {
  if (props.type === 'password') {
    return isPasswordVisible.value ? 'text' : 'password'
  }
  return props.type
})

function handleInput(event: Event) {
  const target = event.target as HTMLInputElement
  emit('update:modelValue', target.value)
}

function handleClear() {
  emit('update:modelValue', '')
  emit('clear')
  inputRef.value?.focus()
}

defineExpose({
  inputRef,
  focus: () => inputRef.value?.focus(),
  blur: () => inputRef.value?.blur(),
})
</script>
