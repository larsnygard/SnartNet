# SnartNet implementation plan

This is the delivery ledger for the native daemon, desktop, terminal UI, Iroh
connectivity, and distributed replication work. It is the source of truth for
implementation progress; `docs/ROADMAP.md` remains the higher-level product
roadmap.

## Status

- **Status:** Active
- **Current milestone:** M10 — Android and power-aware operation
- **Last updated:** 2026-09-26
- **Last completed:** M9.1–M9.5 — contact replication with encrypted leases and signed
  receipts (M8 relays and M7 durable delivery before it; M4.3 tray: Linux only)
- **Known blockers:** None

## Working agreement

- A task is checked only once its implementation, tests, and required
  documentation are merged into `main`.
- Partial work stays unchecked and receives a short note in the progress log.
- Every implementation change updates this file if it completes or materially
  changes a task.
- Architecture changes require an ADR in `docs/adr/`.
- Each milestone completion receives a dated progress-log entry.

## M0 — Plan tracking and baseline

- [x] **M0.1** Add this implementation ledger.
- [x] **M0.2** Link the ledger from the README and product roadmap.
- [x] **M0.3** Record ADRs for daemon ownership, transport roles, per-device
  Iroh identities, and the local API.
- [x] **M0.4** Define the milestone completion and progress-update rules.
- [x] **M0.5** Validate and format the v0.3.3 baseline.

## M1 — Canonical indexed storage

- [x] **M1.1** Define the SQLite schema and migration framework.
- [x] **M1.2** Index immutable objects by signed ID, creation time, ingestion
  sequence, type, owner, and torrent descriptor.
- [x] **M1.3** Make the backend the exclusive writer of identity records.
- [x] **M1.4** Implement atomic transactions and migration rollback.
- [x] **M1.5** Import and verify v0.3.3 JSON and `client_state.json` data.
- [x] **M1.6** Back up imported data and reject conflicting identities.
- [x] **M1.7** Test migrations, deduplication, cursors, corruption, and clock
  skew.

## M2 — Persistent local daemon

- [x] **M2.1** Move storage and network ownership into one backend service.
- [x] **M2.2** Implement daemon locking and runtime metadata.
- [x] **M2.3** Add `snartnet daemon run/start/status/stop`.
- [x] **M2.4** Add authenticated loopback HTTP and health/snapshot endpoints.
- [x] **M2.5** Add typed commands, manual sync, and SSE state events.
- [x] **M2.6** Add API revisions and reconnect recovery.
- [x] **M2.7** Implement Always-on, Balanced, and Paused sync modes.
- [x] **M2.8** Add lifecycle, authentication, and concurrent-client tests.

## M3 — Shared frontend SDK

- [x] **M3.1** Define the versioned API types and Rust client.
- [x] **M3.2** Add authentication, retry, SSE reconnect, and auto-start.
- [x] **M3.3** Restrict `snartnet` to daemon administration.
- [x] **M3.4** Add API compatibility checks and client integration tests.

## M4 — Desktop migration and tray

- [x] **M4.1** Move desktop state and actions to the shared API client.
- [x] **M4.2** Preserve avatars, QR workflows, drafts, and ciphertext views.
- [ ] **M4.3** Add tray controls without stopping the daemon on window close.
- [x] **M4.4** Port desktop tests to daemon-backed behavior.

*M4.3 is Linux-only so far: the freedesktop StatusNotifierItem tray runs on its
own thread through `ksni`. macOS and Windows need tray work on the platform
event loop that iced owns on the main thread; those targets currently compile
with the tray disabled.*

## M5 — `snartnet-tui`

- [x] **M5.1** Add the Ratatui/Crossterm workspace crate and daemon client.
- [x] **M5.2** Implement Messages, Contacts, Feed, Profile, and Network tabs.
- [x] **M5.3** Implement profile, contact, post, message, and sync workflows.
- [x] **M5.4** Add text invitation export, responsive layout, help, and recovery.
- [x] **M5.5** Test reducers, rendering, narrow terminals, and API failures.

