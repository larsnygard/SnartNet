import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'
import { resolve } from 'path'
import pkg from './package.json'

// https://vitejs.dev/config/
export default defineConfig({
  base: '/net/',
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      workbox: {
        navigateFallback: '/net/index.html',
        clientsClaim: true,
        skipWaiting: true,
        runtimeCaching: [
          {
            urlPattern: /\/net\/.*$/,
            handler: 'NetworkFirst',
            options: {
              cacheName: 'snartnet-pages',
              expiration: { maxEntries: 50, maxAgeSeconds: 60 * 60 },
              networkTimeoutSeconds: 5
            }
          },
          {
            urlPattern: ({ request }) => request.destination === 'script' || request.destination === 'style',
            handler: 'StaleWhileRevalidate',
            options: {
              cacheName: 'snartnet-assets',
              expiration: { maxEntries: 100, maxAgeSeconds: 60 * 60 * 24 }
            }
          }
        ]
      },
      devOptions: {
        enabled: true
      },
      includeAssets: ['icons/icon-192.png', 'icons/icon-512.png', 'icons/logo.png'],
      manifest: {
        name: 'SnartNet',
        short_name: 'SnartNet',
        description: 'Decentralized social media, powered by swarms',
        theme_color: '#0d1117',
        background_color: '#ffffff',
        display: 'standalone',
        scope: '/net/',
  start_url: '/net/',
        icons: [
          {
            src: 'icons/icon-192.png',
            sizes: '192x192',
            type: 'image/png'
          },
          {
            src: 'icons/icon-512.png',
            sizes: '512x512',
            type: 'image/png'
          }
        ]
      }
    })
  ],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
      events: 'events',
      util: 'util',
      buffer: 'buffer',
      path: 'path-browserify',
      crypto: 'crypto-browserify',
      stream: 'stream-browserify',
      process: 'process',
    },
  },
  server: {
    port: 3000,
    host: true,
    fs: {
      allow: ['..']
    }
  },
  build: {
    target: 'esnext',
    sourcemap: true,
    rollupOptions: {
      external: ['webtorrent'],
      output: {
        manualChunks: (id) => {
          if (id.includes('node_modules')) {
            return 'vendor'
          }
        }
      }
    }
  },
  define: {
    global: 'globalThis',
    'import.meta.env.VITE_APP_VERSION': JSON.stringify(pkg.version || '0.0.0-dev')
  },
  optimizeDeps: {
    exclude: ['snartnet-core'],
    include: ['events', 'util', 'buffer']
  },
})