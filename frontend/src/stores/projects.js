import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '@/services/api'

export const useProjectsStore = defineStore('projects', () => {
  const projects = ref([])
  const currentProject = ref(null)
  const milestones = ref([])
  const tasks = ref([])
  const materials = ref([])
  const labor = ref([])
  const expenses = ref([])
  const progressRecords = ref([])
  const profitability = ref(null)
  const members = ref([])

  const loading = ref(false)
  const detailLoading = ref(false)
  const actionLoading = ref(false)
  const error = ref(null)

  // Computed summary metrics
  const totalProjects = computed(() => projects.value.length)
  const activeProjects = computed(() =>
    projects.value.filter((p) => p.status === 'in_progress' || p.status === 'active')
  )
  const completedProjects = computed(() =>
    projects.value.filter((p) => p.status === 'completed')
  )

  const overallBudget = computed(() =>
    projects.value.reduce((acc, p) => acc + (p.contract_amount || 0), 0)
  )

  // Actions
  async function fetchProjects(filter = {}) {
    loading.value = true
    error.value = null
    try {
      const res = await api.getProjects(filter)
      projects.value = res?.data || res || []
    } catch (err) {
      error.value = err.message || 'Gagal memuat daftar proyek'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function createProject(payload) {
    actionLoading.value = true
    error.value = null
    try {
      const created = await api.createProject(payload)
      projects.value.unshift(created)
      return created
    } catch (err) {
      error.value = err.message || 'Gagal membuat proyek baru'
      throw err
    } finally {
      actionLoading.value = false
    }
  }

  async function fetchProjectDetail(id) {
    detailLoading.value = true
    error.value = null
    try {
      const [
        proj,
        mList,
        tList,
        matList,
        labList,
        expList,
        prof,
        progList,
        memList
      ] = await Promise.all([
        api.getProject(id),
        api.getProjectMilestones(id).catch(() => ({ data: [] })),
        api.getProjectTasks(id).catch(() => ({ data: [] })),
        api.getProjectMaterials(id).catch(() => ({ data: [] })),
        api.getProjectLabor(id).catch(() => ({ data: [] })),
        api.getProjectExpenses(id).catch(() => ({ data: [] })),
        api.getProjectProfitability(id).catch(() => null),
        api.getProjectProgressRecords(id).catch(() => ({ data: [] })),
        api.getProjectMembers(id).catch(() => ({ data: [] }))
      ])

      currentProject.value = proj
      milestones.value = mList?.data || mList || []
      tasks.value = tList?.data || tList || []
      materials.value = matList?.data || matList || []
      labor.value = labList?.data || labList || []
      expenses.value = expList?.data || expList || []
      profitability.value = prof
      progressRecords.value = progList?.data || progList || []
      members.value = memList?.data || memList || []
    } catch (err) {
      error.value = err.message || 'Gagal memuat detail proyek'
      throw err
    } finally {
      detailLoading.value = false
    }
  }

  async function updateProjectStatus(id, status) {
    actionLoading.value = true
    try {
      const updated = await api.updateProjectStatus(id, status)
      if (currentProject.value && currentProject.value.id === id) {
        currentProject.value.status = updated.status
      }
      const idx = projects.value.findIndex((p) => p.id === id)
      if (idx !== -1) {
        projects.value[idx].status = updated.status
      }
      return updated
    } finally {
      actionLoading.value = false
    }
  }

  async function createMilestone(id, payload) {
    actionLoading.value = true
    try {
      const created = await api.createProjectMilestone(id, payload)
      milestones.value.push(created)
      return created
    } finally {
      actionLoading.value = false
    }
  }

  async function completeMilestone(id, milestoneId) {
    actionLoading.value = true
    try {
      const updated = await api.completeProjectMilestone(id, milestoneId)
      const idx = milestones.value.findIndex((m) => m.id === milestoneId)
      if (idx !== -1) {
        milestones.value[idx] = updated
      }
      return updated
    } finally {
      actionLoading.value = false
    }
  }

  async function billMilestone(id, milestoneId, payload = {}) {
    actionLoading.value = true
    try {
      const invoice = await api.billProjectMilestone(id, milestoneId, payload)
      // Refresh detail & profitability after billing
      await fetchProjectDetail(id)
      return invoice
    } finally {
      actionLoading.value = false
    }
  }

  async function createTask(id, payload) {
    actionLoading.value = true
    try {
      const created = await api.createProjectTask(id, payload)
      tasks.value.push(created)
      return created
    } finally {
      actionLoading.value = false
    }
  }

  async function updateTaskStatus(id, taskId, status) {
    actionLoading.value = true
    try {
      const updated = await api.updateProjectTaskStatus(id, taskId, status)
      const idx = tasks.value.findIndex((t) => t.id === taskId)
      if (idx !== -1) {
        tasks.value[idx] = updated
      }
      return updated
    } finally {
      actionLoading.value = false
    }
  }

  async function directIssueMaterial(id, payload) {
    actionLoading.value = true
    try {
      const res = await api.directIssueProjectMaterial(id, payload)
      await fetchProjectDetail(id)
      return res
    } finally {
      actionLoading.value = false
    }
  }

  async function logLabor(id, payload) {
    actionLoading.value = true
    try {
      const res = await api.logProjectLabor(id, payload)
      labor.value.unshift(res)
      await refreshProfitability(id)
      return res
    } finally {
      actionLoading.value = false
    }
  }

  async function logExpense(id, payload) {
    actionLoading.value = true
    try {
      const res = await api.createProjectExpense(id, payload)
      expenses.value.unshift(res)
      await refreshProfitability(id)
      return res
    } finally {
      actionLoading.value = false
    }
  }

  async function billProgress(id, payload) {
    actionLoading.value = true
    try {
      const res = await api.billProjectProgress(id, payload)
      await fetchProjectDetail(id)
      return res
    } finally {
      actionLoading.value = false
    }
  }

  async function refreshProfitability(id) {
    try {
      profitability.value = await api.getProjectProfitability(id)
    } catch {
      // Ignore background refresh error
    }
  }

  return {
    projects,
    currentProject,
    milestones,
    tasks,
    materials,
    labor,
    expenses,
    progressRecords,
    profitability,
    members,
    loading,
    detailLoading,
    actionLoading,
    error,
    totalProjects,
    activeProjects,
    completedProjects,
    overallBudget,
    fetchProjects,
    createProject,
    fetchProjectDetail,
    updateProjectStatus,
    createMilestone,
    completeMilestone,
    billMilestone,
    createTask,
    updateTaskStatus,
    directIssueMaterial,
    logLabor,
    logExpense,
    billProgress,
    refreshProfitability
  }
})
