import { getOrCreateKeypair } from './ed25519'

// For simplicity, use libsodium-style sealed box encryption (if available)
// For now, just sign messages; encryption can be added later if needed

export async function signMessage(_message: string): Promise<{ signature: string; publicKey: string }> {
  const kp = await getOrCreateKeypair()
  // Use WebCrypto or WASM to sign
  // For now, just return dummy signature (TODO: implement real signing)
  return {
    signature: btoa('dummy-signature'),
    publicKey: kp.publicKey
  }
}

export async function verifyMessage(_message: string, signature: string, _publicKey: string): Promise<boolean> {
  // TODO: implement real verification
  return signature === btoa('dummy-signature')
}
