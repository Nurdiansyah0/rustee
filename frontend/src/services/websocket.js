/**
 * Invinite Foreground WebSocket Client
 * 
 * Complies with Master Specification v3.1.0 §25 & §27:
 * - Active ONLY when document.visibilityState === 'visible'
 * - Gracefully terminates on app backgrounding to conserve battery & server resources
 * - Automatically reconnects with exponential backoff on foregrounding
 * - Dispatches TransactionCreated, BalanceChanged, SyncHint events
 */

class ForegroundWebSocketClient {
  constructor() {
    this.ws = null;
    this.subscribers = new Map(); // eventType -> Set of callbacks
    this.reconnectAttempts = 0;
    this.maxReconnectAttempts = 8;
    this.baseReconnectDelay = 1000;
    this.maxReconnectDelay = 30000;
    this.reconnectTimer = null;
    this.heartbeatTimer = null;
    this.isExplicitlyClosed = false;
    this.isInitialized = false;
    this.onStatusChangeCallbacks = new Set();
  }

  /**
   * Initializes the lifecycle-aware foreground WebSocket service.
   */
  init() {
    if (this.isInitialized || typeof window === 'undefined') return;
    this.isInitialized = true;

    // Handle document visibility changes (Foreground vs Background)
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        // App returned to foreground: reconnect if closed
        if (!this.isConnected()) {
          this.connect();
        }
      } else {
        // App moved to background: disconnect immediately to save battery (§27)
        this.disconnect(true, 'App moved to background');
      }
    });

    // Handle online/offline network transitions
    window.addEventListener('online', () => {
      if (document.visibilityState === 'visible' && !this.isConnected()) {
        this.connect();
      }
    });

    window.addEventListener('offline', () => {
      this.disconnect(true, 'Network offline');
    });

    // Initial connection if currently in foreground
    if (document.visibilityState === 'visible') {
      this.connect();
    }
  }

  /**
   * Resolves authoritative WebSocket URL.
   */
  getWebSocketUrl() {
    if (typeof window === 'undefined') return '';
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    return `${protocol}//${host}/api/v1/ws`;
  }

  /**
   * Connects to the backend WebSocket endpoint.
   */
  connect() {
    if (typeof window === 'undefined') return;

    // Do not connect if app is in background
    if (document.visibilityState !== 'visible') {
      return;
    }

    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    this.clearReconnectTimer();
    this.isExplicitlyClosed = false;
    this.notifyStatus('connecting');

    try {
      const url = this.getWebSocketUrl();
      this.ws = new WebSocket(url);

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.notifyStatus('connected');
        this.startHeartbeat();
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event);
      };

      this.ws.onerror = (err) => {
        // Ignore expected connection errors in dev/mock environments
      };

      this.ws.onclose = (event) => {
        this.stopHeartbeat();
        this.notifyStatus('disconnected');

        if (!this.isExplicitlyClosed && document.visibilityState === 'visible') {
          this.scheduleReconnect();
        }
      };
    } catch (err) {
      this.notifyStatus('disconnected');
      if (!this.isExplicitlyClosed && document.visibilityState === 'visible') {
        this.scheduleReconnect();
      }
    }
  }

  /**
   * Disconnects the WebSocket.
   * 
   * @param {boolean} backgrounded - Whether disconnect is triggered by lifecycle backgrounding
   * @param {string} reason 
   */
  disconnect(backgrounded = false, reason = 'Client disconnect') {
    this.isExplicitlyClosed = !backgrounded;
    this.clearReconnectTimer();
    this.stopHeartbeat();

    if (this.ws) {
      try {
        if (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING) {
          this.ws.close(1000, reason);
        }
      } catch {}
      this.ws = null;
    }

    this.notifyStatus(backgrounded ? 'backgrounded' : 'disconnected');
  }

  /**
   * Checks if WebSocket is actively connected.
   */
  isConnected() {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  /**
   * Handles incoming message frame.
   */
  handleMessage(event) {
    try {
      const payload = JSON.parse(event.data);
      const eventType = payload.type || payload.event;
      if (!eventType) return;

      // Broadcast to registered subscribers
      const handlers = this.subscribers.get(eventType);
      if (handlers) {
        handlers.forEach((callback) => {
          try {
            callback(payload.data || payload);
          } catch (err) {
            console.error(`Error in WebSocket handler for ${eventType}:`, err);
          }
        });
      }

      // Also broadcast to wildcard '*' listeners
      const wildcardHandlers = this.subscribers.get('*');
      if (wildcardHandlers) {
        wildcardHandlers.forEach((callback) => {
          try {
            callback(payload);
          } catch (err) {}
        });
      }
    } catch (err) {
      // Non-JSON frame (e.g. heartbeat pong)
    }
  }

  /**
   * Subscribes a listener to a specific event type (e.g. TransactionCreated, BalanceChanged, SyncHint).
   * 
   * @param {string} eventType 
   * @param {Function} callback 
   * @returns {Function} Unsubscribe function
   */
  subscribe(eventType, callback) {
    if (!this.subscribers.has(eventType)) {
      this.subscribers.set(eventType, new Set());
    }
    this.subscribers.get(eventType).add(callback);

    return () => {
      const set = this.subscribers.get(eventType);
      if (set) {
        set.delete(callback);
      }
    };
  }

  /**
   * Subscribes to status changes ('connecting' | 'connected' | 'disconnected' | 'backgrounded').
   */
  onStatusChange(callback) {
    this.onStatusChangeCallbacks.add(callback);
    return () => {
      this.onStatusChangeCallbacks.delete(callback);
    };
  }

  notifyStatus(status) {
    this.onStatusChangeCallbacks.forEach((cb) => {
      try {
        cb(status);
      } catch {}
    });
  }

  /**
   * Heartbeat ping to detect stale socket connections.
   */
  startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      if (this.isConnected()) {
        try {
          this.ws.send(JSON.stringify({ type: 'ping', timestamp: Date.now() }));
        } catch {}
      }
    }, 25000);
  }

  stopHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  /**
   * Exponential backoff reconnection scheduler.
   */
  scheduleReconnect() {
    if (this.reconnectTimer || document.visibilityState !== 'visible') return;

    this.reconnectAttempts++;
    const delay = Math.min(
      this.baseReconnectDelay * Math.pow(2, this.reconnectAttempts - 1),
      this.maxReconnectDelay
    );

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (document.visibilityState === 'visible' && !this.isExplicitlyClosed) {
        this.connect();
      }
    }, delay);
  }

  clearReconnectTimer() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}

export const websocketService = new ForegroundWebSocketClient();
