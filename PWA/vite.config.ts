import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'
import { resolve } from 'path'

// https://vitejs.dev/config/
export default defineConfig({
  base: '/net/',
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
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
  },
  optimizeDeps: {
    exclude: ['snartnet-core'],
    include: ['events', 'util', 'buffer']
  },
})