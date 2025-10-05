import { getFs } from '@/lib/fs'

const POSTS_DIR = '/data/posts'
const INDEX_FILE = '/data/posts/index.json'

export interface PersistedPostMeta { id: string; createdAt: string; updatedAt?: string }

export async function ensurePostsDir() {
  const fs = await getFs()
  try { await fs.mkdir(POSTS_DIR) } catch {}
}

export async function savePostsBulk(posts: any[]) {
  const fs = await getFs()
  await ensurePostsDir()
  // Write individual files (id.json) and an index for quick load
  const index: PersistedPostMeta[] = []
  for (const p of posts) {
    if (!p?.id) continue
    const path = `${POSTS_DIR}/${p.id}.json`
    try {
      await fs.writeText(path, JSON.stringify(p))
      index.push({ id: p.id, createdAt: p.createdAt, updatedAt: p.updatedAt })
    } catch (e) {
      console.warn('[postPersistence] Failed writing', path, e)
    }
  }
  try { await fs.writeText(INDEX_FILE, JSON.stringify(index)) } catch (e) { console.warn('[postPersistence] index write failed', e) }
}

export async function appendOrUpdatePost(post: any) {
  if (!post?.id) return
  const fs = await getFs()
  await ensurePostsDir()
  const path = `${POSTS_DIR}/${post.id}.json`
  try { await fs.writeText(path, JSON.stringify(post)) } catch (e) { console.warn('[postPersistence] write fail', e) }
  // Update index (read-modify-write) with a tiny optimistic retry to avoid concurrent write races
  const maxAttempts = 3
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      let index: PersistedPostMeta[] = []
      try { const raw = await fs.readText(INDEX_FILE); index = JSON.parse(raw) } catch { /* first writer wins */ }
      const existing = index.find(i => i.id === post.id)
      if (existing) { existing.updatedAt = post.updatedAt || post.createdAt } else { index.push({ id: post.id, createdAt: post.createdAt, updatedAt: post.updatedAt }) }
      // Overwrite (ZenFS writeFile already overwrites); no need for create-exclusive
      await fs.writeText(INDEX_FILE, JSON.stringify(index))
      break // success
    } catch (e:any) {
      if (attempt === maxAttempts) {
        console.warn('[postPersistence] index update fail after retries', e)
      } else {
        // brief backoff (non-blocking via setTimeout style sleep)
        await new Promise(r => setTimeout(r, 10 * attempt))
      }
    }
  }
}

export async function loadPersistedPosts(): Promise<any[]> {
  const fs = await getFs()
  let index: PersistedPostMeta[] = []
  try { const raw = await fs.readText(INDEX_FILE); index = JSON.parse(raw) } catch { return [] }
  const out: any[] = []
  for (const meta of index) {
    const path = `${POSTS_DIR}/${meta.id}.json`
    try { const raw = await fs.readText(path); out.push(JSON.parse(raw)) } catch (e) { /* ignore missing */ }
  }
  return out
}
