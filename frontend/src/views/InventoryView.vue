<template>
  <div class="space-y-5">
    <!-- Header & Action Row -->
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-xl font-bold text-content-primary tracking-tight">Inventaris & Stok Multi-Gudang</h2>
          <Badge variant="brand" size="sm">Multi-Lokasi</Badge>
        </div>
        <p class="text-xs text-content-secondary mt-0.5">
          Pelacakan stok per lokasi gudang, mutasi berkala, valuasi WAC, dan stok opname
        </p>
      </div>

      <div class="flex items-center gap-2 flex-wrap">
        <!-- Switch to Purchasing View -->
        <button
          type="button"
          @click="$emit('nav', 'purchasing')"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl border border-border-default hover:bg-surface-subtle text-content-secondary hover:text-content-primary transition font-bold text-xs cursor-pointer shadow-xs"
          title="Buka Manajemen Pesanan Pembelian"
        >
          <ShoppingCart class="w-3.5 h-3.5 text-brand-default" />
          <span>Pesanan Pembelian</span>
          <ArrowRight class="w-3 h-3 text-content-muted" />
        </button>

        <!-- Quick Opname Adjustment -->
        <Button
          variant="secondary"
          size="sm"
          @click="openAdjustmentModal"
        >
          <template #prefix>
            <SlidersHorizontal class="w-3.5 h-3.5 mr-1" />
          </template>
          Opname
        </Button>

        <!-- Transfer Stock -->
        <Button
          variant="secondary"
          size="sm"
          @click="openTransferModal"
        >
          <template #prefix>
            <ArrowLeftRight class="w-3.5 h-3.5 mr-1" />
          </template>
          Transfer
        </Button>

        <!-- Add Product -->
        <Button
          variant="primary"
          size="sm"
          @click="showCreateProductModal = true"
        >
          <template #prefix>
            <Plus class="w-3.5 h-3.5 mr-1" />
          </template>
          + Produk
        </Button>
      </div>
    </div>

    <!-- Low Stock Alert Banner (PRD §27, Feature 4) -->
    <div
      v-if="inventoryStore.lowStockItems.length > 0"
      class="p-3.5 sm:p-4 rounded-2xl bg-amber-500/10 border border-amber-500/30 flex items-start gap-3 text-amber-600 dark:text-amber-400"
    >
      <AlertTriangle class="w-5 h-5 shrink-0 mt-0.5 text-amber-500" />
      <div class="flex-1 min-w-0 text-xs">
        <div class="font-bold flex items-center gap-2">
          <span>Peringatan Stok Menipis</span>
          <span class="px-1.5 py-0.2 rounded-full bg-amber-500/20 text-[10px] font-extrabold">
            {{ inventoryStore.lowStockItems.length }} Item Perlu Reorder
          </span>
        </div>
        <p class="mt-0.5 text-content-secondary">
          Beberapa barang berada pada atau di bawah batas ambang pemesanan ulang (reorder threshold).
          Segera terbitkan Pesanan Pembelian (PO) ke pemasok untuk mencegah kekosongan stok.
        </p>
      </div>
      <button
        type="button"
        @click="$emit('nav', 'purchasing')"
        class="text-xs font-bold underline hover:no-underline shrink-0 text-amber-600 dark:text-amber-400 self-center cursor-pointer"
      >
        Buat PO &rarr;
      </button>
    </div>

    <!-- Warehouse Selector Filter Bar -->
    <div class="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 p-3 rounded-2xl bg-surface-card border border-border-subtle shadow-card">
      <div class="flex items-center gap-2 overflow-x-auto scrollbar-none pb-1 sm:pb-0">
        <span class="text-xs font-bold text-content-secondary shrink-0 mr-1 flex items-center gap-1.5">
          <WarehouseIcon class="w-3.5 h-3.5 text-brand-default" />
          Lokasi:
        </span>
        <button
          type="button"
          @click="handleSelectWarehouse(null)"
          :class="[
            'px-3 py-1.5 rounded-xl text-xs font-bold transition shrink-0 cursor-pointer',
            !inventoryStore.activeWarehouseId
              ? 'bg-brand-default text-content-inverse shadow-xs'
              : 'bg-surface-subtle text-content-secondary hover:text-content-primary'
          ]"
        >
          Semua Gudang ({{ inventoryStore.warehouses.length }})
        </button>
        <button
          v-for="wh in inventoryStore.warehouses"
          :key="wh.id"
          type="button"
          @click="handleSelectWarehouse(wh.id)"
          :class="[
            'px-3 py-1.5 rounded-xl text-xs font-bold transition shrink-0 cursor-pointer flex items-center gap-1.5',
            inventoryStore.activeWarehouseId === wh.id
              ? 'bg-brand-default text-content-inverse shadow-xs'
              : 'bg-surface-subtle text-content-secondary hover:text-content-primary'
          ]"
        >
          <span>{{ wh.name }}</span>
          <span
            v-if="wh.is_default"
            class="text-[9px] px-1 py-0.2 rounded font-extrabold uppercase"
            :class="inventoryStore.activeWarehouseId === wh.id ? 'bg-white/20' : 'bg-brand-muted text-brand-default'"
          >
            Utama
          </span>
        </button>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <Button
          variant="outline"
          size="sm"
          @click="showCreateWarehouseModal = true"
        >
          <template #prefix>
            <Plus class="w-3.5 h-3.5 mr-1" />
          </template>
          + Tambah Lokasi Gudang
        </Button>
      </div>
    </div>

    <!-- KPI Summary Cards -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
      <!-- Total SKU -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Total Katalog SKU</span>
          <div class="w-8 h-8 rounded-xl bg-blue-500/10 text-blue-500 flex items-center justify-center">
            <Package class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-2xl font-extrabold text-content-primary tracking-tight">
            {{ inventoryStore.totalSkuCount }}
          </span>
          <span class="text-xs text-content-muted ml-1">Produk</span>
        </div>
      </Card>

      <!-- Total Qty On Hand -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Total Unit Fisik</span>
          <div class="w-8 h-8 rounded-xl bg-brand-default/10 text-brand-default flex items-center justify-center">
            <Boxes class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-2xl font-extrabold text-content-primary tracking-tight">
            {{ formatNumber(inventoryStore.totalQuantityOnHand) }}
          </span>
          <span class="text-xs text-content-muted ml-1">Unit</span>
        </div>
      </Card>

      <!-- Total Valuation (IDR WAC) -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Total Valuasi Aset</span>
          <div class="w-8 h-8 rounded-xl bg-emerald-500/10 text-emerald-500 flex items-center justify-center">
            <Coins class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2">
          <span class="text-lg sm:text-xl font-extrabold text-content-primary tracking-tight">
            {{ formatIDR(inventoryStore.totalStockValuation) }}
          </span>
          <div class="text-[10px] text-content-muted">Metode Rata-Rata Tertimbang (WAC)</div>
        </div>
      </Card>

      <!-- Low Stock Items Alert -->
      <Card padding="md" variant="default" class="relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-content-secondary">Peringatan Reorder</span>
          <div
            class="w-8 h-8 rounded-xl flex items-center justify-center"
            :class="inventoryStore.lowStockItems.length > 0 ? 'bg-amber-500/15 text-amber-500' : 'bg-surface-subtle text-content-muted'"
          >
            <AlertCircle class="w-4 h-4 stroke-[2]" />
          </div>
        </div>
        <div class="mt-2 flex items-baseline gap-2">
          <span
            class="text-2xl font-extrabold tracking-tight"
            :class="inventoryStore.lowStockItems.length > 0 ? 'text-amber-500' : 'text-content-primary'"
          >
            {{ inventoryStore.lowStockItems.length }}
          </span>
          <Badge
            :variant="inventoryStore.lowStockItems.length > 0 ? 'warning' : 'neutral'"
            size="sm"
          >
            {{ inventoryStore.lowStockItems.length > 0 ? 'Perlu Restock' : 'Aman' }}
          </Badge>
        </div>
      </Card>
    </div>

    <!-- Sub-tab Navigation (Stok Barang, Mutasi, Gudang) -->
    <div class="flex items-center gap-1 border-b border-border-subtle pt-2">
      <button
        type="button"
        @click="activeSubTab = 'stock'"
        :class="[
          'px-4 py-2.5 text-xs font-bold border-b-2 transition cursor-pointer flex items-center gap-2 -mb-px',
          activeSubTab === 'stock'
            ? 'border-brand-default text-brand-default'
            : 'border-transparent text-content-secondary hover:text-content-primary'
        ]"
      >
        <Package class="w-4 h-4" />
        <span>Daftar Stok Barang</span>
        <span class="px-1.5 py-0.2 rounded-full bg-surface-subtle text-[10px] text-content-muted font-semibold">
          {{ displayedStockItems.length }}
        </span>
      </button>

      <button
        type="button"
        @click="activeSubTab = 'movements'"
        :class="[
          'px-4 py-2.5 text-xs font-bold border-b-2 transition cursor-pointer flex items-center gap-2 -mb-px',
          activeSubTab === 'movements'
            ? 'border-brand-default text-brand-default'
            : 'border-transparent text-content-secondary hover:text-content-primary'
        ]"
      >
        <History class="w-4 h-4" />
        <span>Riwayat Mutasi Stok</span>
        <span class="px-1.5 py-0.2 rounded-full bg-surface-subtle text-[10px] text-content-muted font-semibold">
          {{ inventoryStore.movements.length }}
        </span>
      </button>

      <button
        type="button"
        @click="activeSubTab = 'warehouses'"
        :class="[
          'px-4 py-2.5 text-xs font-bold border-b-2 transition cursor-pointer flex items-center gap-2 -mb-px',
          activeSubTab === 'warehouses'
            ? 'border-brand-default text-brand-default'
            : 'border-transparent text-content-secondary hover:text-content-primary'
        ]"
      >
        <WarehouseIcon class="w-4 h-4" />
        <span>Master Lokasi Gudang</span>
        <span class="px-1.5 py-0.2 rounded-full bg-surface-subtle text-[10px] text-content-muted font-semibold">
          {{ inventoryStore.warehouses.length }}
        </span>
      </button>
    </div>

    <!-- SUB-TAB 1: DAFTAR STOK BARANG -->
    <div v-if="activeSubTab === 'stock'" class="space-y-3">
      <!-- Search and filter -->
      <div class="flex flex-col sm:flex-row gap-2.5 items-stretch sm:items-center justify-between">
        <div class="relative flex-1">
          <Search class="w-4 h-4 text-content-muted absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
          <input
            v-model="stockSearchQuery"
            type="text"
            placeholder="Cari berdasarkan SKU, nama produk, atau lokasi bin..."
            class="w-full pl-9 pr-8 py-2 bg-surface-card border border-border-subtle hover:border-border-default focus:border-brand-default focus:ring-1 focus:ring-brand-default rounded-xl text-xs text-content-primary placeholder:text-content-muted transition outline-none"
          />
          <button
            v-if="stockSearchQuery"
            type="button"
            @click="stockSearchQuery = ''"
            class="absolute right-2.5 top-1/2 -translate-y-1/2 text-content-muted hover:text-content-primary p-0.5"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>

        <div class="flex items-center gap-2">
          <button
            type="button"
            @click="filterOnlyLowStock = !filterOnlyLowStock"
            :class="[
              'px-3 py-2 rounded-xl text-xs font-bold transition flex items-center gap-1.5 cursor-pointer border',
              filterOnlyLowStock
                ? 'bg-amber-500/15 text-amber-500 border-amber-500/30'
                : 'bg-surface-card text-content-secondary border-border-subtle hover:border-border-default'
            ]"
          >
            <AlertTriangle class="w-3.5 h-3.5" />
            <span>Hanya Stok Menipis</span>
          </button>
        </div>
      </div>

      <!-- Table View -->
      <div class="overflow-x-auto rounded-2xl bg-surface-card border border-border-subtle shadow-card">
        <table class="w-full text-left border-collapse text-xs">
          <thead>
            <tr class="border-b border-border-subtle bg-surface-subtle/50 text-content-secondary font-bold">
              <th class="py-3 px-4">SKU / Produk</th>
              <th class="py-3 px-4">Lokasi Gudang</th>
              <th class="py-3 px-4 text-right">Stok Fisik</th>
              <th class="py-3 px-4 text-right">Dipesan (Rsv)</th>
              <th class="py-3 px-4 text-right">Batas Min</th>
              <th class="py-3 px-4 text-right">Biaya Rata-Rata (WAC)</th>
              <th class="py-3 px-4 text-right">Total Valuasi</th>
              <th class="py-3 px-4 text-center">Status</th>
              <th class="py-3 px-4 text-right">Aksi</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border-subtle">
            <tr
              v-for="item in displayedStockItems"
              :key="item.id"
              class="hover:bg-surface-subtle/60 transition group"
            >
              <!-- SKU & Name -->
              <td class="py-3 px-4">
                <div class="font-bold text-content-primary flex items-center gap-2">
                  <span class="font-mono text-[11px] px-1.5 py-0.5 rounded bg-surface-subtle border border-border-subtle font-extrabold text-brand-default">
                    {{ item.product_sku || resolveProduct(item.product_id)?.sku || 'SKU' }}
                  </span>
                  <span>{{ item.product_name || resolveProduct(item.product_id)?.name || 'Produk' }}</span>
                </div>
                <div class="text-[10px] text-content-muted mt-0.5 flex items-center gap-2">
                  <span>Satuan: <strong class="text-content-secondary uppercase">{{ item.product_unit || resolveProduct(item.product_id)?.unit || 'PCS' }}</strong></span>
                  <span v-if="item.bin_location">&bull; Bin: <strong class="text-content-secondary">{{ item.bin_location }}</strong></span>
                </div>
              </td>

              <!-- Warehouse -->
              <td class="py-3 px-4 text-content-secondary">
                <div class="font-medium">
                  {{ item.warehouse_name || resolveWarehouse(item.warehouse_id)?.name || 'Gudang' }}
                </div>
              </td>

              <!-- Quantity on hand -->
              <td class="py-3 px-4 text-right font-extrabold text-content-primary">
                {{ formatNumber(item.quantity_on_hand) }}
              </td>

              <!-- Quantity reserved -->
              <td class="py-3 px-4 text-right text-content-muted">
                {{ formatNumber(item.quantity_reserved) }}
              </td>

              <!-- Reorder threshold -->
              <td class="py-3 px-4 text-right text-content-secondary font-medium">
                {{ formatNumber(item.reorder_threshold) }}
              </td>

              <!-- WAC Unit Cost -->
              <td class="py-3 px-4 text-right font-medium text-content-secondary">
                {{ formatIDR(item.average_cost) }}
              </td>

              <!-- Total Valuation -->
              <td class="py-3 px-4 text-right font-extrabold text-content-primary">
                {{ formatIDR(Number(item.quantity_on_hand || 0) * Number(item.average_cost || 0)) }}
              </td>

              <!-- Status Badge -->
              <td class="py-3 px-4 text-center">
                <Badge
                  v-if="Number(item.quantity_on_hand) === 0"
                  variant="expense"
                  size="sm"
                >
                  Habis
                </Badge>
                <Badge
                  v-else-if="Number(item.quantity_on_hand) <= Number(item.reorder_threshold)"
                  variant="warning"
                  size="sm"
                >
                  Menipis
                </Badge>
                <Badge
                  v-else
                  variant="income"
                  size="sm"
                >
                  Tersedia
                </Badge>
              </td>

              <!-- Action buttons -->
              <td class="py-3 px-4 text-right">
                <div class="flex items-center justify-end gap-1.5">
                  <button
                    type="button"
                    @click="initiateAdjustment(item)"
                    class="p-1 rounded-lg hover:bg-surface-subtle text-content-muted hover:text-brand-default transition"
                    title="Sesuaikan Stok Opname"
                  >
                    <SlidersHorizontal class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    @click="initiateTransfer(item)"
                    class="p-1 rounded-lg hover:bg-surface-subtle text-content-muted hover:text-brand-default transition"
                    title="Transfer ke Gudang Lain"
                  >
                    <ArrowLeftRight class="w-3.5 h-3.5" />
                  </button>
                </div>
              </td>
            </tr>

            <tr v-if="displayedStockItems.length === 0">
              <td colspan="9" class="py-8 text-center text-content-muted text-xs">
                Tidak ada data stok yang sesuai dengan filter atau pencarian.
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- SUB-TAB 2: RIWAYAT MUTASI STOK -->
    <div v-else-if="activeSubTab === 'movements'" class="space-y-3">
      <div class="overflow-x-auto rounded-2xl bg-surface-card border border-border-subtle shadow-card">
        <table class="w-full text-left border-collapse text-xs">
          <thead>
            <tr class="border-b border-border-subtle bg-surface-subtle/50 text-content-secondary font-bold">
              <th class="py-3 px-4">Waktu / Tanggal</th>
              <th class="py-3 px-4">Tipe Mutasi</th>
              <th class="py-3 px-4">Produk</th>
              <th class="py-3 px-4">Gudang Asal &rarr; Tujuan</th>
              <th class="py-3 px-4 text-right">Kuantitas</th>
              <th class="py-3 px-4 text-right">Biaya Satuan</th>
              <th class="py-3 px-4">Batch / Catatan</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border-subtle">
            <tr
              v-for="mov in inventoryStore.movements"
              :key="mov.id"
              class="hover:bg-surface-subtle/60 transition"
            >
              <!-- Timestamp -->
              <td class="py-3 px-4 text-content-secondary font-mono text-[11px]">
                {{ formatDateTime(mov.created_at) }}
              </td>

              <!-- Movement Type Badge -->
              <td class="py-3 px-4">
                <Badge
                  :variant="getMovementBadgeVariant(mov.movement_type)"
                  size="sm"
                >
                  {{ mov.movement_type }}
                </Badge>
              </td>

              <!-- Product -->
              <td class="py-3 px-4">
                <div class="font-bold text-content-primary">
                  {{ resolveProduct(mov.product_id)?.name || 'Produk' }}
                </div>
                <div class="text-[10px] text-content-muted font-mono">
                  {{ resolveProduct(mov.product_id)?.sku }}
                </div>
              </td>

              <!-- Route -->
              <td class="py-3 px-4 text-content-secondary">
                <div class="flex items-center gap-1.5 font-medium">
                  <span v-if="mov.source_warehouse_id">
                    {{ resolveWarehouse(mov.source_warehouse_id)?.name || 'Gudang Asal' }}
                  </span>
                  <span v-else class="text-content-muted">Eksternal (Supplier)</span>

                  <ArrowRight class="w-3 h-3 text-content-muted shrink-0" />

                  <span v-if="mov.destination_warehouse_id">
                    {{ resolveWarehouse(mov.destination_warehouse_id)?.name || 'Gudang Tujuan' }}
                  </span>
                  <span v-else class="text-content-muted">Eksternal (Konsumen)</span>
                </div>
              </td>

              <!-- Quantity -->
              <td
                class="py-3 px-4 text-right font-extrabold"
                :class="mov.movement_type === 'INBOUND' ? 'text-income-default' : (mov.movement_type === 'OUTBOUND' ? 'text-expense-default' : 'text-content-primary')"
              >
                {{ mov.movement_type === 'INBOUND' ? '+' : (mov.movement_type === 'OUTBOUND' ? '-' : '') }}{{ formatNumber(mov.quantity) }}
              </td>

              <!-- Unit cost -->
              <td class="py-3 px-4 text-right text-content-secondary font-medium">
                {{ mov.unit_cost ? formatIDR(mov.unit_cost) : '-' }}
              </td>

              <!-- Batch & Notes -->
              <td class="py-3 px-4 text-content-secondary">
                <div v-if="mov.batch_number" class="font-mono text-[10px] text-brand-default font-bold">
                  {{ mov.batch_number }}
                </div>
                <div class="text-[11px] text-content-muted truncate max-w-xs">
                  {{ mov.notes || '-' }}
                </div>
              </td>
            </tr>

            <tr v-if="inventoryStore.movements.length === 0">
              <td colspan="7" class="py-8 text-center text-content-muted text-xs">
                Belum ada riwayat mutasi stok.
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- SUB-TAB 3: MASTER LOKASI GUDANG -->
    <div v-else-if="activeSubTab === 'warehouses'" class="space-y-4">
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <div
          v-for="wh in inventoryStore.warehouses"
          :key="wh.id"
          class="p-5 rounded-2xl bg-surface-card border border-border-subtle shadow-card flex flex-col justify-between space-y-4"
        >
          <div>
            <div class="flex items-center justify-between">
              <span class="font-mono text-xs font-extrabold px-2 py-0.5 rounded bg-brand-muted text-brand-default border border-brand-border">
                {{ wh.code }}
              </span>
              <Badge v-if="wh.is_default" variant="brand" size="sm">Gudang Utama</Badge>
            </div>
            <h3 class="text-base font-extrabold text-content-primary mt-2.5">{{ wh.name }}</h3>
            <p class="text-xs text-content-secondary mt-1 flex items-start gap-1.5">
              <MapPin class="w-3.5 h-3.5 shrink-0 mt-0.5 text-content-muted" />
              <span>{{ wh.address || 'Alamat belum diatur' }}</span>
            </p>
          </div>

          <div class="pt-3 border-t border-border-subtle flex items-center justify-between text-xs text-content-secondary">
            <span>Dibuat: {{ formatDate(wh.created_at) }}</span>
            <button
              type="button"
              @click="handleSelectWarehouse(wh.id); activeSubTab = 'stock'"
              class="text-brand-default font-bold hover:underline cursor-pointer"
            >
              Lihat Stok &rarr;
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- MODAL 1: CREATE PRODUCT -->
    <ModalSheet
      v-model="showCreateProductModal"
      title="Tambah Produk Katalog Baru"
      description="Daftarkan SKU produk baru untuk inventaris & multi-gudang"
      size="md"
    >
      <form @submit.prevent="handleCreateProduct" class="space-y-4 py-2">
        <Input
          v-model="productForm.name"
          label="Nama Produk"
          placeholder="Contoh: Biji Kopi Arabika 1kg"
          required
        />

        <div class="grid grid-cols-2 gap-3">
          <Input
            v-model="productForm.unit"
            label="Satuan Unit"
            placeholder="PCS / KG / BOX"
            required
          />
          <Input
            v-model.number="productForm.reorder_threshold"
            type="number"
            label="Batas Reorder (Min Stok)"
            placeholder="10"
            required
          />
        </div>

        <div class="grid grid-cols-2 gap-3">
          <Input
            v-model.number="productForm.cost_price"
            type="number"
            label="Harga Pokok Pembelian (HPP)"
            prefix="Rp"
            placeholder="100000"
            required
          />
          <Input
            v-model.number="productForm.sale_price"
            type="number"
            label="Harga Jual Standar"
            prefix="Rp"
            placeholder="150000"
            required
          />
        </div>

        <div class="p-3 rounded-xl bg-surface-subtle text-xs text-content-secondary">
          <p>
            Nomor SKU akan digenerate otomatis oleh server secara berurutan (<code class="font-bold text-brand-default">SKU-XXXXXX</code>) untuk menjamin keunikan katalog.
          </p>
        </div>
      </form>

      <template #footer>
        <Button variant="outline" size="sm" @click="showCreateProductModal = false">
          Batal
        </Button>
        <Button
          variant="primary"
          size="sm"
          :loading="submitting"
          @click="handleCreateProduct"
        >
          Simpan Produk
        </Button>
      </template>
    </ModalSheet>

    <!-- MODAL 2: CREATE WAREHOUSE -->
    <ModalSheet
      v-model="showCreateWarehouseModal"
      title="Tambah Lokasi Gudang Baru"
      description="Buat lokasi fisik atau transit baru untuk multi-gudang"
      size="md"
    >
      <form @submit.prevent="handleCreateWarehouse" class="space-y-4 py-2">
        <div class="grid grid-cols-3 gap-3">
          <div class="col-span-1">
            <Input
              v-model="warehouseForm.code"
              label="Kode Gudang"
              placeholder="WH-03"
              required
            />
          </div>
          <div class="col-span-2">
            <Input
              v-model="warehouseForm.name"
              label="Nama Lokasi Gudang"
              placeholder="Contoh: Gudang Surabaya"
              required
            />
          </div>
        </div>

        <Input
          v-model="warehouseForm.address"
          label="Alamat Lengkap"
          placeholder="Jl. Raya Industri No. 45, Rungkut"
        />

        <div class="flex items-center gap-2 pt-1">
          <input
            id="is_default_checkbox"
            v-model="warehouseForm.is_default"
            type="checkbox"
            class="rounded border-border-default text-brand-default focus:ring-brand-default w-4 h-4 cursor-pointer"
          />
          <label for="is_default_checkbox" class="text-xs font-semibold text-content-primary cursor-pointer select-none">
            Jadikan sebagai Gudang Utama (Default)
          </label>
        </div>
      </form>

      <template #footer>
        <Button variant="outline" size="sm" @click="showCreateWarehouseModal = false">
          Batal
        </Button>
        <Button
          variant="primary"
          size="sm"
          :loading="submitting"
          @click="handleCreateWarehouse"
        >
          Simpan Gudang
        </Button>
      </template>
    </ModalSheet>

    <!-- MODAL 3: TRANSFER STOCK -->
    <ModalSheet
      v-model="showTransferModal"
      title="Transfer Stok Antar Gudang"
      description="Pindahkan unit produk secara aman dari satu lokasi ke lokasi lain"
      size="md"
    >
      <form @submit.prevent="handleTransferStock" class="space-y-4 py-2">
        <!-- Product selector -->
        <div class="space-y-1.5">
          <label class="text-xs font-semibold text-content-secondary">Pilih Produk</label>
          <select
            v-model="transferForm.product_id"
            class="w-full p-2.5 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
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

        <div class="grid grid-cols-2 gap-3">
          <!-- Source warehouse -->
          <div class="space-y-1.5">
            <label class="text-xs font-semibold text-content-secondary">Gudang Asal</label>
            <select
              v-model="transferForm.source_warehouse_id"
              class="w-full p-2.5 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
              required
            >
              <option value="" disabled>-- Gudang Asal --</option>
              <option
                v-for="wh in inventoryStore.warehouses"
                :key="wh.id"
                :value="wh.id"
              >
                {{ wh.name }}
              </option>
            </select>
          </div>

          <!-- Destination warehouse -->
          <div class="space-y-1.5">
            <label class="text-xs font-semibold text-content-secondary">Gudang Tujuan</label>
            <select
              v-model="transferForm.destination_warehouse_id"
              class="w-full p-2.5 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
              required
            >
              <option value="" disabled>-- Gudang Tujuan --</option>
              <option
                v-for="wh in availableDestinationWarehouses"
                :key="wh.id"
                :value="wh.id"
              >
                {{ wh.name }}
              </option>
            </select>
          </div>
        </div>

        <!-- Quantity -->
        <Input
          v-model.number="transferForm.quantity"
          type="number"
          label="Jumlah Unit yang Ditransfer"
          placeholder="1"
          required
        />

        <Input
          v-model="transferForm.notes"
          label="Catatan Pengiriman / Alasan Transfer"
          placeholder="Pengiriman stok ke cabang bandung"
        />

        <div v-if="transferError" class="p-2.5 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-500 text-xs">
          {{ transferError }}
        </div>
      </form>

      <template #footer>
        <Button variant="outline" size="sm" @click="showTransferModal = false">
          Batal
        </Button>
        <Button
          variant="primary"
          size="sm"
          :loading="submitting"
          @click="handleTransferStock"
        >
          Kirim Transfer
        </Button>
      </template>
    </ModalSheet>

    <!-- MODAL 4: STOCK OPNAME ADJUSTMENT -->
    <ModalSheet
      v-model="showAdjustmentModal"
      title="Penyesuaian Fisik Stok Opname"
      description="Catat selisih hasil perhitungan fisik aktual dan sinkronkan pembukuan"
      size="md"
    >
      <form @submit.prevent="handleAdjustStock" class="space-y-4 py-2">
        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <label class="text-xs font-semibold text-content-secondary">Lokasi Gudang</label>
            <select
              v-model="adjustmentForm.warehouse_id"
              class="w-full p-2.5 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
              required
            >
              <option value="" disabled>-- Pilih Gudang --</option>
              <option
                v-for="wh in inventoryStore.warehouses"
                :key="wh.id"
                :value="wh.id"
              >
                {{ wh.name }}
              </option>
            </select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-semibold text-content-secondary">Pilih Produk</label>
            <select
              v-model="adjustmentForm.product_id"
              class="w-full p-2.5 bg-surface-sunken border border-border-default focus:border-brand-default rounded-lg text-xs text-content-primary outline-none"
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
        </div>

        <!-- Current vs Actual -->
        <div class="p-3 rounded-xl bg-surface-subtle border border-border-subtle flex items-center justify-between text-xs">
          <div>
            <div class="text-content-muted">Stok Sistem Saat Ini:</div>
            <div class="text-base font-extrabold text-content-primary">
              {{ formatNumber(currentSystemQuantity) }} Unit
            </div>
          </div>
          <div class="text-right">
            <div class="text-content-muted">Selisih (Variance):</div>
            <div
              class="text-base font-extrabold"
              :class="stockVariance === 0 ? 'text-content-primary' : (stockVariance > 0 ? 'text-income-default' : 'text-expense-default')"
            >
              {{ stockVariance > 0 ? '+' : '' }}{{ formatNumber(stockVariance) }} Unit
            </div>
          </div>
        </div>

        <Input
          v-model.number="adjustmentForm.actual_quantity"
          type="number"
          label="Jumlah Stok Fisik Aktual (Hasil Hitung Opname)"
          placeholder="0"
          required
        />

        <Input
          v-model="adjustmentForm.reason"
          label="Alasan Penyesuaian"
          placeholder="Selisih fisik berkala / kerusakan barang / audit"
          required
        />

        <div v-if="adjustmentError" class="p-2.5 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-500 text-xs">
          {{ adjustmentError }}
        </div>
      </form>

      <template #footer>
        <Button variant="outline" size="sm" @click="showAdjustmentModal = false">
          Batal
        </Button>
        <Button
          variant="primary"
          size="sm"
          :loading="submitting"
          @click="handleAdjustStock"
        >
          Terapkan Opname
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
  Package,
  Boxes,
  Coins,
  AlertTriangle,
  AlertCircle,
  Warehouse as WarehouseIcon,
  Plus,
  SlidersHorizontal,
  ArrowLeftRight,
  ArrowRight,
  Search,
  X,
  History,
  MapPin,
  ShoppingCart
} from 'lucide-vue-next'

