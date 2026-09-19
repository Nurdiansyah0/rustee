/**
 * Invinite Cursor Delta Synchronization & Offline Mutation Queue Service
 * 
 * Complies with Master Specification v3.1.0 §25 & §26:
 * - Incremental delta sync via /api/v1/sync?cursor=<cursor>
 * - Offline mutations persisted locally with UUID Idempotency-Key
 * - Reconciled sequentially upon network reconnect
 * - Reconciles delta changes directly into reactive Pinia stores
 * - Declares Cache-Control: private, no-store
 */

import { useWalletStore } from '@/stores/wallets'
import { useCategoryStore } from '@/stores/categories'
import { useTransactionStore } from '@/stores/transactions'
import { useAnalyticsStore } from '@/stores/analytics'
import { api } from '@/services/api'

const OFFLINE_QUEUE_KEY = 'invinite_offline_mutations_v1';
const SYNC_CURSOR_KEY = 'invinite_sync_cursor_v1';

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

class CursorDeltaSyncService {
  constructor() {
    this.isSyncing = false;
    this.isDrainingQueue = false;
  }

  /**
   * Retrieves the current local sync cursor.
   */
  getCursor() {
    if (typeof localStorage === 'undefined') return '0';
    return localStorage.getItem(SYNC_CURSOR_KEY) || '0';
  }

  /**
   * Updates the local sync cursor.
   */
  setCursor(newCursor) {
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(SYNC_CURSOR_KEY, String(newCursor));
  }

  /**
   * Resets local cursor to 0 (forces full delta replay).
   */
  resetCursor() {
    if (typeof localStorage === 'undefined') return;
    localStorage.removeItem(SYNC_CURSOR_KEY);
  }

  /**
   * Retrieves all pending offline mutations.
   */
  getOfflineQueue() {
    if (typeof localStorage === 'undefined') return [];
    try {
      const raw = localStorage.getItem(OFFLINE_QUEUE_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch {
      return [];
    }
  }

  /**
   * Appends an offline mutation to the durable local queue.
   * 
   * @param {Object} mutation
   * @param {string} mutation.endpoint
   * @param {string} mutation.method
   * @param {Object} mutation.payload
   * @returns {Object} Enqueued mutation record
   */
  enqueueMutation({ endpoint, method = 'POST', payload = {} }) {
    const queue = this.getOfflineQueue();
    const idempotencyKey = payload.idempotency_key || generateUUID();
    const record = {
      id: generateUUID(),
      idempotency_key: idempotencyKey,
      endpoint,
      method,
      payload: {
        ...payload,
        idempotency_key: idempotencyKey,
      },
      queued_at: new Date().toISOString(),
    };

    queue.push(record);
    try {
      localStorage.setItem(OFFLINE_QUEUE_KEY, JSON.stringify(queue));
    } catch (err) {
      console.error('Failed to persist offline mutation', err);
    }
    return record;
  }

  /**
   * Clears the entire offline queue.
   */
  clearQueue() {
    if (typeof localStorage === 'undefined') return;
    localStorage.removeItem(OFFLINE_QUEUE_KEY);
  }

  /**
   * Removes a successfully executed mutation from the queue.
   */
  removeMutation(mutationId) {
    const queue = this.getOfflineQueue().filter((m) => m.id !== mutationId);
    try {
      localStorage.setItem(OFFLINE_QUEUE_KEY, JSON.stringify(queue));
    } catch {}
  }

  /**
   * Drains and reconciles the offline mutation queue against the backend.
   * Uses original Idempotency-Key headers to guarantee zero duplicate creations.
   */
  async drainOfflineQueue() {
    if (this.isDrainingQueue || typeof navigator === 'undefined' || !navigator.onLine) {
      return { drained: 0, failed: 0 };
    }

    this.isDrainingQueue = true;
    const queue = this.getOfflineQueue();
    let drainedCount = 0;
    let failedCount = 0;

    for (const item of queue) {
      try {
        const response = await fetch(item.endpoint, {
          method: item.method,
          headers: {
            'Content-Type': 'application/json',
            'Idempotency-Key': item.idempotency_key,
            'Cache-Control': 'private, no-store',
          },
          credentials: 'include',
          body: JSON.stringify(item.payload),
        });

        // 2xx Success or 409 Conflict (idempotently deduplicated)
        if (response.ok || response.status === 409) {
          this.removeMutation(item.id);
          drainedCount++;
        } else if (response.status >= 400 && response.status < 500) {
          // Client error (e.g. invalid payload) - remove to prevent blocking queue
          this.removeMutation(item.id);
          failedCount++;
        } else {
          // Server error 5xx or offline network drop - leave in queue for next retry
          failedCount++;
          break;
        }
      } catch (err) {
        // Network drop during drain
        failedCount++;
        break;
      }
    }

    this.isDrainingQueue = false;
    return { drained: drainedCount, failed: failedCount };
  }

  /**
   * Performs incremental cursor delta synchronization via /api/v1/sync?cursor=<cursor>.
   * 
   * @param {string|number|null} [explicitCursor=null] 
   */
  async syncWithCursor(explicitCursor = null) {
    if (this.isSyncing || typeof navigator === 'undefined' || !navigator.onLine) {
      return null;
    }

    this.isSyncing = true;
    const currentCursor = explicitCursor !== null ? String(explicitCursor) : this.getCursor();

    try {
      const delta = await api.getSync(currentCursor);
      if (!delta) {
        return null;
      }

      // 1. Advance cursor
      if (delta.cursor !== undefined) {
        this.setCursor(delta.cursor);
      }

      // 2. Reconcile Delta into Pinia Stores
      let hasMutations = false;

      // Accounts / Wallets reconciliation
      if (delta.accounts && delta.accounts.length > 0) {
        hasMutations = true;
        try {
          const walletStore = useWalletStore();
          walletStore.fetchWallets();
        } catch {}
      }

      // Categories reconciliation
      if (delta.categories && delta.categories.length > 0) {
        hasMutations = true;
        try {
          const categoryStore = useCategoryStore();
          categoryStore.fetchCategories();
        } catch {}
      }

      // Transactions reconciliation
      if (delta.transactions && delta.transactions.length > 0) {
        hasMutations = true;
        try {
          const txStore = useTransactionStore();
          if (typeof txStore.fetchTransactions === 'function') {
            txStore.fetchTransactions();
          }
        } catch {}
      }

      // Refresh dashboard if any data changed
      if (hasMutations) {
        try {
          const analyticsStore = useAnalyticsStore();
          analyticsStore.fetchDashboard();
        } catch {}
      }

      // 3. Recursive fetch if has_more is true
      if (delta.has_more && delta.cursor) {
        this.isSyncing = false;
        return this.syncWithCursor(delta.cursor);
      }

      return delta;
    } catch (err) {
      console.warn('Delta sync could not complete', err);
      return null;
    } finally {
      this.isSyncing = false;
    }
  }

  /**
   * Complete reconciliation cycle on network reconnection:
   * Step 1: Drain offline mutations
   * Step 2: Sync cursor deltas
   */
  async reconcileOnReconnect() {
    await this.drainOfflineQueue();
    await this.syncWithCursor();
  }
}

export const syncService = new CursorDeltaSyncService();
