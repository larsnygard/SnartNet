import type { Profile } from '@/stores/profileStore'
import type { TorrentPost } from '@/stores/postStore'

// Simple torrent service that works in both browser and build
class TorrentService {
  private client: any = null
  private activeTorrents: Map<string, any> = new Map()
  private eventCallbacks: Array<(event: any) => void> = []
  private readyPromise: Promise<void>
  private resolveReady!: () => void
  private clientReady = false
  private pendingSeeds: Array<() => void> = []
  private stats = {
    torrents: 0,
    downloadSpeed: 0,
    uploadSpeed: 0,
    downloaded: 0,
    uploaded: 0,
    peers: 0
  }

  constructor() {
    this.readyPromise = new Promise(resolve => {
      this.resolveReady = resolve
    })
    this.initializeClient()
  }

  private async initializeClient() {
    if (typeof window === 'undefined') return
    let attempts = 0
    const maxAttempts = 10

    const tryInit = () => {
      attempts++
      const WebTorrentConstructor = (window as any).WebTorrent
      if (!WebTorrentConstructor || typeof WebTorrentConstructor !== 'function') {
        if (attempts < maxAttempts) {
          console.warn(`[TorrentService] WebTorrent not available yet (attempt ${attempts}), retrying...`)
          setTimeout(tryInit, 500)
          return
        } else {
          const err = new Error('WebTorrent not available after retries')
          console.error('[TorrentService] ', err)
          this.emitEvent({ type: 'error', error: err.message })
          return
        }
      }

      try {
        this.client = new WebTorrentConstructor()
        this.clientReady = true
        this.client.on('error', (err: any) => {
          console.error('[TorrentService] Client error:', err)
          this.emitEvent({ type: 'error', error: err.message })
        })
        this.client.on('torrent', (torrent: any) => {
          console.log('[TorrentService] Torrent added:', { name: torrent.name, infoHash: torrent.infoHash })
          this.activeTorrents.set(torrent.infoHash, torrent)
          this.emitEvent({ type: 'torrent-added', torrent })
          this.updateStats()
        })
        console.log('[TorrentService] WebTorrent client initialized successfully')
        this.resolveReady()
        // Flush pending seeds
        const queue = [...this.pendingSeeds]
        this.pendingSeeds = []
        queue.forEach(fn => {
          try { fn() } catch (e) { console.error('[TorrentService] Pending seed failed:', e) }
        })
      } catch (error) {
        console.error('[TorrentService] Failed to initialize:', error)
        this.emitEvent({ type: 'error', error: String(error) })
      }
    }

    tryInit()
  }

  private emitEvent(event: any) {
    this.eventCallbacks.forEach(callback => {
      try {
        callback(event)
      } catch (err) {
        console.error('Error in event callback:', err)
      }
    })
  }

  private updateStats() {
    if (!this.client) return
    
    try {
      this.stats = {
        torrents: this.client.torrents.length,
        downloadSpeed: this.client.downloadSpeed || 0,
        uploadSpeed: this.client.uploadSpeed || 0,
        downloaded: this.client.downloaded || 0,
        uploaded: this.client.uploaded || 0,
        peers: this.client.torrents.reduce((total: number, torrent: any) => {
          return total + (torrent.numPeers || 0)
        }, 0)
      }
    } catch (error) {
      console.error('Error updating stats:', error)
    }
  }

