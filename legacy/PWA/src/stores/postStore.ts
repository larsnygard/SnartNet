import { create } from 'zustand'
import { getTorrentService } from '@/lib/torrent'
import { verifyPostSignature, deriveFingerprint, signPost } from '@/lib/crypto/ed25519'
import { useContactStore } from './contactStore'

export interface PostImage {
  id: string
  data: string // base64 data
  filename: string
  size: number
  mimeType: string
}

export interface TorrentPost {
  id: string
  author: string
  authorDisplayName?: string
  authorAvatar?: string
  content: string
  images?: PostImage[]
  magnetUri?: string // The torrent magnet link for this post
  createdAt: string
  updatedAt?: string
  tags?: string[]
  // Signing fields
  authorPublicKey?: string // base64 Ed25519 public key
  signature?: string // base64 signature of canonical payload
  signatureVerified?: boolean
  signatureError?: string
  fingerprint?: string // short display fingerprint derived from public key
  isSeeding?: boolean
  seedProgress?: number
  seedError?: string
}

export interface NormalizedPost {
  id: string
  author: string
  authorDisplayName?: string
  authorAvatar?: string
  content: string
  images?: PostImage[]
  magnetUri?: string
  createdAt: string
  updatedAt?: string
  tags?: string[]
  version: number
  schemaVersion: 1
  authorPublicKey?: string
  signature?: string
  signatureVerified?: boolean
  signatureError?: string
  fingerprint?: string
}

function normalizePost(raw: any): NormalizedPost | null {
  if (!raw) return null
  return {
    id: raw.id || raw.postId || Math.random().toString(36).slice(2),
    author: raw.author || raw.author_fingerprint || raw.fingerprint || 'unknown',
    authorDisplayName: raw.authorDisplayName || raw.author_display_name || raw.displayName,
    authorAvatar: raw.authorAvatar || raw.author_avatar,
    content: raw.content || '',
    images: raw.images,
    magnetUri: raw.magnetUri || raw.magnet_uri,
    createdAt: raw.createdAt || raw.created_at || new Date().toISOString(),
    updatedAt: raw.updatedAt || raw.updated_at,
    tags: raw.tags,
    version: raw.version || 1,
    schemaVersion: 1,
    authorPublicKey: raw.authorPublicKey || raw.author_public_key,
    signature: raw.signature,
    signatureVerified: raw.signatureVerified || raw.signature_verified,
    signatureError: raw.signatureError || raw.signature_error,
    fingerprint: raw.fingerprint,
  }
}

interface PostState {
  posts: TorrentPost[]
  loading: boolean
  error: string | null

  // Actions
  addPost: (post: Omit<TorrentPost, 'id' | 'createdAt' | 'magnetUri'>) => Promise<void>
  removePost: (postId: string) => void
  updatePost: (postId: string, updates: Partial<TorrentPost>) => void
  updateSeedingStatus: (postId: string, isSeeding: boolean, progress?: number) => void
  loadPostsFromContacts: () => Promise<void>
  setLoading: (loading: boolean) => void
  setError: (error: string | null) => void
  clearError: () => void
  syncPostsForContact: (contactId: string, options?: { maxPosts?: number; monthsLookback?: number }) => Promise<void>
  fetchPostFromMagnet: (magnetUri: string) => Promise<TorrentPost | null>
  editPost: (postId: string, newContent: string) => Promise<void>
  regenerateAuthorIndex: (author: string) => Promise<void>
  deletePost: (postId: string) => Promise<void>
}

const LOCAL_STORAGE_KEY = 'snartnet:posts';

