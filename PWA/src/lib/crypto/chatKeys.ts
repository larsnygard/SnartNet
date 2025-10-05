import * as nacl from 'tweetnacl'
import { debugEnabled } from '@/lib/debug'

// Cache of derived shared keys (fingerprintPair -> Uint8Array[32])
const sharedKeyCache = new Map<string, Uint8Array>()

// Convert Ed25519 key (32-byte public / secret) to Curve25519 per RFC 7748 via tweetnacl; if already curve key, assume length 32.
// For simplicity we assume provided keys are raw 32-byte ed25519 public keys and secret keys.
// In a full implementation we may need ed2curve conversions (ed2curve library). Here we fallback to hashing if conversion not available.

// Placeholder simple hash-based fallback (NOT full ed->x25519 conversion). TODO: replace with proper ed2curve for prod security.
async function sha256(data: Uint8Array): Promise<Uint8Array> {
  const digest = await crypto.subtle.digest('SHA-256', new Uint8Array(data))
  return new Uint8Array(digest)
}

export async function deriveSharedSymmetricKey(ourPrivEd25519: Uint8Array, theirPubEd25519: Uint8Array, fpA: string, fpB: string): Promise<Uint8Array> {
  const keyId = [fpA, fpB].sort().join('|')
  const cached = sharedKeyCache.get(keyId)
  if (cached) return cached

  // Fallback: hash concat (insecure vs proper X25519; flagged for dev)
  // TODO: integrate ed2curve (ed25519->curve25519) then nacl.box.before
  const material = new Uint8Array(ourPrivEd25519.length + theirPubEd25519.length)
  material.set(ourPrivEd25519, 0)
  material.set(theirPubEd25519, ourPrivEd25519.length)
  const hashed = await sha256(material)
  const key = hashed.slice(0, 32) // secretbox key length
  sharedKeyCache.set(keyId, key)
  if (debugEnabled) console.info('[chatKeys] Derived (fallback) shared key for', keyId)
  return key
}

export interface EncryptedPayload {
  ciphertext: string // base64
  nonce: string // base64
  alg: string
}

function toBase64(u8: Uint8Array): string { return btoa(String.fromCharCode(...u8)) }
function fromBase64(b64: string): Uint8Array { return new Uint8Array(atob(b64).split('').map(c=>c.charCodeAt(0))) }

export async function encryptMessage(sharedKey: Uint8Array, plaintext: string): Promise<EncryptedPayload> {
  const nonce = nacl.randomBytes(24)
  const msg = new TextEncoder().encode(plaintext)
  // Using secretbox with symmetric key (already derived)
  const ct = nacl.secretbox(msg, nonce, sharedKey)
  return { ciphertext: toBase64(ct), nonce: toBase64(nonce), alg: 'chat-fallback-secretbox-v1' }
}

export async function decryptMessage(sharedKey: Uint8Array, payload: EncryptedPayload): Promise<string | null> {
  try {
    const nonce = fromBase64(payload.nonce)
    const ct = fromBase64(payload.ciphertext)
    const pt = nacl.secretbox.open(ct, nonce, sharedKey)
    if (!pt) return null
    return new TextDecoder().decode(pt)
  } catch (e) {
    if (debugEnabled) console.warn('[chatKeys] decrypt failed', e)
    return null
  }
}

export function clearChatKeyCache() { sharedKeyCache.clear() }