## M6 — Iroh device identity and peer protocol

- [x] **M6.1** Generate a separate Iroh identity per device.
- [x] **M6.2** Add profile-signed device certificates and validation.
- [x] **M6.3** Define and implement the `snartnet/peer/1` framed ALPN.
- [x] **M6.4** Replace global gossip with authenticated contact-scoped updates.
- [x] **M6.5** Add DNS/Pkarr lookup and optional DHT lookup.
- [x] **M6.6** Test replay, malformed input, endpoint mismatch, and expiry.

## M7 — Durable BitTorrent plus realtime Iroh delivery

- [x] **M7.1** Persist and publish an object before realtime delivery.
- [x] **M7.2** Deliver the same encrypted message over Iroh when online.
- [x] **M7.3** Persist inbound Iroh objects before acknowledgement.
- [x] **M7.4** Deduplicate torrent and Iroh arrivals by object ID.
- [x] **M7.5** Add accurate queued, available, replica-stored, and received states.
- [x] **M7.6** Test online, offline, retry, restart, and dual-path delivery.

## M8 — Transparent relay selection

- [x] **M8.1** Use production Iroh relay configuration by default.
- [x] **M8.2** Add configured, trusted-referral, community, and n0 fallback sources.
- [x] **M8.3** Define signed, expiring relay referrals and encrypted grants.
- [x] **M8.4** Score local relay health and update the active map safely.
- [x] **M8.5** Test direct paths, relays, failover, bad referrals, and outages.

## M9 — Contact replication and storage policy

- [x] **M9.1** Implement policy precedence and desktop/mobile defaults.
- [x] **M9.2** Add encrypted replica leases and signed storage receipts.
- [x] **M9.3** Replicate approved profiles, feeds, and opaque mailbox objects.
- [x] **M9.4** Add expiry, storage caps, eviction, and low-disk protection.
- [x] **M9.5** Add Storage & availability settings and policy tests.

## M10 — Android and power-aware operation

- [ ] **M10.1** Route Android through the shared backend service.
- [ ] **M10.2** Integrate Android foreground/background and battery-saver state.
- [ ] **M10.3** Enforce mobile storage defaults and test lifecycle recovery.

## M11 — Hardening and release readiness

- [ ] **M11.1** Add resource limits, backoff, and bounded queues.
- [ ] **M11.2** Threat-model API, device, relay, replica, and migration flows.
- [ ] **M11.3** Add redacted diagnostics and clean-install/migration testing.
- [ ] **M11.4** Run the complete Linux/macOS/Windows/Android release matrix.
- [ ] **M11.5** Update protocol, setup, troubleshooting, and release documents.

## Deferred security programs

- [ ] **F1** Multi-device enrollment and encrypted authority transfer.
- [ ] **F2** Selective full-profile restoration and retention negotiation.
- [ ] **F3** Friend-backup quorum and availability accounting.
- [ ] **F4** Circle-of-trust membership, revocation, and Sybil resistance.
- [ ] **F5** Threshold recovery-key generation, rotation, and reconstruction.
- [ ] **F6** Group messaging and group-specific replication policy.

## Progress log

### 2026-09-23

