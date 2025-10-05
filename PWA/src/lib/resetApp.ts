import { useProfileStore } from '@/stores/profileStore'
import { useContactStore } from '@/stores/contactStore'
import { usePostStore } from '@/stores/postStore'
import { getFs } from '@/lib/fs'

// Keys we know we use in localStorage (prefix-based fallback handled)
const LS_PREFIXES = [
  'snartnet:posts',
  'snartnet-contacts',
  'snartnet:profile:seed-enabled',
  'profile-picture-',
  'profile-picture-thumb-'
]

async function wipeFs() {
  try {
    const fs = await getFs()
    // Brutal approach: list '/' and delete files under /data and /profiles if present
    const targets = ['/data', '/profiles']
    for (const dir of targets) {
      try {
        const entries = await fs.list(dir)
        for (const e of entries) {
          if (e.type === 'file') await fs.delete(e.path)
          if (e.type === 'dir') {
            // shallow delete (recursively list)
            try {
              const sub = await fs.list(e.path)
              for (const s of sub) { if (s.type === 'file') await fs.delete(s.path) }
            } catch {}
          }
        }
      } catch {}
    }
  } catch (e) { console.warn('[resetApp] FS wipe failed', e) }
}

function wipeLocalStorage() {
  try {
    const toRemove: string[] = []
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i)
      if (!k) continue
      if (LS_PREFIXES.some(p => k.startsWith(p))) toRemove.push(k)
    }
    toRemove.forEach(k => localStorage.removeItem(k))
  } catch (e) { console.warn('[resetApp] localStorage wipe failed', e) }
}

export async function resetApplicationState() {
  console.info('[resetApp] Resetting application state...')
  // Clear zustand stores
  useProfileStore.setState({ currentProfile: null, profiles: new Map() })
  useContactStore.setState({ contacts: [] })
  usePostStore.setState({ posts: [], loading: false, error: null })
  wipeLocalStorage()
  await wipeFs()
  console.info('[resetApp] Reset complete. A reload is recommended.')
}
