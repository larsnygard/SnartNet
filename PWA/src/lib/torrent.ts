import type { Profile } from '@/stores/profileStore'
import type { TorrentPost } from '@/stores/postStore'

// ---- Post Index (Chain) Concept -------------------------------------------------
// Each index torrent is a JSON file containing up to N (e.g. 200) post references.
// A reference has basic metadata needed for preview & deciding whether to fetch full post torrent.
// The index file also contains a pointer to the next (older) index magnet URI, or null if end.
// This creates a backward-linked list (chain) enabling clients to paginate historically
// without needing a global catalog. Clients choose a cut-off using:
//   - maxPosts
//   - since timestamp (ISO) OR months lookback
// Index JSON shape (v1):
// {
//   "version": 1,
//   "author": "<username or pubkey>",
//   "generatedAt": "ISO timestamp",
//   "posts": [ { id, magnetUri, author, createdAt, tags?, size?, infoHash? } ],
//   "next": "magnet:?xt=..." | null,
//   "count": <number of posts in this chunk>
// }
// Seeding strategy: when local posts change, optionally rebuild head index chunk(s)
// and seed new head; previous head becomes next in chain if content diverged.
// NOTE: For now we implement simple full rebuild creating a single chunk and seeding it.

interface PostIndexEntry {
  id: string
  magnetUri: string
  author: string
  createdAt: string
  tags?: string[]
  infoHash?: string
  size?: number
}

interface PostIndexFileV1 {
  version: 1
  author: string
  generatedAt: string
  posts: PostIndexEntry[]
  next: string | null
  count: number
}

export interface DownloadPostIndexOptions {
  maxPosts?: number
  since?: string // ISO timestamp
  monthsLookback?: number // Alternative to since
  maxChunks?: number // Safety cap on chain depth
  signal?: AbortSignal
}

