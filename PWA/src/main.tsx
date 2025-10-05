import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import './index.css'
// Register service worker with auto-update & immediate activation
import { registerSW } from 'virtual:pwa-register'

// Force reload when a new service worker is activated
const updateSW = registerSW({
  immediate: true,
  onNeedRefresh() {
    console.info('[PWA] New content available, updating...')
    updateSW(true)
  },
  onOfflineReady() {
    console.info('[PWA] App ready for offline use.')
  },
  onRegisteredSW(swUrl: string, r: ServiceWorkerRegistration | undefined) {
    console.info('[PWA] Service worker registered at', swUrl)
    if (r) {
      // Listen for controller change to auto reload once when new SW activates
      let refreshed = false
      navigator.serviceWorker.addEventListener('controllerchange', () => {
        if (refreshed) return
        refreshed = true
        console.info('[PWA] New service worker activated, reloading...')
        window.location.reload()
      })
      // Periodic check for updates (every 30 min)
      if (typeof r.update === 'function') {
        setInterval(() => {
          r.update().catch(e => console.warn('[PWA] update check failed', e))
        }, 30 * 60 * 1000)
      }
    } else if (navigator.onLine) {
      // Fallback manual fetch if registration object missing
      fetch(swUrl).catch(() => {})
    }
  }
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)