  async seedProfile(profile: Profile): Promise<string> {
    await this.readyPromise
    if (!this.client) {
      throw new Error('WebTorrent client not initialized. Are you in a browser environment?')
    }

    try {
      const profileData = JSON.stringify(profile, null, 2)
      const fileName = `${profile.username}_profile.json`
      
      // Ensure UTF-8 encoding by converting to Uint8Array first
      const encoder = new TextEncoder()
      const encodedData = encoder.encode(profileData)
      const file = new File([encodedData], fileName, { 
        type: 'application/json; charset=utf-8' 
      })

      return new Promise((resolve, reject) => {
        const timeoutId = setTimeout(() => {
          reject(new Error('Seeding timeout'))
        }, 10000)

        try {
          console.log('Seeding profile file:', {
            fileName,
            fileSize: file.size,
            fileType: file.type,
            profileDataLength: profileData.length
          })
          
          const torrent = this.client.seed([file], (torrent: any) => {
            clearTimeout(timeoutId)
            console.log('Profile seeded successfully:', torrent.magnetURI)
            this.emitEvent({ 
              type: 'seeding-started', 
              profile, 
              magnetURI: torrent.magnetURI 
            })
            resolve(torrent.magnetURI)
          })

          torrent.on('error', (err: any) => {
            clearTimeout(timeoutId)
            this.emitEvent({ type: 'error', error: err.message })
            reject(err)
          })

          torrent.on('wire', (wire: any) => {
            this.emitEvent({ 
              type: 'peer-connected', 
              peerId: wire.peerId || 'unknown' 
            })
          })

          torrent.on('upload', () => {
            this.emitEvent({ 
              type: 'upload-progress', 
              uploadSpeed: torrent.uploadSpeed || 0 
            })
            this.updateStats()
          })
        } catch (error) {
          clearTimeout(timeoutId)
          reject(error)
        }
      })
    } catch (error) {
      console.error('Error seeding profile:', error)
      throw error
    }
  }

  private ensureReadyAction(action: () => void) {
    if (this.clientReady) {
      action()
    } else {
      console.log('[TorrentService] Client not ready, queuing action')
      this.pendingSeeds.push(action)
    }
  }

  async seedPost(post: Omit<TorrentPost, 'id' | 'createdAt'>): Promise<string> {
    await this.readyPromise
    if (!this.client || !this.clientReady) {
      throw new Error('WebTorrent client not ready for seeding')
    }

    const startTime = performance.now()
    console.log('[TorrentService] Seeding post start', { author: post.author, hasImages: !!post.images?.length })

    const postData = JSON.stringify(post, null, 2)
    const fileName = `post_${Date.now()}.json`
    const encodedData = new TextEncoder().encode(postData)
    const file = new File([encodedData], fileName, { type: 'application/json; charset=utf-8' })

    return new Promise((resolve, reject) => {
      let resolved = false
      const seedAction = () => {
        try {
          const torrent = this.client.seed([file], (torrent: any) => {
            resolved = true
            const duration = (performance.now() - startTime).toFixed(0)
            console.log('[TorrentService] Post torrent ready', { infoHash: torrent.infoHash, magnet: torrent.magnetURI, durationMs: duration })
            this.emitEvent({ type: 'seeding-started', post, magnetURI: torrent.magnetURI })
            resolve(torrent.magnetURI)
          })

          torrent.on('error', (err: any) => {
            if (!resolved) {
              console.error('[TorrentService] Torrent error before ready:', err)
              reject(err)
            } else {
              console.warn('[TorrentService] Torrent error after ready:', err)
            }
          })

          torrent.on('wire', (wire: any) => {
            this.emitEvent({ type: 'peer-connected', peerId: wire.peerId || 'unknown' })
          })
        } catch (e) {
          console.error('[TorrentService] Seed action failed synchronously:', e)
          reject(e)
        }
      }

      this.ensureReadyAction(seedAction)

      // Timeout safeguard
      setTimeout(() => {
        if (!resolved) {
          reject(new Error('Seeding post timed out'))
        }
      }, 15000)
    })
  }