- Completed: M0.1–M0.5.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace` pass after baseline formatting.
- Decisions: daemon ownership, BitTorrent/Iroh separation, per-device Iroh identities, and the local API are captured in ADRs 0001–0004.
- Next: M1.1 — define the SQLite schema and migration framework.
- Blockers: none.

### 2026-09-24

- Completed: M1.1–M1.7.
- Delivered: a versioned SQLite canonical store with immutable object indexing,
  resumable cursors, transactional writes, v0.3.3 JSON import, conflict
  rejection, and legacy-file backups. `Session` now uses this store as its
  authoritative state writer; legacy JSON is a best-effort compatibility mirror.
- Validation: focused repository and session tests cover migration, backup,
  conflict rollback, deduplication, torrent descriptors, corrupt records, and
  signed-clock skew.
- Next: M2.1 — move storage and network ownership into one backend service.
- Blockers: none.

### 2026-09-25 — M2

- Completed: M2.1–M2.8.
- Delivered: `snartnet-daemon`, the exclusive native `Session` owner, with a
  private runtime lock/token/metadata record, an authenticated loopback JSON
  API, revisioned SSE events, manual and scheduled sync, and selectable
  Always-on, Balanced, and Paused modes. The CLI now supports daemon run,
  start, status, and stop administration.
- Validation: daemon unit tests cover exclusive ownership and stable private
  authentication tokens; full-workspace verification is recorded under M3.
- Next: M3.1 — define the versioned API types and Rust client.
- Blockers: none.

### 2026-09-25 — M3

- Completed: M3.1–M3.4.
- Delivered: `snartnet-sdk` pins the version 1 loopback contract in
  `sdk/src/types.rs` (`API_VERSION`, runtime metadata, health, snapshot, tagged
  commands, command/sync/stop responses, and SSE state events) and provides the
  blocking `Client`, `DaemonPaths`, and `Subscription` API. The client rereads
  runtime metadata and the bearer token for each request, rejects non-loopback or
  port-0 metadata, disables proxies and redirects, retries idempotent GETs three
  times with 100/200 ms backoff, never replays writes, refreshes a full snapshot
  after every SSE reconnect, and auto-starts the daemon through
  `ensure_running(executable)` with bounded readiness polling.
- Delivered: `snartnet` is daemon administration only (`daemon
  run/start/status/stop`); `specs/CLI.md` is annotated as a historical proposal,
  and the README documents the daemon-only CLI, the SDK, and the daemon's
  loopback endpoint (`127.0.0.1:47469`).
- Fixed: the flaky `snartnet-client` lib test
  `session::tests::canonical_commit_survives_a_broken_legacy_mirror_and_keeps_new_contacts`,
  which failed once in a pre-fix workspace run with `actor thread unexpectedly
  shutdown: "SendError(..)"` (`mainline-8.0.0/src/dht.rs:143`). All three
  `session::tests` reach `Session::start_distributed`, which asks for the shared
  UDP `47473`, and `Session::open` derived `bind.port() + 2` – port `2` for the
  tests' OS-assigned bind port. mainline 8.0 answers the startup `Check` only if
  that message already reached its actor thread, so a taken port panicked the
  caller instead of returning the bind error. `DhtNode::open` now probes the
  port, lets mainline choose its own when the port is unavailable, and converts
  the remaining race into a startup error;
  `dht::tests::a_port_that_is_already_taken_does_not_abort_startup` covers the
  path, and the client-lib suite passes twelve consecutive full runs.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo build --workspace`, and three post-fix
  `cargo test --workspace` runs pass. SDK integration tests cover the bearer
  header, GET retry and no-write-replay behavior, SSE EOF and revision-reset
  recovery, non-loopback metadata rejection, unknown-command rejection, no spawn
  for a healthy daemon, and auto-start readiness; the CLI test asserts that
  non-daemon subcommands are rejected.
- Follow-up found during validation (not yet scheduled, pre-existing): the
  auxiliary torrent/DHT ports collide by construction. `Session::open` and
  `TcpSwarmTransport::new` both bind torrent on `bind.port() + 1` and DHT on
  `bind.port() + 2`, LAN discovery binds reusable UDP `47471` (the same port as
  torrent for the default bind), and `Session::start_distributed` falls back to
  the fixed `47472`/`47473` for any bind address. Every failure is swallowed by
  `.ok()`/`is_none()` (and the DHT now moves to another port instead of
  aborting), so the second binder – for example the transport torrent when a
  `Session` already owns `47471` – silently ends up with a different or missing
  node. The daemon inherits this from the Android bridge, which passes the same
  `:47470` bind. One owner for auxiliary ports should be decided before the M7
  network work.
