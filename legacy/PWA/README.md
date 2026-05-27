# 🌐 SnartNet PWA

> Legacy archive: this web client is no longer an active development target. It is preserved for migration reference while native clients are being implemented.

SnartNet is a decentralized social media protocol and client, designed around torrent swarms, cryptographic identity, and trust-based recovery.  
This directory contains the archived **Progressive Web App (PWA)** client implementation.

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
