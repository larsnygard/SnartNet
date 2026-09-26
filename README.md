# SnartNet

A native, peer-to-peer space for your people. Create an identity, exchange invitations, and have direct, encrypted conversations without a central account service.

SnartNet is experimental. The desktop and Android clients share the Rust client protocol and encrypted sync implementation. The older web client is archived in `legacy/PWA`.

## Run the desktop app

Install a current stable Rust toolchain, then:

```bash
git clone https://github.com/larsnygard/SnartNet.git
cd SnartNet
cargo run -p snartnet-desktop
```

On Linux, the app needs a graphical session and the platform libraries used by iced (X11/Wayland and a working graphics backend). `./build.sh` builds the entire native workspace. The CLI is available with `cargo run -p snartnet-cli -- --help`.

## Run the local daemon

The persistent daemon owns a native profile's storage and network sessions.
Start it with `cargo run -p snartnet-cli -- daemon start`, inspect it with
`cargo run -p snartnet-cli -- daemon status`, and stop it with `... daemon
stop`. It listens only on `127.0.0.1:47469`; its bearer token and runtime
metadata live in `$SNARTNET_HOME/runtime/` (0600 files on Unix).

The CLI now supports daemon administration only. Profile, post, and contact
workflows belong to frontends. Rust frontends can use `snartnet-sdk`; see the
[local API and SDK guide](docs/LOCAL_API.md) for types, compatibility, auto-start,
and reconnect behavior. Desktop migration is still M4: use separate data homes
for the current desktop and daemon during this transition.

## Run the terminal client

`snartnet-tui` is a pure client of the same daemon: it renders and requests, and
never opens the database, holds keys, or talks to peers itself.

```bash
cargo run -p snartnet-tui
# or attach to a specific daemon home
cargo run -p snartnet-tui -- --data-dir /tmp/snartnet-alice
```

It resolves the daemon home exactly like the CLI (`--data-dir`,
`SNARTNET_DATA_DIR`, then `SNARTNET_HOME`), so `cargo run -p snartnet-cli --
daemon status` describes the daemon the view is showing. Press `?` inside for the
full key map; see the [terminal client guide](docs/TUI.md) for workflows and
recovery behavior.

## Start a conversation

1. Open **My profile**, choose a username and display name, and save.
2. Choose **Copy invitation link** or **Save PNG**. The QR code contains the same invitation link. The invitation includes a shareable `snartnet://profile/...` identity URI and, after publication, a standard BitTorrent magnet for retrieving the signed profile torrent.
3. Your friend opens **Contacts → Invitation**, pastes your link, or imports the saved QR image. Compressed and older base64 invite codes still work.
4. Exchange invitations in both directions so each person has the other as a contact. On the same local network, **Contacts → Nearby** also works.
5. Open **Messages**. Once the signed profile and encryption key are available, type a message and press Enter or **Send**.

The desktop shows readable conversations while retaining ciphertext on disk. **View ciphertext** lets you inspect an individual encrypted payload. Drafts stay with their conversations while the app is open.

- **Queued**: saved locally; automatic sync will retry after a peer becomes reachable, including after an app restart.
- **Relayed**: at least one peer acknowledged storing the envelope. This is not a recipient delivery or read receipt.

## Connect across networks

Invitations include the detected local IP and actual listening port. For remote friends, set **My profile → Connection address** to a reachable IP and port, then save and share a fresh invitation. Profile, post, and message objects are also published to BitTorrent swarms and announced through the DHT, so peers can synchronize without a central server. A profile identity can be shared before its torrent is published; the app will show that the downloadable magnet is pending. A VPN or TCP port forwarding may still be required for the initial direct connection or when DHT bootstrap is unavailable.

| Setting | Default | Purpose |
| --- | --- | --- |
| `SNARTNET_BIND` | `0.0.0.0:47470` | TCP listen IP and port |
| `SNARTNET_PEERS` | empty | Additional comma-separated IP:port endpoints; bracket IPv6 addresses |
| `SNARTNET_HOME` | `~/.snartnet` | Isolated root for `data/` and `swarm/` |
| `SNARTNET_DHT_BOOTSTRAP` | library defaults | Comma-separated replacement bootstrap nodes for signed DHT discovery |
| `SNARTNET_DHT_EXTRA_BOOTSTRAP` | empty | Comma-separated additional signed DHT bootstrap nodes |
| `SNARTNET_TORRENT_BOOTSTRAP` | library defaults | Comma-separated replacement BitTorrent DHT bootstrap nodes |
| `SNARTNET_IROH_DISCOVERY` | `dns` | Publish and resolve contact device endpoints through Iroh's n0 DNS/Pkarr services; `off` keeps direct addresses only |
| `SNARTNET_IROH_RELAY` | n0 production | Relay policy: unset or `prod` uses n0's production relays, `staging` uses their test relays, `off` disables relaying entirely |
| `SNARTNET_RELAY_URLS` | empty | Comma-separated relay URLs this deployment runs or trusts; they outrank every other source |
| `SNARTNET_RELAY_TOKEN` | empty | Bearer token for those relays; it is sealed into an encrypted grant when a contact is referred, never published |
| `SNARTNET_COMMUNITY_RELAYS` | empty | Comma-separated relays of a community this deployment joined; used when no configured relay or referral applies |

