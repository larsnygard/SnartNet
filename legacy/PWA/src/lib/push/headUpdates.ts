// Simple in-process push emitter for head updates (stub for DHT/pubsub)
import type { CanonicalHeadUpdate, SignedHeadUpdate } from '../crypto/headUpdate'
import { selectTransport, HeadUpdateTransport } from './transport'

let transportPromise: Promise<HeadUpdateTransport> | null = null
const localListeners: ((evt: SignedHeadUpdate) => void)[] = []

async function ensureTransport(): Promise<HeadUpdateTransport> {
  if (!transportPromise) {
    transportPromise = selectTransport().then(t => {
      t.onHeadUpdate(evt => {
        localListeners.forEach(l => l(evt))
      })
      return t
    })
  }
  return transportPromise
}

export async function publishHeadUpdate(evt: Omit<CanonicalHeadUpdate, 'authorPublicKey'> & { signature?: string }) {
  const mod = await import('../crypto/headUpdate')
  const signed = evt.signature ? (evt as SignedHeadUpdate) : await mod.signHeadUpdate(evt)
  const t = await ensureTransport()
  await t.publish(signed)
  if (typeof window !== 'undefined') {
    (window as any).lastPublishedHeadUpdate = signed
    // Increment published counter for UI
    const w = window as any
    w.__sn_head_published_count = (w.__sn_head_published_count || 0) + 1
  }
}

export function onHeadUpdate(cb: (evt: SignedHeadUpdate) => void) {
  localListeners.push(cb)
}

export function startHeadUpdateListener() {
  ensureTransport().catch(e => console.warn('[headUpdates] transport init failed', e))
}
