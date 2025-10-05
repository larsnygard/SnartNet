// ZenFS integration layer
// Provides a unified async initializer exposing a Node-like fs API subset.

import { configure, fs as zenFs } from '@zenfs/core'
import { IndexedDB } from '@zenfs/dom'

export interface ZenFsStatus {
  backend: 'opfs' | 'indexeddb'
  ready: boolean
}

let initPromise: Promise<ZenFsStatus> | null = null

export async function initZenFs(): Promise<ZenFsStatus> {
  if (initPromise) return initPromise
  initPromise = (async () => {
    // For now we only wire IndexedDB backend (OPFS backend not exported in @zenfs/dom current version).
    await configure({
      mounts: {
        '/': { backend: IndexedDB, storeName: 'snartnet-zenfs' }
      },
      defaultDirectories: true,
      addDevices: false,
      disableAccessChecks: true
    })
    try { await zenFs.promises.mkdir('/data') } catch {}
    return { backend: 'indexeddb', ready: true }
  })()
  return initPromise
}

export function getZenFs() {
  return zenFs
}

export async function ensureZenFs() {
  await initZenFs()
  return zenFs
}

// Convenience helpers mirroring minimal subset we need.
export async function writeText(path: string, text: string) {
  const fs = await ensureZenFs()
  await fs.promises.writeFile(path, text, 'utf8')
}

export async function readText(path: string): Promise<string> {
  const fs = await ensureZenFs()
  return await fs.promises.readFile(path, 'utf8') as unknown as string
}

export async function exists(path: string): Promise<boolean> {
  const fs = await ensureZenFs()
  try { await fs.promises.stat(path); return true } catch { return false }
}
