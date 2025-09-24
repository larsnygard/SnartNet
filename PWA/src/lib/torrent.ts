import type { Profile } from '@/stores/profileStore'

// Simple torrent service that works in both browser and build
class TorrentService {
  private client: any = null
  private activeTorrents: Map<string, any> = new Map()
  private eventCallbacks: Array<(event: any) => void> = []
  private stats = {
    torrents: 0,
    downloadSpeed: 0,
    uploadSpeed: 0,
    downloaded: 0,
    uploaded: 0,
    peers: 0
  }

  constructor() {
    this.initializeClient()
  }

  private async initializeClient() {
    // Only initialize in browser environment
    if (typeof window === 'undefined') return
    
    try {
      // Use global WebTorrent from CDN
      const WebTorrentConstructor = (window as any).WebTorrent
      
      if (!WebTorrentConstructor || typeof WebTorrentConstructor !== 'function') {
        // Fallback - wait for CDN to load or show error
        throw new Error('WebTorrent not available - please ensure CDN is loaded')
      } else {
        this.client = new WebTorrentConstructor()
      }

      this.client.on('error', (err: any) => {
        this.emitEvent({ type: 'error', error: err.message })
      })

      this.client.on('torrent', (torrent: any) => {
        this.activeTorrents.set(torrent.infoHash, torrent)
        this.emitEvent({ type: 'torrent-added', torrent })
        this.updateStats()
      })

      console.log('WebTorrent client initialized successfully')
    } catch (error) {
      console.error('Failed to initialize WebTorrent:', error)
      this.emitEvent({ type: 'error', error: `Failed to initialize WebTorrent: ${error}` })
    }
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
    if (!this.client) {
      throw new Error('WebTorrent client not initialized. Are you in a browser environment?')
    }

    try {
      const profileData = JSON.stringify(profile, null, 2)
      const fileName = `${profile.username}_profile.json`
      const file = new File([profileData], fileName, { type: 'application/json' })

      return new Promise((resolve, reject) => {
        const timeoutId = setTimeout(() => {
          reject(new Error('Seeding timeout'))
        }, 10000)

        try {
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

  async downloadProfile(magnetURI: string): Promise<Profile | null> {
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
                  const profileData = JSON.parse(buffer.toString())
                  this.emitEvent({ type: 'profile-downloaded', profile: profileData })
                  resolve(profileData)
                } catch (parseError) {
                  this.emitEvent({ type: 'error', error: 'Failed to parse profile data' })
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
  | { type: 'peer-connected'; peerId: string }
  | { type: 'upload-progress'; uploadSpeed: number }
  | { type: 'profile-downloaded'; profile: Profile }
  | { type: 'download-progress'; progress: number; downloadSpeed: number }
  | { type: 'error'; error: string }
  | { type: 'torrent-added'; torrent: any }
  | { type: 'download-complete'; torrent: any }

export { TorrentService }
export default getTorrentService