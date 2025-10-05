import { useEffect, useState, useCallback } from 'react'
import { initZenFs, getZenFs, writeText as zenWriteText, readText as zenReadText, exists as zenExists } from '@/lib/zenfs'

export interface UseZenFsResult {
  ready: boolean
  backend: string | null
  error: Error | null
  writeText: (path: string, text: string) => Promise<void>
  readText: (path: string) => Promise<string>
  exists: (path: string) => Promise<boolean>
  fs: any | null
}

export function useZenFs(): UseZenFsResult {
  const [ready, setReady] = useState(false)
  const [backend, setBackend] = useState<string | null>(null)
  const [error, setError] = useState<Error | null>(null)
  const [fsObj, setFsObj] = useState<any | null>(null)

  useEffect(() => {
    let mounted = true
    initZenFs()
      .then(status => {
        if (!mounted) return
        setBackend(status.backend)
        setReady(status.ready)
        setFsObj(getZenFs())
      })
      .catch(e => { if (mounted) setError(e instanceof Error ? e : new Error(String(e))) })
    return () => { mounted = false }
  }, [])

  const writeText = useCallback(async (p: string, t: string) => {
    await zenWriteText(p, t)
  }, [])
  const readText = useCallback(async (p: string) => zenReadText(p), [])
  const exists = useCallback(async (p: string) => zenExists(p), [])

  return { ready, backend, error, writeText, readText, exists, fs: fsObj }
}
