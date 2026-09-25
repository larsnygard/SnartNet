# SnartNet implementation plan

This is the delivery ledger for the native daemon, desktop, terminal UI, Iroh
connectivity, and distributed replication work. It is the source of truth for
implementation progress; `docs/ROADMAP.md` remains the higher-level product
roadmap.

## Status

- **Status:** Active
- **Current milestone:** M5 — `snartnet-tui` (not started)
- **Last updated:** 2026-09-25
- **Last completed:** M4.1, M4.2, and M4.4 — the desktop frontend is daemon-backed
  (M4.3 tray: Linux only)
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

- [ ] **M5.1** Add the Ratatui/Crossterm workspace crate and daemon client.
- [ ] **M5.2** Implement Messages, Contacts, Feed, Profile, and Network tabs.
- [ ] **M5.3** Implement profile, contact, post, message, and sync workflows.
- [ ] **M5.4** Add text invitation export, responsive layout, help, and recovery.
- [ ] **M5.5** Test reducers, rendering, narrow terminals, and API failures.

## M6 — Iroh device identity and peer protocol

- [ ] **M6.1** Generate a separate Iroh identity per device.
- [ ] **M6.2** Add profile-signed device certificates and validation.
- [ ] **M6.3** Define and implement the `snartnet/peer/1` framed ALPN.
- [ ] **M6.4** Replace global gossip with authenticated contact-scoped updates.
- [ ] **M6.5** Add DNS/Pkarr lookup and optional DHT lookup.
- [ ] **M6.6** Test replay, malformed input, endpoint mismatch, and expiry.

## M7 — Durable BitTorrent plus realtime Iroh delivery

- [ ] **M7.1** Persist and publish an object before realtime delivery.
- [ ] **M7.2** Deliver the same encrypted message over Iroh when online.
- [ ] **M7.3** Persist inbound Iroh objects before acknowledgement.
- [ ] **M7.4** Deduplicate torrent and Iroh arrivals by object ID.
- [ ] **M7.5** Add accurate queued, available, replica-stored, and received states.
- [ ] **M7.6** Test online, offline, retry, restart, and dual-path delivery.

## M8 — Transparent relay selection

- [ ] **M8.1** Use production Iroh relay configuration by default.
- [ ] **M8.2** Add configured, trusted-referral, community, and n0 fallback sources.
- [ ] **M8.3** Define signed, expiring relay referrals and encrypted grants.
- [ ] **M8.4** Score local relay health and update the active map safely.
- [ ] **M8.5** Test direct paths, relays, failover, bad referrals, and outages.

## M9 — Contact replication and storage policy

- [ ] **M9.1** Implement policy precedence and desktop/mobile defaults.
- [ ] **M9.2** Add encrypted replica leases and signed storage receipts.
- [ ] **M9.3** Replicate approved profiles, feeds, and opaque mailbox objects.
- [ ] **M9.4** Add expiry, storage caps, eviction, and low-disk protection.
- [ ] **M9.5** Add Storage & availability settings and policy tests.

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
