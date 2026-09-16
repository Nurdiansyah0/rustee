<template>
  <div class="p-3 sm:p-3.5 bg-zinc-900/90 rounded-2xl border border-zinc-800 shadow-xl backdrop-blur-md space-y-2 text-white">
    <!-- Total Balance Widget -->
    <div class="flex items-center justify-between gap-2 pb-2 border-b border-zinc-800/80">
      <div>
        <span class="text-[10px] font-medium text-zinc-400 block leading-none mb-1">Saldo</span>
        <div class="text-base sm:text-xl font-black text-white tracking-tight tabular-nums leading-none">
          Rp 18.450.000
        </div>
      </div>
      <div class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-[10px] sm:text-[11px] font-bold tabular-nums shrink-0">
        <TrendingUp class="w-3 h-3 stroke-[2.5]" />
        <span>+8.4% bulan ini</span>
      </div>
    </div>

    <!-- Dual Flow Stat Cards: Uang Masuk vs Uang Keluar -->
    <div class="grid grid-cols-2 gap-2">
      <div class="p-1.5 sm:p-2 rounded-xl bg-zinc-950/60 border border-zinc-800/80 flex items-center gap-2">
        <div class="w-5 h-5 sm:w-6 sm:h-6 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-emerald-400 shrink-0">
          <ArrowDownLeft class="w-3 h-3 sm:w-3.5 sm:h-3.5 stroke-[2.5]" />
        </div>
        <div class="min-w-0">
          <span class="text-[9px] text-zinc-400 font-medium block leading-none">Uang Masuk</span>
          <span class="text-[10px] sm:text-xs font-bold text-emerald-400 tabular-nums truncate block mt-0.5">+Rp 22.700.000</span>
        </div>
      </div>

      <div class="p-1.5 sm:p-2 rounded-xl bg-zinc-950/60 border border-zinc-800/80 flex items-center gap-2">
        <div class="w-5 h-5 sm:w-6 sm:h-6 rounded-lg bg-rose-500/10 border border-rose-500/20 flex items-center justify-center text-rose-400 shrink-0">
          <ArrowUpRight class="w-3 h-3 sm:w-3.5 sm:h-3.5 stroke-[2.5]" />
        </div>
        <div class="min-w-0">
          <span class="text-[9px] text-zinc-400 font-medium block leading-none">Uang Keluar</span>
          <span class="text-[10px] sm:text-xs font-bold text-rose-400 tabular-nums truncate block mt-0.5">-Rp 4.250.000</span>
        </div>
      </div>
    </div>

    <!-- Simulated Monthly Cash Flow Trend Graphic with Milestone Annotations -->
    <div class="space-y-1">
      <div class="flex items-center justify-between text-[10px] sm:text-[11px]">
        <span class="text-zinc-400 font-medium">Tren Keuangan 30 Hari</span>
        <span class="text-emerald-400 font-bold tabular-nums">Surplus Rp 4.250.000</span>
      </div>

      <div class="h-14 sm:h-16 w-full relative">
        <svg class="w-full h-full" viewBox="0 0 340 100" preserveAspectRatio="none">
          <defs>
            <linearGradient id="livePreviewEmeraldArea" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#10b981" stop-opacity="0.3" />
              <stop offset="60%" stop-color="#10b981" stop-opacity="0.08" />
              <stop offset="100%" stop-color="#10b981" stop-opacity="0.0" />
            </linearGradient>
          </defs>

          <!-- Horizontal subtle grid lines -->
          <line x1="15" y1="85" x2="315" y2="85" stroke="#27272a" stroke-width="1" stroke-dasharray="3 3" />
          <line x1="15" y1="45" x2="315" y2="45" stroke="#27272a" stroke-width="1" stroke-dasharray="3 3" />

          <!-- Milestone 1: Pengeluaran Dropdown (12 Sep: -Rp 2.8jt Operasional) -->
          <line x1="110" y1="78" x2="110" y2="85" stroke="#f43f5e" stroke-width="1.5" stroke-dasharray="2 2" opacity="0.8" />
          <!-- Milestone 2: Uang Masuk Surge (25 Sep: +Rp 14.5jt Invoice Masuk) -->
          <line x1="235" y1="20" x2="235" y2="85" stroke="#10b981" stroke-width="1.5" stroke-dasharray="2 2" opacity="0.8" />

          <!-- Area Fill -->
          <path
            d="M 15 65 Q 60 65 85 68 T 110 78 Q 150 76 190 55 T 235 20 Q 260 22 280 36 T 315 30 L 315 85 L 15 85 Z"
            fill="url(#livePreviewEmeraldArea)"
          />

          <!-- Line Curve -->
          <path
            d="M 15 65 Q 60 65 85 68 T 110 78 Q 150 76 190 55 T 235 20 Q 260 22 280 36 T 315 30"
            fill="none"
            stroke="#10b981"
            stroke-width="2.5"
            stroke-linecap="round"
          />

          <!-- Marker 1: Pengeluaran Dip (12 Sep) -->
          <circle cx="110" cy="78" r="4" fill="#f43f5e" stroke="#18181b" stroke-width="1.5" />
          <g transform="translate(68, 54)">
            <rect width="84" height="18" rx="4" fill="#18181b" stroke="#f43f5e" stroke-width="1" opacity="0.95" />
            <text x="42" y="12.5" text-anchor="middle" class="text-[8.5px] font-bold fill-rose-400 tabular-nums">-Rp 2.8jt (Sewa)</text>
          </g>

          <!-- Marker 2: Uang Masuk Peak (25 Sep) -->
          <circle cx="235" cy="20" r="4.5" fill="#10b981" stroke="#18181b" stroke-width="2" />
          <g transform="translate(185, 2)">
            <rect width="100" height="16" rx="4" fill="#18181b" stroke="#10b981" stroke-width="1" opacity="0.95" />
            <text x="50" y="11.5" text-anchor="middle" class="text-[8.5px] font-bold fill-emerald-400 tabular-nums">+Rp 14.5jt (Invoice)</text>
          </g>

          <!-- Marker 3: Saldo Sekarang (30 Sep) -->
          <circle cx="315" cy="30" r="4.5" fill="#10b981" stroke="#18181b" stroke-width="2" />
          <circle cx="315" cy="30" r="1.5" fill="#ffffff" />

          <!-- X-Axis Dates -->
          <text x="15" y="97" class="text-[9px] fill-zinc-400 font-medium">01 Sep</text>
          <text x="110" y="97" class="text-[9px] fill-zinc-400 font-medium" text-anchor="middle">12 Sep</text>
          <text x="235" y="97" class="text-[9px] fill-zinc-400 font-medium" text-anchor="middle">25 Sep</text>
          <text x="315" y="97" class="text-[9px] fill-zinc-400 font-medium" text-anchor="end">30 Sep</text>
        </svg>
      </div>
    </div>

    <!-- Transaction Activity Items Showing Inflow & Outflow Impact -->
    <div class="pt-1.5 border-t border-zinc-800/80 space-y-1">
      <!-- Item 1: Uang Masuk -->
      <div class="flex items-center justify-between py-1 px-2 rounded-lg bg-zinc-950/40 border border-zinc-800/50 gap-2">
        <div class="flex items-center gap-1.5 min-w-0">
          <span class="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0"></span>
          <div class="truncate">
            <span class="text-[10px] sm:text-[11px] font-semibold text-zinc-200 block truncate leading-tight">Pembayaran Invoice Proyek</span>
            <span class="text-[8.5px] sm:text-[9px] text-zinc-400 block truncate leading-none">25 Sep • Rekening Utama</span>
          </div>
        </div>
        <span class="text-[10px] sm:text-[11px] font-bold text-emerald-400 tabular-nums shrink-0">+Rp 14.500.000</span>
      </div>

      <!-- Item 2: Pengeluaran -->
      <div class="flex items-center justify-between py-1 px-2 rounded-lg bg-zinc-950/40 border border-zinc-800/50 gap-2">
        <div class="flex items-center gap-1.5 min-w-0">
          <span class="w-1.5 h-1.5 rounded-full bg-rose-400 shrink-0"></span>
          <div class="truncate">
            <span class="text-[10px] sm:text-[11px] font-semibold text-zinc-200 block truncate leading-tight">Sewa Kantor & Operasional</span>
            <span class="text-[8.5px] sm:text-[9px] text-zinc-400 block truncate leading-none">12 Sep • Pengeluaran Tetap</span>
          </div>
        </div>
        <span class="text-[10px] sm:text-[11px] font-bold text-rose-400 tabular-nums shrink-0">-Rp 2.800.000</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { TrendingUp, ArrowDownLeft, ArrowUpRight } from 'lucide-vue-next'
</script>
