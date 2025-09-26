// Generic transport abstraction for head update push
import type { SignedHeadUpdate } from '@/lib/crypto/headUpdate'

export interface HeadUpdateTransport {
  start(): Promise<void>
  publish(evt: SignedHeadUpdate): Promise<void>
  onHeadUpdate(cb: (evt: SignedHeadUpdate) => void): void
  stop?(): Promise<void>
  isStarted(): boolean
  name: string
}

export class InMemoryTransport implements HeadUpdateTransport {
  private listeners: ((evt: SignedHeadUpdate) => void)[] = []
  private started = false
  name = 'in-memory'
  async start() { this.started = true }
  async publish(evt: SignedHeadUpdate) { setTimeout(()=> this.listeners.forEach(l=> l(evt)), 0) }
  onHeadUpdate(cb: (evt: SignedHeadUpdate)=>void) { this.listeners.push(cb) }
  isStarted() { return this.started }
}

// Lazy dynamic libp2p transport (will fall back if dependency load fails)
export class Libp2pGossipTransport implements HeadUpdateTransport {
  private node: any = null
  private listeners: ((evt: SignedHeadUpdate) => void)[] = []
  private started = false
  name = 'libp2p-gossipsub'
  private topic = 'snartnet.head.v1'
  private pending: SignedHeadUpdate[] = []
  private bootstrap: string[] = []

  constructor(bootstrap?: string[]) {
    this.bootstrap = bootstrap || []
  }

  isStarted() { return this.started }

  async start() {
    if (this.started) return
    try {
      // Dynamic imports to avoid bundling if not used
      const [{ createLibp2p }, { gossipsub }, { webSockets }, { webTransport }, { noise }, { mplex }] = await Promise.all([
        import('libp2p'),
        import('@libp2p/gossipsub'),
        import('@libp2p/websockets'),
        import('@libp2p/webtransport'),
        import('@chainsafe/libp2p-noise'),
        import('@libp2p/mplex')
      ])

      // Optional bootstrappers if user config provided (simple dial attempt later)
      this.node = await createLibp2p({
        transports: [ webSockets(), webTransport() ],
        connectionEncrypters: [ noise() ],
        streamMuxers: [ mplex() ],
        services: {
          pubsub: gossipsub({ allowPublishToZeroTopicPeers: true })
        }
      })

      await this.node.start()
      this.started = true

      this.node.pubsub.addEventListener('message', (evt: any) => {
        try {
          if (evt.detail.topic !== this.topic) return
          const msg = new TextDecoder().decode(evt.detail.data)
          const parsed = JSON.parse(msg)
          // Basic shape check
            if (parsed && parsed.kind === 'postIndexHeadUpdate' && parsed.signature) {
              this.listeners.forEach(l => l(parsed))
            }
        } catch (e) {
          console.warn('[Libp2pGossipTransport] Failed to process message', e)
        }
      })

      await this.node.pubsub.subscribe(this.topic)

      // Fire any queued publishes
      for (const evt of this.pending) {
        await this.publish(evt)
      }
      this.pending = []

      // Attempt manual dials to bootstrap peers if provided
      for (const addr of this.bootstrap) {
        try { await this.node.dial(addr) } catch (e) { console.warn('[Libp2pGossipTransport] dial failed', addr, e) }
      }
      ;(window as any).snartnetLibp2p = this.node
    } catch (e) {
      console.warn('[Libp2pGossipTransport] Failed to initialize, falling back', e)
      this.started = false
      this.node = null
      throw e
    }
  }

  async publish(evt: SignedHeadUpdate) {
    if (!this.started || !this.node) {
      this.pending.push(evt)
      return
    }
    try {
      const payload = new TextEncoder().encode(JSON.stringify(evt))
      await this.node.pubsub.publish(this.topic, payload)
    } catch (e) {
      console.warn('[Libp2pGossipTransport] publish failed', e)
    }
  }

  onHeadUpdate(cb: (evt: SignedHeadUpdate) => void) { this.listeners.push(cb) }

  async stop() {
    try { if (this.node) await this.node.stop() } catch {}
    this.started = false
  }
}

export async function selectTransport(): Promise<HeadUpdateTransport> {
  // Selection heuristic: if window.VITE_ENABLE_LIBP2P or env variable set use libp2p
  const enable = (import.meta as any).env?.VITE_ENABLE_LIBP2P || (window as any).SNARTNET_ENABLE_LIBP2P
  if (enable) {
    try {
      const bootstrapraw = localStorage.getItem('snartnet:bootstrapPeers')
      const bootstrap = bootstrapraw ? JSON.parse(bootstrapraw) : []
      const t = new Libp2pGossipTransport(bootstrap)
      await t.start()
      console.log('[PushTransport] Using libp2p gossip transport')
      return t
    } catch (e) {
      console.warn('[PushTransport] libp2p unavailable; fallback to in-memory')
    }
  }
  const mem = new InMemoryTransport()
  await mem.start()
  return mem
}
