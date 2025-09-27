import { useState, useEffect } from 'react'
//import initWasm, { SnartNetCore as WasmCore, init_core } from '../wasm/snartnet_core'
import initWasm, { SnartNetCore as WasmCore, init_core } from 'PWA/wasm/snartnet_core.js'
import { getTorrentService } from './torrent'
import profileSchema from '../schemas/profile.v1.json'
import postSchema from '../schemas/post.v1.json'

// Normalized types
export interface NormalizedProfile {
  id: string
  username: string
  displayName?: string
  bio?: string
  avatarHash?: string
  publicKey: string
  fingerprint: string
  createdAt: string
  updatedAt: string
  version: number
  magnetUri?: string
  schemaVersion: 1
}

function normalizeProfile(raw: any): NormalizedProfile | null {
  if (!raw) return null
  return {
    id: raw.id,
    username: raw.username,
    displayName: raw.display_name || raw.displayName || undefined,
    bio: raw.bio || undefined,
    avatarHash: raw.avatar_hash || raw.avatarHash || undefined,
    publicKey: raw.public_key || raw.publicKey,
    fingerprint: raw.fingerprint,
    createdAt: typeof raw.created_at === 'string' ? raw.created_at : raw.createdAt,
    updatedAt: typeof raw.updated_at === 'string' ? raw.updated_at : raw.updatedAt,
    version: raw.version || 1,
    magnetUri: raw.magnet_uri || raw.magnetUri || undefined,
    schemaVersion: 1,
  }
}

// Event types that the core can emit
export type CoreEvent = 
  | { type: 'ProfileLoaded'; profile: NormalizedProfile }
  | { type: 'ProfileUpdated'; profile: NormalizedProfile }
  | { type: 'PostAdded'; post: any }
  | { type: 'MessageReceived'; message: any }
  | { type: 'SyncStateChanged'; state: any }
  | { type: 'AttachmentProgress'; progress: any }

export type EventCallback = (event: CoreEvent) => void

export interface CoreCapabilities {
  profileJsonApi: boolean
  postJsonApi: boolean
  messageJsonApi: boolean
  version: string
}

/**
 * TypeScript wrapper for the WASM core
 */
class SnartNetCore {
  private wasmCore: WasmCore
  private eventCallbacks: Set<EventCallback> = new Set()
  private capabilities: CoreCapabilities | null = null

  constructor(wasmCore: WasmCore) {
    this.wasmCore = wasmCore
  }

  // Initialize the core
  async init(): Promise<void> {
    try {
      await this.wasmCore.init()
      console.log('[SnartNetCore] WASM core initialized')
    } catch (error) {
      console.error('[SnartNetCore] Init error:', error)
      throw error
    }
  }

  async getCapabilities(): Promise<CoreCapabilities> {
    if (this.capabilities) return this.capabilities
    // Try calling a wasm capability probe if it exists; fall back to defaults.
    let cap: CoreCapabilities = {
      profileJsonApi: false,
      postJsonApi: false,
      messageJsonApi: false,
      version: '0'
    }
    // @ts-ignore dynamic probing
    const probe = (this.wasmCore as any).get_capabilities
    if (typeof probe === 'function') {
      try {
        const raw = probe.call(this.wasmCore)
        if (raw) {
          // Accept either JSON string or object
            let obj = raw
            if (typeof raw === 'string') {
              try { obj = JSON.parse(raw) } catch {}
            }
            cap = {
              profileJsonApi: !!(obj.profileJsonApi || obj.profile_json_api),
              postJsonApi: !!(obj.postJsonApi || obj.post_json_api),
              messageJsonApi: !!(obj.messageJsonApi || obj.message_json_api),
              version: obj.version ? String(obj.version) : '0'
            }
        }
      } catch (e) {
        console.warn('[Core] capability probe failed, using defaults', e)
      }
    }
    this.capabilities = cap
    return cap
  }

