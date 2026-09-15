import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'

export const useCategoryStore = defineStore('categories', () => {
  // State
  const categories = ref([])
  const loading = ref(false)
  const error = ref(null)

  // Getters
  const incomeCategories = computed(() => {
    return categories.value.filter((c) => c.category_type === 'income' && !c.deleted_at)
  })

  const expenseCategories = computed(() => {
    return categories.value.filter((c) => c.category_type === 'expense' && !c.deleted_at)
  })

  const categoryMap = computed(() => {
    return new Map(categories.value.map((c) => [c.id, c]))
  })

  function getCategoryName(id) {
    if (!id) return 'Tanpa Kategori'
    const cat = categoryMap.value.get(id)
    return cat ? cat.name : 'Tanpa Kategori'
  }

  function getCategoryColor(id) {
    if (!id) return '#71717A'
    const cat = categoryMap.value.get(id)
    if (cat?.color) return cat.color
    return cat?.category_type === 'income' ? '#059669' : '#E11D48'
  }

  function getCategoryIcon(id) {
    if (!id) return 'Tag'
    const cat = categoryMap.value.get(id)
    return cat?.icon || (cat?.category_type === 'income' ? 'TrendingUp' : 'ShoppingBag')
  }

  // Actions
  async function fetchCategories() {
    loading.value = true
    error.value = null
    try {
      const data = await api.getCategories()
      categories.value = Array.isArray(data) ? data : (data?.data || [])
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal memuat kategori.'
      console.error('Failed to fetch categories', err)
    } finally {
      loading.value = false
    }
  }

  async function createCategory(data) {
    loading.value = true
    error.value = null
    try {
      const res = await api.createCategory(data)
      categories.value.push(res)
      return res
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal membuat kategori.'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function deleteCategory(id) {
    loading.value = true
    error.value = null
    try {
      await api.deleteCategory(id)
      const target = categories.value.find((c) => c.id === id)
      if (target) {
        target.deleted_at = new Date().toISOString()
      }
    } catch (err) {
      error.value = err.detail || err.message || 'Gagal menghapus kategori.'
      throw err
    } finally {
      loading.value = false
    }
  }

  function $reset() {
    categories.value = []
    loading.value = false
    error.value = null
  }

  return {
    // State
    categories,
    loading,
    error,
    // Getters
    incomeCategories,
    expenseCategories,
    categoryMap,
    getCategoryName,
    getCategoryColor,
    getCategoryIcon,
    // Actions
    fetchCategories,
    createCategory,
    deleteCategory,
    $reset,
  }
})
