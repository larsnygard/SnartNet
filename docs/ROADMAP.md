# SnartNet Implementation Roadmap

Renamed from PeerSocial (original working title). Historical references to PeerSocial map to SnartNet.

## Overview

This roadmap outlines the phased approach to implementing the SnartNet decentralized social media protocol. The project is divided into five major phases, each building upon the previous to create a complete, production-ready system.

> Status update (May 2026): PWA development is paused and archived under `legacy/PWA`. Active implementation now targets native desktop and mobile clients over a shared Rust core.

## Phase 1: Core Infrastructure (Months 1-4)

### 1.1 Cryptographic Foundation
**Timeline:** Month 1
- [ ] Implement Ed25519 key generation and management
- [ ] Implement Curve25519 key exchange
- [ ] Implement AES-256/ChaCha20 symmetric encryption
- [ ] Create key derivation functions (HKDF)
- [ ] Implement signature verification and validation
- [ ] Create secure key storage mechanisms

**Dependencies:** libsodium, OpenSSL, or equivalent cryptographic libraries

### 1.2 Profile System
**Timeline:** Month 2
- [ ] Design profile.json schema and validation
- [ ] Implement profile creation and serialization
- [ ] Create profile signing and verification
- [ ] Implement profile versioning system
- [ ] Create profile update mechanisms
- [ ] Implement contacts.json Ring of Trust structure

**Dependencies:** JSON schema validation, cryptographic foundation

### 1.3 Torrent Integration
**Timeline:** Month 3
- [ ] Integrate BitTorrent library (libtorrent or equivalent)
- [ ] Implement profile torrent creation
- [ ] Create magnet URI generation and parsing
- [ ] Implement DHT integration for peer discovery
- [ ] Create seeding and downloading mechanisms
- [ ] Implement torrent health monitoring

**Dependencies:** libtorrent-rasterbar, DHT implementation

### 1.4 Basic CLI Interface
**Timeline:** Month 4
- [ ] Implement profile creation commands
- [ ] Create key management commands
- [ ] Implement basic torrent operations
- [ ] Create profile viewing and validation commands
- [ ] Implement contact management
- [ ] Create configuration management system

**Dependencies:** CLI framework (clap, commander.js, etc.)

### Phase 1 Deliverables
- Working profile creation and distribution
- Basic cryptographic operations
- Simple CLI for profile management
- DHT-based peer discovery
- Profile torrent seeding/downloading

## Phase 2: Native Foundation & Core Messaging (Months 5-7)

Shift focus to delivering a native-first experience early. A shared Rust core, desktop GUI foundation, and mobile host adapters are prioritized alongside essential messaging primitives.

### 2.1 Shared Core Runtime
**Timeline:** Month 5 (overlaps with late Phase 1)
- [ ] Extract protocol core into reusable Rust crate (`core`)
- [ ] Separate target-specific adapters from platform-neutral core services
- [ ] Define FFI boundary / host adapter layer (Rust <-> host platforms)
- [ ] Implement async event bus (subscriptions: profiles, posts, messages)
- [ ] Provide deterministic serialization (canon JSON + protobuf draft)

**Deliverable:** Reusable core library usable by CLI and GUI.

### 2.2 Desktop Native Client (MVP UI)
**Timeline:** Month 5-6
- [ ] GUI stack spike and decision (Slint vs iced)
- [ ] App shell: auth/key presence, session unlock flow
- [ ] Global state store/event handling fed by core events
- [ ] Profile view (self + remote)
- [ ] Timeline/feed (latest posts from followed profiles)
- [ ] Post composer (text + tags + attachment placeholder)
- [ ] Basic DM panel (list + open thread)
- [ ] Notification indicators (new posts / DMs)
- [ ] Build & packaging pipeline (native CI artifacts)

### 2.3 Messaging Primitives (Foundational)
**Timeline:** Month 6
- [ ] Implement X3DH prekey publishing in core
- [ ] Establish Double Ratchet session bootstrap (1:1)
- [ ] Plaintext message envelope integration (dev mode)
- [ ] Cipher variants (ChaCha20-Poly1305 primary, AES-256-GCM fallback)
- [ ] Message persistence (local RocksDB/IndexedDB abstraction)
- [ ] Replay protection cache
- [ ] Attachment reference model (torrent infohash placeholder)