- Next: M4.1 — move desktop state and actions to the shared API client.
- Blockers: none.

### 2026-09-25 — M4

- Completed: M4.1, M4.2, and M4.4. M4.3 is partial (Linux only) and stays
  unchecked; see the note below.
- Delivered: the desktop window is now a pure daemon-backed frontend per ADR
  0001. `desktop/src/backend.rs` wraps `snartnet_sdk::Client` behind an explicit
  `DaemonPaths` seam, `desktop/src/state.rs` turns one snapshot into the view
  models the window renders, and the old `transport`, `sync`, `discovery`,
  `gossip`, and `actions` modules are gone. `snartnet-client` is no longer a
  desktop dependency, so this process owns no identity, database, torrent
  session, or peer listener and a window close can never stop background work.
  Avatars, QR generation/scanning, per-conversation drafts, the ciphertext view,
  and the identity link all still render, now sourced from daemon data; the
  ciphertext toggle appears only for messages the daemon reports as encrypted.
- Delivered: the daemon reports the scheduler-owned sync mode in
  `extra["syncMode"]` alongside the session `paused` flag, so a frontend shows
  the mode it is actually in; `snartnet_sdk` gains `SyncMode::default()`
  (`Balanced`), the Network panel offers Always on / Balanced / Paused, and a
  paused mode now propagates into the session through `Command::Pause` instead
  of only stopping the scheduler. The loader explains an unreachable daemon and
  offers an explicit start instead of only promising a button, and subsystem
  errors (DHT, torrent, gossip) are rendered from snapshot extras.
- Delivered: a Linux StatusNotifierItem tray (`desktop/src/tray.rs`, `ksni`)
  with Show, Quit (daemon keeps running), and Stop the daemon and quit. Its
  tooltip mirrors the same status line the window shows, and closing the window
  hides it rather than exiting.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` pass. The desktop suite is
  seventeen tests rewritten around daemon behavior, including one that starts a
  real daemon (`snartnet_daemon::run_with`), drives onboarding, posts, chat,
  invites, pausing, and shutdown through the SDK, and asserts the window renders
  every panel before and after the daemon has answered. The daemon suite asserts
  that pausing and resuming are both visible in the snapshot as `paused` plus
  `syncMode`.
- M4.3 partial: the tray is Linux-only. macOS and Windows need tray items on the
  platform event loop that iced owns on the main thread; those targets compile
  with the tray disabled and the work stays tracked under M4.3.
- Next: M5.1 — add the Ratatui/Crossterm workspace crate and daemon client.
- Blockers: none.

### 2026-09-25 — M5

- Completed: M5.1–M5.5. `snartnet-tui` is a daemon-backed terminal client per ADR
  0001; nothing in the crate opens the database, holds keys, or talks to peers.
- Delivered: the new workspace crate renders with Ratatui 0.29 and Crossterm 0.28
  and depends only on `snartnet-sdk` (plus `snartnet-core` for types and
  `snartnet-daemon` in tests). `tui/src/daemon.rs` is the only seam to the daemon
  and wraps `snartnet_sdk::Client` behind an explicit `DaemonPaths`, so its
  `--data-dir`/`SNARTNET_DATA_DIR`/`SNARTNET_HOME` resolution matches the CLI.
  `tui/src/state.rs` turns one snapshot into the view models the terminal draws,
  and `tui/src/app.rs` is a pure reducer: one `Message` in, at most one `Action`
  out, executed only by the worker in `main`. `tui/src/ui.rs` renders Messages,
  Contacts, Feed, Profile, and Network, plus a status line, per-tab hint bar, and
  a help overlay, all from daemon data alone.
- Delivered: profile creation and edits, contact import by invite/magnet/
  fingerprint, posting, sending, opening a conversation (which marks it read),
  manual sync, sync-mode cycling, discovery toggling, cache cleanup, and
  daemon-generated invitation links land as typed `Command`s. Drafts live per
  conversation and a rejected command keeps the user's text; the status line
  shows the daemon's own message, and a stopped daemon is explained instead of
  rendered as empty state. `q`/`Ctrl+C` end the view only, and `D`/`S` are the
  explicit start/stop paths (stop asks for confirmation). Invitation URIs stay
  selectable text in the Profile tab, so a key never leaves the daemon to be
  exported.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` pass. The terminal suite is nine
  tests: reducer one-in-one-out, the key map (a focused field never loses a typed
  character to a shortcut), every tab rendering from daemon state including
  narrow terminals down to `4x3`, an unreadable snapshot entry showing the
  daemon's error, and two integration tests that start a real daemon with
  `snartnet_daemon::run_with` on an OS-assigned port in a temporary home and run
  the same action-to-call mapping `main` uses.