`SNARTNET_BIND` is the only port you configure. Its owner derives the rest, and no
derived port is a fixed second choice: LAN discovery keeps UDP `47471`, the torrent
session listens on `base + 3` (TCP and uTP), and the Mainline DHT uses `base + 4`
(UDP). A derived port that is already taken moves to an OS-assigned port, and a `:0`
bind (tests, embedded hosts) owns neither auxiliary socket.

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

The active delivery work is tracked in the [implementation plan](docs/IMPLEMENTATION_PLAN.md). Architectural decisions are recorded in [`docs/adr/`](docs/adr/).

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
| `cli/` | Daemon administration CLI |
| `tui/src/main.rs`, `tui/src/daemon.rs` | Terminal event loop, jobs, and the SDK-backed daemon client |
| `tui/src/app.rs`, `state.rs`, `input.rs`, `ui.rs` | Reducer, snapshot view models, key map, Ratatui rendering |
| `sdk/` | Shared local API contract and Rust frontend client |
| `daemon/` | Persistent backend and authenticated loopback API |
| `client/src/device.rs`, `peer.rs` | Per-device Iroh identity, device certificates, and the authenticated `snartnet/peer/1` contact protocol |
| `android/`, `android-bridge/`, `client/` | Android client, JNI adapter, and shared native client session; see [Android setup](android/README.md) |
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

Tests use isolated temporary directories and real loopback TCP. They cover signature/identity binding, invitation limits, QR round trips, two-client encrypted conversations, concurrent inbox merging, offline outbox recovery, draft isolation, unread counts, and failed persistence. The shared client suite also covers per-device Iroh identities and device certificates (expiry, tampering, endpoint mismatch, replay, and renewal) and drives two live `snartnet/peer/1` endpoints through a real handshake, object exchange, and refused-stranger case. The terminal client suite covers the reducer, the key map, every tab's rendering (down to a few terminal cells), and API failures, and two of its tests drive a real daemon over the local API.

To generate disposable sample data for a visual review, use a fresh directory:

```bash
SNARTNET_REVIEW_DIR=/tmp/snartnet-review cargo test -p snartnet-desktop create_visual_review_fixture -- --ignored
SNARTNET_HOME=/tmp/snartnet-review cargo run -p snartnet-desktop
```

## Current scope

The desktop implements Ed25519 signed identities and X25519 + ChaCha20-Poly1305 direct messages. Signatures are checked against the contact's public-key fingerprint before trusting profile keys or displaying incoming messages. Sync uses BitTorrent objects with signed DHT descriptors and retains bounded TCP as a fallback. It runs outside the UI thread.

Contacts are also reached over Iroh (ADR 0003). Each device holds its own Iroh key, separate from the profile signing key, and proves which profile it belongs to with a profile-signed device certificate that names the endpoint id, capabilities, and expiry. The certificate is validated against the endpoint id Iroh's TLS handshake already proved before a single application frame is read; a stranger is closed immediately, a replayed older certificate is refused against the pin kept from the newest one accepted, and `hello`/`hello_ack` nonces stop a recorded handshake from being replayed onto a new connection. Since M6 there is no global topic: presence and post/profile notices go only to contacts whose device endpoint is already known, and a queued message that plain TCP/BitTorrent could not relay is delivered as a signed object over the same authenticated channel. Address lookup uses Iroh's n0 DNS/Pkarr services by default (`SNARTNET_IROH_DISCOVERY=off` disables it) with a signed DHT device descriptor as the fallback.

Relay selection is local policy (ADR 0006). Four sources are tried in order — relays you configure (`SNARTNET_RELAY_URLS`), relays a verified contact referred you to with a signed and expiring referral, a community list you opted into (`SNARTNET_COMMUNITY_RELAYS`), and finally Iroh's own production relays — and the first usable one decides which relays the endpoint offers. A referral names a relay; a relay's bearer token travels only as an encrypted grant addressed to your own key, so sharing a private relay does not publish its secret. Each device scores the relays it uses from its own connection status, so a relay that keeps failing moves behind one that works and is eventually dropped, while Iroh's own relays remain the floor. Only `SNARTNET_IROH_RELAY=off` disables relaying; direct connections are preferred in every case.

Private keys are stored locally and are not password-encrypted. Back up the complete `data/` directory securely. Messages use static X25519 keys; forward secrecy, Double Ratchet, group chat, attachments, and key recovery remain future work. The shared low-level service also exposes legacy signed plaintext messages; the desktop chat always encrypts new messages.

Torrent and DHT exchange is best-effort and uses direct peer connections; there is no SnartNet-operated relay. IPv6 and UPnP port mapping are enabled when available. DHT records contain routing metadata only; downloaded objects and message envelopes are verified before use. If both peers are behind unreachable NAT, use IPv6, VPN, or port forwarding. The implementation is intended for experimentation and does not provide anonymity or forward secrecy yet.

Since M7 the durable copy comes first: an outbound message is published as a signed torrent object with a DHT mailbox pointer before it is pushed to a peer, and only a publication that *failed* holds the message back (the UI then shows the state and the reason, e.g. `queued (disk is full)`). A host with no durable transport at all — the swarm switched off, or an ephemeral bind — says so and delivers over the direct paths only. Inbound objects that arrive over the authenticated peer channel are written to the local store before they are acknowledged, and the acknowledgement count is what the sender treats as "stored", so a redelivery after a crash cannot be lost or double-counted. Both paths are attempted for every message and arrivals are deduplicated by signed object ID.

Licensed under [AGPL-3.0-only](LICENSE).