### 2.4 Mobile Prototype Scaffolding
**Timeline:** Month 7
- [ ] Create thin Android shell consuming shared core via FFI
- [ ] Create thin iOS shell consuming shared core via FFI
- [ ] Implement key storage abstraction (secure enclave / keystore draft)
- [ ] Background sync strategy draft (notification torrent polling window)
- [ ] Offline bootstrap (cached profiles + pending actions queue)

### Phase 2 Deliverables
- Platform-neutral protocol core with target adapters
- MVP desktop native client (profiles, feed, basic posting, basic DMs)
- Initial encrypted 1:1 messaging (alpha)
- Mobile shell prototypes (Android + iOS)
- Unified event & storage abstractions

### 2.1 Direct Messaging
**Timeline:** Month 5
- [ ] Implement X3DH key agreement protocol
- [ ] Create hybrid encryption for messages
- [ ] Implement message serialization and signing
- [ ] Create ephemeral torrent messaging
- [ ] Implement forward secrecy mechanisms
- [ ] Create message delivery confirmation

**Dependencies:** Phase 1 cryptographic foundation, X3DH implementation

### 2.2 Group Messaging
**Timeline:** Month 6
- [ ] Implement shared key distribution
- [ ] Create group member management
- [ ] Implement group message encryption
- [ ] Create group invitation system
- [ ] Implement message ordering and synchronization
- [ ] Create group administrative controls

**Dependencies:** Direct messaging system, group key management

### 2.3 Message Routing
**Timeline:** Month 7
- [ ] Implement DHT-based message routing
- [ ] Create relay node functionality
- [ ] Implement message queuing for offline users
- [ ] Create delivery retry mechanisms
- [ ] Implement notification torrents
- [ ] Create wake-on-swarm functionality (mobile)

**Dependencies:** DHT integration, torrent system

### Phase 2 Deliverables
- Secure direct messaging
- Group messaging functionality
- Offline message delivery
- Basic mobile optimization
- Message routing infrastructure

## Phase 3: Rich Content, Discovery & UX Iteration (Months 8-11)

### 3.1 Post System Expansion
**Timeline:** Month 8
- [ ] Implement post creation and signing
- [ ] Create post torrent distribution
- [ ] Implement post validation and verification
- [ ] Create post threading and replies
- [ ] Implement media attachment handling
- [ ] Create post deletion and editing mechanisms

**Dependencies:** Profile system, torrent infrastructure

### 3.2 Discovery & Search System
**Timeline:** Month 9
- [ ] Implement hashtag extraction and indexing
- [ ] Create hashtag torrent distribution
- [ ] Implement distributed search protocol
- [ ] Create content discovery mechanisms
- [ ] Implement search result ranking
- [ ] Create topic-based content aggregation

**Dependencies:** Post system, DHT infrastructure

### 3.3 Content Propagation & Caching
**Timeline:** Month 10
- [ ] Implement follower-based seeding
- [ ] Create selective synchronization
- [ ] Implement delta updates for profiles
- [ ] Create content deduplication
- [ ] Implement bandwidth management
- [ ] Create content caching strategies

**Dependencies:** Post system, follower management

### 3.4 Swarm Routing & Index Optimization
**Timeline:** Month 11
- [ ] Implement BGP-inspired routing tables
- [ ] Create swarm advertisement protocol
- [ ] Implement query forwarding
- [ ] Create routing path optimization
- [ ] Implement route signing and verification
- [ ] Create swarm health monitoring

**Dependencies:** DHT system, cryptographic signing

### Phase 3 Deliverables
- Complete content creation and distribution
- Hashtag-based discovery system
- Efficient content propagation
- Scalable swarm routing
- Advanced search capabilities

## Phase 4: Trust, Verification & Advanced Messaging (Months 12-14)