- Documented: `docs/TUI.md` covers running the view, the key map, each workflow,
  recovery, and what the client never does; the README links it and lists the
  crate, and `docs/LOCAL_API.md` now points at the shipped terminal frontend.
- Next: M6.1 — generate a separate Iroh identity per device.
- Blockers: none.

### 2026-09-26 — M6

- Completed: M6.1–M6.6. The unsigned global gossip topic is gone; contacts are
  reached over per-device Iroh endpoints that must present a certificate before a
  single application frame is read.
- Delivered: `client/src/device.rs` owns the per-device identity. `DeviceKey` is a
  separate Ed25519 secret generated on first start, persisted through
  `Repository::save_device_key`/`device_key` in the canonical `identity_records`
  table, and never written to a snapshot, the legacy JSON mirror, or a frontend.
  `DeviceCertificate::issue` signs the endpoint id, capabilities, and lifetime
  with the profile key, and `verify_at`/`verify_for_profile`/
  `verify_with_capability` re-derive the fingerprint from `profile_key` (one
  shared `snartnet_core::fingerprint_from_public_key_bytes`) rather than trusting
  the claimed one.
- Delivered: `client/src/peer.rs` implements `snartnet/peer/1` on its own Iroh
  endpoint: a 4-byte big-endian length prefix plus internally tagged JSON frames
  (`hello`, `hello_ack`, `object`, `notice`, `ack`, `goodbye`), bounded at 1 MiB
  with a refused zero-length frame. The handshake exchanges certificates and a
  fresh 16-byte nonce that must be echoed, and the dialer checks both that the
  endpoint it reached is the one it asked for and that the ack belongs to this
  connection. The accept side validates the certificate against the TLS-proven
  remote endpoint id, refuses strangers with `CLOSE_UNKNOWN_CONTACT` before any
  frame is read, and refuses a replayed older certificate with
  `CLOSE_STALE_CERTIFICATE` using the pin persisted from the newest accepted
  `issued_at`. `Ack { frames }` is written only after every frame is queued, and
  the handler lingers until the sender hangs up because Iroh drops a connection
  when its accept handler returns — a bug the live two-endpoint test caught.
- Delivered: the session dials only contacts whose device endpoint it has already
  pinned (`peer_targets`), announces profile and post changes plus presence as
  contact-scoped notices, and falls back to the authenticated peer channel for a
  queued message that plain TCP/BitTorrent could not relay. Ingested objects are
  re-verified against the contact's key, signature, and recipient, and accepted
  certificates are committed as pins so a crash cannot reopen a replay window.
- Delivered: M6.5 is `SNARTNET_IROH_DISCOVERY` (default `dns` publishes to and
  resolves through Iroh's n0 DNS/Pkarr services; `off` leaves only direct
  addresses) and `SNARTNET_IROH_RELAY` (default `staging`), plus a
  profile-signed `DeviceDescriptor` under the `snartnet/device` DHT namespace
  whose target is only produced when the embedded certificate verifies for the
  profile that was asked about.
