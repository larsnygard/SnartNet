# 🌐 SnartNet PWA

SnartNet is a decentralized social media protocol and client, designed around torrent swarms, cryptographic identity, and trust-based recovery.  
This repository contains the **Progressive Web App (PWA)** client implementation.

---

## 🚀 Features

- **Decentralized Profiles**: Each user profile is a signed torrent containing metadata, posts, and keys.
- **Cryptographic Identity**: Ed25519/Curve25519 keypairs for signing, verification, and secure messaging.
- **Ring of Trust**: Trusted contacts can verify, recover, or re-sign profiles.
- **Encrypted Messaging**: Direct and group messaging with AES/ChaCha20 + Curve25519 hybrid encryption.
- **Content Discovery**: Hashtags, distributed search indexes, and swarm-based routing.
- **Verification Services**: Domain/DNS-based verification and Verifiable Credentials.
- **Mobile-First**: PWA with offline caching, background sync, and push-like notifications.

---

## 🏗️ Architecture

The PWA is structured into four main layers:

1. **UI Layer**  
   React/Next.js components, service worker, and PWA manifest.

2. **Application Layer**  
   State management, routing, background sync jobs.

3. **Domain Layer**  
   Crypto utilities, profile/post models, trust logic, verification services.

4. **Infrastructure Layer**  
   WebTorrent/WebRTC networking, DHT over WebSocket bridges, IndexedDB storage.

---

## 📂 File/Folder Structure

```text
SnartNet/
├── public/                     # Static assets (icons, manifest.json, etc.)
│   ├── favicon.ico
│   ├── manifest.json           # PWA manifest
│   └── service-worker.js       # Service worker for caching & push
│
├── src/
│   ├── app/                    # App entry (Next.js or React root)
│   │   ├── layout.tsx
│   │   └── page.tsx
│   │
│   ├── components/             # UI components
│   │   ├── Timeline.tsx
│   │   ├── ProfileCard.tsx
│   │   ├── PostComposer.tsx
│   │   ├── MessageThread.tsx
│   │   └── VerificationBadge.tsx
│   │
│   ├── pages/                  # Routes (if not using Next.js app dir)
│   │   ├── index.tsx
│   │   ├── profile/[id].tsx
│   │   └── messages/[id].tsx
│   │
│   ├── lib/                    # Core logic
│   │   ├── crypto/             # Cryptography utilities
│   │   │   ├── keys.ts         # Ed25519/Curve25519 key mgmt
│   │   │   ├── sign.ts         # Signing & verification
│   │   │   └── encrypt.ts      # AES/ChaCha20 hybrid encryption
│   │   │
│   │   ├── protocol/           # PeerSocial protocol logic
│   │   │   ├── profile.ts      # Profile torrent handling
│   │   │   ├── posts.ts        # Post creation/verification
│   │   │   ├── trust.ts        # Ring of Trust & recovery
│   │   │   ├── discovery.ts    # DHT, hashtag torrents, search
│   │   │   └── routing.ts      # Swarm BGP-style routing
│   │   │
│   │   ├── messaging/          # Messaging subsystem
│   │   │   ├── direct.ts       # Direct messages
│   │   │   ├── group.ts        # Group messaging
│   │   │   └── notifications.ts# Push/notification torrents
│   │   │
│   │   └── verification/       # Identity verification
│   │       ├── vc.ts           # Verifiable credentials
│   │       ├── domain.ts       # DNS/.well-known checks
│   │       └── registry.ts     # Trusted verifiers
│   │
│   ├── store/                  # State management
│   │   ├── profileStore.ts
│   │   ├── postStore.ts
│   │   └── messageStore.ts
│   │
│   └── styles/                 # Global and component styles
│       └── globals.css
│
├── tests/                      # Unit and integration tests
│   ├── crypto.test.ts
│   ├── profile.test.ts
│   └── messaging.test.ts
│
├── package.json
├── tsconfig.json
└── README.md
```

---

## ⚡ Getting Started

### Prerequisites
- Node.js (>=18)
- npm or yarn

### Install

```bash
cd PWA
npm install
```

### Run Dev Server

```bash
npm run dev
```

### Build for Production

```bash
npm run build
npm run start
```

---

## 🔐 Cryptography

- **Ed25519**: Identity and signatures
- **Curve25519**: Key exchange
- **AES-256 / ChaCha20**: Symmetric encryption
- **SHA-256**: Content integrity
- **X3DH**: Forward secrecy in messaging

---

## 📡 Roadmap

- [ ] Implement profile torrent creation and seeding
- [ ] Add post composer and signed post propagation
- [ ] Integrate hashtag torrents and distributed search
- [ ] Implement direct/group messaging with hybrid crypto
- [ ] Add verification services (VCs, domain checks)
- [ ] Optimize mobile push/notification handling

---

## 🤝 Contributing

Contributions are welcome!

- Fork the repo
- Create a feature branch
- Submit a pull request

Or if you are a skilled programmer, ask to join the project. 

---

## 📜 License