defineEmits(['nav', 'open-add'])

const inventoryStore = useInventoryStore()

const activeSubTab = ref('stock')
const stockSearchQuery = ref('')
const filterOnlyLowStock = ref(false)

const submitting = ref(false)
const showCreateProductModal = ref(false)
const showCreateWarehouseModal = ref(false)
const showTransferModal = ref(false)
const showAdjustmentModal = ref(false)

const transferError = ref('')
const adjustmentError = ref('')

// Forms
const productForm = ref({
  name: '',
  unit: 'PCS',
  reorder_threshold: 10,
  cost_price: 0,
  sale_price: 0
})

const warehouseForm = ref({
  code: '',
  name: '',
  address: '',
  is_default: false
})

const transferForm = ref({
  source_warehouse_id: '',
  destination_warehouse_id: '',
  product_id: '',
  quantity: 1,
  notes: ''
})

const adjustmentForm = ref({
  warehouse_id: '',
  product_id: '',
  actual_quantity: 0,
  reason: 'Penyesuaian stok opname'
})

onMounted(async () => {
  try {
    await inventoryStore.refreshAll()
  } catch (err) {
    console.warn('Initial inventory refresh error:', err)
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

function formatDateTime(iso) {
  if (!iso) return '-'
  return new Date(iso).toLocaleString('id-ID', {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit'
  })
}

function resolveProduct(id) {
  return inventoryStore.getProductById(id)
}

function resolveWarehouse(id) {
  return inventoryStore.getWarehouseById(id)
}

function getMovementBadgeVariant(type) {
  switch (type) {
    case 'INBOUND':
      return 'income'
    case 'OUTBOUND':
      return 'expense'
    case 'TRANSFER':
      return 'brand'
    case 'ADJUSTMENT':
      return 'warning'
    default:
      return 'neutral'
  }
}

function handleSelectWarehouse(id) {
  inventoryStore.setActiveWarehouse(id)
}

const displayedStockItems = computed(() => {
  let list = inventoryStore.filteredStockItems

  if (filterOnlyLowStock.value) {
    list = list.filter((it) => Number(it.quantity_on_hand || 0) <= Number(it.reorder_threshold || 0))
  }

  if (stockSearchQuery.value.trim()) {
    const q = stockSearchQuery.value.toLowerCase().trim()
    list = list.filter((it) => {
      const sku = (it.product_sku || resolveProduct(it.product_id)?.sku || '').toLowerCase()
      const name = (it.product_name || resolveProduct(it.product_id)?.name || '').toLowerCase()
      const bin = (it.bin_location || '').toLowerCase()
      return sku.includes(q) || name.includes(q) || bin.includes(q)
    })
  }

  return list
})

const availableDestinationWarehouses = computed(() => {
  return inventoryStore.warehouses.filter((w) => w.id !== transferForm.value.source_warehouse_id)
})

const currentSystemQuantity = computed(() => {
  if (!adjustmentForm.value.warehouse_id || !adjustmentForm.value.product_id) return 0
  const item = inventoryStore.stockItems.find(
    (s) => s.warehouse_id === adjustmentForm.value.warehouse_id && s.product_id === adjustmentForm.value.product_id
  )
  return item ? Number(item.quantity_on_hand || 0) : 0
})

const stockVariance = computed(() => {
  const actual = Number(adjustmentForm.value.actual_quantity || 0)
  return actual - currentSystemQuantity.value
})

// Modal Openers
function openTransferModal() {
  transferError.value = ''
  transferForm.value = {
    source_warehouse_id: inventoryStore.activeWarehouseId || inventoryStore.warehouses[0]?.id || '',
    destination_warehouse_id: '',
    product_id: inventoryStore.products[0]?.id || '',
    quantity: 1,
    notes: ''
  }
  showTransferModal.value = true
}

function initiateTransfer(item) {
  transferError.value = ''
  transferForm.value = {
    source_warehouse_id: item.warehouse_id,
    destination_warehouse_id: '',
    product_id: item.product_id,
    quantity: 1,
    notes: ''
  }
  showTransferModal.value = true
}

function openAdjustmentModal() {
  adjustmentError.value = ''
  const defaultWh = inventoryStore.activeWarehouseId || inventoryStore.warehouses[0]?.id || ''
  const defaultPrd = inventoryStore.products[0]?.id || ''
  const currentStk = inventoryStore.stockItems.find((s) => s.warehouse_id === defaultWh && s.product_id === defaultPrd)

  adjustmentForm.value = {
    warehouse_id: defaultWh,
    product_id: defaultPrd,
    actual_quantity: currentStk ? Number(currentStk.quantity_on_hand || 0) : 0,
    reason: 'Penyesuaian stok opname'
  }
  showAdjustmentModal.value = true
}

function initiateAdjustment(item) {
  adjustmentError.value = ''
  adjustmentForm.value = {
    warehouse_id: item.warehouse_id,
    product_id: item.product_id,
    actual_quantity: Number(item.quantity_on_hand || 0),
    reason: 'Penyesuaian fisik opname'
  }
  showAdjustmentModal.value = true
}

// Handlers
async function handleCreateProduct() {
  if (!productForm.value.name.trim()) return
  submitting.value = true
  try {
    await inventoryStore.createProduct({
      name: productForm.value.name.trim(),
      unit: productForm.value.unit.toUpperCase().trim(),
      reorder_threshold: Number(productForm.value.reorder_threshold) || 10,
      cost_price: Number(productForm.value.cost_price) || 0,
      sale_price: Number(productForm.value.sale_price) || 0
    })
    showCreateProductModal.value = false
    productForm.value = {
      name: '',
      unit: 'PCS',
      reorder_threshold: 10,
      cost_price: 0,
      sale_price: 0
    }
  } catch (err) {
    console.error('Failed to create product:', err)
  } finally {
    submitting.value = false
  }
}

async function handleCreateWarehouse() {
  if (!warehouseForm.value.name.trim()) return
  submitting.value = true
  try {
    await inventoryStore.createWarehouse({
      code: warehouseForm.value.code.toUpperCase().trim(),
      name: warehouseForm.value.name.trim(),
      address: warehouseForm.value.address.trim() || null,
      is_default: Boolean(warehouseForm.value.is_default)
    })
    showCreateWarehouseModal.value = false
    warehouseForm.value = {
      code: '',
      name: '',
      address: '',
      is_default: false
    }
  } catch (err) {
    console.error('Failed to create warehouse:', err)
  } finally {
    submitting.value = false
  }
}

async function handleTransferStock() {
  transferError.value = ''
  if (!transferForm.value.source_warehouse_id || !transferForm.value.destination_warehouse_id) {
    transferError.value = 'Pilih gudang asal dan tujuan.'
    return
  }
  if (transferForm.value.source_warehouse_id === transferForm.value.destination_warehouse_id) {
    transferError.value = 'Gudang tujuan harus berbeda dari gudang asal.'
    return
  }
  if (Number(transferForm.value.quantity) <= 0) {
    transferError.value = 'Jumlah kuantitas transfer harus lebih dari 0.'
    return
  }

  submitting.value = true
  try {
    await inventoryStore.transferStock({
      source_warehouse_id: transferForm.value.source_warehouse_id,
      destination_warehouse_id: transferForm.value.destination_warehouse_id,
      product_id: transferForm.value.product_id,
      quantity: Number(transferForm.value.quantity),
      notes: transferForm.value.notes.trim() || null
    })
    showTransferModal.value = false
  } catch (err) {
    transferError.value = err.detail || err.message || 'Gagal melakukan transfer stok.'
  } finally {
    submitting.value = false
  }
}

async function handleAdjustStock() {
  adjustmentError.value = ''
  if (!adjustmentForm.value.warehouse_id || !adjustmentForm.value.product_id) {
    adjustmentError.value = 'Pilih gudang dan produk.'
    return
  }
  if (Number(adjustmentForm.value.actual_quantity) < 0) {
    adjustmentError.value = 'Jumlah fisik aktual tidak boleh negatif.'
    return
  }

  submitting.value = true
  try {
    await inventoryStore.adjustStock({
      warehouse_id: adjustmentForm.value.warehouse_id,
      product_id: adjustmentForm.value.product_id,
      actual_quantity: Number(adjustmentForm.value.actual_quantity),
      reason: adjustmentForm.value.reason.trim() || 'Penyesuaian stok opname'
    })
    showAdjustmentModal.value = false
  } catch (err) {
    adjustmentError.value = err.detail || err.message || 'Gagal menerapkan penyesuaian stok opname.'
  } finally {
    submitting.value = false
  }
}
</script>
