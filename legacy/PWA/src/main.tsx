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
    // Immediately update without asking user
    updateSW(true)
  },
  onOfflineReady() {
    // Optional: could display a toast
  },
  onRegisteredSW(swUrl: string, r: ServiceWorkerRegistration | undefined) {
    // Periodic check for updates (every 30 min)
  if (r && typeof r.update === 'function') {
      setInterval(() => {
        r.update()
      }, 30 * 60 * 1000)
    } else if (navigator.serviceWorker.controller) {
      fetch(swUrl).catch(() => {})
    }
  }
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)