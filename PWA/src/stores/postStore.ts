import { create } from 'zustand'

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
}

interface PostState {
  posts: TorrentPost[]
  loading: boolean
  error: string | null
  
  // Actions
  addPost: (post: Omit<TorrentPost, 'id' | 'createdAt'>) => void
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

  addPost: (postData) => {
    const newPost: TorrentPost = {
      ...postData,
      id: Math.random().toString(36).slice(2),
      createdAt: new Date().toISOString(),
    }
    
    set((state) => ({
      posts: [newPost, ...state.posts].sort((a, b) => 
        new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      )
    }))
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