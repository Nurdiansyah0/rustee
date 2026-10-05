<template>
  <div class="space-y-5">
    <!-- Header & Action Row -->
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-xl font-bold text-content-primary tracking-tight">Pesanan Pembelian & Penerimaan Barang</h2>
          <Badge variant="brand" size="sm">Procurement</Badge>
        </div>
        <p class="text-xs text-content-secondary mt-0.5">
          Siklus pesanan pembelian (PO), pelacakan penerimaan gudang, nomor batch, dan pemutakhiran biaya WAC
        </p>
      </div>

      <div class="flex items-center gap-2 flex-wrap">
        <!-- Switch to Inventory View -->
        <button
          type="button"
          @click="$emit('nav', 'inventory')"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl border border-border-default hover:bg-surface-subtle text-content-secondary hover:text-content-primary transition font-bold text-xs cursor-pointer shadow-xs"
          title="Buka Inventaris & Stok"
        >
          <Package class="w-3.5 h-3.5 text-brand-default" />
          <span>Inventaris & Stok</span>
          <ArrowRight class="w-3 h-3 text-content-muted" />
        </button>

        <!-- Create PO Button -->
        <Button
          variant="primary"
          size="sm"
          @click="openCreatePOModal"
        >
          <template #prefix>
            <Plus class="w-3.5 h-3.5 mr-1" />
          </template>
          + Buat Pesanan (PO)
        </Button>
      </div>
    </div>

    <!-- KPI Summary Cards -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
      <!-- Total POs -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Total Seluruh PO</span>
          <div class="w-8 h-8 rounded-xl bg-blue-500/10 text-blue-500 flex items-center justify-center">
            <FileText class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-2xl font-extrabold text-content-primary tracking-tight">
            {{ inventoryStore.purchaseOrders.length }}
          </span>
          <span class="text-xs text-content-muted ml-1">Pesanan</span>
        </div>
      </Card>

      <!-- Draft POs -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Draft PO</span>
          <div class="w-8 h-8 rounded-xl bg-surface-subtle text-content-muted flex items-center justify-center">
            <Clock class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-2xl font-extrabold text-content-primary tracking-tight">
            {{ draftCount }}
          </span>
          <span class="text-xs text-content-muted ml-1">Belum dipesan</span>
        </div>
      </Card>

      <!-- Ordered / In Delivery -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Menunggu Pengiriman</span>
          <div class="w-8 h-8 rounded-xl bg-amber-500/10 text-amber-500 flex items-center justify-center">
            <Truck class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-2xl font-extrabold text-amber-500 tracking-tight">
            {{ orderedCount }}
          </span>
          <span class="text-xs text-content-muted ml-1">Dalam proses</span>
        </div>
      </Card>

      <!-- Received / Completed -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Selesai Diterima</span>
          <div class="w-8 h-8 rounded-xl bg-emerald-500/10 text-emerald-500 flex items-center justify-center">
            <CheckCircle2 class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-2xl font-extrabold text-income-default tracking-tight">
            {{ receivedCount }}
          </span>
          <span class="text-xs text-content-muted ml-1">Stok masuk</span>
        </div>
      </Card>
    </div>

    <!-- Status Tabs & Search Bar -->
    <div class="flex flex-col sm:flex-row gap-3 items-stretch sm:items-center justify-between">
      <!-- Status Tabs -->
      <div class="flex items-center gap-1.5 overflow-x-auto scrollbar-none pb-1 sm:pb-0">
        <button
          v-for="tab in statusTabs"
          :key="tab.id"
          type="button"
          @click="selectedStatusTab = tab.id"
          :class="[
            'px-3 py-1.5 rounded-xl text-xs font-bold transition shrink-0 cursor-pointer flex items-center gap-1.5',
            selectedStatusTab === tab.id
              ? 'bg-brand-default text-content-inverse shadow-xs'
              : 'bg-surface-card text-content-secondary border border-border-subtle hover:border-border-default'
          ]"
        >
          <span>{{ tab.label }}</span>
          <span
            class="text-[10px] px-1 py-0.2 rounded-full font-extrabold"
            :class="selectedStatusTab === tab.id ? 'bg-white/20' : 'bg-surface-subtle text-content-muted'"
          >
            {{ getStatusCount(tab.id) }}
          </span>
        </button>
      </div>

      <!-- Search Input -->
      <div class="relative w-full sm:w-64">
        <Search class="w-4 h-4 text-content-muted absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
        <input
          v-model="poSearchQuery"
          type="text"
          placeholder="Cari nomor PO / pemasok..."
          class="w-full pl-9 pr-8 py-2 bg-surface-card border border-border-subtle hover:border-border-default focus:border-brand-default focus:ring-1 focus:ring-brand-default rounded-xl text-xs text-content-primary placeholder:text-content-muted transition outline-none"
        />
        <button
          v-if="poSearchQuery"
          type="button"
          @click="poSearchQuery = ''"
          class="absolute right-2.5 top-1/2 -translate-y-1/2 text-content-muted hover:text-content-primary p-0.5"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <!-- Purchase Orders List -->
    <div class="space-y-3">
      <div
        v-for="po in displayedPurchaseOrders"
        :key="po.id"
        class="p-4 sm:p-5 rounded-2xl bg-surface-card border border-border-subtle shadow-card hover:border-border-default transition space-y-3"
      >
        <!-- PO Header -->
        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2.5 border-b border-border-subtle pb-3">
          <div class="flex items-center gap-3">
            <span class="font-mono text-xs font-extrabold px-2 py-0.5 rounded bg-brand-muted text-brand-default border border-brand-border">
              {{ po.po_number }}
            </span>
            <div class="text-sm font-extrabold text-content-primary flex items-center gap-1.5">
              <span>{{ po.supplier_name }}</span>
            </div>
            <Badge :variant="getPOBadgeVariant(po.status)" size="sm">
              {{ formatPOStatus(po.status) }}
            </Badge>
          </div>

          <div class="flex items-center gap-3 text-xs text-content-secondary">
            <div class="flex items-center gap-1 text-content-muted">
              <Calendar class="w-3.5 h-3.5" />
              <span>{{ formatDate(po.created_at) }}</span>
            </div>
            <div class="flex items-center gap-1 font-semibold text-content-primary">
              <WarehouseIcon class="w-3.5 h-3.5 text-brand-default" />
              <span>{{ resolveWarehouse(po.destination_warehouse_id)?.name || 'Gudang Tujuan' }}</span>
            </div>
          </div>
        </div>

        <!-- Line items table -->
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs border-collapse">
            <thead>
              <tr class="text-[11px] text-content-muted uppercase tracking-wider font-semibold border-b border-border-subtle/60 pb-1">
                <th class="py-1.5 pr-4">Item Produk</th>
                <th class="py-1.5 px-4 text-right">Dipesan</th>
                <th class="py-1.5 px-4 text-right">Diterima</th>
                <th class="py-1.5 px-4 text-right">Harga Beli Satuan</th>
                <th class="py-1.5 pl-4 text-right">Subtotal</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border-subtle/40">
              <tr v-for="item in (po.items || [])" :key="item.id || item.product_id">
                <td class="py-2 pr-4">
                  <div class="font-bold text-content-primary">
                    {{ resolveProduct(item.product_id)?.name || 'Produk' }}
                  </div>
                  <div class="text-[10px] font-mono text-content-muted">
                    {{ resolveProduct(item.product_id)?.sku }}
                  </div>
                </td>
                <td class="py-2 px-4 text-right font-semibold text-content-primary">
                  {{ formatNumber(item.quantity_ordered) }}
                </td>
                <td class="py-2 px-4 text-right font-extrabold" :class="item.quantity_received >= item.quantity_ordered ? 'text-income-default' : 'text-amber-500'">
                  {{ formatNumber(item.quantity_received) }}
                </td>
                <td class="py-2 px-4 text-right text-content-secondary">
                  {{ formatIDR(item.unit_cost) }}
                </td>
                <td class="py-2 pl-4 text-right font-extrabold text-content-primary">
                  {{ formatIDR(item.total_cost || (item.quantity_ordered * item.unit_cost)) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- PO Footer & Actions -->
        <div class="pt-3 border-t border-border-subtle flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-xs text-content-secondary">Total Nilai Pembelian:</span>
            <span class="text-base font-extrabold text-content-primary">
              {{ formatIDR(po.total_amount) }}
            </span>
          </div>

          <!-- State Machine Action Triggers (Feature 10, 11, 12) -->
          <div class="flex items-center gap-2 flex-wrap">
            <!-- If DRAFT -> Can Order or Cancel -->
            <template v-if="po.status === 'DRAFT'">
              <Button
                variant="outline"
                size="sm"
                :loading="actionLoadingId === po.id"
                @click="handleCancelPO(po.id)"
              >
                Batalkan
              </Button>
              <Button
                variant="primary"
                size="sm"
                :loading="actionLoadingId === po.id"
                @click="handleOrderPO(po.id)"
              >
                <template #prefix>
                  <Send class="w-3.5 h-3.5 mr-1" />
                </template>
                Pesan ke Pemasok
              </Button>
            </template>

            <!-- If ORDERED -> Can Receive Goods or Cancel -->
            <template v-else-if="po.status === 'ORDERED'">
              <Button
                variant="outline"
                size="sm"
                :loading="actionLoadingId === po.id"
                @click="handleCancelPO(po.id)"
              >
                Batalkan
              </Button>
              <Button
                variant="primary"
                size="sm"
                @click="openReceiveModal(po)"
              >
                <template #prefix>
                  <Inbox class="w-3.5 h-3.5 mr-1" />
                </template>
                Terima Barang
              </Button>
            </template>

            <!-- If PARTIALLY_RECEIVED -> Can Receive Remaining Goods -->
            <template v-else-if="po.status === 'PARTIALLY_RECEIVED'">
              <Button
                variant="primary"
                size="sm"
                @click="openReceiveModal(po)"
              >
                <template #prefix>
                  <Inbox class="w-3.5 h-3.5 mr-1" />
                </template>
                Terima Sisa Barang
              </Button>
            </template>

            <!-- If RECEIVED -> Terminal state badge -->
            <div v-else-if="po.status === 'RECEIVED'" class="text-xs text-income-default font-extrabold flex items-center gap-1">
              <CheckCircle2 class="w-4 h-4" />
              <span>Seluruh barang telah diterima di gudang</span>
            </div>

            <!-- If CANCELLED -> Terminal state badge -->
            <div v-else-if="po.status === 'CANCELLED'" class="text-xs text-expense-default font-semibold flex items-center gap-1">
              <XCircle class="w-4 h-4" />
              <span>Pesanan pembelian dibatalkan</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Empty state -->
      <div
        v-if="displayedPurchaseOrders.length === 0"
        class="py-12 px-4 rounded-2xl bg-surface-card border border-border-subtle text-center space-y-3"
      >
        <div class="w-12 h-12 rounded-2xl bg-brand-muted text-brand-default flex items-center justify-center mx-auto">
          <ShoppingCart class="w-6 h-6 stroke-[2]" />
        </div>
        <div>
          <h4 class="text-sm font-bold text-content-primary">Tidak Ada Pesanan Pembelian</h4>
          <p class="text-xs text-content-secondary mt-0.5">
            Belum ada PO yang cocok dengan filter atau pencarian saat ini.
          </p>
        </div>
        <Button variant="primary" size="sm" @click="openCreatePOModal">
          Buat PO Pertama
        </Button>
      </div>
    </div>

    <!-- MODAL 1: CREATE PURCHASE ORDER -->
    <ModalSheet
      v-model="showCreatePOModal"
      title="Buat Pesanan Pembelian (PO)"
      description="Terbitkan pesanan pembelian baru ke pemasok bahan/barang"
      size="lg"
    >
      <form @submit.prevent="handleCreatePO" class="space-y-4 py-2">
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <Input
            v-model="poForm.supplier_name"
            label="Nama Pemasok (Supplier)"
            placeholder="PT Kopi Nusantara Jaya"
            required
          />

          <div class="space-y-1.5">
            <label class="text-xs font-semibold text-content-secondary">Gudang Tujuan Penerimaan</label>
            <select
              v-model="poForm.destination_warehouse_id"
              class="w-full p-2.5 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
              required
            >
              <option value="" disabled>-- Pilih Gudang Tujuan --</option>
              <option
                v-for="wh in inventoryStore.warehouses"
                :key="wh.id"
                :value="wh.id"
              >
                {{ wh.name }}
              </option>
            </select>
          </div>
        </div>

        <Input
          v-model="poForm.notes"
          label="Catatan Pesanan"
          placeholder="Pengadaan rutin bulanan / termin pembayaran 30 hari"
        />

        <!-- Items Builder -->
        <div class="space-y-2 pt-2">
          <div class="flex items-center justify-between">
            <span class="text-xs font-bold text-content-primary">Daftar Barang yang Dipesan:</span>
            <button
              type="button"
              @click="addPOLineItem"
              class="text-xs font-bold text-brand-default hover:underline flex items-center gap-1 cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Tambah Item</span>
            </button>
          </div>

          <div class="space-y-2.5">
            <div
              v-for="(item, idx) in poForm.items"
              :key="idx"
              class="p-3 rounded-xl bg-surface-subtle border border-border-subtle flex flex-col sm:flex-row items-end gap-2.5"
            >
              <!-- Product -->
              <div class="flex-1 w-full space-y-1">
                <label class="text-[11px] font-semibold text-content-secondary">Produk</label>
                <select
                  v-model="item.product_id"
                  @change="handleProductSelected(item)"
                  class="w-full p-2 bg-surface-card border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
                  required
                >
                  <option value="" disabled>-- Pilih Produk --</option>
                  <option
                    v-for="prd in inventoryStore.products"
                    :key="prd.id"
                    :value="prd.id"
                  >
                    [{{ prd.sku }}] {{ prd.name }}
                  </option>
                </select>
              </div>

              <!-- Qty -->
              <div class="w-full sm:w-28 space-y-1">
                <label class="text-[11px] font-semibold text-content-secondary">Kuantitas</label>
                <input
                  v-model.number="item.quantity_ordered"
                  type="number"
                  min="1"
                  class="w-full p-2 bg-surface-card border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none text-right font-semibold"
                  required
                />
              </div>

              <!-- Unit Cost -->
              <div class="w-full sm:w-36 space-y-1">
                <label class="text-[11px] font-semibold text-content-secondary">Harga Beli Satuan</label>
                <input
                  v-model.number="item.unit_cost"
                  type="number"
                  min="0"
                  class="w-full p-2 bg-surface-card border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none text-right font-semibold"
                  required
                />
              </div>

              <!-- Subtotal -->
              <div class="w-full sm:w-36 text-right pb-1">
                <div class="text-[10px] text-content-muted">Subtotal</div>
                <div class="text-xs font-extrabold text-content-primary truncate">
                  {{ formatIDR(item.quantity_ordered * item.unit_cost) }}
                </div>
              </div>

              <!-- Remove button -->
              <button
                v-if="poForm.items.length > 1"
                type="button"
                @click="removePOLineItem(idx)"
                class="p-2 text-content-muted hover:text-expense-default transition cursor-pointer"
                title="Hapus baris item"
              >
                <Trash2 class="w-4 h-4" />
              </button>
            </div>
          </div>

          <!-- Total PO Calculation Summary -->
          <div class="p-3 rounded-xl bg-surface-card border border-border-subtle flex items-center justify-between">
            <span class="text-xs font-bold text-content-secondary">Estimasi Total Nilai PO:</span>
            <span class="text-base font-extrabold text-brand-default">
              {{ formatIDR(calculatedPOTotal) }}
            </span>
          </div>
        </div>

        <div v-if="poCreateError" class="p-2.5 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-500 text-xs">
          {{ poCreateError }}
        </div>
      </form>

      <template #footer>
        <Button variant="outline" size="sm" @click="showCreatePOModal = false">
          Batal
        </Button>
        <Button
          variant="primary"
          size="sm"
          :loading="submittingPO"
          @click="handleCreatePO"
        >
          Simpan Sebagai DRAFT
        </Button>
      </template>
    </ModalSheet>

    <!-- MODAL 2: RECEIVE GOODS -->
    <ModalSheet
      v-model="showReceiveModal"
      title="Penerimaan Barang Fisik (Goods Receipt)"
      description="Catat unit barang yang diterima di gudang, nomor batch, dan biaya aktual untuk pemutakhiran WAC"
      size="lg"
    >
      <form v-if="selectedPOForReceive" @submit.prevent="handleExecuteReceive" class="space-y-4 py-2">
        <div class="p-3.5 rounded-xl bg-surface-subtle border border-border-subtle flex items-center justify-between text-xs">
          <div>
            <div class="text-content-muted">Nomor PO:</div>
            <div class="font-mono font-extrabold text-brand-default">{{ selectedPOForReceive.po_number }}</div>
          </div>
          <div>
            <div class="text-content-muted">Pemasok:</div>
            <div class="font-bold text-content-primary">{{ selectedPOForReceive.supplier_name }}</div>
          </div>
          <div class="text-right">
            <div class="text-content-muted">Gudang Tujuan:</div>
            <div class="font-semibold text-content-primary">
              {{ resolveWarehouse(selectedPOForReceive.destination_warehouse_id)?.name }}
            </div>
          </div>
        </div>

        <div class="space-y-3">
          <div class="text-xs font-bold text-content-primary">Input Rincian Penerimaan Per Item:</div>

          <div
            v-for="(item, idx) in receiveForm.items"
            :key="idx"
            class="p-3 rounded-xl bg-surface-card border border-border-subtle space-y-2.5"
          >
            <div class="flex items-center justify-between">
              <div>
                <span class="font-bold text-xs text-content-primary">
                  {{ resolveProduct(item.product_id)?.name }}
                </span>
                <span class="text-[10px] text-content-muted font-mono ml-1.5">
                  ({{ resolveProduct(item.product_id)?.sku }})
                </span>
              </div>
              <div class="text-xs text-content-secondary">
                Dipesan: <strong class="text-content-primary">{{ item.quantity_ordered }}</strong> |
                Sudah Diterima: <strong class="text-income-default">{{ item.previously_received }}</strong> |
                Sisa: <strong class="text-amber-500">{{ Math.max(0, item.quantity_ordered - item.previously_received) }}</strong>
              </div>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
              <!-- Qty Received Now -->
              <div>
                <label class="text-[11px] font-semibold text-content-secondary">Kuantitas Diterima Saat Ini</label>
                <input
                  v-model.number="item.quantity_received"
                  type="number"
                  min="0"
                  class="w-full p-2 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none text-right font-extrabold"
                  required
                />
              </div>

              <!-- Unit Cost for WAC -->
              <div>
                <label class="text-[11px] font-semibold text-content-secondary">Biaya Satuan Aktual (Rp)</label>
                <input
                  v-model.number="item.unit_cost"
                  type="number"
                  min="0"
                  class="w-full p-2 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none text-right font-semibold"
                  required
                />
              </div>

              <!-- Batch Number -->
              <div>
                <label class="text-[11px] font-semibold text-content-secondary">Nomor Batch / Lot</label>
                <input
                  v-model="item.batch_number"
                  type="text"
                  placeholder="BATCH-2026-10A"
                  class="w-full p-2 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none font-mono"
                />
              </div>
            </div>
          </div>
        </div>

        <div class="p-3 rounded-xl bg-blue-500/10 border border-blue-500/20 text-xs text-blue-600 dark:text-blue-400">
          <p>
            Konfirmasi penerimaan ini akan secara otomatis:
            (1) Menambah saldo kuantitas stok di gudang tujuan,
            (2) Menghitung ulang harga rata-rata tertimbang (WAC) secara integer Rupiah,
            (3) Memposting entri jurnal akuntansi berpasangan (Debit Akun 1300 Persediaan / Kredit Akun 2000 Utang Usaha).
          </p>
        </div>

        <div v-if="receiveError" class="p-2.5 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-500 text-xs">
          {{ receiveError }}
        </div>
      </form>

      <template #footer>
        <Button variant="outline" size="sm" @click="showReceiveModal = false">
          Batal
        </Button>
        <Button
          variant="primary"
          size="sm"
          :loading="submittingReceive"
          @click="handleExecuteReceive"
        >
          Konfirmasi Penerimaan Barang
        </Button>
      </template>
    </ModalSheet>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { Card, Badge, Button, Input, ModalSheet } from '@/components/ui'
import { useInventoryStore } from '@/stores/inventory'
import { formatIDR } from '@/utils/currency'
import {
  FileText,
  Clock,
  Truck,
  CheckCircle2,
  XCircle,
  Plus,
  Search,
  X,
  Package,
  ArrowRight,
  Warehouse as WarehouseIcon,
  Calendar,
  Send,
  Inbox,
  Trash2,
  ShoppingCart
} from 'lucide-vue-next'

defineEmits(['nav'])

const inventoryStore = useInventoryStore()

const selectedStatusTab = ref('ALL')
const poSearchQuery = ref('')

const showCreatePOModal = ref(false)
const showReceiveModal = ref(false)
const selectedPOForReceive = ref(null)

const submittingPO = ref(false)
const submittingReceive = ref(false)
const actionLoadingId = ref(null)

const poCreateError = ref('')
const receiveError = ref('')

// Forms
const poForm = ref({
  supplier_name: '',
  destination_warehouse_id: '',
  notes: '',
  items: []
})

const receiveForm = ref({
  items: []
})

const statusTabs = [
  { id: 'ALL', label: 'Semua Status' },
  { id: 'DRAFT', label: 'Draft' },
  { id: 'ORDERED', label: 'Dipesan' },
  { id: 'PARTIALLY_RECEIVED', label: 'Parsial' },
  { id: 'RECEIVED', label: 'Selesai' },
  { id: 'CANCELLED', label: 'Dibatalkan' }
]

onMounted(async () => {
  try {
    await inventoryStore.refreshAll()
  } catch (err) {
    console.warn('Initial PO refresh error:', err)
  }
})

// Helpers
function formatNumber(val) {
  return new Intl.NumberFormat('id-ID').format(Number(val) || 0)
}

function formatDate(iso) {
  if (!iso) return '-'
  return new Date(iso).toLocaleDateString('id-ID', {
    day: 'numeric',
    month: 'short',
    year: 'numeric'
  })
}

function resolveProduct(id) {
  return inventoryStore.getProductById(id)
}

function resolveWarehouse(id) {
  return inventoryStore.getWarehouseById(id)
}

function getPOBadgeVariant(status) {
  switch (status) {
    case 'DRAFT':
      return 'neutral'
    case 'ORDERED':
      return 'brand'
    case 'PARTIALLY_RECEIVED':
      return 'warning'
    case 'RECEIVED':
      return 'income'
    case 'CANCELLED':
      return 'expense'
    default:
      return 'default'
  }
}

function formatPOStatus(status) {
  switch (status) {
    case 'DRAFT':
      return 'Draft'
    case 'ORDERED':
      return 'Dipesan'
    case 'PARTIALLY_RECEIVED':
      return 'Diterima Sebagian'
    case 'RECEIVED':
      return 'Selesai Diterima'
    case 'CANCELLED':
      return 'Dibatalkan'
    default:
      return status
  }
}

function getStatusCount(statusId) {
  if (statusId === 'ALL') return inventoryStore.purchaseOrders.length
  return inventoryStore.purchaseOrders.filter((po) => po.status === statusId).length
}

const draftCount = computed(() => getStatusCount('DRAFT'))
const orderedCount = computed(() => getStatusCount('ORDERED') + getStatusCount('PARTIALLY_RECEIVED'))
const receivedCount = computed(() => getStatusCount('RECEIVED'))

const displayedPurchaseOrders = computed(() => {
  let list = inventoryStore.purchaseOrders

  if (selectedStatusTab.value !== 'ALL') {
    list = list.filter((po) => po.status === selectedStatusTab.value)
  }

  if (poSearchQuery.value.trim()) {
    const q = poSearchQuery.value.toLowerCase().trim()
    list = list.filter((po) => {
      const num = (po.po_number || '').toLowerCase()
      const sup = (po.supplier_name || '').toLowerCase()
      return num.includes(q) || sup.includes(q)
    })
  }

  return list
})

const calculatedPOTotal = computed(() => {
  return poForm.value.items.reduce((acc, it) => {
    const q = Number(it.quantity_ordered) || 0
    const c = Number(it.unit_cost) || 0
    return acc + q * c
  }, 0)
})

// Modal Openers
function openCreatePOModal() {
  poCreateError.value = ''
  const defaultWh = inventoryStore.activeWarehouseId || inventoryStore.warehouses[0]?.id || ''
  const defaultPrd = inventoryStore.products[0]

  poForm.value = {
    supplier_name: '',
    destination_warehouse_id: defaultWh,
    notes: '',
    items: [
      {
        product_id: defaultPrd?.id || '',
        quantity_ordered: 10,
        unit_cost: defaultPrd?.cost_price || 0
      }
    ]
  }
  showCreatePOModal.value = true
}

function addPOLineItem() {
  const defaultPrd = inventoryStore.products[0]
  poForm.value.items.push({
    product_id: defaultPrd?.id || '',
    quantity_ordered: 1,
    unit_cost: defaultPrd?.cost_price || 0
  })
}

function removePOLineItem(index) {
  poForm.value.items.splice(index, 1)
}

function handleProductSelected(lineItem) {
  const p = resolveProduct(lineItem.product_id)
  if (p) {
    lineItem.unit_cost = p.cost_price || 0
  }
}

function openReceiveModal(po) {
  receiveError.value = ''
  selectedPOForReceive.value = po

  receiveForm.value = {
    items: (po.items || []).map((it) => {
      const remaining = Math.max(0, Number(it.quantity_ordered || 0) - Number(it.quantity_received || 0))
      return {
        product_id: it.product_id,
        quantity_ordered: it.quantity_ordered,
        previously_received: it.quantity_received || 0,
        quantity_received: remaining,
        unit_cost: it.unit_cost,
        batch_number: ''
      }
    })
  }

  showReceiveModal.value = true
}

// Handlers
async function handleCreatePO() {
  poCreateError.value = ''
  if (!poForm.value.supplier_name.trim()) {
    poCreateError.value = 'Nama pemasok wajib diisi.'
    return
  }
  if (!poForm.value.destination_warehouse_id) {
    poCreateError.value = 'Gudang tujuan wajib dipilih.'
    return
  }
  if (poForm.value.items.length === 0) {
    poCreateError.value = 'Minimal harus ada satu item barang.'
    return
  }

  submittingPO.value = true
  try {
    await inventoryStore.createPurchaseOrder({
      supplier_name: poForm.value.supplier_name.trim(),
      destination_warehouse_id: poForm.value.destination_warehouse_id,
      notes: poForm.value.notes.trim() || null,
      items: poForm.value.items.map((it) => ({
        product_id: it.product_id,
        quantity_ordered: Number(it.quantity_ordered),
        unit_cost: Number(it.unit_cost)
      }))
    })
    showCreatePOModal.value = false
  } catch (err) {
    poCreateError.value = err.detail || err.message || 'Gagal membuat purchase order.'
  } finally {
    submittingPO.value = false
  }
}

async function handleOrderPO(id) {
  actionLoadingId.value = id
  try {
    await inventoryStore.orderPurchaseOrder(id)
  } catch (err) {
    console.error('Failed to order PO:', err)
  } finally {
    actionLoadingId.value = null
  }
}

async function handleCancelPO(id) {
  actionLoadingId.value = id
  try {
    await inventoryStore.cancelPurchaseOrder(id, { reason: 'Dibatalkan oleh pengguna' })
  } catch (err) {
    console.error('Failed to cancel PO:', err)
  } finally {
    actionLoadingId.value = null
  }
}

async function handleExecuteReceive() {
  receiveError.value = ''
  const validItems = receiveForm.value.items.filter((it) => Number(it.quantity_received) > 0)
  if (validItems.length === 0) {
    receiveError.value = 'Isi kuantitas yang diterima (minimal 1 unit pada setidaknya satu item).'
    return
  }

  submittingReceive.value = true
  try {
    await inventoryStore.receivePurchaseOrder(selectedPOForReceive.value.id, {
      items: validItems.map((it) => ({
        product_id: it.product_id,
        quantity_received: Number(it.quantity_received),
        unit_cost: Number(it.unit_cost),
        batch_number: it.batch_number?.trim() || null
      }))
    })
    showReceiveModal.value = false
  } catch (err) {
    receiveError.value = err.detail || err.message || 'Gagal mencatat penerimaan barang.'
  } finally {
    submittingReceive.value = false
  }
}
</script>