### 4.1 Ring of Trust Enhancement & Recovery UX
**Timeline:** Month 12
- [ ] Implement quorum-based recovery
- [ ] Create trust level management
- [ ] Implement revocation certificate system
- [ ] Create trust propagation mechanisms
- [ ] Implement reputation scoring
- [ ] Create trust visualization tools

**Dependencies:** Profile system, cryptographic foundation

### 4.2 Identity Verification Services & UI Badging
**Timeline:** Month 13
- [ ] Implement Verifiable Credentials (VC) system
- [ ] Create domain verification mechanism
- [ ] Implement DNS-based verification
- [ ] Create verification badge system
- [ ] Implement verification registry
- [ ] Create credential revocation system

**Dependencies:** Trust system, external DNS/web verification

### 4.3 Security Enhancements & Hardening
**Timeline:** Month 14
- [ ] Implement Sybil attack resistance
- [ ] Create DDoS mitigation strategies
- [ ] Implement spam detection and filtering
- [ ] Create rate limiting mechanisms
- [ ] Implement privacy preservation features
- [ ] Create security audit tools

**Dependencies:** Complete protocol implementation

### Phase 4 Deliverables
- Robust trust and recovery system
- Identity verification infrastructure
- Enhanced security measures
- Spam and abuse resistance
- Privacy-preserving features

## Phase 5: Production Hardening & Ecosystem (Months 15-18)

### 5.1 Mobile Clients (Feature-Complete)
**Timeline:** Month 15-16
- [ ] Implement mobile-optimized protocol stack
- [ ] Create iOS/Android native applications
- [ ] Implement background synchronization
- [ ] Create push notification system
- [ ] Implement offline-first functionality
- [ ] Create mobile-specific UI/UX

**Dependencies:** Complete protocol implementation, mobile development frameworks

### 5.2 Native Client Polishing & Accessibility
**Timeline:** Month 16-17
- [ ] Harden desktop networking transports
- [ ] Improve native accessibility and keyboard navigation
- [ ] Optimize startup, memory, and background sync behavior
- [ ] Improve native key management UX
- [ ] Add installer/update strategy per OS
- [ ] Add crash reporting and diagnostics surfaces

**Dependencies:** Platform SDKs, native packaging/signing, transport libraries

### 5.3 Performance, Observability & Scalability
**Timeline:** Month 17-18
- [ ] Implement performance monitoring
- [ ] Create load testing infrastructure
- [ ] Optimize DHT and torrent performance
- [ ] Implement advanced caching strategies
- [ ] Create network topology optimization
- [ ] Implement graceful degradation

**Dependencies:** Complete system implementation

### 5.4 Ecosystem & Integration
**Timeline:** Month 18
- [ ] Create developer SDK and APIs
- [ ] Implement IPFS/libp2p integration
- [ ] Create third-party application support
- [ ] Implement protocol bridges (ActivityPub, etc.)
- [ ] Create migration tools from existing platforms
- [ ] Establish governance and standards process

**Dependencies:** Stable protocol implementation

### Phase 5 Deliverables
- Production-ready mobile applications
- Production-ready native desktop application
- Optimized performance and scalability
- Developer ecosystem and integrations
- Complete documentation and standards

## Technical Architecture

### Core Technologies
- **Cryptography:** libsodium, Ed25519, Curve25519, AES-256, ChaCha20
- **Networking:** libtorrent-rasterbar, DHT, native transports (relay + direct)
- **Serialization:** JSON, Protocol Buffers, MessagePack
- **Storage:** SQLite, RocksDB for local data
- **Mobile:** Native host shells over shared Rust core
- **Desktop GUI:** Slint or iced

### Language Recommendations
- **Core Protocol:** Rust (performance, safety, cross-platform)
- **Mobile Apps:** Native host shells with Rust FFI bridge
- **Desktop Client:** Rust GUI framework (Slint/iced)
- **CLI Tools:** Rust or Go

### Infrastructure Requirements
- **Development:** CI/CD pipelines, automated testing, documentation generation
- **Testing:** Unit tests, integration tests, network simulation, security audits
- **Distribution:** Package managers, app stores, self-hosted binaries

