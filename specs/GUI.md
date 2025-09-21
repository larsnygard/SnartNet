# SnartNet GUI Architecture Specification

Note: Project renamed from PeerSocial to SnartNet; terminology updated accordingly.

## 1. Objectives

Deliver a cross-platform graphical client (Web PWA first, Mobile second) backed by a shared Rust core that ensures cryptographic correctness, performance, and protocol consistency. The GUI must be:

- Secure by default (never exposes raw private keys to JS logic)
- Incrementally offline-capable (cached profiles, deferred actions)
- Efficient on constrained mobile networks (selective sync, streaming rendering)
- Extensible (plugin points for future moderation, bridges, theming)

## 2. High-Level Architecture

```
+---------------------------------------------------------------+
|                        GUI (React / RN)                      |
|  - Views / Screens                                           |
|  - Components (Feed, Composer, DM Thread, Profile, Search)   |
|  - State Store (Zustand/Redux)                               |
|  - Event Subscriptions (Core -> UI)                          |
+--------------------------▲------------------------------------+
                           | events (async channels / callbacks)
                           | commands (imperative API)
+--------------------------▼------------------------------------+
|            Core Binding Layer (WASM Bridge / FFI)            |
|  - Type-safe wrapper funcs                                   |
|  - Object lifetimes (handles for sessions, streams)          |
|  - Serialization (JSON/protobuf canonical)                    |
+--------------------------▲------------------------------------+
                           | FFI boundary
+--------------------------▼------------------------------------+
|                        Rust Core                             |
|  Modules:                                                    |
|   * identity   (keys, signing, prekeys)                      |
|   * profile    (profile store, torrent mapping)              |
|   * messaging  (envelope, X3DH bootstrap, ratchet)           |
|   * storage    (RocksDB / IndexedDB abstraction)             |
|   * transport  (torrent interface, DHT queries)              |
|   * dispatcher (pub/sub event bus)                           |
|   * search     (local index + remote query orchestration)    |
|   * sync       (selective replication policies)              |
+---------------------------------------------------------------+
```

## 3. Technology Choices

| Layer            | Web (PWA)                               | Mobile (RN target)                          | Rationale |
|------------------|------------------------------------------|---------------------------------------------|-----------|
| UI Framework     | React 18 + Vite                         | React Native (Hermes)                      | Shared mental model / component reuse |
| Styling          | TailwindCSS + CSS variables             | Styled Components / Tailwind RN variant     | Design token portability |
| State Mgmt       | Zustand (light, selector perf)          | Zustand (same patterns)                     | Minimal boilerplate |
| Data Queries     | Event-driven + derived selectors        | Same                                         | Avoid overfetch complexity |
| Rust Bridge      | wasm-bindgen + generated TS bindings    | uniffi-rs or napi-rs (decision)             | Type safety + maintainability |
| Storage (Local)  | IndexedDB (via idb-keyval wrapper)      | SQLite (react-native-quick-sqlite)          | Durable & performant |
| Crypto           | Rust only (WASM compiled)               | Rust only (no JS crypto)                    | Consistency, audit scope |
| Transport        | WebTorrent + WebRTC Data Channels       | Native libtorrent binding (future)          | Platform capability alignment |
| Notifications    | Service Worker + Notification API       | Push bridge + background fetch manager      | OS integration |

## 4. Module Responsibilities (Core)

### 4.1 identity
- Key generation (Ed25519, Curve25519 prekeys) — in Rust, sealed interface.
- Key vault: encrypted at rest (password-derived Argon2id key).
- Export minimally: public fingerprints, session handles.

### 4.2 profile
- Local profile store + remote profile cache.
- Signature verification pipeline (batched validation job). 
- Delta computation for efficient torrent patching.

### 4.3 messaging
- Envelope construction & validation.
- X3DH bootstrap (fetch prekey bundles via DHT/torrent metadata).
- Double Ratchet sessions (per-peer context keyed by fingerprint).
- Replay window + nonce management.

### 4.4 storage
- Trait `KVStore` with implementations: RocksDB (native), IndexedDB (web).
- Column families / logical buckets: `profiles`, `messages`, `attachments`, `metadata`, `queue`.
- Write-ahead log for pending outbound messages (resend on wake).

### 4.5 transport
- Torrent session abstraction (start/stop, add magnet, fetch piece).
- DHT query wrapper (get peers, announce, fetch value buckets if extended).
- Rate limiting + backoff policy.

### 4.6 dispatcher (event bus)
- Async channel fan-out (Tokio broadcast) → WASM boundary exported as callback registry.
- Event types: `ProfileUpdated`, `MessageReceived`, `PostAdded`, `AttachmentProgress`, `SyncStateChanged`.

### 4.7 search
- Local inverted index (tantivy or custom lightweight) — optional in early alpha.
- Query planner merges local + remote hints.

### 4.8 sync
- Subscription graph (followed profiles + priority tiers).
- Selective sync policy rules (battery saver / bandwidth cap modes).
- Background tick scheduler (cron-like).