  // Profile management
  async createProfile(username: string, displayName?: string, bio?: string): Promise<string> {
    const caps = await this.getCapabilities()
    if (caps.profileJsonApi && (this.wasmCore as any).create_profile_json) {
      const payload = JSON.stringify({ username, displayName: displayName || null, bio: bio || null })
      const env = (this.wasmCore as any).create_profile_json(payload)
      const obj = typeof env === 'string' ? JSON.parse(env) : env
      const profile = obj.profile || obj.profileJson || obj
      const normalized = normalizeProfile(profile)
      if (normalized) this.emitEvent({ type: 'ProfileLoaded', profile: normalized })
      return obj.magnetUri || profile.magnetUri || ''
    }
    try {
      // Pass undefined instead of null for Option<String> to satisfy wasm-bindgen expectations
      const magnetUri = (this.wasmCore as any).create_profile(
        username,
        displayName ? displayName : undefined,
        bio ? bio : undefined
      )
      const raw = (this.wasmCore as any).get_current_profile()
      const normalized = normalizeProfile(raw)
      if (normalized) this.emitEvent({ type: 'ProfileLoaded', profile: normalized })
      return magnetUri
    } catch (e: any) {
      const msg = String(e?.message || e)
      console.error('[Core] legacy create_profile failed:', e)
      // Attempt fallback to JSON API directly if capability probe failed earlier
      const jsonApi = (this.wasmCore as any).create_profile_json
      if (jsonApi) {
        try {
          const payload = JSON.stringify({ username, displayName: displayName || null, bio: bio || null })
          const env = jsonApi(payload)
          const obj = typeof env === 'string' ? JSON.parse(env) : env
          const profile = obj.profile || obj.profileJson || obj
          const normalized = normalizeProfile(profile)
          if (normalized) this.emitEvent({ type: 'ProfileLoaded', profile: normalized })
          return obj.magnetUri || profile.magnetUri || ''
        } catch (e2) {
          console.error('[Core] JSON fallback also failed:', e2)
        }
      }
      if (/memory access out of bounds/i.test(msg)) {
        // Possible corrupted persisted data; advise reset
        console.warn('[Core] Detected possible corrupted WASM state; clearing persisted key/profile entries')
        try {
          localStorage.removeItem('snartnet_keypair')
          localStorage.removeItem('snartnet_current_profile')
        } catch {}
      }
      throw e
    }
  }

  async getCurrentProfile(): Promise<NormalizedProfile | null> {
    const caps = await this.getCapabilities()
    if (caps.profileJsonApi && (this.wasmCore as any).get_current_profile_json) {
      const env = (this.wasmCore as any).get_current_profile_json()
      if (!env) return null
      const obj = typeof env === 'string' ? JSON.parse(env) : env
      const profile = obj.profile || obj.profileJson || obj
      return normalizeProfile(profile)
    }
    const raw = this.wasmCore.get_current_profile()
    return normalizeProfile(raw)
  }

  async updateProfile(displayName?: string, bio?: string): Promise<void> {
    const caps = await this.getCapabilities()
    if (caps.profileJsonApi && (this.wasmCore as any).update_profile_json) {
      const payload = JSON.stringify({ displayName: displayName || null, bio: bio || null })
      const env = (this.wasmCore as any).update_profile_json(payload)
      const obj = typeof env === 'string' ? JSON.parse(env) : env
      const profile = obj.profile || obj.profileJson || obj
      const normalized = normalizeProfile(profile)
      if (normalized) this.emitEvent({ type: 'ProfileUpdated', profile: normalized })
      return
    }
    await (this.wasmCore as any).update_current_profile(
      displayName ? displayName : undefined,
      bio ? bio : undefined
    )
    const raw = this.wasmCore.get_current_profile()
    const normalized = normalizeProfile(raw)
    if (normalized) this.emitEvent({ type: 'ProfileUpdated', profile: normalized })
  }

  // Key management
  async getPublicKey(): Promise<string> {
    try {
      return this.wasmCore.get_public_key()
    } catch (error) {
      console.error('[SnartNetCore] Get public key error:', error)
      throw error
    }
  }

  async getFingerprint(): Promise<string> {
    try {
      return this.wasmCore.get_fingerprint()
    } catch (error) {
      console.error('[SnartNetCore] Get fingerprint error:', error)
      throw error
    }
  }

  // Posts
  async createPost(content: string, tags?: string[], replyTo?: string): Promise<any> {
    try {
      const signedPost = this.wasmCore.create_post(content, tags || null, replyTo || null)
      console.log('[SnartNetCore] Post created:', signedPost)
      
      this.emitEvent({ type: 'PostAdded', post: signedPost })
      return signedPost
    } catch (error) {
      console.error('[SnartNetCore] Create post error:', error)
      throw error
    }
  }

  // Messages
  async createMessage(recipientFingerprint: string, content: string): Promise<any> {
    try {
      const signedMessage = this.wasmCore.create_message(recipientFingerprint, content)
      console.log('[SnartNetCore] Message created:', signedMessage)
      
      this.emitEvent({ type: 'MessageReceived', message: signedMessage })
      return signedMessage
    } catch (error) {
      console.error('[SnartNetCore] Create message error:', error)
      throw error
    }
  }

  // Status
  hasProfile(): boolean {
    return this.wasmCore.has_profile()
  }

  // Events
  subscribeToEvents(callback: EventCallback): () => void {
    this.eventCallbacks.add(callback)
    return () => this.eventCallbacks.delete(callback)
  }

  private emitEvent(event: CoreEvent): void {
    this.eventCallbacks.forEach(callback => {
      try {
        callback(event)
      } catch (error) {
        console.error('[SnartNetCore] Error in event callback:', error)
      }
    })
  }