  async downloadProfile(magnetURI: string): Promise<Profile | null> {
    await this.readyPromise
    if (!this.client) {
      throw new Error('WebTorrent client not initialized. Are you in a browser environment?')
    }

    try {
      return new Promise((resolve, reject) => {
        const timeoutId = setTimeout(() => {
          reject(new Error('Download timeout - no peers found'))
        }, 30000)

        try {
          const torrent = this.client.add(magnetURI, (torrent: any) => {
            console.log('Started downloading torrent:', torrent.name || 'unnamed')
          })

          torrent.on('done', () => {
            clearTimeout(timeoutId)
            
            const profileFile = torrent.files.find((file: any) => 
              file.name && file.name.endsWith('_profile.json')
            )
            
            if (profileFile) {
              profileFile.getBuffer((err: any, buffer: any) => {
                if (err) {
                  this.emitEvent({ type: 'error', error: err.message })
                  reject(err)
                  return
                }

                try {
                  // Convert buffer to string with proper encoding handling
                  let jsonString: string
                  
                  if (buffer instanceof ArrayBuffer) {
                    // Handle ArrayBuffer
                    const uint8Array = new Uint8Array(buffer)
                    const decoder = new TextDecoder('utf-8', { fatal: false })
                    jsonString = decoder.decode(uint8Array)
                  } else if (buffer && typeof buffer.toString === 'function') {
                    // Handle Buffer-like objects
                    jsonString = buffer.toString('utf8')
                  } else {
                    // Fallback for other buffer types
                    jsonString = String(buffer)
                  }

                  // Parse the JSON
                  const profileData = JSON.parse(jsonString)
                  this.emitEvent({ type: 'profile-downloaded', profile: profileData })
                  resolve(profileData)
                } catch (parseError) {
                  console.error('Profile parsing error:', parseError)
                  console.error('Buffer type:', typeof buffer)
                  console.error('Buffer constructor:', buffer?.constructor?.name)
                  this.emitEvent({ 
                    type: 'error', 
                    error: `Failed to parse profile data: ${parseError instanceof Error ? parseError.message : 'Unknown error'}`
                  })
                  reject(parseError)
                }
              })
            } else {
              const error = 'Profile file not found in torrent'
              this.emitEvent({ type: 'error', error })
              reject(new Error(error))
            }
          })

          torrent.on('download', () => {
            this.emitEvent({ 
              type: 'download-progress', 
              progress: torrent.progress || 0,
              downloadSpeed: torrent.downloadSpeed || 0
            })
            this.updateStats()
          })

          torrent.on('error', (err: any) => {
            clearTimeout(timeoutId)
            this.emitEvent({ type: 'error', error: err.message })
            reject(err)
          })
        } catch (error) {
          clearTimeout(timeoutId)
          reject(error)
        }
      })
    } catch (error) {
      console.error('Error downloading profile:', error)
      throw error
    }
  }

  getStats() {
    this.updateStats()
    return { ...this.stats }
  }

  getActiveTorrents() {
    if (!this.client) return []
    
    try {
      return this.client.torrents.map((torrent: any) => ({
        infoHash: torrent.infoHash || '',
        name: torrent.name || 'Unnamed Torrent',
        magnetURI: torrent.magnetURI || '',
        progress: torrent.progress || 0,
        downloadSpeed: torrent.downloadSpeed || 0,
        uploadSpeed: torrent.uploadSpeed || 0,
        numPeers: torrent.numPeers || 0,
        downloaded: torrent.downloaded || 0,
        uploaded: torrent.uploaded || 0
      }))
    } catch (error) {
      console.error('Error getting active torrents:', error)
      return []
    }
  }

  onEvent(callback: (event: TorrentEvent) => void): () => void {
    this.eventCallbacks.push(callback)
    return () => {
      const index = this.eventCallbacks.indexOf(callback)
      if (index > -1) {
        this.eventCallbacks.splice(index, 1)
      }
    }
  }

  stopSeeding(infoHash: string): boolean {
    if (!this.client) return false
    
    try {
      const torrent = this.client.get(infoHash)
      if (torrent) {
        this.client.remove(torrent)
        this.activeTorrents.delete(infoHash)
        this.updateStats()
        return true
      }
      return false
    } catch (error) {
      console.error('Error stopping torrent seeding:', error)
      return false
    }
  }

  destroy() {
    try {
      if (this.client && typeof this.client.destroy === 'function') {
        this.client.destroy()
      }
    } catch (error) {
      console.error('Error destroying torrent client:', error)
    }
  }
}

// Global service instance
let torrentServiceInstance: TorrentService | null = null

export function getTorrentService(): TorrentService {
  if (!torrentServiceInstance) {
    torrentServiceInstance = new TorrentService()
  }
  return torrentServiceInstance
}

// Event types for better type safety
export type TorrentEvent = 
  | { type: 'seeding-started'; profile: Profile; magnetURI: string }
  | { type: 'seeding-started'; post: Omit<TorrentPost, 'id' | 'createdAt'>; magnetURI: string }
  | { type: 'peer-connected'; peerId: string }
  | { type: 'upload-progress'; uploadSpeed: number }
  | { type: 'profile-downloaded'; profile: Profile }
  | { type: 'download-progress'; progress: number; downloadSpeed: number }
  | { type: 'error'; error: string }
  | { type: 'torrent-added'; torrent: any }
  | { type: 'download-complete'; torrent: any }

export { TorrentService }
export default getTorrentService