- Delivered: twenty-six `device.rs` tests and twenty `peer.rs` tests cover
  framing round trips, empty/oversize/malformed frames and truncation, unknown
  contacts, endpoint mismatch, certificate replay and renewal, pin survival
  across a policy refresh, descriptor tampering and capability gating, nonce
  mismatch, a forged notice, and two live handshakes between local endpoints that
  assert both the ack count and the persisted pins.
- Delivered: the daemon and both frontends now read a `peers` snapshot key
  (`active`, `node_id`, `peer_count`, `discovery`, `last_error`) instead of
  `gossip`; `client/src/gossip.rs`, the `iroh-gossip` dependency, and the
  `Session::gossip` field are removed.
- Documented: ADR 0003 gained the implementation record for the key split,
  handshake, refusal rules, pinning, renewal window, lookups, and the removal of
  the topic. The README lists the two new environment variables.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` pass (64 client, 42 core, 3
  daemon, 17 desktop, 10 integration, 9 terminal tests).
- Next: M7.1 — persist and publish an object before realtime delivery.
- Blockers: none.

### 2026-09-26 — M7

- Completed: M7.1–M7.6. Publication is now a precondition of delivery rather than a
  side effect, inbound peer objects are stored before they are acknowledged, and the
  five delivery states are the ones delivery can actually reach.
- Delivered: `client/src/ports.rs` is the single derivation point for the auxiliary
  sockets. The peer bind owns torrent on `base + 3` (skipping LAN discovery's
  `47471`) and DHT on `base + 4`; a derived port that is taken moves to an
  OS-assigned port instead of a fixed second choice, and an ephemeral bind owns no
  auxiliary socket at all. `TcpSwarmTransport` opens both nodes for a concrete bind
  and exposes them through `torrent()`/`dht()`, so the second binder that used to
  silently end up with a different or missing node is gone; `Session` no longer opens
  a second torrent or DHT node and no longer binds `47472`/`47473` as a fallback.
- Delivered: `client/src/delivery.rs` makes the durability rule explicit.
  `PublishOutcome` is `Stored`, `Unsupported`, or `Failed`, and only `Failed` blocks
  direct delivery. `SwarmStore` is the production `DurableStore`, held by the session
  behind a trait object so a failing store can be tested against the real rules.
  A publication failure is recorded on the message (`delivery_error`), which keeps it
  out of the push list and visible in the UI, so the old silent downgrade of a failed
  publish to "relayed" is impossible.
- Delivered: `Session::sync_once` is the one sync round the daemon and the Android
  bridge both drive (publish, ingest, then both direct push paths). `sync::exchange`
  no longer publishes: it pushes, reports per-message `DeliveryPaths` for the torrent
  and iroh paths, and returns them so `apply_sync` can record the strongest state.
- Delivered: M7.3/M7.4. `repository::InboundSpool` implements the new
  `peer::InboundPersist` sink; the accept handler stores an object *before* counting
  it in the acknowledgement, and a refused persist is neither acknowledged nor
  queued. Objects the session acknowledges are drained from the spool on every sync,
  re-verified against the contact's key, and deduplicated by `object_id_of` (a signed
  id, with a byte hash as the fallback), so a redelivery over another path or across a
  restart collapses onto one row.
- Delivered: M7.5. `DeliveryState` is `queued`, `available`, `replica-stored`,
  `relayed`, and `received`; the snapshot adds `deliveryLabel`, `deliveryError`,
  `viaBittorrent`, `viaIroh`, and a `delivery` block (`durable`, `failed`, `spooled`).
  The desktop and terminal frontends render all five states, show the publication
  reason next to a queued message, and explain a stuck message in the Network panel.
- Delivered: the daemon and the Android bridge now push as well as pull
  (`sync_once`), the daemon no longer serialises its three sync steps under separate
  locks, and LAN announcements carry the device endpoint id and its direct addresses.
  A contact that met us only on the LAN is now dialable over the authenticated peer
  channel, and `Contact.peer_addrs` is separate from `transport_addr`: dialing iroh on
  the TCP sync port was a latent bug that only ever worked by accident.
- Delivered: seven new tests. `session::tests` covers the two-host flow (queue
  offline, retry after a restart, reply over the direct paths, five states), a failed
  publication that stays queued and unpushed until the store recovers, and a spooled
  object that is ingested after a restart; `peer::tests` covers a refused persist that
  is not acknowledged and a working sink that stores before the object reaches the
  inbox. The session tests start both TCP listeners and no longer depend on the public
  DHT/torrent path, so the client suite runs in ~2s instead of ~7s of live network
  traffic.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` pass (73 client, 42 core, 3 daemon,
  17 desktop, 10 integration, 9 terminal tests).
