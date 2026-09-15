import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { VitePWA } from 'vite-plugin-pwa'

export default defineConfig({
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  plugins: [
    vue(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['favicon.ico', 'icons/*.png', 'push-worker.js'],
      manifest: {
        id: '/',
        name: 'Invinite — Personal Finance PWA',
        short_name: 'Invinite',
        description: 'Invinite — Presisi Keuangan Pribadi (Personal Finance Progressive Web App SaaS)',
        theme_color: '#059669',
        background_color: '#09090B',
        display: 'standalone',
        display_override: ['window-controls-overlay', 'standalone', 'minimal-ui'],
        orientation: 'portrait-primary',
        scope: '/',
        start_url: '/',
        categories: ['finance', 'productivity', 'utilities'],
        icons: [
          {
            src: '/icons/icon-192.png',
            sizes: '192x192',
            type: 'image/png',
            purpose: 'any maskable'
          },
          {
            src: '/icons/icon-512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'any maskable'
          }
        ],
        shortcuts: [
          {
            name: 'Catat Transaksi',
            short_name: 'Tambah',
            description: 'Buka form pencatatan transaksi cepat',
            url: '/?action=add',
            icons: [
              {
                src: '/icons/icon-192.png',
                sizes: '192x192'
              }
            ]
          }
        ]
      },
      workbox: {
        importScripts: ['/push-worker.js'],
        runtimeCaching: [
          {
            // Strict NetworkOnly strategy for all financial API endpoints
            urlPattern: ({ url }) => url.pathname.startsWith('/api/v1/'),
            handler: 'NetworkOnly',
            options: {
              backgroundSync: {
                name: 'financial-api-queue',
                options: {
                  maxRetentionTime: 24 * 60 // 24 hours
                }
              }
            }
          },
          {
            // Cache static assets and HTML shell
            urlPattern: ({ request }) =>
              request.destination === 'style' ||
              request.destination === 'script' ||
              request.destination === 'image' ||
              request.destination === 'document',
            handler: 'StaleWhileRevalidate',
            options: {
              cacheName: 'app-shell-cache',
              expiration: {
                maxEntries: 50,
                maxAgeSeconds: 30 * 24 * 60 * 60 // 30 Days
              }
            }
          }
        ]
      }
    })
  ],
  server: {
    port: 3000,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true
      }
    }
  }
})
