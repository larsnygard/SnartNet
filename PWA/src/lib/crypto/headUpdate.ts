// Ed25519-signed Post Index Head Update event utilities
// Schema: see summary
import { getOrCreateKeypair } from './ed25519'

export interface CanonicalHeadUpdate {
  version: number;
  kind: 'postIndexHeadUpdate';
  profileId: string;
  authorPublicKey: string;
  previousHead?: string;
  newHead: string;
  postCount?: number;
  lastPostCreatedAt?: string;
  issuedAt: string;
}

export interface SignedHeadUpdate extends CanonicalHeadUpdate {
  signature: string;
}

// Canonicalize: stable key order, omit undefined/null
export function canonicalizeHeadUpdate(input: CanonicalHeadUpdate): string {
  const obj: any = {};
  const orderedKeys = [
    'version',
    'kind',
    'profileId',
    'authorPublicKey',
    'previousHead',
    'newHead',
    'postCount',
    'lastPostCreatedAt',
    'issuedAt',
  ];
  for (const k of orderedKeys) {
    const v = (input as any)[k];
    if (v !== undefined && v !== null) obj[k] = v;
  }
  return JSON.stringify(obj);
}

export async function signHeadUpdate(input: Omit<CanonicalHeadUpdate, 'authorPublicKey'>): Promise<SignedHeadUpdate> {
  const kp = await getOrCreateKeypair();
  const canonical = canonicalizeHeadUpdate({ ...input, authorPublicKey: kp.publicKey });
  // Use WASM or WebCrypto Ed25519 (reuse post signing logic)
  // For now, use WASM fallback only for simplicity
  // TODO: refactor to share code with post signing
  // @ts-ignore
  const { signPost } = await import('./ed25519');
  // signPost expects post fields, but we want to sign our canonical string
  // So, use WASM directly (or refactor sign_data)
  // For now, use signPost as a hack
  const fakePost = { version: 1, kind: 'post', body: canonical, createdAt: input.issuedAt };
  const signed = await signPost(fakePost);
  // Replace fields with our own
  return {
    ...input,
    authorPublicKey: kp.publicKey,
    signature: signed.signature,
  };
}

// Verification (for now, just compare canonical string and signature)
export async function verifyHeadUpdateSignature(payload: SignedHeadUpdate): Promise<{ ok: boolean; error?: string }> {
  const { signature, authorPublicKey, ...rest } = payload as any;
  const canonical = canonicalizeHeadUpdate({ ...rest, authorPublicKey });
  // Use post verify for now
  // @ts-ignore
  const { verifyPostSignature } = await import('./ed25519');
  // Use fake post with body = canonical
  const fakePost = { ...rest, authorPublicKey, signature, version: 1, kind: 'post', body: canonical };
  const { ok, error } = await verifyPostSignature(fakePost);
  return { ok, error };
}
