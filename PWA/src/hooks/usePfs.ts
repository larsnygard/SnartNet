import { useState, useEffect, useCallback, useRef } from 'react'
import { getPfs, type PfsApi } from '@/lib/pfs'

interface UsePfsResult {
  fs: PfsApi | null
  ready: boolean
  error: Error | null
  writeText: (path: string, data: string) => Promise<void>
  readText: (path: string) => Promise<string>
  list: (prefix?: string) => Promise<string[]>
}

export function usePfs(): UsePfsResult {
  const [fs, setFs] = useState<PfsApi | null>(null)
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<Error | null>(null)
  const started = useRef(false)

  useEffect(() => {
    if (started.current) return
    started.current = true
    getPfs()
      .then(api => { setFs(api); setReady(true) })
      .catch(e => { setError(e instanceof Error ? e : new Error(String(e))); setReady(false) })
  }, [])

  const writeText = useCallback(async (path: string, data: string) => {
    if (!fs) throw new Error('PFS not ready')
    await fs.writeFile(path, data)
  }, [fs])

  const readText = useCallback(async (path: string) => {
    if (!fs) throw new Error('PFS not ready')
    return await fs.readFile(path, true) as string
  }, [fs])

  const list = useCallback(async (prefix?: string) => {
    if (!fs) throw new Error('PFS not ready')
    const stats = await fs.list(prefix)
    return stats.map(s => s.path)
  }, [fs])

  return { fs, ready, error, writeText, readText, list }
}
