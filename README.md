# SnartNet

A native, peer-to-peer space for your people. Create an identity, exchange invitations, and have direct, encrypted conversations without a central account service.

SnartNet is experimental. The active client is the **Rust + iced desktop app**. Android is a small JNI shell; the older web client is archived in `legacy/PWA`.

## Run the desktop app

Install a current stable Rust toolchain, then:

```bash
git clone https://github.com/larsnygard/SnartNet.git
cd SnartNet
cargo run -p snartnet-desktop
```

On Linux, the app needs a graphical session and the platform libraries used by iced (X11/Wayland and a working graphics backend). `./build.sh` builds the entire native workspace. The CLI is available with `cargo run -p snartnet-cli -- --help`.

## Start a conversation

1. Open **My profile**, choose a username and display name, and save.
2. Choose **Copy invitation link** or **Save PNG**. The QR code contains the same invitation link.
3. Your friend opens **Contacts → Invitation**, pastes your link, or imports the saved QR image. Compressed and older base64 invite codes still work.
4. Exchange invitations in both directions so each person has the other as a contact. On the same local network, **Contacts → Nearby** also works.
5. Open **Messages**. Once the signed profile and encryption key are available, type a message and press Enter or **Send**.

The desktop shows readable conversations while retaining ciphertext on disk. **View ciphertext** lets you inspect an individual encrypted payload. Drafts stay with their conversations while the app is open.

- **Queued**: saved locally; automatic sync will retry after a peer becomes reachable, including after an app restart.
- **Relayed**: at least one peer acknowledged storing the envelope. This is not a recipient delivery or read receipt.

## Connect across networks

Invitations include the detected local IP and actual listening port. For remote friends, set **My profile → Connection address** to a reachable IP and port, then save and share a fresh invitation. A VPN or TCP port forwarding may be required. Automatic NAT traversal and hosted offline relays are not implemented.

| Setting | Default | Purpose |
| --- | --- | --- |
| `SNARTNET_BIND` | `0.0.0.0:47470` | TCP listen IP and port |
| `SNARTNET_PEERS` | empty | Additional comma-separated IP:port endpoints; bracket IPv6 addresses |
| `SNARTNET_HOME` | `~/.snartnet` | Isolated root for `data/` and `swarm/` |

For example, run a separate test identity without touching your normal data:

```bash
SNARTNET_HOME=/tmp/snartnet-alice SNARTNET_BIND=127.0.0.1:47570 cargo run -p snartnet-desktop
```

An invitation can also be passed as a command-line argument:

```bash
cargo run -p snartnet-desktop -- 'snartnet://invite/PASTE_INVITE_PAYLOAD_HERE'
```

This opens the import screen. Automatic operating-system registration of the URL scheme is not included; links can always be pasted into the app.

See [Invitations and connectivity](docs/QR_AND_LAN_DISCOVERY.md) for the full workflow.

## Repository structure

| Path | Responsibility |
| --- | --- |
| `core/` | Signed identities, cryptography, invitations, posts, messages, storage, shared service API |
| `desktop/src/main.rs` | Application events and persistence coordination |
| `desktop/src/model.rs` | Persisted records and transient form state |
| `desktop/src/views.rs`, `design.rs` | Native screens, palette, reusable components |
| `desktop/src/actions.rs`, `media.rs` | Validated creation/import, avatars, QR images |
| `desktop/src/sync.rs` | Background exchange and UI delta application |
| `desktop/src/transport.rs`, `discovery.rs` | TCP cache exchange and optional UDP presence |
| `desktop/src/tests.rs` | Chat, persistence, QR, and loopback integration regressions |
| `cli/` | Developer CLI |
| `android/`, `android-bridge/` | Android shell and JNI adapter; see [Android setup](android/README.md) |
| `legacy/PWA/` | Archived web reference with its own npm manifest |
| `RFC`, `specs/`, `docs/ROADMAP.md` | Protocol proposals and future work, not a list of shipped features |

Runtime databases and invitation exports are ignored by Git. Existing local files are preserved.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Tests use isolated temporary directories and real loopback TCP. They cover signature/identity binding, invitation limits, QR round trips, two-client encrypted conversations, concurrent inbox merging, offline outbox recovery, draft isolation, unread counts, and failed persistence.

To generate disposable sample data for a visual review, use a fresh directory:

```bash
SNARTNET_REVIEW_DIR=/tmp/snartnet-review cargo test -p snartnet-desktop create_visual_review_fixture -- --ignored
SNARTNET_HOME=/tmp/snartnet-review cargo run -p snartnet-desktop
```

## Current scope

The desktop implements Ed25519 signed identities and X25519 + ChaCha20-Poly1305 direct messages. Signatures are checked against the contact's public-key fingerprint before trusting profile keys or displaying incoming messages. Sync uses bounded TCP requests, atomic cache replacement, and merge-based inbox updates. It runs outside the UI thread.

Private keys are stored locally and are not password-encrypted. Back up the complete `data/` directory securely. Messages use static X25519 keys; forward secrecy, Double Ratchet, group chat, attachments, and key recovery remain future work. The shared low-level service and CLI also expose legacy signed plaintext messages; the desktop chat always encrypts new messages.

“Swarm” and magnet fields remain for protocol compatibility. The current native transport is direct TCP, not BitTorrent or DHT. It is intended for experimentation with known peers, not deployment as an unrestricted public relay.

Licensed under [AGPL-3.0-only](LICENSE).
