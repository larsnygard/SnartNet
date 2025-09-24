import { useState, useEffect } from 'react'
import initWasm, { SnartNetCore as WasmCore, init_core } from '../wasm/snartnet_core'
import { getTorrentService } from './torrent'

/**
 * Core bindings for SnartNet Rust WASM module with WebTorrent integration
 */

// Event types that the core can emit
export type CoreEvent = 
  | { type: 'ProfileLoaded'; profile: any }
  | { type: 'ProfileUpdated'; profile: any }
  | { type: 'PostAdded'; post: any }
  | { type: 'MessageReceived'; message: any }
  | { type: 'SyncStateChanged'; state: any }
  | { type: 'AttachmentProgress'; progress: any }

export type EventCallback = (event: CoreEvent) => void

/**
 * TypeScript wrapper for the WASM core
 */
class SnartNetCore {
  private wasmCore: WasmCore
  private eventCallbacks: Set<EventCallback> = new Set()

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

  // Profile management
  async createProfile(username: string, displayName?: string, bio?: string): Promise<string> {
    try {
      const magnetUri = this.wasmCore.create_profile(username, displayName || null, bio || null)
      console.log('[SnartNetCore] Profile created:', { username, magnetUri })
      
      // Emit event
      const profile = this.wasmCore.get_current_profile()
      if (profile) {
        this.emitEvent({ type: 'ProfileLoaded', profile })
      }
      
      return magnetUri
    } catch (error) {
      console.error('[SnartNetCore] Create profile error:', error)
      throw error
    }
  }

  async getCurrentProfile(): Promise<any> {
    try {
      return this.wasmCore.get_current_profile()
    } catch (error) {
      console.error('[SnartNetCore] Get profile error:', error)
      return null
    }
  }

  async updateProfile(displayName?: string, bio?: string): Promise<void> {
    try {
      await this.wasmCore.update_current_profile(displayName || null, bio || null)
      
      // Emit event
      const profile = this.wasmCore.get_current_profile()
      if (profile) {
        this.emitEvent({ type: 'ProfileUpdated', profile })
      }
    } catch (error) {
      console.error('[SnartNetCore] Update profile error:', error)
      throw error
    }
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

      const torrentService = getTorrentService()
      const magnetURI = await torrentService.seedProfile(profile)
      
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
  async getTimeline(): Promise<any[]> {
    // Return mock timeline data for now
    return [
      {
        id: 'post_1',
        content: 'Welcome to SnartNet! This is a decentralized social media platform.',
        author: 'alice',
        timestamp: new Date(Date.now() - 3600000).toISOString(),
        tags: ['introduction', 'snartnet']
      },
      {
        id: 'post_2', 
        content: 'Just created my first cryptographically signed post! 🔐',
        author: 'bob',
        timestamp: new Date(Date.now() - 1800000).toISOString(),
        tags: ['crypto', 'security']
      }
    ]
  }
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