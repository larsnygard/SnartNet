// Unified filesystem facade selecting ZenFS first, then PFS.
// Provides a minimal API for the File Manager UI: list, read text, write text, delete, exists.

import { initZenFs, getZenFs } from '@/lib/zenfs'
import { getPfs } from '@/lib/pfs'

export interface FsEntry { path: string; size: number; mtime: number; type: 'file' | 'dir' }
export interface FsFacade {
  backend: string
  list(dir: string): Promise<FsEntry[]>
  readText(path: string): Promise<string>
  writeText(path: string, data: string): Promise<void>
  delete(path: string): Promise<void>
  exists(path: string): Promise<boolean>
  mkdir(path: string): Promise<void>
}

let facadePromise: Promise<FsFacade> | null = null

export async function getFs(): Promise<FsFacade> {
  if (facadePromise) return facadePromise
  facadePromise = (async () => {
    // Try ZenFS
    try {
      await initZenFs()
      const zfs: any = getZenFs()
      const promises = zfs.promises
      return {
        backend: 'zenfs:indexeddb',
        async list(dir: string) {
          if (!dir) dir = '/'
            if (!dir.startsWith('/')) dir = '/' + dir
          const entries = await promises.readdir(dir, { withFileTypes: true })
          const out: FsEntry[] = []
          for (const ent of entries) {
            const full = (dir === '/' ? '' : dir) + '/' + ent.name
            try {
              const st = await promises.stat(full)
              out.push({ path: full, size: st.size ?? 0, mtime: st.mtime ?? Date.now(), type: ent.isDirectory() ? 'dir' : 'file' })
            } catch {/* ignore */}
          }
          return out.sort((a,b)=> a.type===b.type ? a.path.localeCompare(b.path) : a.type==='dir'? -1:1)
        },
        async readText(path: string) { return promises.readFile(path, 'utf8') as unknown as string },
        async writeText(path: string, data: string) { await promises.mkdir(path.substring(0, path.lastIndexOf('/')) || '/', { recursive: true }); await promises.writeFile(path, data, 'utf8') },
        async delete(path: string) { await promises.unlink(path) },
        async exists(path: string) { try { await promises.stat(path); return true } catch { return false } },
        async mkdir(path: string) { await promises.mkdir(path, { recursive: true }) }
      }
    } catch (e) {
      console.warn('[fs facade] ZenFS not available, falling back', e)
    }
    // Fallback PFS (flat, no real dirs). We'll synthesize directory entries from path prefixes.
    const pfs = await getPfs()
    return {
      backend: 'pfs:fallback',
      async list(dir: string) {
        if (!dir) dir = '/'
        if (!dir.endsWith('/')) dir += '/'
        const stats = await pfs.list('/')
        const files = stats.filter(s => s.path.startsWith(dir))
        // Only immediate children
        const seenDirs = new Set<string>()
        const out: FsEntry[] = []
        for (const f of files) {
          const rest = f.path.substring(dir.length)
          if (rest.includes('/')) {
            const top = rest.split('/')[0]
            const dpath = (dir === '/' ? '' : dir) + top
            if (!seenDirs.has(dpath)) {
              seenDirs.add(dpath)
              out.push({ path: dpath, size: 0, mtime: f.mtime, type: 'dir' })
            }
          } else {
            out.push({ path: f.path, size: f.size, mtime: f.mtime, type: 'file' })
          }
        }
        return out.sort((a,b)=> a.type===b.type ? a.path.localeCompare(b.path) : a.type==='dir'? -1:1)
      },
      async readText(path: string) { return pfs.readFile(path, true) as Promise<string> },
      async writeText(path: string, data: string) { await pfs.writeFile(path, data) },
      async delete(path: string) { await pfs.deleteFile(path) },
      async exists(path: string) { return pfs.exists(path) },
      async mkdir(_path: string) { /* directories implicit */ }
    }
  })()
  return facadePromise
}

export async function resetFsFacadeForTests() { facadePromise = null }
