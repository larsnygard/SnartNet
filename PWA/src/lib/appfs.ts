// Application virtual filesystem overlay.
// Presents domain data (profiles, posts, contacts, messages) as structured JSON files.

import { useProfileStore } from '@/stores/profileStore'
import { usePostStore } from '@/stores/postStore'
import { useContactStore } from '@/stores/contactStore'
import { useMessageStore } from '@/stores/messageStore'

export interface AppFsEntry { path: string; type: 'file' | 'dir'; size: number; mtime: number }

function now() { return Date.now() }

export async function listAppFs(dir: string): Promise<AppFsEntry[]> {
  if (!dir.startsWith('/')) dir = '/' + dir
  if (dir !== '/' && dir.endsWith('/')) dir = dir.slice(0,-1)
  const entries: AppFsEntry[] = []
  const push = (path: string, type: 'file' | 'dir', size = 0, mtime = now()) => entries.push({ path, type, size, mtime })

  const profileState = useProfileStore.getState()
  const postState = usePostStore.getState()
  const contactState = useContactStore.getState()
  const messageState = useMessageStore.getState()

  // Root listing: top-level virtual folders + profile.json
  if (dir === '/') {
    push('/profile.json', 'file', JSON.stringify(profileState.currentProfile || {}).length)
    push('/posts', 'dir')
    push('/contacts', 'dir')
    push('/messages', 'dir')
    return entries
  }
  if (dir === '/posts') {
    for (const p of postState.posts) {
      const json = JSON.stringify(p)
      push(`/posts/${p.id}.json`, 'file', json.length, new Date(p.updatedAt || p.createdAt).getTime())
    }
    return entries
  }
  if (dir === '/contacts') {
    for (const c of contactState.contacts) {
      const json = JSON.stringify(c)
      push(`/contacts/${c.id}.json`, 'file', json.length, new Date(c.addedDate).getTime())
    }
    return entries
  }
  if (dir === '/messages') {
    for (const [contactId, thread] of Object.entries(messageState.threads)) {
      const json = JSON.stringify(thread.messages)
      const lastMsg = thread.messages.length ? thread.messages[thread.messages.length - 1] : null
      push(`/messages/${contactId}.json`, 'file', json.length, lastMsg ? new Date(lastMsg.timestamp).getTime() : now())
    }
    return entries
  }
  return []
}

export async function readAppFs(path: string): Promise<string> {
  if (!path.startsWith('/')) path = '/' + path
  const profileState = useProfileStore.getState()
  const postState = usePostStore.getState()
  const contactState = useContactStore.getState()
  const messageState = useMessageStore.getState()

  if (path === '/profile.json') return JSON.stringify(profileState.currentProfile, null, 2)
  if (path.startsWith('/posts/')) {
    const id = path.slice('/posts/'.length).replace(/\.json$/,'')
    const p = postState.posts.find(p=>p.id===id)
    if (!p) throw new Error('Post not found')
    return JSON.stringify(p, null, 2)
  }
  if (path.startsWith('/contacts/')) {
    const id = path.slice('/contacts/'.length).replace(/\.json$/,'')
    const c = contactState.contacts.find(c=>c.id===id)
    if (!c) throw new Error('Contact not found')
    return JSON.stringify(c, null, 2)
  }
  if (path.startsWith('/messages/')) {
    const id = path.slice('/messages/'.length).replace(/\.json$/,'')
    const thread = messageState.threads[id]
    if (!thread) throw new Error('Thread not found')
    return JSON.stringify(thread.messages, null, 2)
  }
  throw new Error('Not found')
}

// Write support (limited): allow editing profile.json and post content text.
export async function writeAppFs(path: string, data: string): Promise<void> {
  if (!path.startsWith('/')) path = '/' + path
  if (path === '/profile.json') {
    try {
      const obj = JSON.parse(data)
      if (obj && obj.username) {
        useProfileStore.getState().setCurrentProfile(obj)
      }
    } catch (e) { throw new Error('Invalid JSON for profile') }
    return
  }
  if (path.startsWith('/posts/')) {
    const id = path.slice('/posts/'.length).replace(/\.json$/,'')
    try {
      const obj = JSON.parse(data)
      if (obj && obj.content) {
        await usePostStore.getState().editPost(id, obj.content)
      }
    } catch (e) { throw new Error('Invalid JSON for post edit') }
    return
  }
  throw new Error('Write not supported for this path')
}

export async function existsAppFs(path: string): Promise<boolean> {
  try { await readAppFs(path); return true } catch { return false }
}

export async function deleteAppFs(_path: string): Promise<void> {
  // Not implemented (intentionally read-mostly overlay)
  throw new Error('Delete not supported in appfs')
}

export interface AppFsFacade {
  mode: 'appfs'
  backend: 'virtual'
  list: typeof listAppFs
  readText: typeof readAppFs
  writeText: typeof writeAppFs
  delete: typeof deleteAppFs
  exists: typeof existsAppFs
  mkdir: (p: string) => Promise<void>
}

export function getAppFs(): AppFsFacade {
  return {
    mode: 'appfs',
    backend: 'virtual',
    list: listAppFs,
    readText: readAppFs,
    writeText: writeAppFs,
    delete: deleteAppFs,
    exists: existsAppFs,
    mkdir: async () => { /* no-op */ }
  }
}
