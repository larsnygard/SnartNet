import { debugEnabled } from '@/lib/debug'

export interface RawWireMessage {
  kind: 'chatMessage.v1'
  topic: string
  msgId: string
  from: string
  to: string
  createdAt: string
  ciphertext: string
  nonce: string
  alg: string
  sig?: string | null
}

export interface ChatMessageTransport {
  start(): Promise<void>
  publish(msg: RawWireMessage): Promise<void>
  subscribe(topic: string): Promise<void>
  onMessage(cb: (msg: RawWireMessage) => void): void
  isStarted(): boolean
  name: string
}

class InMemoryChatTransport implements ChatMessageTransport {
  private listeners: ((m: RawWireMessage) => void)[] = []
  private started = false
  name = 'in-memory-chat'
  private subs = new Set<string>()
  async start() { this.started = true }
  async publish(msg: RawWireMessage) { setTimeout(()=> this.listeners.forEach(l=> l(msg)), 0) }
  async subscribe(topic: string) { this.subs.add(topic) }
  onMessage(cb: (msg: RawWireMessage)=>void) { this.listeners.push(cb) }
  isStarted() { return this.started }
}

class Libp2pChatTransport implements ChatMessageTransport {
  name = 'libp2p-chat'
  private node: any = null
  private started = false
  private listeners: ((m: RawWireMessage) => void)[] = []
  private subscribed = new Set<string>()
  private pendingPublishes: RawWireMessage[] = []
  private pendingSubs: string[] = []

  async start() {
    if (this.started) return
    try {
      const [{ createLibp2p }, { gossipsub }, { webSockets }, { webTransport }, { noise }, { mplex }] = await Promise.all([
        import('libp2p'),
        import('@libp2p/gossipsub'),
        import('@libp2p/websockets'),
        import('@libp2p/webtransport'),
        import('@chainsafe/libp2p-noise'),
        import('@libp2p/mplex')
      ])
      this.node = await createLibp2p({
        transports: [ webSockets(), webTransport() ],
        connectionEncrypters: [ noise() ],
        streamMuxers: [ mplex() ],
        services: { pubsub: gossipsub({ allowPublishToZeroTopicPeers: true }) }
      })
      await this.node.start()
      this.node.pubsub.addEventListener('message', (evt: any) => {
        try {
          const td = new TextDecoder()
            const raw = td.decode(evt.detail.data)
            const parsed = JSON.parse(raw)
            if (parsed && parsed.kind === 'chatMessage.v1') {
              this.listeners.forEach(l => l(parsed))
            }
        } catch (e) {
          if (debugEnabled) console.warn('[Libp2pChatTransport] parse fail', e)
        }
      })
      // Fulfill pending subs
      for (const t of this.pendingSubs) {
        try { await this.node.pubsub.subscribe(t); this.subscribed.add(t) } catch (e) { console.warn('[Libp2pChatTransport] sub fail', t, e) }
      }
      this.pendingSubs = []
      // Publish queued
      for (const m of this.pendingPublishes) await this.publish(m)
      this.pendingPublishes = []
      this.started = true
      ;(window as any).snartnetChatNode = this.node
    } catch (e) {
      console.warn('[Libp2pChatTransport] init failed, fallback to in-memory', e)
      this.started = false
      this.node = null
      throw e
    }
  }

  async subscribe(topic: string) {
    if (this.subscribed.has(topic)) return
    if (!this.node) { this.pendingSubs.push(topic); return }
    try { await this.node.pubsub.subscribe(topic); this.subscribed.add(topic) } catch (e) { console.warn('[Libp2pChatTransport] sub error', topic, e) }
  }

  async publish(msg: RawWireMessage) {
    if (!this.node) { this.pendingPublishes.push(msg); return }
    try {
      const payload = new TextEncoder().encode(JSON.stringify(msg))
      await this.node.pubsub.publish(msg.topic, payload)
    } catch (e) {
      console.warn('[Libp2pChatTransport] publish failed', e)
    }
  }

  onMessage(cb: (msg: RawWireMessage) => void) { this.listeners.push(cb) }
  isStarted() { return this.started }
}

let transportPromise: Promise<ChatMessageTransport> | null = null

export async function getChatTransport(): Promise<ChatMessageTransport> {
  if (!transportPromise) {
    transportPromise = (async () => {
      const enable = (import.meta as any).env?.VITE_ENABLE_LIBP2P || (window as any).SNARTNET_ENABLE_LIBP2P
      if (enable) {
        try {
          const t = new Libp2pChatTransport()
          await t.start()
          if (debugEnabled) console.info('[chatTransport] using libp2p')
          return t
        } catch {}
      }
      const mem = new InMemoryChatTransport()
      await mem.start()
      if (debugEnabled) console.info('[chatTransport] using in-memory')
      return mem
    })()
  }
  return transportPromise
}

const localSubs = new Set<string>()
export async function ensureSubscribed(topic: string) {
  if (localSubs.has(topic)) return
  const t = await getChatTransport()
  await t.subscribe(topic)
  localSubs.add(topic)
}

const listeners: ((m: RawWireMessage) => void)[] = []
export function onRawChatMessage(cb: (m: RawWireMessage)=>void) {
  listeners.push(cb)
  getChatTransport().then(t => t.onMessage(msg => listeners.forEach(l => l(msg))))
}
