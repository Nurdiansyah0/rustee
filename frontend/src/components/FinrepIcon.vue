<template>
  <div
    class="finrep-brand-lockup inline-flex"
    :class="[
      layout === 'vertical'
        ? (align === 'center' ? 'flex-col items-center text-center gap-3' : 'flex-col items-start text-left gap-3')
        : 'flex-row items-center gap-4 sm:gap-5 text-left'
    ]"
  >
    <!-- 1. Shield Icon Vector (Presisi Geometris, Bersih & Terstruktur) -->
    <div
      class="finrep-icon-wrapper flex-shrink-0"
      :style="{ width: size + 'px', height: size + 'px' }"
    >
      <svg
        viewBox="0 0 400 400"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        class="finrep-svg"
      >
        <defs>
          <!-- Gradien Rangka Luar Perisai (Emerald to Mint) -->
          <linearGradient :id="`${uid}_outer`" x1="0%" y1="0%" x2="100%" y2="100%">
            <template v-if="isDark">
              <stop offset="0%" stop-color="#34D399" />
              <stop offset="35%" stop-color="#10B981" />
              <stop offset="70%" stop-color="#059669" />
              <stop offset="100%" stop-color="#064E3B" />
            </template>
            <template v-else>
              <stop offset="0%" stop-color="#10B981" />
              <stop offset="35%" stop-color="#059669" />
              <stop offset="70%" stop-color="#047857" />
              <stop offset="100%" stop-color="#064E3B" />
            </template>
          </linearGradient>

          <!-- Gradien Sayap di Bawah Panah (Luminous Mint/Emerald Ribbon) -->
          <linearGradient :id="`${uid}_wing`" x1="0%" y1="0%" x2="100%" y2="100%">
            <template v-if="isDark">
              <stop offset="0%" stop-color="#A7F3D0" />
              <stop offset="35%" stop-color="#6EE7B7" />
              <stop offset="70%" stop-color="#34D399" />
              <stop offset="100%" stop-color="#10B981" />
            </template>
            <template v-else>
              <stop offset="0%" stop-color="#34D399" />
              <stop offset="50%" stop-color="#10B981" />
              <stop offset="100%" stop-color="#059669" />
            </template>
          </linearGradient>

          <!-- Gradien Sayap Sisi Kanan (Faceted Crest Blade) -->
          <linearGradient :id="`${uid}_wingFacet`" x1="0%" y1="0%" x2="100%" y2="100%">
            <template v-if="isDark">
              <stop offset="0%" stop-color="#6EE7B7" stop-opacity="0.8" />
              <stop offset="35%" stop-color="#34D399" stop-opacity="0.6" />
              <stop offset="70%" stop-color="#10B981" stop-opacity="0.4" />
              <stop offset="100%" stop-color="#064E3B" stop-opacity="0.2" />
            </template>
            <template v-else>
              <stop offset="0%" stop-color="#10B981" stop-opacity="0.7" />
              <stop offset="60%" stop-color="#059669" stop-opacity="0.4" />
              <stop offset="100%" stop-color="#047857" stop-opacity="0.2" />
            </template>
          </linearGradient>

          <!-- Gradien Plat Dasar Perisai Dalam (Harmonisasi Kanvas Gelap) -->
          <linearGradient :id="`${uid}_inner`" x1="50%" y1="0%" x2="50%" y2="100%">
            <template v-if="isDark">
              <stop offset="0%" stop-color="#18181B" stop-opacity="0.85" />
              <stop offset="60%" stop-color="#121215" stop-opacity="0.95" />
              <stop offset="100%" stop-color="#09090B" />
            </template>
            <template v-else>
              <stop offset="0%" stop-color="#FFFFFF" stop-opacity="0.95" />
              <stop offset="60%" stop-color="#FAFAFA" stop-opacity="0.98" />
              <stop offset="100%" stop-color="#F4F4F5" />
            </template>
          </linearGradient>

          <!-- Gradien 3 Batang Grafik Pertumbuhan Finansial (Presisi +45px Arithmatic Step) -->
          <linearGradient :id="`${uid}_bar`" x1="0%" y1="100%" x2="0%" y2="0%">
            <template v-if="isDark">
              <stop offset="0%" stop-color="#065F46" />
              <stop offset="35%" stop-color="#059669" />
              <stop offset="70%" stop-color="#10B981" />
              <stop offset="100%" stop-color="#34D399" />
            </template>
            <template v-else>
              <stop offset="0%" stop-color="#047857" />
              <stop offset="45%" stop-color="#059669" />
              <stop offset="100%" stop-color="#10B981" />
            </template>
          </linearGradient>

          <!-- Gradien Panah Pertumbuhan (Electric Mint to Emerald) -->
          <linearGradient :id="`${uid}_arrow`" x1="0%" y1="100%" x2="100%" y2="0%">
            <template v-if="isDark">
              <stop offset="0%" stop-color="#059669" />
              <stop offset="45%" stop-color="#10B981" />
              <stop offset="85%" stop-color="#34D399" />
              <stop offset="100%" stop-color="#A7F3D0" />
            </template>
            <template v-else>
              <stop offset="0%" stop-color="#047857" />
              <stop offset="45%" stop-color="#059669" />
              <stop offset="100%" stop-color="#10B981" />
            </template>
          </linearGradient>

          <!-- Filter Drop Shadow & Soft Aura -->
          <filter :id="`${uid}_shadow`" x="-20%" y="-20%" width="140%" height="140%">
            <feDropShadow dx="0" dy="8" stdDeviation="8" flood-color="#000000" :flood-opacity="isDark ? 0.65 : 0.12" />
          </filter>
          <filter :id="`${uid}_glow`" x="-20%" y="-20%" width="140%" height="140%">
            <feDropShadow dx="2" dy="6" stdDeviation="6" flood-color="#000000" :flood-opacity="isDark ? 0.45 : 0.12" />
            <feDropShadow v-if="isDark" dx="0" dy="0" stdDeviation="6" flood-color="#10B981" flood-opacity="0.25" />
          </filter>
        </defs>

        <!-- 1. Plat Dasar Dalam Perisai (Simetris Sempurna di Sumbu X=200) -->
        <path
          d="M 200,68 C 230,73 285,80 316,92 C 326,160 322,238 200,358 C 78,238 74,160 84,92 C 115,80 170,73 200,68 Z"
          :fill="`url(#${uid}_inner)`"
          :filter="`url(#${uid}_shadow)`"
        />

        <!-- 2. Sayap Kanan Terpadu (Faceted Crest Blade yang tegas & terikat rapi di perisai) -->
        <path
          d="M 200,68 C 236,74 285,82 316,92 C 324,155 320,234 200,356 C 242,285 272,215 272,158 C 272,118 244,82 200,68 Z"
          :fill="`url(#${uid}_wingFacet)`"
          :stroke="`url(#${uid}_wing)`"
          stroke-width="2"
          stroke-linejoin="round"
          opacity="0.9"
        />

        <!-- 3. Sayap Lengkung di Bawah Panah (Aerodynamic Sweep dengan jarak presisi terhadap panah) -->
        <path
          d="M 205,74 C 246,84 286,98 316,118 C 298,154 270,188 236,212"
          :stroke="`url(#${uid}_wing)`"
          stroke-width="10"
          stroke-linecap="round"
          stroke-linejoin="round"
          fill="none"
          opacity="0.95"
        />

        <!-- 4. Rangka Luar Perisai Utama (Presisi Simetris dengan Stroke 16px) -->
        <path
          d="M 200,56 C 236,61 296,69 328,82 C 340,165 336,252 200,376 C 64,252 60,165 72,82 C 104,69 164,61 200,56 Z"
          :stroke="`url(#${uid}_outer)`"
          stroke-width="16"
          stroke-linecap="round"
          stroke-linejoin="round"
          fill="none"
        />

        <!-- 5. Tiga Batang Grafik Pertumbuhan (Tangga Aritmatika Bersih, Lebar 22px, Spasi 12px) -->
        <!-- Batang 1 (Kiri): Tinggi 70px -->
        <rect x="132" y="215" width="22" height="70" rx="5" :fill="`url(#${uid}_bar)`" />
        <!-- Batang 2 (Tengah): Tinggi 115px (+45px) -->
        <rect x="166" y="170" width="22" height="115" rx="5" :fill="`url(#${uid}_bar)`" />
        <!-- Batang 3 (Kanan): Tinggi 160px (+45px) -->
        <rect x="200" y="125" width="22" height="160" rx="5" :fill="`url(#${uid}_bar)`" />
        
        <!-- Garis Dasar Grafik (Tepat sejajar & simetris di bawah 3 batang) -->
        <line
          x1="120"
          y1="290"
          x2="234"
          y2="290"
          :stroke="isDark ? '#27272A' : '#E4E4E7'"
          stroke-width="3"
          stroke-linecap="round"
        />

        <!-- 6. Panah Pertumbuhan Presisi (Mulai rapi di dalam perisai, menembus sudut 45 derajat) -->
        <g :filter="`url(#${uid}_glow)`">
          <!-- Batang Panah (Kurva Bézier Mulus) -->
          <path
            d="M 104,260 C 150,250 254,156 318,92"
            :stroke="`url(#${uid}_arrow)`"
            stroke-width="16"
            stroke-linecap="round"
            stroke-linejoin="round"
            fill="none"
          />

          <!-- Kepala Panah Chevron 45 Derajat Simetris -->
          <path
            d="M 348,62 L 296,78 L 314,96 L 332,114 Z"
            :fill="`url(#${uid}_arrow)`"
            :stroke="`url(#${uid}_arrow)`"
            stroke-width="2"
            stroke-linejoin="round"
          />
        </g>
      </svg>
    </div>

    <!-- 2. Typography: Merek Utama FinRep Bolder & Clean -->
    <div
      v-if="showTypography"
      class="finrep-typography flex flex-col justify-center select-none font-sans"
      :class="[
        layout === 'vertical' && align === 'center' ? 'items-center text-center' : 'items-start text-left'
      ]"
    >
      <span
        class="finrep-title font-black tracking-[-0.025em] leading-none whitespace-nowrap"
        :class="titleColorClass"
        :style="{ fontSize: titleFontSize }"
      >
        FinRep
      </span>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'

