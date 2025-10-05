/**
 * Persistent File System (PFS) abstraction.
 * Attempts OPFS first (Origin Private File System) for true persistence & quota,
 * falls back to IndexedDB, then in-memory Map (ephemeral) as last resort.
 *
 * This is a practical substitute for ZenFS in the browser context.
 */

export interface PfsStat { path: string; size: number; mtime: number }
export interface PfsApi {
  writeFile(path: string, data: Blob | Uint8Array | string): Promise<void>
  readFile(path: string, asText?: boolean): Promise<Uint8Array | string>
  readBlob(path: string): Promise<Blob>
  deleteFile(path: string): Promise<void>
  exists(path: string): Promise<boolean>
  stat(path: string): Promise<PfsStat | null>
  list(prefix?: string): Promise<PfsStat[]>
  estimate(): Promise<{ usage: number; quota: number }>
}

function norm(p: string): string {
  if (!p.startsWith('/')) p = '/' + p
  return p.replace(/\\+/g, '/').replace(/\/+/g, '/')
}

// --- OPFS Driver -------------------------------------------------------
class OpfsDriver implements PfsApi {
  constructor(private root: FileSystemDirectoryHandle) {}
  private async getFileHandle(path: string, create: boolean) {
    const segs = norm(path).split('/').filter(Boolean)
    let dir = this.root
    for (let i = 0; i < segs.length - 1; i++) {
      dir = await dir.getDirectoryHandle(segs[i], { create })
    }
    const name = segs[segs.length - 1]
    return dir.getFileHandle(name, { create })
  }
  async writeFile(path: string, data: Blob | Uint8Array | string): Promise<void> {
    const fh = await this.getFileHandle(path, true)
    const w = await fh.createWritable()
    try {
      if (typeof data === 'string') {
        await w.write(new Blob([data]))
      } else if (data instanceof Blob) {
        await w.write(data)
      } else {
        const copy = new Uint8Array(data as Uint8Array)
        await w.write(new Blob([copy.buffer]))
      }
    } finally { await w.close() }
  }
  async readFile(path: string, asText = false): Promise<Uint8Array | string> {
    const blob = await this.readBlob(path)
    if (asText) return await blob.text()
    return new Uint8Array(await blob.arrayBuffer())
  }
  async readBlob(path: string): Promise<Blob> {
    const fh = await this.getFileHandle(path, false)
    const f = await fh.getFile(); return f
  }
  async deleteFile(path: string): Promise<void> {
    const segs = norm(path).split('/').filter(Boolean)
    let dir = this.root
    for (let i = 0; i < segs.length - 1; i++) dir = await dir.getDirectoryHandle(segs[i])
    await dir.removeEntry(segs[segs.length - 1])
  }
  async exists(path: string): Promise<boolean> { try { await this.readBlob(path); return true } catch { return false } }
  async stat(path: string): Promise<PfsStat | null> {
    try { const f = await (await this.getFileHandle(path, false)).getFile(); return { path: norm(path), size: f.size, mtime: f.lastModified } } catch { return null }
  }
  async list(prefix = '/'): Promise<PfsStat[]> {
    const out: PfsStat[] = []
    // OPFS directory iteration (shallow) – can be extended if nested structures required
    for await (const [name, handle] of (this.root as any).entries()) {
      if (handle.kind === 'file') {
        const f = await (handle as FileSystemFileHandle).getFile()
        const p = '/' + name
        if (p.startsWith(prefix)) out.push({ path: p, size: f.size, mtime: f.lastModified })
      }
    }
    return out
  }
  async estimate() {
    if (navigator.storage?.estimate) {
      const e = await navigator.storage.estimate(); return { usage: e.usage || 0, quota: e.quota || 0 }
    }
    return { usage: 0, quota: 0 }
  }
}

