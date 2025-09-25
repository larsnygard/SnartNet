import { create } from 'zustand'
import { getTorrentService } from '@/lib/torrent'

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
}

export const usePostStore = create<PostState>((set) => ({
  posts: [],
  loading: false,
  error: null,

  addPost: async (postData) => {
    const base = {
      ...postData,
      id: Math.random().toString(36).slice(2),
      createdAt: new Date().toISOString(),
      isSeeding: true,
      seedProgress: 0,
      version: 1,
      schemaVersion: 1 as const,
    }
    const newPost: TorrentPost = normalizePost(base) as any
    
    set((state) => ({
      posts: [newPost, ...state.posts].sort((a, b) => 
        new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      )
    }))

    try {
      const torrentService = getTorrentService()
      const magnetUri = await torrentService.seedPost(postData)
      
      set((state) => ({
        posts: state.posts.map(p => 
          p.id === newPost.id ? { ...p, magnetUri, seedProgress: 100, isSeeding: false } : p
        )
      }))
    } catch (error) {
      console.error('Failed to seed post:', error)
      set((state) => ({
        posts: state.posts.map(p => 
          p.id === newPost.id ? { ...p, isSeeding: false, seedProgress: 0, seedError: (error instanceof Error ? error.message : 'Failed to seed') } : p
        )
      }))
    }
  },

  removePost: (postId) => {
    set((state) => ({
      posts: state.posts.filter(post => post.id !== postId)
    }))
  },

  updatePost: (postId, updates) => {
    set((state) => ({
      posts: state.posts.map(post => 
        post.id === postId ? { ...post, ...updates } : post
      )
    }))
  },

  updateSeedingStatus: (postId, isSeeding, progress) => {
    set((state) => ({
      posts: state.posts.map(post => 
        post.id === postId ? { ...post, isSeeding, seedProgress: progress } : post
      )
    }))
  },

  loadPostsFromContacts: async () => {
    // TODO: Implement loading posts from contacts via P2P
    // This would fetch posts from magnet URIs of contacts
    set({ loading: true })
    
    try {
      // Mock implementation - replace with actual P2P fetching
      await new Promise(resolve => setTimeout(resolve, 1000))
      
      set({ loading: false })
    } catch (error) {
      set({ 
        loading: false, 
        error: error instanceof Error ? error.message : 'Failed to load posts'
      })
    }
  },

  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
}))