## 5. Data Flow Examples

### 5.1 Receiving a Direct Message
1. Transport receives metadata (magnet / DHT pointer → envelope torrent piece).
2. Core downloads envelope, validates signature, decrypts body.
3. Messaging module inserts plaintext into storage and emits `MessageReceived`.
4. Dispatcher broadcasts event → UI state store updates conversation thread.
5. UI re-renders minimal components (virtualized list diff).

### 5.2 Posting Content
1. User composes in UI → command `core.post_create()`.
2. Core signs post JSON, writes to local store, updates profile torrent structure.
3. Torrent module updates/announces new piece hash (magnet stable root).
4. Event `PostAdded` triggers feed optimistic update.
5. Peers pull via subscription or periodic feed sync.

## 6. State Management Strategy

- Core is source of truth for canonical cryptographic state.
- UI holds derived / denormalized slices only.
- Use immutable snapshots for large list rendering to avoid tearing.
- Provide streaming pagination for message histories (windowed fetch). 

## 7. Offline & Sync Model

| Scenario            | Strategy |
|---------------------|----------|
| App cold start      | Load cached profiles + last N posts from local store before network ready |
| Compose offline     | Queue in `outbox` (signed, flagged `pending`); background worker publishes later |
| Attachment offline  | Defer torrent creation; placeholder object with `status=awaiting-upload` |
| Partial profile     | Display skeleton UI; fetch avatar & posts asynchronously |
| Battery saver mode  | Downgrade sync frequency, skip attachment prefetch |

## 8. Security Boundaries

- Private keys NEVER cross into JS; all signing/encryption inside Rust.
- Memory zeroization for ephemeral secrets (Rust `zeroize` crate).
- WASM module instantiated with `--features minimal-exports` pattern.
- Clipboard / sharing actions require explicit user gesture.

## 9. Performance Considerations

- Virtualized scrolling for feed & message threads.
- Batching UI updates using requestAnimationFrame + React concurrent rendering.
- Incremental torrent piece prioritization (newest post pointers first).
- Adaptive polling backoff (network idle vs. active interaction).
- WASM memory profiling budget target: < 64MB steady-state initial alpha.

## 10. Accessibility & UX

- Semantic HTML + ARIA roles (web).
- High contrast + reduced motion themes.
- Keyboard navigation (Tab, Arrow, Enter) for core interactions.
- Offline indicators + retry affordances.

## 11. Feature Flags (Early Releases)

| Flag                   | Purpose |
|------------------------|---------|
| `experimentalGroups`   | Enable group messaging UI panels |
| `attachmentUploads`    | Allow media attachment flow |
| `hashtagDiscovery`     | Enable search & trending sections |
| `verificationBadges`   | Display trust / VC indicators |
| `mobileOptimizations`  | Activate aggressive selective sync |

## 12. Open Decision Records (To Finalize)

| Topic                    | Options | Target Decision Date |
|--------------------------|---------|----------------------|
| Mobile Framework         | React Native vs Flutter | Month 6 |
| Group Ratchet Mechanism  | Custom vs MLS subset | Month 9 |
| Local Search Engine      | tantivy vs custom index | Month 8 |
| WASM Bindings Tooling    | wasm-bindgen vs uniffi layering | Month 5 |
| Torrent Lib (Web)        | WebTorrent vs custom minimal | Month 5 |

## 13. Initial Implementation Milestones

| Milestone | Description | Target |
|-----------|-------------|--------|
| M1        | Core crate builds to native + WASM; keygen + profile load | Month 5 wk2 |
| M2        | Event bus + profile subscription in web UI | Month 5 wk4 |
| M3        | Feed rendering + post composer operational | Month 6 wk2 |
| M4        | 1:1 encrypted DM alpha (no persistence resend) | Month 6 wk4 |
| M5        | Offline cache bootstrap + outbox queue | Month 7 wk1 |
| M6        | Mobile shell + key unlock flow prototype | Month 7 wk3 |

## 14. Metrics (Alpha → Beta)

| Metric | Alpha Target | Beta Target |
|--------|--------------|-------------|
| Time to interactive (cold) | < 4s | < 2.5s |
| Feed initial render posts  | 20 | 50 |
| DM send latency (median)   | < 3s | < 1.5s |
| WASM memory steady-state   | < 64MB | < 48MB |
| Offline start success      | 90% | 99% |

## 15. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| WASM cryptography side channels | Audit + constant-time libsodium paths |
| Mobile background limits | Relay notification torrents + OS push fallback |
| State divergence (UI vs core) | Unidirectional event bus + idempotent reducers |
| Attachment bloat | Deduplicated chunks + lazy fetch | 
| Large follow graphs | Progressive sync tiers + LRU profile cache |

## 16. Future Enhancements

- Plugin API for moderation overlays
- Theming system with runtime palette switching
- In-app credential issuance & verification workflows
- Local-first CRDT layer for optimistic collaborative edits

---
*This document will evolve as prototype constraints and performance data emerge.*
