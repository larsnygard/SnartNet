import { useState, useEffect } from 'react'

/**
 * Core bindings interface for SnartNet Rust WASM module
 * 
 * This module will eventually import and interact with the compiled WASM
 * module from the Rust core library. For now, it provides mock implementations
 * to allow UI development to proceed.
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
 * Core interface that will be implemented by the Rust WASM module
 */
export interface SnartNetCore {
  // Initialization
  init(): Promise<void>
  
  // Profile management
  createProfile(username: string, displayName?: string, bio?: string): Promise<string>
  loadProfile(magnetUri: string): Promise<any>
  updateProfile(updates: any): Promise<void>
  publishProfile(): Promise<string>
  
  // Key management
  generateKeys(): Promise<{ publicKey: string; fingerprint: string }>
  exportPublicKey(fingerprint: string): Promise<string>
  signData(data: string): Promise<string>
  verifySignature(data: string, signature: string, publicKey: string): Promise<boolean>
  
  // Posts
  createPost(content: string, tags?: string[]): Promise<string>
  getTimeline(): Promise<any[]>
  
  // Messaging (placeholder)
  sendMessage(recipientId: string, content: string): Promise<void>
  getMessages(contactId: string): Promise<any[]>
  
  // Event subscription
  subscribeToEvents(callback: EventCallback): () => void
  
  // Storage
  setItem(key: string, value: any): Promise<void>
  getItem(key: string): Promise<any>
}

/**
 * Mock implementation for development
 * This will be replaced with actual WASM bindings when the Rust core is ready
 */
class MockCore implements SnartNetCore {
  private eventCallbacks: Set<EventCallback> = new Set()
  private storage: Map<string, any> = new Map()

  async init(): Promise<void> {
    console.log('[MockCore] Initialized')
    // Simulate some async initialization
    await new Promise(resolve => setTimeout(resolve, 100))
  }

  async createProfile(username: string, displayName?: string, bio?: string): Promise<string> {
    const profile = {
      username,
      displayName: displayName || username,
      bio: bio || '',
      publicKey: 'mock_public_key_' + Date.now(),
      fingerprint: 'mock_fingerprint_' + Date.now(),
      magnetUri: 'magnet:?xt=urn:mock:' + username + '_' + Date.now()
    }
    
    this.storage.set('currentProfile', profile)
    this.emitEvent({ type: 'ProfileLoaded', profile })
    
    return profile.magnetUri
  }

  async loadProfile(magnetUri: string): Promise<any> {
    // Simulate loading from network
    await new Promise(resolve => setTimeout(resolve, 500))
    
    const mockProfile = {
      username: 'user_' + Date.now(),
      displayName: 'Mock User',
      bio: 'This is a mock profile for development',
      publicKey: 'mock_public_key',
      magnetUri
    }
    
    this.emitEvent({ type: 'ProfileLoaded', profile: mockProfile })
    return mockProfile
  }

  async updateProfile(updates: any): Promise<void> {
    const current = this.storage.get('currentProfile')
    if (current) {
      const updated = { ...current, ...updates }
      this.storage.set('currentProfile', updated)
      this.emitEvent({ type: 'ProfileUpdated', profile: updated })
    }
  }

  async publishProfile(): Promise<string> {
    const profile = this.storage.get('currentProfile')
    if (profile) {
      console.log('[MockCore] Publishing profile:', profile.username)
      return profile.magnetUri
    }
    throw new Error('No current profile to publish')
  }

  async generateKeys(): Promise<{ publicKey: string; fingerprint: string }> {
    return {
      publicKey: 'ed25519_public_key_' + Date.now(),
      fingerprint: 'fp_' + Date.now().toString(16)
    }
  }

  async exportPublicKey(fingerprint: string): Promise<string> {
    return `-----BEGIN PUBLIC KEY-----\nmock_key_for_${fingerprint}\n-----END PUBLIC KEY-----`
  }

  async signData(data: string): Promise<string> {
    return 'mock_signature_' + btoa(data).substring(0, 16)
  }

  async verifySignature(_data: string, signature: string, _publicKey: string): Promise<boolean> {
    // Mock verification - in real implementation this would use Ed25519
    return signature.startsWith('mock_signature_')
  }

  async createPost(content: string, tags?: string[]): Promise<string> {
    const post = {
      id: 'post_' + Date.now(),
      content,
      tags: tags || [],
      timestamp: new Date().toISOString(),
      author: this.storage.get('currentProfile')?.username || 'unknown'
    }
    
    this.emitEvent({ type: 'PostAdded', post })
    return post.id
  }

  async getTimeline(): Promise<any[]> {
    // Return mock timeline data
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
        content: 'Just set up my profile on this new decentralized platform. Loving the crypto-powered security!',
        author: 'bob',
        timestamp: new Date(Date.now() - 1800000).toISOString(),
        tags: ['crypto', 'security']
      }
    ]
  }

  async sendMessage(recipientId: string, content: string): Promise<void> {
    const message = {
      id: 'msg_' + Date.now(),
      recipientId,
      content,
      timestamp: new Date().toISOString(),
      sender: this.storage.get('currentProfile')?.username || 'unknown'
    }
    
    console.log('[MockCore] Sending message:', message)
    this.emitEvent({ type: 'MessageReceived', message })
  }

  async getMessages(contactId: string): Promise<any[]> {
    return [
      {
        id: 'msg_1',
        sender: contactId,
        content: 'Hello! How are you enjoying SnartNet?',
        timestamp: new Date(Date.now() - 1800000).toISOString(),
        encrypted: true
      }
    ]
  }

  subscribeToEvents(callback: EventCallback): () => void {
    this.eventCallbacks.add(callback)
    return () => this.eventCallbacks.delete(callback)
  }

  async setItem(key: string, value: any): Promise<void> {
    this.storage.set(key, value)
  }

  async getItem(key: string): Promise<any> {
    return this.storage.get(key)
  }

  private emitEvent(event: CoreEvent): void {
    this.eventCallbacks.forEach(callback => {
      try {
        callback(event)
      } catch (error) {
        console.error('[MockCore] Error in event callback:', error)
      }
    })
  }
}

// Global core instance
let coreInstance: SnartNetCore | null = null

/**
 * Initialize and get the core instance
 * In the future, this will load and instantiate the WASM module
 */
export async function getCore(): Promise<SnartNetCore> {
  if (!coreInstance) {
    // TODO: Replace with actual WASM loading
    // const wasmModule = await import('./snartnet_core.wasm')
    // coreInstance = new WasmCore(wasmModule)
    
    coreInstance = new MockCore()
    await coreInstance.init()
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