// --- IndexedDB Driver --------------------------------------------------
interface IdxRec { path: string; data: ArrayBuffer; mtime: number }
class IndexedDbDriver implements PfsApi {
  private dbp: Promise<IDBDatabase>
  constructor() { this.dbp = this.open() }
  private open(): Promise<IDBDatabase> {
    return new Promise((res, rej) => {
      const req = indexedDB.open('snartnet-pfs', 1)
      req.onupgradeneeded = () => {
        const db = req.result
        if (!db.objectStoreNames.contains('files')) db.createObjectStore('files', { keyPath: 'path' })
      }
      req.onsuccess = () => res(req.result)
      req.onerror = () => rej(req.error)
    })
  }
  private async store(mode: IDBTransactionMode) {
    const db = await this.dbp
    return db.transaction('files', mode).objectStore('files')
  }
  async writeFile(path: string, data: Blob | Uint8Array | string): Promise<void> {
    let ab: ArrayBuffer
    if (typeof data === 'string') ab = new TextEncoder().encode(data).buffer
    else if (data instanceof Blob) ab = await data.arrayBuffer()
    else {
      const buf = data.buffer
      ab = buf instanceof ArrayBuffer ? buf.slice(data.byteOffset, data.byteOffset + data.byteLength) : new Uint8Array(data).slice().buffer
    }
    const rec: IdxRec = { path: norm(path), data: ab, mtime: Date.now() }
    const os = await this.store('readwrite')
    await new Promise((res, rej) => { const r = os.put(rec); r.onsuccess = () => res(undefined); r.onerror = () => rej(r.error) })
  }
  private async get(path: string): Promise<IdxRec | null> {
    const os = await this.store('readonly')
    return await new Promise((res, rej) => { const r = os.get(norm(path)); r.onsuccess = () => res(r.result || null); r.onerror = () => rej(r.error) })
  }
  async readFile(path: string, asText=false): Promise<Uint8Array | string> {
    const rec = await this.get(path); if (!rec) throw new Error('Not found')
    const bytes = new Uint8Array(rec.data)
    return asText ? new TextDecoder().decode(bytes) : bytes
  }
  async readBlob(path: string): Promise<Blob> { const rec = await this.get(path); if (!rec) throw new Error('Not found'); return new Blob([rec.data]) }
  async deleteFile(path: string): Promise<void> { const os = await this.store('readwrite'); await new Promise((res, rej) => { const r = os.delete(norm(path)); r.onsuccess = () => res(undefined); r.onerror = () => rej(r.error) }) }
  async exists(path: string) { return (await this.get(path)) !== null }
  async stat(path: string) { const rec = await this.get(path); return rec ? { path: norm(path), size: rec.data.byteLength, mtime: rec.mtime } : null }
  async list(prefix = '/'): Promise<PfsStat[]> {
    const os = await this.store('readonly')
    return await new Promise((res, rej) => {
      const out: PfsStat[] = []
      const cur = os.openCursor()
      cur.onsuccess = () => {
        const c = cur.result
        if (c) {
          const v = c.value as IdxRec
            if (v.path.startsWith(prefix)) out.push({ path: v.path, size: v.data.byteLength, mtime: v.mtime })
            c.continue()
        } else res(out)
      }
      cur.onerror = () => rej(cur.error)
    })
  }
  async estimate() {
    if (navigator.storage?.estimate) { const e = await navigator.storage.estimate(); return { usage: e.usage || 0, quota: e.quota || 0 } }
    return { usage: 0, quota: 0 }
  }
}

// --- Memory Driver -----------------------------------------------------
class MemoryDriver implements PfsApi {
  private map = new Map<string,{data:Uint8Array;mtime:number}>()
  async writeFile(path: string, data: Blob | Uint8Array | string) {
    let bytes: Uint8Array
    if (typeof data === 'string') bytes = new TextEncoder().encode(data)
    else if (data instanceof Blob) bytes = new Uint8Array(await data.arrayBuffer())
    else bytes = new Uint8Array(data)
    this.map.set(norm(path), { data: bytes, mtime: Date.now() })
  }
  async readFile(path: string, asText=false) { const rec = this.map.get(norm(path)); if (!rec) throw new Error('Not found'); return asText ? new TextDecoder().decode(rec.data) : rec.data }
  async readBlob(path: string) { const rec = this.map.get(norm(path)); if (!rec) throw new Error('Not found'); const copy = new Uint8Array(rec.data); return new Blob([copy.buffer]) }
  async deleteFile(path: string) { this.map.delete(norm(path)) }
  async exists(path: string) { return this.map.has(norm(path)) }
  async stat(path: string) { const rec = this.map.get(norm(path)); return rec ? { path: norm(path), size: rec.data.byteLength, mtime: rec.mtime } : null }
  async list(prefix='/') { const out: PfsStat[] = []; for (const [p,r] of this.map.entries()) if (p.startsWith(prefix)) out.push({ path:p,size:r.data.byteLength,mtime:r.mtime }); return out }
  async estimate() { return { usage: [...this.map.values()].reduce((s,r)=>s+r.data.byteLength,0), quota: 0 } }
}

let pfsPromise: Promise<PfsApi> | null = null
export async function getPfs(): Promise<PfsApi> {
  if (!pfsPromise) {
    pfsPromise = (async () => {
      // Try OPFS
      try {
        if ('storage' in navigator && (navigator as any).storage.getDirectory) {
          const root = await (navigator as any).storage.getDirectory()
          console.info('[PFS] Using OPFS')
          return new OpfsDriver(root)
        }
      } catch (e) { console.warn('[PFS] OPFS failed', e) }
      // Try IndexedDB
      try { console.info('[PFS] Using IndexedDB'); return new IndexedDbDriver() } catch (e) { console.warn('[PFS] IndexedDB failed', e) }
      console.warn('[PFS] Falling back to memory driver (non-persistent)')
      return new MemoryDriver()
    })()
  }
  return pfsPromise
}

export async function pfsReady() { try { await getPfs(); return true } catch { return false } }
