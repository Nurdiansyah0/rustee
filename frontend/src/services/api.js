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

async function request(path, options = {}) {
  const headers = {
    'Content-Type': 'application/json',
    ...(options.headers || {}),
  };

  try {
    const response = await fetch(path, {
      ...options,
      headers,
      credentials: 'include', // Ensure HttpOnly auth cookie is automatically sent
    });

    if (!response.ok) {
      // If Vite proxy returns 502/504 or 404 due to offline backend, fallback to mock
      if (response.status === 404 || response.status >= 500) {
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
  createCheckout: (provider) =>
    request('/api/v1/subscriptions/checkout', {
      method: 'POST',
      body: JSON.stringify({ provider }),
    }),
  activateTrial: () =>
    request('/api/v1/subscriptions/trial', {
      method: 'POST',
    }),
};
