import { useEffect } from 'react'
import { getCore } from '@/lib/core'
import { useProfileStore } from '@/stores/profileStore'
import { getTorrentService } from '@/lib/torrent'
import { usePostStore } from '@/stores/postStore'
import { getPfs } from '@/lib/pfs'
import { initZenFs } from '@/lib/zenfs'
import { perfMark, perfMeasure, startMemoryLogging, debugEnabled } from '@/lib/debug'
import { getFs } from '@/lib/fs'
import { readProfileJson } from '@/lib/persistence'
import { useMessageStore } from '@/stores/messageStore'
import { readAllMessageThreads } from '@/lib/persistence'

/**
 * Hook to initialize the core and load existing profile
 */
export function useInitializeCore() {
  const { setCurrentProfile, setLoading, setError } = useProfileStore()

  useEffect(() => {
    let isMounted = true

    const initializeCore = async () => {
      try {
        setLoading(true)
        setError(null)
        
        console.log('[useInitializeCore] Initializing core...')
        if (debugEnabled) { perfMark('core:init:start'); startMemoryLogging() }
  // Ensure FS ready before any loads to avoid memory fallback issues
  await getFs().catch(()=>{})
  const core = await getCore()
        console.log('[useInitializeCore] Core initialized successfully')
        if (debugEnabled) perfMark('core:init:done')
        if (debugEnabled) perfMeasure('core:init', 'core:init:start', 'core:init:done')

        if (debugEnabled) {
          // List /data for diagnostics
            try {
              const fs = await getFs()
              const list = await fs.list('/data')
              console.info('[useInitializeCore] FS backend:', fs.backend, 'entries:', list.map(e=>e.path+':'+e.size).join(', '))
            } catch (e) {
              console.warn('[useInitializeCore] FS list failed', e)
            }
        }

        // Defer heavy storage subsystem initialization to avoid blocking initial paint.
        // Use requestIdleCallback if available, otherwise setTimeout.
        const schedule = (cb: () => void) => {
          if ('requestIdleCallback' in window) {
            ;(window as any).requestIdleCallback(cb, { timeout: 1500 })
          } else setTimeout(cb, 50)
        }
        schedule(() => {
          let cancelled = false
          if (debugEnabled) perfMark('storage:init:start')
          initZenFs()
            .then(status => { if (!cancelled) console.log(`[useInitializeCore] ZenFS ready (backend=${status.backend})`) })
            .catch(e => {
              if (cancelled) return
              console.warn('[useInitializeCore] ZenFS init failed, falling back to PFS', e)
              getPfs()
                .then(() => !cancelled && console.log('[useInitializeCore] PFS fallback ready'))
                .catch(err => !cancelled && console.warn('[useInitializeCore] PFS fallback failed', err))
            })
            .finally(() => {
              if (debugEnabled) {
                perfMark('storage:init:done')
                perfMeasure('storage:init', 'storage:init:start', 'storage:init:done')
              }
            })
          // Attach a cleanup in case component unmounts before completion
          cleanupFns.push(() => { cancelled = true })
        })

        // Load persisted profile.json fallback if core profile missing
        try {
          if (!core.hasProfile()) {
            const persistedProfile = await readProfileJson()
            if (persistedProfile) {
              console.log('[useInitializeCore] Loaded profile.json fallback from FS')
              setCurrentProfile(persistedProfile)
            }
          }
        } catch (e) { console.warn('[useInitializeCore] profile.json fallback load failed', e) }

        // Load persisted messages
        try {
          const threads = await readAllMessageThreads()
          const msgStore = useMessageStore.getState()
          for (const [cid, msgs] of Object.entries(threads)) {
            msgStore.setMessages(cid, msgs as any)
          }
          if (Object.keys(threads).length) console.log('[useInitializeCore] Loaded message threads:', Object.keys(threads).length)
        } catch (e) { console.warn('[useInitializeCore] messages load failed', e) }

        // Load persisted posts (filesystem) before network sync
        try {
          await usePostStore.getState().loadPersisted()
          console.log('[useInitializeCore] Loaded persisted posts')
        } catch (e) {
          console.warn('[useInitializeCore] Failed loading persisted posts', e)
        }

        // Initialize torrent service (after local restore)
        console.log('[useInitializeCore] Initializing torrent service...')
        getTorrentService();
        console.log('[useInitializeCore] Torrent service initialized.')

        // Check if there's an existing profile
        if (core.hasProfile()) {
          console.log('[useInitializeCore] Found existing profile, loading...')
          const profile = await core.getCurrentProfile()
          if (profile && isMounted) {
            console.log('[useInitializeCore] Loaded existing profile:', profile)
            setCurrentProfile(profile)
          }
        } else {
          console.log('[useInitializeCore] No existing profile found')
        }

      } catch (error) {
        console.error('[useInitializeCore] Failed to initialize core:', error)
        if (isMounted) {
          const errorMessage = error instanceof Error ? error.message : 'Failed to initialize'
          setError(errorMessage)
        }
      } finally {
        if (isMounted) {
          setLoading(false)
        }
      }
    }

    const cleanupFns: Array<() => void> = []
    initializeCore()

    return () => {
      isMounted = false
      for (const fn of cleanupFns) {
        try { fn() } catch {}
      }
    }
  }, [setCurrentProfile, setLoading, setError])
}