const props = defineProps({
  size: {
    type: [Number, String],
    default: 80
  },
  showTypography: {
    type: Boolean,
    default: true
  },
  layout: {
    type: String,
    default: 'horizontal',
    validator: (v) => ['vertical', 'horizontal'].includes(v)
  },
  align: {
    type: String,
    default: 'left',
    validator: (v) => ['left', 'center'].includes(v)
  },
  theme: {
    type: String,
    default: 'auto',
    validator: (v) => ['auto', 'light', 'dark'].includes(v)
  },
  titleSize: {
    type: [Number, String],
    default: null
  }
})

// Scoped UID agar gradient ID tidak tabrakan antar instance
const uid = computed(() => `fr_${props.theme}_${Math.round(Number(props.size) || 80)}`)

const isDark = computed(() => {
  if (props.theme === 'dark') return true
  if (props.theme === 'light') return false
  return true // default dark context fallback
})

// Skala FinRep proporsional dan tegap di samping logo
const computedTitleSize = computed(() => {
  if (props.titleSize) return Number(props.titleSize)
  const base = Number(props.size) || 80
  return Math.max(26, Math.round(base * 0.58))
})

const titleFontSize = computed(() => `${computedTitleSize.value}px`)

// Warna teks FinRep
const titleColorClass = computed(() => {
  if (props.theme === 'dark') return 'text-white'
  if (props.theme === 'light') return 'text-[#064E3B]'
  return 'text-[#064E3B] dark:text-white'
})
</script>

<style scoped>
.finrep-brand-lockup {
  font-family: "Plus Jakarta Sans", Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
}

.finrep-icon-wrapper {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.finrep-svg {
  width: 100%;
  height: 100%;
  overflow: visible;
}
</style>
