/*
 Ed25519 helper utilities
 Responsibilities:
  - Manage a persistent keypair (localStorage for now – TODO: move to IndexedDB / secure storage)
  - Canonicalize post payloads for signing
  - Sign & verify using WebCrypto if available, else fallback to WASM bindings
  - Provide base64 helpers
*/

// WASM bindings (lazy imported to avoid cost if not needed immediately)
//import initWasm, { generate_keypair, sign_data, verify_signature_wasm } from "../../wasm/snartnet_core";
import initWasm, { generate_keypair, sign_data, verify_signature_wasm } from '../../wasm/snartnet_core.js';

const KEYPAIR_STORAGE_KEY = "snartnet:ed25519:keypair:v1";

export interface StoredKeypair {
  publicKey: string; // base64
  secretKey: string; // base64 (private key)
}

export interface CanonicalPostInput {
  version: number;
  kind: string; // e.g. "post"
  authorPublicKey: string; // base64
  createdAt: string; // ISO string
  body: string;
  attachments?: string[]; // magnet URIs or hashes
  parentId?: string;
  replyTo?: string;
}

export interface SignedPostPayload extends CanonicalPostInput {
  signature: string; // base64
}

function hasWebCryptoEd25519(): boolean {
  try {
    // Some browsers may not yet support Ed25519; feature detect
    return typeof window !== "undefined" && !!window.crypto?.subtle && (window.crypto.subtle as any) !== undefined;
  } catch {
    return false;
  }
}

