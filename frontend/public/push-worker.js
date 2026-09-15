// ==============================================================================
// Invinite PWA — Lock-Screen Web Push & Notification Event Handler
// Imported into Workbox Service Worker via importScripts('/push-worker.js')
// ==============================================================================

self.addEventListener('push', (event) => {
  if (!event.data) {
    return;
  }

  let payload = {};
  try {
    payload = event.data.json();
  } catch (err) {
    payload = {
      title: 'Invinite Keuangan',
      body: event.data.text(),
    };
  }

  const title = payload.title || 'Invinite Keuangan';
  const options = {
    body: payload.body || 'Pemberitahuan aktivitas keuangan baru.',
    icon: payload.icon || '/icons/icon-192.png',
    badge: payload.badge || '/icons/icon-192.png',
    tag: payload.tag || 'invinite-transaction',
    data: {
      url: payload.url || '/',
      timestamp: Date.now(),
      ...payload.data,
    },
    vibrate: [100, 50, 100], // Tactile vibration pattern on Android
    renotify: true,
    requireInteraction: payload.requireInteraction ?? false,
    actions: payload.actions || [
      { action: 'open', title: 'Buka Aplikasi' },
      { action: 'dismiss', title: 'Tutup' },
    ],
  };

  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();

  if (event.action === 'dismiss') {
    return;
  }

  const targetUrl = (event.notification.data && event.notification.data.url) || '/';

  event.waitUntil(
    clients.matchAll({ type: 'window', includeUncontrolled: true }).then((windowClients) => {
      // Focus existing tab if open
      for (const client of windowClients) {
        if ('focus' in client) {
          if (client.url.includes(self.location.origin)) {
            client.navigate(targetUrl);
            return client.focus();
          }
        }
      }
      // Otherwise open a new window
      if (clients.openWindow) {
        return clients.openWindow(targetUrl);
      }
    })
  );
});
