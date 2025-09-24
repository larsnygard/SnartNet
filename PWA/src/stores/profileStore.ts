import { create } from 'zustand'

interface Profile {
  username: string
  publicKey?: string
  displayName?: string
  bio?: string
  avatarHash?: string
  avatar?: string
  location?: string
  website?: string
  magnetUri?: string
  createdAt?: string
  updatedAt?: string
}

interface ProfileState {
  currentProfile: Profile | null
  profiles: Map<string, Profile>
  loading: boolean
  error: string | null
  
  // Actions
  setCurrentProfile: (profile: Profile | null) => void
  addProfile: (profile: Profile) => void
  updateProfile: (username: string, updates: Partial<Profile>) => void
  setLoading: (loading: boolean) => void
  setError: (error: string | null) => void
  clearError: () => void
}

export const useProfileStore = create<ProfileState>((set) => ({
  currentProfile: null,
  profiles: new Map(),
  loading: false,
  error: null,

  setCurrentProfile: (profile) => set({ currentProfile: profile }),
  
  addProfile: (profile) => set((state) => {
    const newProfiles = new Map(state.profiles)
    newProfiles.set(profile.username, profile)
    return { profiles: newProfiles }
  }),
  
  updateProfile: (username, updates) => set((state) => {
    const newProfiles = new Map(state.profiles)
    const existing = newProfiles.get(username)
    if (existing) {
      newProfiles.set(username, { ...existing, ...updates })
    }
    return { profiles: newProfiles }
  }),
  
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
}))

export type { Profile }