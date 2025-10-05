import { useEffect } from 'react'
import { getCore } from '@/lib/core'
import { useProfileStore } from '@/stores/profileStore'
import { getTorrentService } from '@/lib/torrent'
import { getPfs } from '@/lib/pfs'
import { initZenFs } from '@/lib/zenfs'

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
        const core = await getCore()
        console.log('[useInitializeCore] Core initialized successfully')

        // Fire & forget ZenFS init, fallback to PFS if unavailable
        initZenFs()
          .then(status => console.log(`[useInitializeCore] ZenFS ready (backend=${status.backend})`))
          .catch(e => {
            console.warn('[useInitializeCore] ZenFS init failed, falling back to PFS', e)
            getPfs()
              .then(() => console.log('[useInitializeCore] PFS fallback ready'))
              .catch(err => console.warn('[useInitializeCore] PFS fallback failed', err))
          })

        // Initialize torrent service
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

    initializeCore()

    return () => {
      isMounted = false
    }
  }, [setCurrentProfile, setLoading, setError])
}