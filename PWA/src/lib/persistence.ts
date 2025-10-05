import { getFs } from '@/lib/fs'

const PROFILE_PATH = '/data/profile.json'
const MESSAGES_DIR = '/data/messages'
const POSTS_INDEX_PATH = '/data/posts/index.json' // already used by postPersistence

async function ensureDir(path: string) {
  const fs = await getFs()
  try { await fs.mkdir(path) } catch {}
}

export async function writeProfileJson(profile: any) {
  if (!profile) return
  const fs = await getFs()
  await ensureDir('/data')
  try { await fs.writeText(PROFILE_PATH, JSON.stringify(profile, null, 2)) } catch (e) { console.warn('[persistence] writeProfileJson failed', e) }
}

export async function readProfileJson(): Promise<any | null> {
  const fs = await getFs()
  try { return JSON.parse(await fs.readText(PROFILE_PATH)) } catch { return null }
}

export async function writeMessageThread(contactId: string, messages: any[]) {
  const fs = await getFs()
  await ensureDir('/data')
  await ensureDir(MESSAGES_DIR)
  const path = `${MESSAGES_DIR}/${contactId}.json`
  try { await fs.writeText(path, JSON.stringify(messages, null, 2)) } catch (e) { console.warn('[persistence] writeMessageThread failed', contactId, e) }
}

export async function readAllMessageThreads(): Promise<Record<string, any[]>> {
  const fs = await getFs()
  const out: Record<string, any[]> = {}
  try {
    const list = await fs.list(MESSAGES_DIR)
    for (const ent of list) {
      if (ent.type === 'file' && ent.path.endsWith('.json')) {
        try {
          const raw = await fs.readText(ent.path)
          const arr = JSON.parse(raw)
          const id = ent.path.substring(MESSAGES_DIR.length + 1).replace(/\.json$/,'')
          if (Array.isArray(arr)) out[id] = arr
        } catch {}
      }
    }
  } catch { /* directory may not exist yet */ }
  return out
}

export { PROFILE_PATH, POSTS_INDEX_PATH }
