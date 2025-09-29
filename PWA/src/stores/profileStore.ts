import { create } from 'zustand'
import { SnartStorage } from '../lib/SnartStorage'

const storage = new SnartStorage();

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
  postIndexMagnetUri?: string; // Added for post index support
}

interface ProfileState {
  currentProfile: Profile | null;
  profiles: Map<string, Profile>;
  loading: boolean;
  error: string | null;
  seedProfileEnabled: boolean;

  // Actions
  setCurrentProfile: (profile: Profile | null) => void;
  addProfile: (profile: Profile) => void;
  updateProfile: (username: string, updates: Partial<Profile>) => void;
  updateProfilePicture: (username: string, profilePicture: string, thumbnail?: string) => void;
  setSeedProfileEnabled: (enabled: boolean) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  clearError: () => void;
  saveProfile: (profile: Profile) => Promise<void>;
  loadProfiles: () => Promise<void>;
}


const PROFILE_DIR = '/profiles';
const SEED_PREF_PATH = '/settings/profile-seed-enabled.json';

export const useProfileStore = create<ProfileState>((set, get) => ({
  currentProfile: null,
  profiles: new Map(),
  loading: false,
  error: null,
  seedProfileEnabled: true,

  // Load all profiles from Filer FS
  async loadProfiles() {
    set({ loading: true });
    try {
      const files = await storage.listFiles(PROFILE_DIR);
      const profiles = new Map();
      for (const file of files) {
        const data = await storage.readFile(`${PROFILE_DIR}/${file}`);
        const profile = JSON.parse(data);
        profiles.set(profile.username, profile);
      }
      set({ profiles, loading: false });
    } catch (e) {
      set({ error: (e as Error).message, loading: false });
    }
  },

  // Save a profile to Filer FS
  async saveProfile(profile: Profile) {
    await storage.writeFile(`${PROFILE_DIR}/${profile.username}.json`, JSON.stringify(profile));
  },

  setCurrentProfile: (profile) => set({ currentProfile: profile }),

  addProfile: async (profile) => {
    await get().saveProfile(profile);
    get().loadProfiles();
  },

  updateProfile: async (username, updates) => {
    const state = get();
    const existing = state.profiles.get(username);
    if (existing) {
      const updated = { ...existing, ...updates };
      await get().saveProfile(updated);
      get().loadProfiles();
      if (state.currentProfile?.username === username) {
        set({ currentProfile: updated });
      }
    }
  },

  updateProfilePicture: async (username, profilePicture, thumbnail) => {
    await get().updateProfile(username, { profilePicture, profilePictureThumbnail: thumbnail });
  },

  setSeedProfileEnabled: async (enabled) => {
    await storage.writeFile(SEED_PREF_PATH, JSON.stringify({ enabled }));
    set({ seedProfileEnabled: enabled });
  },

  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
}));
// (Legacy/duplicate logic removed. All persistence now uses SnartStorage async helpers above.)

export type { Profile, ProfilePost }