- Next: M8.1 — use production Iroh relay configuration by default.
- Blockers: none.

### 2026-09-26 — M8

- Completed: M8.1–M8.5. Relay selection is now local policy with four sources rather
  than one constant, referrals are signed and expiring records, and local health moves a
  failing relay out of the way without touching the device identity.
- Delivered: `client/src/relay.rs` and ADR 0006. `RelayPlan::build` resolves the sources
  most trusted first (configured, verified referral, community, n0), normalizes and
  bounds the list to `MAX_ACTIVE_RELAYS`, and reports `n0` for an empty plan: relaying is
  the fallback that keeps a symmetric-NAT device reachable, so nothing but an explicit
  `SNARTNET_IROH_RELAY=off` may switch it off. Production relays are the default again;
  `staging` is now an explicit opt-in.
- Delivered: M8.3. `RelayReferral` is a profile-key-signed, expiring record whose
  canonical body covers a hash of its grant, so a swapped grant or a changed URL
  invalidates it. `RelayGrant` seals a relay's bearer token to one recipient's X25519
  key with the chat construction under its own algorithm label. Only the operator's own
  configured relay is referred, and only its own `SNARTNET_RELAY_TOKEN` is sealed; a
  token received in someone else's referral is never re-shared. A referral travels as an
  object on the authenticated peer channel (`relay_referral`), throttled per contact by
  `REFERRAL_REFRESH_SECS`.
- Delivered: M8.4. `RelayHealth` scores each relay from the endpoint's own home-relay
  status; `DEMOTE_AFTER_FAILURES` consecutive failures demote a relay behind an untried
  one and eventually out of the map, and a success clears the count so it can win its
  place back. `PeerNode::apply_relay_map` reconciles the live map through iroh's
  `insert_relay`/`remove_relay`, touching only the relays this client added, so a plan
  change keeps the device key — and therefore every contact's certificate pin — intact.
  When every planned relay is failing, iroh's production relays are added back as a
  floor.
- Delivered: the session verifies a referral against the sender's own profile key before
  storing the signed record (the grant stays ciphertext in state), opens the grant when a
  plan needs the token, and rebuilds the plan from configured, referred, and community
  sources; the snapshot exposes a `relay` block (plan, source, disabled, planned, active,
  and per-relay health) and both frontends render it, including a warning when relaying
  is switched off.
