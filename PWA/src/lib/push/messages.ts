// Message push transport abstraction (DHT/libp2p or in-memory fallback)
import type { Message } from '@/stores/messageStore'

export interface MessagePushTransport {
  start(): Promise<void>
  publish(msg: Message): Promise<void>
  onMessage(cb: (msg: Message) => void): void
  stop?(): Promise<void>
  isStarted(): boolean
  name: string
}

const localListeners: ((msg: Message) => void)[] = []

class InMemoryMessageTransport implements MessagePushTransport {
  private listeners: ((msg: Message) => void)[] = []
  private started = false
  name = 'in-memory-msg'
  async start() { this.started = true }
  async publish(msg: Message) { setTimeout(()=> this.listeners.forEach(l=> l(msg)), 0) }
  onMessage(cb: (msg: Message)=>void) { this.listeners.push(cb) }
  isStarted() { return this.started }
}

let transport: MessagePushTransport | null = null

export async function ensureMessageTransport(): Promise<MessagePushTransport> {
  if (!transport) {
    // TODO: Add libp2p transport if available
    transport = new InMemoryMessageTransport()
    transport.onMessage(msg => localListeners.forEach(l => l(msg)))
    await transport.start()
  }
  return transport
}

export async function publishMessage(msg: Message) {
  const t = await ensureMessageTransport()
  await t.publish(msg)
}

export function onMessage(cb: (msg: Message) => void) {
  localListeners.push(cb)
}
