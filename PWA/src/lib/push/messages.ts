// Encrypted message layer using per-contact topic + shared key
import type { Message } from '@/stores/messageStore'
import { getChatTransport, ensureSubscribed, onRawChatMessage } from './messageTransport'
import { deriveSharedSymmetricKey, encryptMessage, decryptMessage } from '@/lib/crypto/chatKeys'
import { debugEnabled } from '@/lib/debug'
import { useProfileStore } from '@/stores/profileStore'
import { useContactStore } from '@/stores/contactStore'

const localPlainListeners: ((msg: Message) => void)[] = []

function topicForPair(aFp: string, bFp: string) {
  return 'snartnet.msg.v1.' + [aFp, bFp].sort().join('_')
}


// Subscribe to raw messages once
let rawBound = false
function bindRawListenerOnce() {
  if (rawBound) return
  rawBound = true
  onRawChatMessage(async wire => {
    try {
      const contactStore = useContactStore.getState()
      const profileStore = useProfileStore.getState()
      const me = profileStore.currentProfile
      if (!me || !me.publicKey) return
  const myFp = (me as any).fingerprint || me.publicKey?.slice(0,12)
      if (wire.kind !== 'chatMessage.v1') return
      // Determine if we are participant
      if (wire.from !== myFp && wire.to !== myFp) return
      const otherFp = wire.from === myFp ? wire.to : wire.from
      const contact = contactStore.contacts.find(c => c.fingerprint === otherFp || c.publicKey?.startsWith(otherFp))
      if (!contact || !contact.publicKey) return
      // Derive shared key
      // NOTE: Current fallback uses ed25519 raw bytes from localStorage? Not yet exposed; skipping if missing.
      const privRaw = (window as any).__sn_local_ed25519_priv as Uint8Array | undefined
      if (!privRaw) { if (debugEnabled) console.warn('[messages] missing local private key for decrypt'); return }
      const pubOther = base64ToBytes(contact.publicKey)
      const shared = await deriveSharedSymmetricKey(privRaw, pubOther, myFp, otherFp)
      const plaintext = await decryptMessage(shared, { ciphertext: wire.ciphertext, nonce: wire.nonce, alg: wire.alg })
      const content = plaintext || '(undecryptable)'
      const plainMsg: Message = {
        id: wire.msgId,
        sender: wire.from,
        recipient: wire.to,
        content,
        timestamp: wire.createdAt,
        encrypted: true,
        status: wire.to === myFp ? 'delivered' : 'sent'
      }
      localPlainListeners.forEach(l => l(plainMsg))
      if (debugEnabled) console.info('[messages] recv', wire.msgId, 'len', content.length)
    } catch (e) {
      if (debugEnabled) console.warn('[messages] raw handler error', e)
    }
  })
}

function randomId() { return Math.random().toString(36).slice(2) }
// bytesToBase64 not needed currently
function base64ToBytes(b64: string) { return new Uint8Array(atob(b64).split('').map(c=>c.charCodeAt(0))) }

export async function publishEncryptedMessage(contactId: string, plaintext: string): Promise<Message> {
  bindRawListenerOnce()
  const profile = useProfileStore.getState().currentProfile
  if (!profile || !profile.publicKey) throw new Error('No current profile public key')
  const contact = useContactStore.getState().contacts.find(c => c.id === contactId)
  if (!contact) throw new Error('Contact not found')
  if (!contact.publicKey) throw new Error('Contact missing public key')
  // Acquire local private key (placeholder). For now expect window.__sn_local_ed25519_priv set externally.
  const privRaw: Uint8Array | undefined = (window as any).__sn_local_ed25519_priv
  if (!privRaw) throw new Error('Local private key not set (__sn_local_ed25519_priv)')
  const myFp = (profile as any).fingerprint || profile.publicKey.slice(0,12)
  const otherFp = contact.fingerprint || contact.publicKey.slice(0,12)
  const topic = topicForPair(myFp, otherFp)
  await ensureSubscribed(topic)
  const shared = await deriveSharedSymmetricKey(privRaw, base64ToBytes(contact.publicKey), myFp, otherFp)
  const { ciphertext, nonce, alg } = await encryptMessage(shared, plaintext)
  const wire = {
    kind: 'chatMessage.v1' as const,
    topic,
    msgId: randomId(),
    from: myFp,
    to: otherFp,
    createdAt: new Date().toISOString(),
    ciphertext,
    nonce,
    alg,
    sig: null
  }
  const transport = await getChatTransport()
  await transport.publish(wire)
  if (debugEnabled) console.info('[messages] sent', wire.msgId, 'len', plaintext.length)
  const localMsg: Message = {
    id: wire.msgId,
    sender: wire.from,
    recipient: wire.to,
    content: plaintext,
    timestamp: wire.createdAt,
    encrypted: true,
    status: 'sent'
  }
  // Locally also emit to listeners to update UI immediately
  localPlainListeners.forEach(l => l(localMsg))
  return localMsg
}

export function onMessage(cb: (msg: Message) => void) {
  bindRawListenerOnce()
  localPlainListeners.push(cb)
}

