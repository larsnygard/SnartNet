import { create } from 'zustand'

interface ProfilePost {
  id: string;
  content: string;
  createdAt: string;
}

interface Profile {
  username: string;
  publicKey?: string;
  displayName?: string;
  bio?: string;
  avatarHash?: string;
  avatar?: string; // URL or base64 data URL for profile picture
  profilePicture?: string; // Base64 encoded profile picture
  profilePictureThumbnail?: string; // Base64 encoded thumbnail
  location?: string;
  website?: string;
  magnetUri?: string;
  createdAt?: string;
  updatedAt?: string;
  posts?: ProfilePost[]; // Add posts array
}

interface ProfileState {
  currentProfile: Profile | null;
  profiles: Map<string, Profile>;
  loading: boolean;
  error: string | null;
  
  // Actions
  setCurrentProfile: (profile: Profile | null) => void;
  addProfile: (profile: Profile) => void;
  updateProfile: (username: string, updates: Partial<Profile>) => void;
  updateProfilePicture: (username: string, profilePicture: string, thumbnail?: string) => void;
  addProfilePost: (username: string, content: string) => void;
  removeProfilePost: (username: string, postId: string) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  clearError: () => void;
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
    
    // Also update current profile if it matches
    let newCurrentProfile = state.currentProfile
    if (state.currentProfile?.username === username) {
      newCurrentProfile = { ...state.currentProfile, ...updates }
    }
    
    return { 
      profiles: newProfiles,
      currentProfile: newCurrentProfile
    }
  }),
  
  updateProfilePicture: (username, profilePicture, thumbnail) => set((state) => {
    const updates = { 
      profilePicture,
      profilePictureThumbnail: thumbnail || profilePicture,
      updatedAt: new Date().toISOString()
    }
    
    const newProfiles = new Map(state.profiles)
    const existing = newProfiles.get(username)
    if (existing) {
      newProfiles.set(username, { ...existing, ...updates })
    }
    
    // Also update current profile if it matches
    let newCurrentProfile = state.currentProfile
    if (state.currentProfile?.username === username) {
      newCurrentProfile = { ...state.currentProfile, ...updates }
      
      // Store profile picture in localStorage for persistence
      localStorage.setItem(`profile-picture-${username}`, profilePicture)
      if (thumbnail) {
        localStorage.setItem(`profile-picture-thumb-${username}`, thumbnail)
      }
    }
    
    return { 
      profiles: newProfiles,
      currentProfile: newCurrentProfile
    }
  }),
  
  addProfilePost: (username, content) => set((state) => {
    const newProfiles = new Map(state.profiles);
    const existing = newProfiles.get(username);
    const newPost = {
      id: Math.random().toString(36).slice(2),
      content,
      createdAt: new Date().toISOString(),
    };
    if (existing) {
      const posts = existing.posts ? [newPost, ...existing.posts] : [newPost];
      newProfiles.set(username, { ...existing, posts });
    }
    let newCurrentProfile = state.currentProfile;
    if (state.currentProfile?.username === username) {
      const posts = state.currentProfile.posts ? [newPost, ...state.currentProfile.posts] : [newPost];
      newCurrentProfile = { ...state.currentProfile, posts };
    }
    return {
      profiles: newProfiles,
      currentProfile: newCurrentProfile,
    };
  }),
  
  removeProfilePost: (username, postId) => set((state) => {
    const newProfiles = new Map(state.profiles);
    const existing = newProfiles.get(username);
    if (existing && existing.posts) {
      const posts = existing.posts.filter((p) => p.id !== postId);
      newProfiles.set(username, { ...existing, posts });
    }
    let newCurrentProfile = state.currentProfile;
    if (state.currentProfile?.username === username && state.currentProfile.posts) {
      const posts = state.currentProfile.posts.filter((p) => p.id !== postId);
      newCurrentProfile = { ...state.currentProfile, posts };
    }
    return {
      profiles: newProfiles,
      currentProfile: newCurrentProfile,
    };
  }),
  
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
}))

export type { Profile, ProfilePost }