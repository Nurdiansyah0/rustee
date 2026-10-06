import { handleMockApiRequest } from './mockData.js';

export class ApiError extends Error {
  constructor(status, title, detail, code) {
    super(detail || title || `HTTP ${status}`);
    this.status = status;
    this.title = title;
    this.detail = detail;
    this.code = code;
  }
}

function generateUUID() {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

function getActiveTenantId() {
  try {
    if (typeof localStorage !== 'undefined') {
      const directId = localStorage.getItem('invinite_active_tenant_id');
      if (directId) return directId;
      const raw = localStorage.getItem('invinite_active_workspace');
      if (raw) {
        const parsed = JSON.parse(raw);
        return parsed?.id || parsed?.active_tenant_id || null;
      }
    }
  } catch {
    // Ignore storage parse error
  }
  return null;
}

async function request(path, options = {}) {
  const activeTenantId = getActiveTenantId();
  const headers = {
    'Content-Type': 'application/json',
    ...(activeTenantId ? { 'X-Tenant-ID': activeTenantId } : {}),
    ...(options.headers || {}),
  };

  try {
    const response = await fetch(path, {
      ...options,
      headers,
      credentials: 'include', // Ensure HttpOnly auth cookie is automatically sent
    });

    if (!response.ok) {
      // If Vite proxy returns 502/504 or server 5xx error due to offline backend, fallback to mock
      if (response.status >= 500) {
        return handleMockApiRequest(path, options);
      }

      let errorData = {};
      try {
        errorData = await response.json();
      } catch {
        // Non-JSON response
      }
      throw new ApiError(
        response.status,
        errorData.title || 'Error',
        errorData.detail || errorData.message || response.statusText,
        errorData.code || 'UNKNOWN_ERROR'
      );
    }

    if (response.status === 204) {
      return null;
    }

    return response.json();
  } catch (err) {
    // If backend is offline (Failed to fetch, network error, proxy down)
    if (
      err instanceof TypeError ||
      err.message?.includes('Failed to fetch') ||
      err.message?.includes('NetworkError') ||
      err.message?.includes('network')
    ) {
      return handleMockApiRequest(path, options);
    }
    throw err;
  }
}

export const api = {
  // Auth
  login: (email, password) =>
    request('/api/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    }),

  register: (name, email, password) =>
    request('/api/v1/auth/register', {
      method: 'POST',
      body: JSON.stringify({ display_name: name, email, password }),
    }),

  logout: () =>
    request('/api/v1/auth/logout', {
      method: 'POST',
    }),

  getMe: () => request('/api/v1/auth/me'),

  forgotPassword: (email) =>
    request('/api/v1/auth/forgot-password', {
      method: 'POST',
      body: JSON.stringify({ email }),
    }),

  resetPassword: (token, password) =>
    request('/api/v1/auth/reset-password', {
      method: 'POST',
      body: JSON.stringify({ token, password }),
    }),

  // Accounts
  getAccounts: () => request('/api/v1/accounts'),
  createAccount: (data) =>
    request('/api/v1/accounts', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  archiveAccount: (id) =>
    request(`/api/v1/accounts/${id}/archive`, {
      method: 'POST',
    }),

  // Categories
  getCategories: () => request('/api/v1/categories'),
  createCategory: (data) =>
    request('/api/v1/categories', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  deleteCategory: (id) =>
    request(`/api/v1/categories/${id}`, {
      method: 'DELETE',
    }),

  // Transactions
  getTransactions: (params = {}) => {
    const query = new URLSearchParams();
    if (params.page) query.set('page', params.page);
    if (params.per_page) query.set('per_page', params.per_page);
    if (params.account_id) query.set('account_id', params.account_id);
    if (params.category_id) query.set('category_id', params.category_id);
    if (params.transaction_type) query.set('transaction_type', params.transaction_type);
    const queryString = query.toString();
    return request(`/api/v1/transactions${queryString ? `?${queryString}` : ''}`);
  },

  createTransaction: (data) => {
    const idempotencyKey = data.idempotency_key || generateUUID();
    return request('/api/v1/transactions', {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    });
  },

  deleteTransaction: (id) =>
    request(`/api/v1/transactions/${id}`, {
      method: 'DELETE',
    }),

  // Dashboard & Analytics
  getDashboard: () => request('/api/v1/dashboard'),
  getBasicAnalytics: () => request('/api/v1/analytics/basic'),
  getAdvancedAnalytics: () => request('/api/v1/analytics/advanced'),

  // Subscription
  getSubscriptionStatus: () => request('/api/v1/subscription'),
  createCheckout: (data) => {
    const payload = typeof data === 'string' ? { provider: data } : data;
    return request('/api/v1/subscriptions/checkout', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  },
  activateTrial: () =>
    request('/api/v1/subscriptions/trial/activate', {
      method: 'POST',
    }),

  // Personalization & Onboarding (§4, §5)
  getPersonalization: () => request('/api/v1/users/personalization'),
  updatePersonalization: (data) =>
    request('/api/v1/users/personalization', {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
  submitOnboarding: (data) =>
    request('/api/v1/users/onboarding', {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  // Cursor Delta Sync (§25, §26)
  getSync: (cursor = 0) =>
    request(`/api/v1/sync?cursor=${encodeURIComponent(cursor)}`),

  // Tenancy & Workspaces (v4.1 §5, §8, §35)
  getWorkspaces: () => request('/api/v1/tenants'),
  createWorkspace: (data) =>
    request('/api/v1/tenants', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  getWorkspace: (id) => request(`/api/v1/tenants/${id}`),
  getWorkspaceProfile: (id) => request(`/api/v1/tenants/${id}/profile`),
  updateWorkspaceProfile: (id, data) =>
    request(`/api/v1/tenants/${id}/profile`, {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
  getWorkspaceMembers: (id) => request(`/api/v1/tenants/${id}/members`),
  inviteWorkspaceMember: (id, data) =>
    request(`/api/v1/tenants/${id}/members`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  updateWorkspaceMemberRole: (id, userId, role) =>
    request(`/api/v1/tenants/${id}/members/${userId}`, {
      method: 'PUT',
      body: JSON.stringify({ role }),
    }),
  removeWorkspaceMember: (id, userId) =>
    request(`/api/v1/tenants/${id}/members/${userId}`, {
      method: 'DELETE',
    }),
  // Workspace Invitations (invite-link flow for staff onboarding)
  createWorkspaceInvitation: (id, data) =>
    request(`/api/v1/tenants/${id}/invitations`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  listWorkspaceInvitations: (id) => request(`/api/v1/tenants/${id}/invitations`),
  revokeWorkspaceInvitation: (id, invitationId) =>
    request(`/api/v1/tenants/${id}/invitations/${invitationId}`, {
      method: 'DELETE',
    }),
  // Public invitation endpoints (no auth required)
  previewInvitation: (token) => request(`/api/v1/invitations/${token}`),
  acceptInvitation: (token, data) =>
    request(`/api/v1/invitations/${token}/accept`, {
      method: 'POST',
      body: JSON.stringify({ token, ...data }),
    }),
  switchWorkspace: (id) =>
    request(`/api/v1/tenants/${id}/switch`, {
      method: 'POST',
    }),
  getTenantCapabilities: (id) => request(`/api/v1/tenants/${id}/capabilities`),

  // Warehouses (v4.1 Features 1, 22)
  getWarehouses: () => request('/api/v1/warehouses'),
  createWarehouse: (data) =>
    request('/api/v1/warehouses', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  getWarehouse: (id) => request(`/api/v1/warehouses/${id}`),

  // Products & SKU Engine (v4.1 Features 2, 22)
  getProducts: () => request('/api/v1/products'),
  createProduct: (data) =>
    request('/api/v1/products', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  getProduct: (id) => request(`/api/v1/products/${id}`),

  // Multi-Location Inventory & Movements (v4.1 Features 3, 4, 5, 6, 8, 9, 17, 23)
  getStockItems: (params = {}) => {
    const query = new URLSearchParams();
    if (params.warehouse_id) query.set('warehouse_id', params.warehouse_id);
    if (params.product_id) query.set('product_id', params.product_id);
    if (params.low_stock !== undefined) query.set('low_stock', params.low_stock);
    const queryString = query.toString();
    return request(`/api/v1/inventory${queryString ? `?${queryString}` : ''}`);
  },

  getStockMovements: (params = {}) => {
    const query = new URLSearchParams();
    if (params.warehouse_id) query.set('warehouse_id', params.warehouse_id);
    if (params.product_id) query.set('product_id', params.product_id);
    if (params.limit) query.set('limit', params.limit);
    if (params.offset) query.set('offset', params.offset);
    const queryString = query.toString();
    return request(`/api/v1/inventory/movements${queryString ? `?${queryString}` : ''}`);
  },

  createStockMovement: (data) => {
    const idempotencyKey = data.idempotency_key || generateUUID();
    return request('/api/v1/inventory/movements', {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    });
  },

  transferStock: (data) => {
    const idempotencyKey = data.idempotency_key || generateUUID();
    return request('/api/v1/inventory/transfer', {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    });
  },

  adjustStock: (data) => {
    const idempotencyKey = data.idempotency_key || generateUUID();
    return request('/api/v1/inventory/adjust', {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    });
  },

  // Purchase Orders & Goods Receipt (v4.1 Features 10, 11, 12, 13, 14, 23)
  getPurchaseOrders: () => request('/api/v1/purchase-orders'),
  createPurchaseOrder: (data) =>
    request('/api/v1/purchase-orders', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  getPurchaseOrder: (id) => request(`/api/v1/purchase-orders/${id}`),
  orderPurchaseOrder: (id) =>
    request(`/api/v1/purchase-orders/${id}/order`, {
      method: 'POST',
    }),
  receivePurchaseOrder: (id, data = {}) => {
    const idempotencyKey = data.idempotency_key || generateUUID();
    return request(`/api/v1/purchase-orders/${id}/receive`, {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    });
  },
  cancelPurchaseOrder: (id, data = {}) =>
    request(`/api/v1/purchase-orders/${id}/cancel`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  // Projects & Contractor Operations (v4.1 Phase 3, Milestones 1-4)
  getProjects: (params = {}) => {
    const query = new URLSearchParams()
    if (params.status) query.set('status', params.status)
    if (params.customer_id) query.set('customer_id', params.customer_id)
    if (params.search) query.set('search', params.search)
    if (params.limit) query.set('limit', params.limit)
    if (params.offset) query.set('offset', params.offset)
    const qs = query.toString()
    return request(`/api/v1/projects${qs ? `?${qs}` : ''}`)
  },
  createProject: (data) =>
    request('/api/v1/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  getProject: (id) => request(`/api/v1/projects/${id}`),
  updateProjectStatus: (id, status) =>
    request(`/api/v1/projects/${id}/status`, {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
  getProjectMembers: (id) => request(`/api/v1/projects/${id}/members`),
  addProjectMember: (id, data) =>
    request(`/api/v1/projects/${id}/members`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  getProjectMilestones: (id) => request(`/api/v1/projects/${id}/milestones`),
  createProjectMilestone: (id, data) =>
    request(`/api/v1/projects/${id}/milestones`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  updateProjectMilestoneStatus: (id, milestoneId, status) =>
    request(`/api/v1/projects/${id}/milestones/${milestoneId}/status`, {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
  completeProjectMilestone: (id, milestoneId, data = {}) =>
    request(`/api/v1/projects/${id}/milestones/${milestoneId}/complete`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  billProjectMilestone: (id, milestoneId, data = {}) => {
    const idempotencyKey = data.idempotency_key || generateUUID()
    return request(`/api/v1/projects/${id}/milestones/${milestoneId}/bill`, {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    })
  },
  getProjectTasks: (id, params = {}) => {
    const query = new URLSearchParams()
    if (params.milestone_id) query.set('milestone_id', params.milestone_id)
    if (params.assigned_to) query.set('assigned_to', params.assigned_to)
    if (params.status) query.set('status', params.status)
    const qs = query.toString()
    return request(`/api/v1/projects/${id}/tasks${qs ? `?${qs}` : ''}`)
  },
  createProjectTask: (id, data) =>
    request(`/api/v1/projects/${id}/tasks`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  updateProjectTaskStatus: (id, taskId, status) =>
    request(`/api/v1/projects/${id}/tasks/${taskId}/status`, {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
  getProjectLabor: (id) => request(`/api/v1/projects/${id}/labor`),
  logProjectLabor: (id, data) =>
    request(`/api/v1/projects/${id}/labor`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  deleteProjectLabor: (id, laborId) =>
    request(`/api/v1/projects/${id}/labor/${laborId}`, {
      method: 'DELETE',
    }),
  getProjectExpenses: (id) => request(`/api/v1/projects/${id}/expenses`),
  createProjectExpense: (id, data) =>
    request(`/api/v1/projects/${id}/expenses`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  deleteProjectExpense: (id, expenseId) =>
    request(`/api/v1/projects/${id}/expenses/${expenseId}`, {
      method: 'DELETE',
    }),
  getProjectMaterials: (id) => request(`/api/v1/projects/${id}/materials`),
  createProjectMaterial: (id, data) =>
    request(`/api/v1/projects/${id}/materials`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  issueProjectMaterial: (id, materialId, data = {}) => {
    const idempotencyKey = data.idempotency_key || generateUUID()
    return request(`/api/v1/projects/${id}/materials/${materialId}/issue`, {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    })
  },
  directIssueProjectMaterial: (id, data = {}) => {
    const idempotencyKey = data.idempotency_key || generateUUID()
    return request(`/api/v1/projects/${id}/materials/issue`, {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    })
  },
  deleteProjectMaterial: (id, materialId) =>
    request(`/api/v1/projects/${id}/materials/${materialId}`, {
      method: 'DELETE',
    }),
  getProjectProfitability: (id) => request(`/api/v1/projects/${id}/profitability`),
  billProjectProgress: (id, data = {}) => {
    const idempotencyKey = data.idempotency_key || generateUUID()
    return request(`/api/v1/projects/${id}/billing/progress`, {
      method: 'POST',
      headers: {
        'Idempotency-Key': idempotencyKey,
      },
      body: JSON.stringify({
        ...data,
        idempotency_key: idempotencyKey,
      }),
    })
  },
  getProjectProgressRecords: (id) => request(`/api/v1/projects/${id}/progress`),
  createProjectProgressRecord: (id, data) =>
    request(`/api/v1/projects/${id}/progress`, {
      method: 'POST',
      body: JSON.stringify(data),
    }),
};