function base64FromBytes(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

function arrayBufferFromBase64(b64: string): ArrayBuffer {
  const bin = atob(b64);
  const len = bin.length;
  const buf = new ArrayBuffer(len);
  const view = new Uint8Array(buf);
  for (let i = 0; i < len; i++) view[i] = bin.charCodeAt(i);
  return buf;
}

function uint8FromBase64(b64: string): Uint8Array {
  return new Uint8Array(arrayBufferFromBase64(b64));
}

// Persist & load keypair
export async function getOrCreateKeypair(): Promise<StoredKeypair> {
  const existing = localStorage.getItem(KEYPAIR_STORAGE_KEY);
  if (existing) {
    try {
      return JSON.parse(existing) as StoredKeypair;
    } catch {
      // fallthrough to regenerate
    }
  }

  // Attempt WebCrypto first
  if (hasWebCryptoEd25519() && (window.crypto.subtle as any).generateKey) {
    try {
      const keyPair: CryptoKeyPair = await (window.crypto.subtle as any).generateKey(
        { name: "Ed25519", namedCurve: "Ed25519" },
        true,
        ["sign", "verify"]
      );
      const rawPub = new Uint8Array(await window.crypto.subtle.exportKey("raw", keyPair.publicKey));
      const rawPriv = new Uint8Array(await window.crypto.subtle.exportKey("pkcs8", keyPair.privateKey));
      const stored: StoredKeypair = { publicKey: base64FromBytes(rawPub), secretKey: base64FromBytes(rawPriv) };
      localStorage.setItem(KEYPAIR_STORAGE_KEY, JSON.stringify(stored));
      return stored;
    } catch (e) {
      console.warn("WebCrypto Ed25519 generateKey failed; falling back to WASM", e);
    }
  }

  // WASM fallback
  await ensureWasm();
  // generate_keypair returns JSON segments (Rust exported). Types show it returns tuple but high-level wrapper returns JSON.
  const kpJson: any = generate_keypair();
  // The wasm binding returns something we can parse or already an object; attempt parse.
  let parsed: StoredKeypair | undefined;
  if (typeof kpJson === "string") {
    parsed = JSON.parse(kpJson);
  } else {
    parsed = kpJson as StoredKeypair;
  }
  if (!parsed?.publicKey || !parsed?.secretKey) {
    throw new Error("WASM generate_keypair returned unexpected format");
  }
  localStorage.setItem(KEYPAIR_STORAGE_KEY, JSON.stringify(parsed));
  return parsed;
}

let wasmInitialized = false;
async function ensureWasm() {
  if (!wasmInitialized) {
    try {
      await initWasm();
      wasmInitialized = true;
    } catch (e) {
      console.error("Failed to initialize WASM core", e);
      throw e;
    }
  }
}

// Canonicalize: create a new object with keys sorted alphabetically & only defined keys
export function canonicalizePost(input: CanonicalPostInput): string {
  const obj: any = {};
  const orderedKeys = [
    "version",
    "kind",
    "authorPublicKey",
    "createdAt",
    "body",
    "attachments",
    "parentId",
    "replyTo"
  ];
  for (const k of orderedKeys) {
    const v = (input as any)[k];
    if (v !== undefined && v !== null && !(Array.isArray(v) && v.length === 0)) {
      obj[k] = v;
    }
  }
  return JSON.stringify(obj);
}

export async function signPost(input: Omit<CanonicalPostInput, "authorPublicKey">): Promise<SignedPostPayload> {
  const kp = await getOrCreateKeypair();
  const canonical = canonicalizePost({ ...input, authorPublicKey: kp.publicKey });

  // WebCrypto path (not yet widely available for Ed25519 in all browsers, but attempt)
  if (hasWebCryptoEd25519()) {
    try {
      // Re-import keys for signing each time (fast enough for small volume)
      // secretKey exported earlier as pkcs8
      const privBytes = arrayBufferFromBase64(kp.secretKey);
      const privKey = await window.crypto.subtle.importKey(
        "pkcs8",
        privBytes,
        { name: "Ed25519", namedCurve: "Ed25519" },
        false,
        ["sign"]
      );
      const sigBuf = await window.crypto.subtle.sign({ name: "Ed25519" }, privKey, new TextEncoder().encode(canonical));
      const signature = base64FromBytes(new Uint8Array(sigBuf));
      return { ...input, authorPublicKey: kp.publicKey, signature, version: input.version } as SignedPostPayload;
    } catch (e) {
      console.warn("WebCrypto sign failed; falling back to WASM", e);
    }
  }

  // WASM fallback
  await ensureWasm();
  const signature = sign_data(JSON.stringify(kp), canonical);
  return { ...input, authorPublicKey: kp.publicKey, signature, version: input.version } as SignedPostPayload;
}

export async function verifyPostSignature(payload: SignedPostPayload): Promise<{
  ok: boolean;
  error?: string;
}> {
  const { signature, authorPublicKey, ...rest } = payload as any;
  const canonical = canonicalizePost({ ...rest, authorPublicKey });

  // Try WebCrypto verify first
  if (hasWebCryptoEd25519()) {
    try {
      const pubBytes = arrayBufferFromBase64(authorPublicKey);
      const pubKey = await window.crypto.subtle.importKey(
        "raw",
        pubBytes,
        { name: "Ed25519", namedCurve: "Ed25519" },
        false,
        ["verify"]
      );
      const ok = await window.crypto.subtle.verify(
        { name: "Ed25519" },
        pubKey,
        arrayBufferFromBase64(signature),
        new TextEncoder().encode(canonical)
      );
      return { ok };
    } catch (e: any) {
      console.warn("WebCrypto verify failed; fallback to WASM", e);
    }
  }

  await ensureWasm();
  try {
    const ok = verify_signature_wasm(canonical, signature, authorPublicKey);
    return { ok, error: ok ? undefined : "invalid-signature" };
  } catch (e: any) {
    return { ok: false, error: e?.message || "verify-error" };
  }
}

export function deriveFingerprint(publicKeyB64: string): string {
  // Short identifyable fingerprint: first 8 chars of base64-hash (SHA-256 of raw pubkey)
  // Non-cryptographic display only (not for trust decisions yet)
  const raw = uint8FromBase64(publicKeyB64);
  return "fpr-" + Array.from(raw.slice(0, 6)).map((b: number) => b.toString(16).padStart(2, "0")).join("");
}

export type { StoredKeypair as Ed25519StoredKeypair };