  // Torrent functionality
  async seedCurrentProfile(): Promise<string> {
    try {
      const profile = this.wasmCore.get_current_profile()
      if (!profile) {
        throw new Error('No current profile to seed')
      }

      console.log('[SnartNetCore] Profile data type:', typeof profile)
      console.log('[SnartNetCore] Profile data:', profile)
      
      // Ensure profile is properly serializable by converting to plain object
      let cleanProfile: any
      if (typeof profile === 'object' && profile !== null) {
        // Convert to plain JavaScript object to ensure proper serialization
        cleanProfile = JSON.parse(JSON.stringify(profile))
      } else {
        cleanProfile = profile
      }

      console.log('[SnartNetCore] Clean profile:', cleanProfile)

      const torrentService = getTorrentService()

      // Merge in extended / local enhancements (profilePicture etc.) if available
      try {
        const username = cleanProfile.username || cleanProfile.id
        if (username) {
          const pic = localStorage.getItem(`profile-picture-${username}`)
          const thumb = localStorage.getItem(`profile-picture-thumb-${username}`)
          if (pic) cleanProfile.profilePicture = pic
          if (thumb) cleanProfile.profilePictureThumbnail = thumb
          // Extended profile fields (location, website, avatar URL) persisted separately
          const extendedRaw = localStorage.getItem(`profile-extended-${username}`)
          if (extendedRaw) {
            try {
              const extended = JSON.parse(extendedRaw)
              if (extended && typeof extended === 'object') {
                if (extended.location) cleanProfile.location = extended.location
                if (extended.website) cleanProfile.website = extended.website
                if (extended.avatar) cleanProfile.avatar = extended.avatar
              }
            } catch { /* ignore parse errors */ }
          }
        }
      } catch (e) {
        console.warn('[SnartNetCore] Failed merging extended profile data', e)
      }

      // Ensure a post index magnet exists (even if empty) so consumers can always sync
      if (!cleanProfile.postIndexMagnetUri && !cleanProfile.post_index_magnet) {
        try {
          const emptyIndex = await torrentService.seedPostIndex([], cleanProfile.username || cleanProfile.id || 'unknown', {})
          cleanProfile.postIndexMagnetUri = emptyIndex.magnetURI
        } catch (e) {
          console.warn('[SnartNetCore] Failed to seed empty post index', e)
        }
      }

      const magnetURI = await torrentService.seedProfile(cleanProfile)
      
      console.log('[SnartNetCore] Profile seeding started:', magnetURI)
      return magnetURI
    } catch (error) {
      console.error('[SnartNetCore] Failed to seed profile:', error)
      throw error
    }
  }

  async downloadProfileFromMagnet(magnetURI: string): Promise<any> {
    try {
      const torrentService = getTorrentService()
      const profile = await torrentService.downloadProfile(magnetURI)
      
      console.log('[SnartNetCore] Profile downloaded:', profile)
      return profile
    } catch (error) {
      console.error('[SnartNetCore] Failed to download profile:', error)
      throw error
    }
  }

  getTorrentStats() {
    const torrentService = getTorrentService()
    return torrentService.getStats()
  }

  getActiveTorrents() {
    const torrentService = getTorrentService()
    return torrentService.getActiveTorrents()
  }

  async stopSeeding(infoHash: string): Promise<void> {
    const torrentService = getTorrentService()
    torrentService.stopSeeding(infoHash)
  }

  // Mock methods for timeline (will be replaced with real P2P later)
  // (Removed legacy mock getTimeline; real data flows through torrent + stores)

  validateProfile(obj: any): boolean {
    // minimal structural checks using schema constants (not full JSON Schema validation runtime yet)
    if (!obj || typeof obj !== 'object') return false
    for (const key of ['id','username','publicKey','fingerprint','createdAt','updatedAt']) {
      if (!(key in obj)) return false
    }
    return true
  }
  // placeholder for future post validation
  validatePost(obj: any): boolean { return !!obj && typeof obj === 'object' && 'id' in obj && 'author' in obj }
}

// Global core instance
let coreInstance: SnartNetCore | null = null

/**
 * Initialize and get the core instance
 */
export async function getCore(): Promise<SnartNetCore> {
  if (!coreInstance) {
    try {
      // Initialize WASM
      await initWasm()
      await init_core()
      
      // Create WASM core instance
      const wasmCore = new WasmCore()
      coreInstance = new SnartNetCore(wasmCore)
      await coreInstance.init()
      
      console.log('[Core] Initialized successfully')
    } catch (error) {
      console.error('[Core] Initialization failed:', error)
      throw error
    }
  }
  return coreInstance
}

/**
 * Hook for using the core in React components
 */
export function useCore() {
  const [core, setCore] = useState<SnartNetCore | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    getCore().then(instance => {
      setCore(instance)
      setLoading(false)
    }).catch(error => {
      console.error('Failed to initialize core:', error)
      setLoading(false)
    })
  }, [])

  return { core, loading }
}

export { profileSchema, postSchema }
