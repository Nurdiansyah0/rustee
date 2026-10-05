import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'

export const useInventoryStore = defineStore('inventory', () => {
  // State
  const warehouses = ref([])
  const activeWarehouseId = ref(null)
  const products = ref([])
  const stockItems = ref([])
  const movements = ref([])
  const purchaseOrders = ref([])
  const loading = ref(false)
  const error = ref(null)

  // Getters
  const activeWarehouse = computed(() => {
    if (!activeWarehouseId.value) return null
    return warehouses.value.find((w) => w.id === activeWarehouseId.value) || null
  })

  const filteredStockItems = computed(() => {
    if (!activeWarehouseId.value) return stockItems.value
    return stockItems.value.filter((s) => s.warehouse_id === activeWarehouseId.value)
  })

  const lowStockItems = computed(() => {
    const list = activeWarehouseId.value ? filteredStockItems.value : stockItems.value
    return list.filter((s) => Number(s.quantity_on_hand || 0) <= Number(s.reorder_threshold || 0))
  })

  const totalStockValuation = computed(() => {
    const list = activeWarehouseId.value ? filteredStockItems.value : stockItems.value
    return list.reduce((acc, s) => {
      const qty = Number(s.quantity_on_hand || 0)
      const cost = Number(s.average_cost || 0)
      return acc + qty * cost
    }, 0)
  })

  const totalSkuCount = computed(() => {
    const list = activeWarehouseId.value ? filteredStockItems.value : stockItems.value
    const uniqueProducts = new Set(list.map((s) => s.product_id))
    return uniqueProducts.size
  })

  const totalQuantityOnHand = computed(() => {
    const list = activeWarehouseId.value ? filteredStockItems.value : stockItems.value
    return list.reduce((acc, s) => acc + Number(s.quantity_on_hand || 0), 0)
  })

  const getWarehouseById = (id) => warehouses.value.find((w) => w.id === id)
  const getProductById = (id) => products.value.find((p) => p.id === id)

  // Actions
  function setActiveWarehouse(id) {
    activeWarehouseId.value = id || null
  }

  async function fetchWarehouses() {
    loading.value = true
    error.value = null
    try {
      const res = await api.getWarehouses()
      const list = Array.isArray(res) ? res : (res?.warehouses || [])
      warehouses.value = list
      if (!activeWarehouseId.value && list.length > 0) {
        const defaultWh = list.find((w) => w.is_default) || list[0]
        activeWarehouseId.value = defaultWh.id
      }
      return warehouses.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat data gudang'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function createWarehouse(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.createWarehouse(data)
      await fetchWarehouses()
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membuat gudang baru'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function fetchProducts() {
    loading.value = true
    error.value = null
    try {
      const res = await api.getProducts()
      const list = Array.isArray(res) ? res : (res?.products || [])
      products.value = list
      return products.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat katalog produk'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function createProduct(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.createProduct(data)
      await Promise.all([fetchProducts(), fetchStock()])
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membuat produk baru'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function fetchStock(params = {}) {
    loading.value = true
    error.value = null
    try {
      const queryParams = { ...params }
      if (activeWarehouseId.value && !queryParams.warehouse_id) {
        // If caller didn't explicitly request all warehouses or a specific one
        // keep queryParams flexible
      }
      const res = await api.getStockItems(queryParams)
      const list = Array.isArray(res) ? res : (res?.stock_items || [])
      stockItems.value = list
      return stockItems.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat data stok barang'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function fetchMovements(params = {}) {
    loading.value = true
    error.value = null
    try {
      const res = await api.getStockMovements(params)
      const list = Array.isArray(res) ? res : (res?.movements || [])
      movements.value = list
      return movements.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat riwayat mutasi stok'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function createMovement(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.createStockMovement(data)
      await Promise.all([fetchStock(), fetchMovements()])
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal mencatat mutasi stok'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function transferStock(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.transferStock(data)
      await Promise.all([fetchStock(), fetchMovements()])
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal melakukan transfer stok antar gudang'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function adjustStock(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.adjustStock(data)
      await Promise.all([fetchStock(), fetchMovements()])
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal melakukan penyesuaian stok opname'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function fetchPurchaseOrders() {
    loading.value = true
    error.value = null
    try {
      const res = await api.getPurchaseOrders()
      const list = Array.isArray(res) ? res : (res?.purchase_orders || [])
      purchaseOrders.value = list
      return purchaseOrders.value
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat daftar pesanan pembelian'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function createPurchaseOrder(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.createPurchaseOrder(data)
      await fetchPurchaseOrders()
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membuat purchase order'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function orderPurchaseOrder(id) {
    loading.value = true
    error.value = null
    try {
      const res = await api.orderPurchaseOrder(id)
      await fetchPurchaseOrders()
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal mengubah status PO ke Dipesan'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function receivePurchaseOrder(id, data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.receivePurchaseOrder(id, data)
      await Promise.all([fetchPurchaseOrders(), fetchStock(), fetchMovements()])
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal menerima barang pesanan pembelian'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function cancelPurchaseOrder(id, data = {}) {
    loading.value = true
    error.value = null
    try {
      const res = await api.cancelPurchaseOrder(id, data)
      await fetchPurchaseOrders()
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membatalkan pesanan pembelian'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function refreshAll() {
    loading.value = true
    error.value = null
    try {
      await Promise.allSettled([
        fetchWarehouses(),
        fetchProducts(),
        fetchStock(),
        fetchMovements(),
        fetchPurchaseOrders()
      ])
    } finally {
      loading.value = false
    }
  }

  return {
    // State
    warehouses,
    activeWarehouseId,
    products,
    stockItems,
    movements,
    purchaseOrders,
    loading,
    error,

    // Getters
    activeWarehouse,
    filteredStockItems,
    lowStockItems,
    totalStockValuation,
    totalSkuCount,
    totalQuantityOnHand,
    getWarehouseById,
    getProductById,

    // Actions
    setActiveWarehouse,
    fetchWarehouses,
    createWarehouse,
    fetchProducts,
    createProduct,
    fetchStock,
    fetchMovements,
    createMovement,
    transferStock,
    adjustStock,
    fetchPurchaseOrders,
    createPurchaseOrder,
    orderPurchaseOrder,
    receivePurchaseOrder,
    cancelPurchaseOrder,
    refreshAll
  }
})
