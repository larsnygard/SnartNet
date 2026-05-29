# SnartNet

This is a decentralized social + messaging protocol and application suite in the conceptual / early implementation stage.

**Verified, signed, and decentralized social media and messaging.**

[![License: AGPL v3](https://img.shields.io/badge/License-AGPLv3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Status: Experimental](https://img.shields.io/badge/Status-Experimental-red.svg)](#)
[![Protocol Version](https://img.shields.io/badge/Protocol-Draft_0.1-blue.svg)](#)

SnartNet is a revolutionary decentralized social media protocol that gives users complete control over their identity, data, and social connections. Built on BitTorrent-like swarm technology and modern cryptography, it enables truly peer-to-peer social networking without reliance on centralized servers or corporate platforms.

> Current Focus (2026): Native-first clients on desktop and mobile backed by a clean shared Rust core. The legacy web client has been archived to `legacy/PWA` and is no longer an active development target.

## 🌟 Key Features

### 🔐 **Cryptographic Identity**
- **Ed25519** signatures for authentication and content integrity
- **Curve25519** key exchange for secure messaging
- **Hybrid encryption** (AES-256/ChaCha20) for performance and security
- **Forward secrecy** with ephemeral keys and ratcheting

### 🌐 **Decentralized Distribution**
- **Profile torrents** containing user identity and content
- **DHT-based discovery** for peer and content lookup
- **Swarm routing** inspired by BGP for scalable content propagation
- **Magnet URIs** for content-addressable references

### 🤝 **Ring of Trust**
- **Quorum-based recovery** for lost private keys
- **Trust levels** and reputation scoring
- **Verifiable credentials** for identity verification
- **Revocation certificates** for compromised accounts

### 💬 **Secure Messaging**
- **End-to-end encryption** for direct and group messages
- **Offline message delivery** via trusted relay nodes
- **Notification torrents** for mobile optimization
- **Forward secrecy** with X3DH and Double Ratchet protocols

### 📱 **Mobile Optimized**
- **Wake-on-swarm** functionality for battery efficiency
- **Selective synchronization** to minimize bandwidth
- **Push-like messaging** via lightweight notification torrents
- **Delta updates** to reduce redundant data transfer

## 🚀 Quick Start

> **Note:** SnartNet is currently in active development (early rename from PeerSocial). The implementation is not yet ready for production use.

### Installation

```bash
# Install from source (once available)
git clone https://github.com/larsnygard/SnartNet.git
cd SnartNet
cargo build --release

# Or download pre-built binaries (future)
curl -sSL https://github.com/larsnygard/SnartNet/releases/latest/download/snartnet-linux.tar.gz | tar xz
```

### Create Your Profile

```bash
# Initialize a new profile
ps init alice --email alice@example.com --name "Alice Smith"

# Add trusted contacts to your Ring of Trust
ps contacts add bob --trust-level 5
ps contacts add charlie --trust-level 4

# Publish your profile to the network
ps profile publish
```

### Start Networking

```bash
# Start the background daemon
ps daemon start

# Sync with the network
ps sync all

# Search for interesting content
ps search posts "decentralization"
ps search users "Alice"
```

### Create and Share Content

```bash
# Create a post
ps post create "Hello, decentralized world! #SnartNet #privacy"

# Share media
ps post create --attach photo.jpg "Beautiful sunset from my hike"

# Reply to others
ps post create --reply-to ABC123DEF456 "Great point about privacy!"
```

### Secure Messaging

```bash
# Send a direct message
ps message send alice "Hi there!"

# Create a group chat
ps message group create "Dev Team" alice bob charlie

# Send to the group
ps message group send DEV123 "Meeting in 10 minutes"
```

## 📚 Documentation

### Protocol Specifications
- **[RFC](./RFC)** - Formal protocol specification
- **[Implementation Roadmap](./docs/ROADMAP.md)** - Development phases and milestones
- **[CLI Specification](./specs/CLI.md)** - Complete command-line interface reference

### Architecture Documents
- **[Cryptographic Strategy](./docs/CRYPTOGRAPHY.md)** - Security model and implementations *(coming soon)*
- **[Network Protocol](./docs/NETWORK.md)** - Swarm routing and DHT operations *(coming soon)*
- **[Trust System](./docs/TRUST.md)** - Ring of Trust and verification mechanisms *(coming soon)*

### User Guides
- **[Getting Started](./docs/GETTING_STARTED.md)** - Comprehensive setup guide *(coming soon)*
- **[Mobile Usage](./docs/MOBILE.md)** - Mobile-specific features and optimization *(coming soon)*
- **[Security Best Practices](./docs/SECURITY.md)** - Protecting your identity and data *(coming soon)*

## 🏗️ Development Status

SnartNet is currently in **Phase 2 native implementation**: establishing a reusable, platform-neutral Rust core and bootstrapping native desktop/mobile hosts.

### ✅ Completed
- Protocol specification and RFC
- CLI command specification
- Implementation roadmap
- Project architecture design

### 🔄 In Progress (Core + Native GUI)
- [ ] Rust core crate (profiles, key mgmt, DHT hooks)
- [ ] Storage backend abstraction (native + wasm compatibility)
- [ ] Desktop GUI spike (Slint vs iced)
- [ ] Native host adapters (Windows/macOS/Android/iOS)
- [ ] Initial X3DH + Double Ratchet bootstrap
- [ ] Attachment reference model (torrent infohash placeholder)

### 📋 Upcoming (Revised Roadmap Highlights)
- Group messaging + ratcheting enhancements
- Discovery & hashtag indexing UI
- Mobile shells over shared Rust contracts
- Trust & verification UX (revocation, badges)
- Performance & offline sync optimization

See the **[complete roadmap](./docs/ROADMAP.md)** for detailed timelines and milestones.

## � DHT Push Updates (Experimental)

This section currently documents the archived web client behavior and should be treated as legacy reference while native transports are being implemented.

SnartNet includes an experimental real-time push notification system for head updates using libp2p gossipsub. This feature enables instant propagation of new posts without polling.

### How It Works

- When you create or edit a post, your client regenerates the post index head and publishes a signed head update event
- Other clients subscribed to the gossipsub topic (`snartnet.head.v1`) receive the update immediately
- The update contains the new head magnet URI, allowing peers to sync your latest posts instantly
- All updates are Ed25519 signed and verified to prevent spam and ensure authenticity

### Enabling libp2p Push

By default, SnartNet uses an in-memory transport for head updates (local testing only). To enable real network push:

**Option 1: Environment Variable (Recommended)**
```bash
# In legacy/PWA/.env.development or legacy/PWA/.env.local
VITE_ENABLE_LIBP2P=true
```

**Option 2: Runtime Flag**
```javascript
// In browser console before initialization
window.SNARTNET_ENABLE_LIBP2P = true
```

Then restart your development server (`npm run dev` in the `legacy/PWA` directory).

### Debugging Push Network

When libp2p is enabled, several debugging objects are exposed on `window`:

- `window.snartnetLibp2p` - The libp2p node instance
- `window.lastPublishedHeadUpdate` - Most recent head update you published
- `window.__sn_head_sig_cache` - Cache of received signature hashes (dedupe)
- `window.__sn_head_published_count` - Number of head updates you've published

The PWA includes a live status indicator in the bottom-right corner showing:
- Transport type (in-memory vs libp2p)
- Connected peer count
- Head updates received/published

### Bootstrap Peers (Optional)

To connect to other libp2p nodes, add bootstrap peer multiaddresses to localStorage:

```javascript
localStorage.setItem('snartnet:bootstrapPeers', JSON.stringify([
  '/ip4/192.168.1.100/tcp/4001/p2p/12D3KooW...',
  '/ip6/::1/tcp/4001/p2p/12D3KooW...'
]))
```

### Security & Rate Limiting

The push system includes built-in protections:
- **Signature verification**: All head updates must be properly signed
- **Deduplication**: Identical signatures are ignored
- **Rate limiting**: Max 30 updates per profile per minute
- **Timestamp validation**: Updates outside ±5 minute window are rejected

## �🛠️ Technology Stack

### Core Protocol
- **Language:** Rust (performance, safety, cross-platform)
- **Cryptography:** libsodium, Ed25519, Curve25519
- **Networking:** libtorrent-rasterbar, DHT, WebRTC, **libp2p gossipsub** (push updates)
- **Serialization:** JSON, Protocol Buffers

### Client Applications
- **Desktop:** Native Rust GUI track (Slint-first, iced fallback)
- **Mobile:** Thin native shells (Android/iOS) over shared Rust core contracts
- **CLI (Developer Tool):** Rust (maintenance & diagnostics)
- **Web:** Archived legacy client in `legacy/PWA` (reference only)

### Shared Core Strategy
- Single Rust core library compiled to native + WASM.
- Deterministic serialization + capability flags for forward compatibility.
- Event-driven state sync (observer pattern over async channels).
- Pluggable storage (RocksDB native / IndexedDB via wasm-bindgen).

## 🤝 Contributing

We welcome contributions from developers, security researchers, and privacy advocates! SnartNet is being built as a truly open-source, community-driven project.

### How to Contribute

1. **🐛 Report Issues** - Found a bug or have a feature request? [Open an issue](https://github.com/larsnygard/SnartNet/issues)
2. **📖 Improve Documentation** - Help make our docs clearer and more comprehensive
3. **🔧 Code Contributions** - Submit pull requests for bug fixes and new features
4. **🔍 Security Review** - Help audit our cryptographic implementations
5. **🌍 Translations** - Translate the interface to your language

### Development Setup

```bash
# Clone the repository
git clone https://github.com/larsnygard/SnartNet.git
cd SnartNet

# Install Rust and dependencies
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update

# Build the project
cargo build

# Run tests
cargo test

# Run the CLI
cargo run -- --help
```

### Android Shell (JNI host)

SnartNet now includes a minimal Android shell project in `android/` wired to the shared Rust core via JNI (`android-bridge` crate).

Build (Linux/macOS):

```bash
./build-android.sh
```

Build (Windows PowerShell):

```powershell
./build-android.ps1
```

See `android/README.md` for prerequisites and details.

### Contribution Guidelines

- Follow the [Rust style guide](https://doc.rust-lang.org/1.0.0/style/README.html)
- Add tests for new functionality
- Update documentation for API changes
- Ensure all tests pass before submitting PRs
- Sign commits with GPG for security contributions

## 🔒 Security

Security is paramount in SnartNet. We employ multiple layers of protection:

### Cryptographic Security
- **Ed25519** for digital signatures (state-of-the-art elliptic curve)
- **Curve25519** for key exchange (perfect forward secrecy)
- **AES-256** and **ChaCha20** for symmetric encryption
- **Argon2** for key derivation and password hashing

### Network Security
- All content is cryptographically signed and verifiable
- Messages use hybrid encryption with forward secrecy
- Metadata leakage minimized through encrypted headers
- Protection against Sybil, DDoS, and MITM attacks

### Reporting Security Issues

If you discover a security vulnerability, please:
1. **DO NOT** create a public issue
2. Email security@snartnet.org with details *(placeholder address until domain provisioning completed; use GPG if sensitive)*
3. Include steps to reproduce if possible
4. Allow 90 days for coordinated disclosure

We'll acknowledge receipt within 48 hours and provide updates on our investigation.

## 📜 License

SnartNet is released under the **GNU Affero General Public License v3.0 only (AGPL-3.0-only)**. See [LICENSE](./LICENSE) for details.

The protocol specification is released under **Creative Commons Attribution 4.0** to encourage implementation by other projects and ensure broad compatibility.

## 🎯 Project Goals

### Primary Objectives
1. **User Sovereignty** - Users own their identity, data, and social connections
2. **Privacy by Design** - End-to-end encryption and minimal metadata leakage
3. **Censorship Resistance** - No central authority can silence users
4. **Network Effects** - Incentivize participation and content sharing
5. **Mobile First** - Optimized for mobile devices and limited bandwidth

### Success Metrics
- **Adoption**: Active users and content creation volume
- **Performance**: Message latency <5s, profile sync <30s
- **Security**: Zero critical vulnerabilities, regular audits
- **Decentralization**: Geographic and organizational diversity of nodes

## 🌍 Roadmap & Vision

### Short Term (6 months)
- Core cryptographic + profile stack stable
- Web MVP (feed, profile, posting, basic DMs)
- WASM performance profiling & optimization loop
- Decision + prototype for mobile bridge
- Developer-facing CLI stabilized

### Medium Term (12 months)
- Beta mobile app (notifications, offline cache)
- Full messaging suite (groups, attachments, ratcheting)
- Discovery UI (hashtags, search facets)
- Trust / verification surfaces (badges, revocation alerts)
- 1,000+ active early adopters

### Long Term (18+ months)
- Production-ready multi-platform clients
- Ecosystem integrations (bridges, plugins, ActivityPub interop)
- Formal DID + credential issuance
- App store + self-hosted distribution channels
- Recognized open standard candidate

## 🙋‍♀️ FAQ

### **Q: How is this different from Mastodon/ActivityPub?**
A: While Mastodon federates servers, SnartNet is truly peer-to-peer with no servers required. Users directly connect and share content via torrent swarms, providing stronger censorship resistance and user control.

### **Q: What about blockchain/cryptocurrency integration?**
A: SnartNet focuses on communication, not monetization. While future versions may support optional cryptocurrency features, the core protocol remains blockchain-free for simplicity and efficiency.

### **Q: How do you handle illegal content or spam?**
A: Each user curates their own network through the Ring of Trust system. Communities can establish shared moderation standards, but there's no central authority to impose universal censorship.

### **Q: What's the mobile battery impact?**
A: Extensive optimizations including wake-on-swarm, notification torrents, and selective sync ensure battery usage comparable to traditional messaging apps.

### **Q: Is this ready for production use?**
A: No, SnartNet is currently experimental. We're in active development with alpha releases planned for early 2026.

## 📞 Community & Support

### Get Involved
- **💬 Discussions** - [GitHub Discussions](https://github.com/larsnygard/SnartNet/discussions)
- **📧 Mailing List** - dev@snartnet.org *(placeholder; to be provisioned)*
- **🐦 Updates** - [@SnartNet](https://twitter.com/SnartNet) *(placeholder handle)*
- **💼 Matrix Chat** - `#snartnet:matrix.org`

### Core Team
- **Lars Nygård** ([@larsnygard](https://github.com/larsnygard)) - Protocol Design & Implementation
- *Looking for contributors!* - Join us in building the future of social media

---

**"Own your identity. Control your data. Connect directly."**

*SnartNet (formerly PeerSocial) is an experimental protocol. Use at your own risk and always maintain backups of important data.*