export interface DownloadedPostIndexResult {
  entries: PostIndexEntry[]
  truncated: boolean // true if stopped due to limits
  chunksFetched: number
  nextPointer: string | null
}

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
    const customTrackers = [
      'wss://tracker.webtorrent.dev',
      'wss://tracker.openwebtorrent.com',
      'udp://tracker.openbittorrent.com:6969',
      'udp://tracker.openbittorrent.com:80',
      'udp://tracker.coppersurfer.tk:6969',
      'udp://9.rarbg.to:2710',
      'udp://tracker.leechers-paradise.org:6969'
    ];

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
        // Configure WebTorrent with only our custom trackers
        this.client = new WebTorrentConstructor({
          announce: customTrackers,
          announceList: [customTrackers],
          dht: true, // Enable DHT for better peer discovery
          webSeeds: false // Disable web seeds for now
        })
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
          
          const customTrackers = [
            'wss://tracker.webtorrent.dev',
            'wss://tracker.openwebtorrent.com',
            'udp://tracker.openbittorrent.com:6969',
            'udp://tracker.openbittorrent.com:80'
          ];
          const torrent = this.client.seed([file], {
            announce: customTrackers,
            announceList: [customTrackers]
          }, (torrent: any) => {
            clearTimeout(timeoutId)
            console.log('Profile seeded successfully:', { 
              magnetURI: torrent.magnetURI,
              infoHash: torrent.infoHash,
              numPeers: torrent.numPeers
            })
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

          torrent.on('warning', (err: any) => {
            if (err.message && err.message.includes('tracker')) {
              console.warn('[TorrentService] Profile tracker warning (expected):', err.message)
            }
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
    console.log('[TorrentService] Seeding post start', { author: post.author, hasImages: !!post.images?.length, signed: !!(post as any).signature })

    const postData = JSON.stringify(post, null, 2)
    const fileName = `post_${Date.now()}.json`
    const encodedData = new TextEncoder().encode(postData)
    const file = new File([encodedData], fileName, { type: 'application/json; charset=utf-8' })

    return new Promise((resolve, reject) => {
      let resolved = false
      let trackerWarningsShown = false
      
      const seedAction = () => {
        try {
          const customTrackers = [
            'wss://tracker.webtorrent.dev',
            'wss://tracker.openwebtorrent.com',
            'udp://tracker.openbittorrent.com:6969',
            'udp://tracker.openbittorrent.com:80'
          ];
          const torrent = this.client.seed([file], {
            announce: customTrackers,
            announceList: [customTrackers]
          }, (torrent: any) => {
            resolved = true
            const duration = (performance.now() - startTime).toFixed(0)
            console.log('[TorrentService] Post torrent ready', { 
              infoHash: torrent.infoHash, 
              magnet: torrent.magnetURI, 
              durationMs: duration,
              numPeers: torrent.numPeers 
            })
            this.emitEvent({ type: 'seeding-started', post, magnetURI: torrent.magnetURI })
            resolve(torrent.magnetURI)
          })

          torrent.on('error', (err: any) => {
            if (!resolved) {
              console.error('[TorrentService] Torrent error before ready:', err)
              reject(err)
            } else {
              console.warn('[TorrentService] Torrent error after ready (non-fatal):', err)
            }
          })

          // Handle tracker warnings gracefully
          torrent.on('warning', (err: any) => {
            if (!trackerWarningsShown && err.message && err.message.includes('tracker')) {
              console.warn('[TorrentService] Tracker warning (expected, trying alternatives):', err.message)
              trackerWarningsShown = true
            }
          })

          torrent.on('wire', (wire: any) => {
            this.emitEvent({ type: 'peer-connected', peerId: wire.peerId || 'unknown' })
          })

          // Log successful tracker connections
          torrent.on('peer', (peer: any) => {
            console.log('[TorrentService] Peer connected for post torrent:', peer.id || 'unknown')
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

  // ---------------- Post Index Seeding ----------------
  async seedPostIndex(posts: TorrentPost[], authorId: string, options?: { chunkSize?: number; previousHeadMagnet?: string | null }): Promise<{ magnetURI: string; count: number }> {
    await this.readyPromise
    if (!this.client || !this.clientReady) {
      throw new Error('WebTorrent client not ready for seeding index')
    }
    const chunkSize = options?.chunkSize || 200
    const slice = posts
      .slice() // copy
      .sort((a,b)=> new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())
      .slice(0, chunkSize)

    const indexFile: PostIndexFileV1 = {
      version: 1,
      author: authorId,
      generatedAt: new Date().toISOString(),
      posts: slice.map(p => ({
        id: p.id,
        magnetUri: p.magnetUri || '',
        author: p.author,
        createdAt: p.createdAt,
        tags: p.tags,
      })),
      next: options?.previousHeadMagnet || null,
      count: slice.length,
    }

    const json = JSON.stringify(indexFile, null, 2)
    const file = new File([new TextEncoder().encode(json)], `post_index_${Date.now()}.json`, { type: 'application/json; charset=utf-8' })

    return new Promise((resolve, reject) => {
      try {
        const customTrackers = [
          'wss://tracker.webtorrent.dev',
          'wss://tracker.openwebtorrent.com',
          'udp://tracker.openbittorrent.com:6969',
          'udp://tracker.openbittorrent.com:80'
        ];
        const torrent = this.client.seed([file], {
          announce: customTrackers,
          announceList: [customTrackers]
        }, (torrent: any) => {
          console.log('[TorrentService] Post index seeded successfully:', {
            magnetURI: torrent.magnetURI,
            count: indexFile.count,
            numPeers: torrent.numPeers
          })
          this.emitEvent({ type: 'post-index-seeded', magnetURI: torrent.magnetURI, count: indexFile.count })
          resolve({ magnetURI: torrent.magnetURI, count: indexFile.count })
        })
        
        torrent.on('error', (err: any) => {
          console.error('[TorrentService] Post index torrent error:', err)
          reject(err)
        })
        
        torrent.on('warning', (err: any) => {
          if (err.message && err.message.includes('tracker')) {
            console.warn('[TorrentService] Post index tracker warning (expected):', err.message)
          }
        })
      } catch (e) {
        console.error('[TorrentService] Failed to seed post index:', e)
        reject(e)
      }
    })
  }

  // ---------------- Post Index Download ----------------
  async downloadPostIndexChain(headMagnetURI: string, opts: DownloadPostIndexOptions = {}): Promise<DownloadedPostIndexResult> {
    await this.readyPromise
    if (!this.client) throw new Error('WebTorrent client not initialized')
    const maxPosts = opts.maxPosts ?? 500
    const maxChunks = opts.maxChunks ?? 20
    let sinceTs: number | null = null
    if (opts.since) {
      sinceTs = new Date(opts.since).getTime()
    } else if (opts.monthsLookback) {
      const d = new Date()
      d.setMonth(d.getMonth() - opts.monthsLookback)
      sinceTs = d.getTime()
    }

    const collected: PostIndexEntry[] = []
    let currentMagnet: string | null = headMagnetURI
    let chunks = 0
    let truncated = false
    let nextPointer: string | null = null

    while (currentMagnet && chunks < maxChunks && collected.length < maxPosts) {
      if (opts.signal?.aborted) throw new Error('Aborted')
      const { indexData, next } = await this.downloadSingleIndex(currentMagnet)
      chunks++
      this.emitEvent({ type: 'post-index-downloaded', magnetURI: currentMagnet, count: indexData.count })

      for (const entry of indexData.posts) {
        if (collected.length >= maxPosts) { truncated = true; break }
        if (sinceTs) {
          const ts = new Date(entry.createdAt).getTime()
            if (isNaN(ts)) continue
          if (ts < sinceTs) { truncated = true; break }
        }
        collected.push(entry)
      }
      if (truncated) break
      currentMagnet = next
      nextPointer = next
    }

    return { entries: collected, truncated, chunksFetched: chunks, nextPointer }
  }

  private async downloadSingleIndex(magnetURI: string): Promise<{ indexData: PostIndexFileV1; next: string | null }> {
    return new Promise((resolve, reject) => {
      try {
        const torrent = this.client.add(magnetURI, () => {
          // torrent metadata fetch initiated
        })
        const timeout = setTimeout(() => reject(new Error('Index download timeout')), 20000)
        torrent.on('done', () => {
          clearTimeout(timeout)
          const file = torrent.files.find((f: any) => f.name && f.name.startsWith('post_index_'))
          if (!file) return reject(new Error('Index file not found in torrent'))
          file.getBuffer((err: any, buffer: any) => {
            if (err) return reject(err)
            try {
              let jsonStr: string
              if (buffer instanceof ArrayBuffer) {
                jsonStr = new TextDecoder('utf-8').decode(new Uint8Array(buffer))
              } else if (buffer && typeof buffer.toString === 'function') {
                jsonStr = buffer.toString('utf8')
              } else {
                jsonStr = String(buffer)
              }
              const parsed = JSON.parse(jsonStr) as PostIndexFileV1
              if (parsed.version !== 1) return reject(new Error('Unsupported index version'))
              resolve({ indexData: parsed, next: parsed.next || null })
            } catch (e) {
              reject(e)
            }
          })
        })
        torrent.on('error', (err: any) => reject(err))
      } catch (e) {
        reject(e)
      }
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

  // Download a single post JSON torrent (expects a file named starting with 'post_' and .json)
  async downloadPost(magnetURI: string): Promise<any | null> {
    await this.readyPromise
    if (!this.client) throw new Error('WebTorrent client not initialized')
    return new Promise((resolve, reject) => {
      try {
        const torrent = this.client.add(magnetURI, () => {})
        const timeout = setTimeout(() => reject(new Error('Post download timeout')), 20000)
        torrent.on('done', () => {
          clearTimeout(timeout)
            const file = torrent.files.find((f: any) => f.name && f.name.startsWith('post_') && f.name.endsWith('.json'))
            if (!file) return reject(new Error('Post JSON file not found in torrent'))
            file.getBuffer((err: any, buffer: any) => {
              if (err) return reject(err)
              try {
                let jsonStr: string
                if (buffer instanceof ArrayBuffer) {
                  jsonStr = new TextDecoder('utf-8').decode(new Uint8Array(buffer))
                } else if (buffer && typeof buffer.toString === 'function') {
                  jsonStr = buffer.toString('utf8')
                } else {
                  jsonStr = String(buffer)
                }
                const parsed = JSON.parse(jsonStr)
                resolve(parsed)
              } catch (e) {
                reject(e)
              }
            })
        })
        torrent.on('error', (err: any) => reject(err))
      } catch (e) {
        reject(e)
      }
    })
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
  | { type: 'post-index-seeded'; magnetURI: string; count: number }
  | { type: 'post-index-downloaded'; magnetURI: string; count: number }

export { TorrentService }
export default getTorrentService