- Delivered: thirteen new tests. Nine `relay.rs` tests cover URL/list validation, source
  precedence, referral verification (forged, expired, future, tampered URL, swapped
  grant, wrong version), grant addressing and refusal for a third party, newest-expiry
  selection, health ordering/demotion/recovery, and the n0 floor; three `peer.rs` tests
  cover a dead relay that does not block a direct delivery, a live map reconciliation
  that keeps the endpoint id, and referral throttling; one `session.rs` test covers a
  referral that arrives spooled before acknowledgement, is verified, has its grant
  opened for the plan, and is refused when forged or expired.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` pass (86 client, 42 core, 3 daemon,
  17 desktop, 10 integration, 9 terminal tests).
- Next: M9.1 — implement policy precedence and desktop/mobile defaults.
- Blockers: none.

### 2026-09-26 — M9

- Completed: M9.1–M9.5. A device can now hold copies of a contact's objects on its own
  terms, with the request sealed to it and the promise signed by it, and space is bounded
  locally instead of by trust.
- Delivered: `client/src/replica.rs` and ADR 0007. `StorageSettings::resolve` applies the
  platform default (desktop volunteers 1 GiB and 30-day leases; a phone volunteers
  nothing, keeps 64 MiB, and uses 7-day leases), then the user's saved overrides, then
  `SNARTNET_STORAGE_*`, then a per-contact rule that may only narrow; `clamp` keeps a
  zero quota or lease from becoming a trap, and `Platform::from_env` reads
  `SNARTNET_PLATFORM` so a desktop standing in for a phone behaves like the phone (M10).
- Delivered: M9.2. `ReplicaLease` is a profile-key-signed envelope naming owner, host,
  lease id, and window, whose payload is encrypted to the host; the signature covers a
  hash of the sealed payload, so replacing it invalidates the lease. `StorageReceipt` is
  the host's own signed promise, accepted by the owner only for a lease it issued and only
  from the contact that lease named, which is what makes `replica-stored` (M7.5) evidence
  rather than a claim. Only the operator's own relay-style secrets mirror that rule: a
  relay token is never re-shared, and a lease never contains an object in the clear.
- Delivered: M9.3. The host resolves the DHT pointer its lease kind implies
  (`snartnet/profile`, `snartnet/feed`, `snartnet/mailbox`), fetches from the torrent
  swarm, re-checks the fetched size against the declared one (so a small lease cannot
  smuggle a large object), writes the bytes under `root/replicas/` with a row in the
  `replicas` table, and answers with a receipt. The owner offers its profile, its feed
  snapshot, and its outbound messages, one lease per contact per object until `copies`
  live receipts exist. Both directions ride the peer channel as `replica_lease` and
  `replica_receipt` objects through the durable spool.
- Delivered: M9.4. `admit` and `at_capacity` refuse the not-hosting case, the per-replica
  cap, the quota, and the free-space headroom; `eviction_plan` orders expired leases
  before the oldest live ones and counts the space its own plan frees, so a healthy host
  evicts nothing. `free_bytes` reads `df` and returns `None` where the platform cannot
  answer, in which case the quota alone governs instead of every replica failing.
- Delivered: M9.5. The `storage` command sets overrides field by field, `cleanupStorage`
  evicts on demand, and the snapshot exposes a `storage` block (platform, settings,
  hosting, quota, used, free, held, stored, issued, receipts, note). Both frontends render
  it; the terminal binds `h` to toggle hosting and `C` to drop expired or over-quota
  replicas, and the SDK gained typed `Storage`/`CleanupStorage` commands.
- Delivered: eight new tests. Six `replica.rs` tests cover policy precedence and
  clamping, lease verification (wrong host, foreign key, expired, future, tampered
  payload, unreadable by a third party), receipt verification (expired, edited, foreign
  key), admission (not hosting, oversized, quota, low disk, unknown free space), and
  eviction ordering; two `session.rs` tests cover the full lease → store → receipt round
  trip through the spool (including a tampered receipt and an expired lease) and a host
  that refuses with a reason because the platform or a per-contact rule says so. The
  frontend fixtures assert the storage block reaches both view models.
- Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, and `cargo test --workspace` pass (94 client, 42 core, 3 daemon,
  17 desktop, 10 integration, 9 terminal tests).
- Next: M10.1 — route Android through the shared backend service.
- Blockers: none.