## Risk Assessment & Mitigation

### Technical Risks
1. **Scalability Challenges**
   - Risk: Network congestion, poor performance at scale
   - Mitigation: Extensive load testing, adaptive algorithms, fallback mechanisms

2. **Security Vulnerabilities**
   - Risk: Cryptographic implementation flaws, protocol attacks
   - Mitigation: Security audits, formal verification, bug bounty programs

3. **Mobile Platform Restrictions**
   - Risk: App store policies, background processing limitations
   - Mitigation: Platform-specific optimizations, relay-first fallback strategy

### Ecosystem Risks
1. **Adoption Challenges**
   - Risk: Network effects, user experience complexity
   - Mitigation: Superior UX, clear migration paths, developer incentives

2. **Regulatory Compliance**
   - Risk: Privacy laws, content moderation requirements
   - Mitigation: Built-in privacy features, moderation tools, legal review

## Success Metrics

### Technical Metrics
- Network uptime and reliability (>99.9%)
- Message delivery latency (<5 seconds typical)
- Profile synchronization time (<30 seconds for updates)
- Mobile battery efficiency (comparable to existing apps)
- Storage efficiency (compression, deduplication)

### Adoption Metrics
- Active user growth rate
- Content creation and sharing volume
- Network health and peer distribution
- Developer ecosystem growth
- Third-party integrations

## Resource Requirements

### Development Team
- **Core Protocol:** 2-3 senior engineers
- **Mobile Development:** 2 engineers
- **Web Development:** 2 engineers
- **Security/Cryptography:** 1 specialist
- **DevOps/Infrastructure:** 1 engineer
- **Documentation/Community:** 1 technical writer

### Budget Considerations
- Development salaries and contractor costs
- Security audits and penetration testing
- Infrastructure for testing and distribution
- Legal review and compliance consulting
- Marketing and community building
- Conference participation and evangelism

## Conclusion

This roadmap provides a structured approach to implementing the SnartNet (formerly PeerSocial) protocol over 18 months. The phased approach ensures that each component is thoroughly tested before moving to the next phase, while allowing for parallel development in later stages.

Success will depend on maintaining high code quality, conducting thorough security reviews, and building a vibrant developer and user community around the protocol. Regular milestone reviews and adaptation of the roadmap based on technical discoveries and user feedback will be essential.

The ultimate goal is to create a truly decentralized social media protocol that empowers users with control over their data, identity, and social connections while maintaining the performance and usability expected from modern applications.

## Immediate GUI Implementation Tasks (Next 6 Weeks)

| Priority | Task | Owner (TBD) | Notes |
|----------|------|-------------|-------|
| P0 | Rust core crate skeleton (`core`) |  | Feature flags for wasm/native |
| P0 | Key management (Ed25519 + storage) |  | Argon2id password wrap |
| P0 | WASM build pipeline (wasm-bindgen + CI) |  | Optimize size (wasm-opt pass) |
| P1 | Event bus prototype (profile loaded event) |  | Tokio broadcast -> JS callbacks |
| P1 | Profile load & verify flow (web) |  | Deterministic JSON canonicalization |
| P1 | React app shell (auth unlock, layout) |  | Tailwind baseline + dark mode tokens |
| P1 | Feed state slice + fake data hydration |  | Replace with real events once ready |
| P2 | Post composer hooking into core signer |  | Local optimistic add |
| P2 | X3DH prekey publish & retrieval |  | Simplified initial set (one-time prekeys) |
| P2 | DM send pipeline (plaintext envelope MVP) |  | Encryption toggle for debugging |
| P3 | IndexedDB abstraction + RocksDB parity |  | Unified trait compliance tests |
| P3 | Attachment placeholder referencing torrent hash |  | No upload yet |
| P3 | Outbox queue + retry backoff |  | Persist across reloads |

Deliverables after 6 weeks: Web MVP shows profile + feed + can create signed post + send basic DM (encrypted pipeline stubbed if necessary), all backed by Rust core & WASM.