Unless otherwise stated, all content in this repository is © Lars Nygard. For usage permissions, contact: lars@snart.com

---

## 📦 Persistent File System Layer (ZenFS + Fallback PFS)

SnartNet stores a large number of small objects (profile fragments, message chunks, packed media blobs) and must retain them across reloads, offline sessions, and PWA updates. We now ship a two‑tier storage strategy:

| Priority | Layer  | Backing Store (current) | Persistence | Purpose |
|----------|--------|-------------------------|-------------|---------|
| 1 | ZenFS  | IndexedDB via `@zenfs/dom` | Durable | Provides a Unix‑like hierarchical FS API with future pluggable backends (OPFS, memory, etc.) |
| 2 | PFS Fallback (`pfs.ts`) | OPFS → IndexedDB → Memory | Durable / Ephemeral | Safety net if ZenFS init fails (permissions, unsupported browser, quota errors) |

### Why ZenFS?
ZenFS consolidates file operations behind a familiar POSIX‑style API while remaining fully in-process (no service worker requirement) and enabling future backend substitution (e.g. direct OPFS mount once available). It reduces custom code surface and provides consistent semantics (mkdirp, stats, directory traversal) useful for future pack/manifest logic.

### Code Locations
| Feature | Path |
|---------|------|
| ZenFS init & helpers | `src/lib/zenfs.ts` |
| ZenFS React hook | `src/hooks/useZenFs.ts` |
| Fallback PFS implementation | `src/lib/pfs.ts` |
| Fallback React hook | `src/hooks/usePfs.ts` |
| Initialization orchestration | `src/hooks/useInitializeCore.ts` |

During app startup we attempt ZenFS first. If it throws or reports failure, we automatically fall back to the lightweight `pfs` abstraction so callers continue to function. Console logs indicate which layer was selected.

### ZenFS Helper API (simplified)
```ts
// from src/lib/zenfs.ts
export async function initZenFs() : Promise<{
  fs: typeof import('@zenfs/core')
  writeText(path: string, data: string): Promise<void>
  readText(path: string): Promise<string>
  exists(path: string): Promise<boolean>
}>;
```

Usage example:
```ts
import { initZenFs } from '@/lib/zenfs'

const { writeText, readText } = await initZenFs()
await writeText('/data/hello.txt', 'Hello SnartNet')
console.log(await readText('/data/hello.txt'))
```

React hook:
```ts
import { useZenFs } from '@/hooks/useZenFs'
const { ready, backend, writeText, readText } = useZenFs()
```

If `ready` is false and a fallback occurred, `backend` may indicate `pfs:fallback` (depending on implementation details).

### Fallback PFS API
`pfs` exposes a richer interface for direct file introspection used internally or as a secondary path:
```ts
interface PfsApi {
  writeFile(path: string, data: Blob | Uint8Array | string): Promise<void>
  readFile(path: string, asText?: boolean): Promise<Uint8Array | string>
  readBlob(path: string): Promise<Blob>
  deleteFile(path: string): Promise<void>
  exists(path: string): Promise<boolean>
  stat(path: string): Promise<{ path: string; size: number; mtime: number } | null>
  list(prefix?: string): Promise<{ path: string; size: number; mtime: number }[]>
  estimate(): Promise<{ usage: number; quota: number }>
}
```

### Planned Enhancements (Roadmap)
1. Pack & Manifest Layer
   - Aggregate many tiny logical files into larger content‑addressed pack blobs (improved torrent and quota performance)
   - BLAKE3 hash tree and per‑entry integrity metadata
2. Service Worker Integration
   - `fetch` interception mapping URLs under `/fs/` → ZenFS / packs
   - Streaming range requests for partial media extraction
3. Quota & Health Dashboard
   - Live storage usage (used vs. quota) + eviction strategies
4. Optional Encryption / Signing
   - Encrypted pack payloads, signed manifests for trust distribution
5. OPFS Backend for ZenFS
   - Evaluate adding OPFS mount once exposed in `@zenfs/dom` (reduces IndexedDB overhead for large binary writes)

### Migration Notes
Existing code that previously imported `getPfs()` can migrate to ZenFS gradually. A unified facade (planned) will expose a superset so application code rarely needs to distinguish the active backend.

### Troubleshooting
| Symptom | Likely Cause | Action |
|---------|-------------|--------|
| ZenFS init throws quota / UnknownError | Browser storage pressure or private mode limits | Clear site data or expand storage allowance |
| Fallback always used | Unsupported API or early exception | Check console for `[zenfs] init failed` logs |
| Large file writes slow | IndexedDB transaction overhead | Await OPFS backend support / implement pack layer |

### Verification
On first load you should see a console log indicating: `ZenFS initialized (backend=indexeddb)`. Reload the PWA; previously written test files (e.g. `/data/hello.txt`) should still be readable—confirming durability.

---

> Historical Note: An earlier placeholder suggested ZenFS was unavailable. This has been superseded by the current integration using `@zenfs/core` + `@zenfs/dom`.