export const usePostStore = create<PostState>((set) => ({
  posts: (() => {
    try {
      const raw = localStorage.getItem(LOCAL_STORAGE_KEY)
      if (raw) return JSON.parse(raw)
    } catch { }
    return []
  })(),
  loading: false,
  error: null,

  addPost: async (postData) => {
    const createdAt = new Date().toISOString()
    // Prepare canonical signing input
    let signature: string | undefined
    let authorPublicKey: string | undefined
    try {
      const signed = await signPost({
        version: 1,
        kind: 'post',
        body: postData.content,
        createdAt,
        attachments: postData.images?.map(i => i.id),
        // parentId / replyTo omitted for now
      } as any)
      signature = signed.signature
      authorPublicKey = signed.authorPublicKey
    } catch (e) {
      console.warn('Signing post failed; proceeding unsigned', e)
    }

    const base: Partial<TorrentPost> = {
      ...postData,
      id: Math.random().toString(36).slice(2),
      createdAt,
      isSeeding: true,
      seedProgress: 0,
      signature,
      authorPublicKey,
      fingerprint: authorPublicKey ? deriveFingerprint(authorPublicKey) : undefined,
      signatureVerified: !!signature, // locally created so we trust it
    }
    const newPost: TorrentPost = normalizePost(base) as any

    set((state) => {
      const posts = [newPost, ...state.posts].sort((a, b) =>
        new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      )
      localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
      return { posts }
    })

    try {
      const torrentService = getTorrentService()
      // Seed full post including signature fields
      const magnetUri = await torrentService.seedPost({ ...(newPost as any) })

      set((state) => {
        const posts = state.posts.map(p =>
          p.id === newPost.id ? { ...p, magnetUri, seedProgress: 100, isSeeding: false } : p
        )
        localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
        return { posts }
      })
    } catch (error) {
      console.error('Failed to seed post:', error)
      set((state) => {
        const posts = state.posts.map(p =>
          p.id === newPost.id ? { ...p, isSeeding: false, seedProgress: 0, seedError: (error instanceof Error ? error.message : 'Failed to seed') } : p
        )
        localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
        return { posts }
      })
    }
  },

  removePost: (postId) => {
    set((state) => {
      const posts = state.posts.filter(post => post.id !== postId)
      localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
      return { posts }
    })
  },

  updatePost: (postId, updates) => {
    set((state) => {
      const posts = state.posts.map(post =>
        post.id === postId ? { ...post, ...updates } : post
      )
      localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
      return { posts }
    })
  },

  updateSeedingStatus: (postId, isSeeding, progress) => {
    set((state) => {
      const posts = state.posts.map(post =>
        post.id === postId ? { ...post, isSeeding, seedProgress: progress } : post
      )
      localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
      return { posts }
    })
  },

  loadPostsFromContacts: async () => {
    const contactStore = useContactStore.getState()
    const contacts = contactStore.contacts.filter(c => !!c.postIndexMagnetUri)
    if (contacts.length === 0) {
      set({ loading: false })
      return
    }
    set({ loading: true })
    const limit = 2 // small concurrency limit to avoid overwhelming torrent engine
    const queue = [...contacts]
    const running: Promise<void>[] = []
    const startedIds = new Set<string>()

    const runNext = () => {
      if (queue.length === 0) return
      const contact = queue.shift()!
      startedIds.add(contact.id)
      const p = (async () => {
        try {
          await usePostStore.getState().syncPostsForContact(contact.id, {
            maxPosts: contact.syncMaxPosts,
            monthsLookback: contact.syncMonthsLookback
          })
        } catch (e) {
          console.warn('Sync failed for contact', contact.id, e)
        }
      })().finally(() => {
        // Remove finished promise from running and schedule next
        const idx = running.indexOf(p as any)
        if (idx >= 0) running.splice(idx, 1)
        if (queue.length > 0) runNext()
        else if (running.length === 0) {
          // All done
          set({ loading: false })
        }
      })
      running.push(p as any)
      if (running.length < limit && queue.length > 0) runNext()
    }

    // Prime initial workers
    for (let i = 0; i < limit && queue.length > 0; i++) runNext()
  },

  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),

  syncPostsForContact: async (contactId, options) => {
    const contactStore = useContactStore.getState()
    const contact = contactStore.contacts.find(c => c.id === contactId)
    if (!contact || !contact.postIndexMagnetUri) return
    set({ loading: true })
    try {
      const torrentService = getTorrentService()
      const result = await (torrentService as any).downloadPostIndexChain(contact.postIndexMagnetUri, {
        maxPosts: options?.maxPosts ?? contact.syncMaxPosts ?? 100,
        monthsLookback: options?.monthsLookback ?? contact.syncMonthsLookback,
      })
      // Download post torrents for new entries (limited concurrency)
      const existingIds = new Set(usePostStore.getState().posts.map(p => p.id))
      const targets = result.entries.filter((e: any) => e.magnetUri && !existingIds.has(e.id))
      const limit = 3
      const queue = [...targets]
      const downloaded: TorrentPost[] = []
      while (queue.length > 0) {
        const batch = queue.splice(0, limit)
        await Promise.all(batch.map(async (e: any) => {
          try {
            const post = await (getTorrentService() as any).downloadPost(e.magnetUri)
            if (post && typeof post === 'object') {
              // Verify signature if present
              let verifiedPost: any = post
              try {
                if (post.signature && post.authorPublicKey) {
                  const { ok, error } = await verifyPostSignature({
                    ...post,
                    version: post.version || 1,
                    kind: 'post'
                  })
                  verifiedPost.signatureVerified = ok
                  verifiedPost.signatureError = ok ? undefined : error || 'invalid-signature'
                  verifiedPost.fingerprint = post.authorPublicKey ? deriveFingerprint(post.authorPublicKey) : post.fingerprint
                } else {
                  verifiedPost.signatureVerified = false
                  verifiedPost.signatureError = 'missing-signature'
                }
              } catch (verr: any) {
                verifiedPost.signatureVerified = false
                verifiedPost.signatureError = verr?.message || 'verify-failed'
              }
              downloaded.push(normalizePost(verifiedPost) as any)
            } else {
              // fallback placeholder
              downloaded.push({
                id: e.id,
                author: e.author,
                content: `(Unavailable content) @${e.author}`,
                createdAt: e.createdAt,
                magnetUri: e.magnetUri,
                images: []
              })
            }
          } catch (err) {
            downloaded.push({
              id: e.id,
              author: e.author,
              content: `(Failed to fetch) @${e.author}`,
              createdAt: e.createdAt,
              magnetUri: e.magnetUri,
              images: []
            })
          }
        }))
      }
      if (downloaded.length) {
        set((state) => {
          const posts = [...state.posts, ...downloaded].sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())
          localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(posts))
          return { posts, loading: false }
        })
      } else {
        set({ loading: false })
      }
    } catch (e) {
      set({ loading: false, error: e instanceof Error ? e.message : 'Failed to sync posts' })
    }
  },

  fetchPostFromMagnet: async (magnetUri) => {
    try {
      const raw = await (getTorrentService() as any).downloadPost(magnetUri)
      if (!raw) return null
      let processed: any = raw
      try {
        if (raw.signature && raw.authorPublicKey) {
          const { ok, error } = await verifyPostSignature({
            ...raw,
            version: raw.version || 1,
            kind: 'post'
          })
          processed.signatureVerified = ok
          processed.signatureError = ok ? undefined : error || 'invalid-signature'
          processed.fingerprint = raw.authorPublicKey ? deriveFingerprint(raw.authorPublicKey) : raw.fingerprint
        } else {
          processed.signatureVerified = false
          processed.signatureError = 'missing-signature'
        }
      } catch (e: any) {
        processed.signatureVerified = false
        processed.signatureError = e?.message || 'verify-failed'
      }
      const normalized = normalizePost(processed)
      if (!normalized) return null
      set((state) => {
        const exists = state.posts.some(p => p.id === normalized.id)
        if (exists) {
          return { posts: state.posts.map(p => p.id === normalized.id ? { ...p, ...normalized } : p) }
        }
        return { posts: [normalized as any, ...state.posts].sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()) }
      })
      return normalized as any
    } catch (e) {
      console.error('Failed to download post', e)
      return null
    }
  }
  ,
  editPost: async (postId, newContent) => {
    const state = usePostStore.getState()
    const post = state.posts.find(p => p.id === postId)
    if (!post) return
    const updatedAt = new Date().toISOString()
    // Re-sign updated content
    let signature: string | undefined
    let authorPublicKey: string | undefined
    try {
      const signed = await signPost({
        version: 1,
        kind: 'post',
        body: newContent,
        createdAt: post.createdAt,
        updatedAt,
        attachments: post.images?.map(i => i.id),
      } as any)
      signature = signed.signature
      authorPublicKey = signed.authorPublicKey
    } catch (e) {
      console.warn('Re-signing edited post failed; proceeding unsigned', e)
    }
    // Optimistic local update (magnet will be updated after seeding)
    set((s) => ({
      posts: s.posts.map(p => p.id === postId ? { ...p, content: newContent, updatedAt, signature, authorPublicKey, signatureVerified: !!signature, isSeeding: true } : p)
    }))
    try {
      const torrentService = getTorrentService()
      const magnetUri = await torrentService.seedPost({ ...(post as any), content: newContent, updatedAt, signature, authorPublicKey })
      set((s) => ({
        posts: s.posts.map(p => p.id === postId ? { ...p, magnetUri, isSeeding: false, seedProgress: 100 } : p)
      }))
    } catch (e) {
      console.error('Failed to reseed edited post', e)
      set((s) => ({
        posts: s.posts.map(p => p.id === postId ? { ...p, isSeeding: false, seedError: 'Edit reseed failed' } : p)
      }))
    }
    await usePostStore.getState().regenerateAuthorIndex(post.author)
  },
  regenerateAuthorIndex: async (author) => {
    try {
      const allPosts = usePostStore.getState().posts.filter(p => p.author === author && p.magnetUri)
      const torrentService = getTorrentService()
      // Use current profile's existing head as previous if present
      const profileStore = (await import('./profileStore'))
      const { useProfileStore } = profileStore as any
      const currentProfile = useProfileStore.getState().currentProfile
      const previousHead = currentProfile?.postIndexMagnetUri || null
      const result = await (torrentService as any).seedPostIndex(allPosts as any, author, { previousHeadMagnet: previousHead })
      if (currentProfile && currentProfile.username === author) {
        // Update profile with new head and reseed profile
        useProfileStore.getState().updateProfile(author, { postIndexMagnetUri: result.magnetURI })
        try { 
          const { getCore } = await import('@/lib/core')
          getCore().then((c: any) => c.seedCurrentProfile()) 
        } catch { }
      }
    } catch (e) {
      console.warn('Failed to regenerate post index', e)
    }
  },
  deletePost: async (postId) => {
    const state = usePostStore.getState()
    const target = state.posts.find(p => p.id === postId)
    if (!target) return
    usePostStore.getState().removePost(postId)
    await usePostStore.getState().regenerateAuthorIndex(target.author)
  }
}))
