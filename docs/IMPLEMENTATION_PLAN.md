# SnartNet implementation plan

This is the delivery ledger for the native daemon, desktop, terminal UI, Iroh
connectivity, and distributed replication work. It is the source of truth for
implementation progress; `docs/ROADMAP.md` remains the higher-level product
roadmap.

## Status

- **Status:** Active
- **Current milestone:** M1 — Canonical indexed storage
- **Last updated:** 2026-09-23
- **Last completed:** M0.5 — v0.3.3 baseline validated
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

- [ ] **M1.1** Define the SQLite schema and migration framework.
- [ ] **M1.2** Index immutable objects by signed ID, creation time, ingestion
  sequence, type, owner, and torrent descriptor.
- [ ] **M1.3** Make the backend the exclusive writer of identity records.
- [ ] **M1.4** Implement atomic transactions and migration rollback.
- [ ] **M1.5** Import and verify v0.3.3 JSON and `client_state.json` data.
- [ ] **M1.6** Back up imported data and reject conflicting identities.
- [ ] **M1.7** Test migrations, deduplication, cursors, corruption, and clock
  skew.

## M2 — Persistent local daemon

- [ ] **M2.1** Move storage and network ownership into one backend service.
- [ ] **M2.2** Implement daemon locking and runtime metadata.
- [ ] **M2.3** Add `snartnet daemon run/start/status/stop`.
- [ ] **M2.4** Add authenticated loopback HTTP and health/snapshot endpoints.
- [ ] **M2.5** Add typed commands, manual sync, and SSE state events.
- [ ] **M2.6** Add API revisions and reconnect recovery.
- [ ] **M2.7** Implement Always-on, Balanced, and Paused sync modes.
- [ ] **M2.8** Add lifecycle, authentication, and concurrent-client tests.

## M3 — Shared frontend SDK

- [ ] **M3.1** Define the versioned API types and Rust client.
- [ ] **M3.2** Add authentication, retry, SSE reconnect, and auto-start.
- [ ] **M3.3** Restrict `snartnet` to daemon administration.
- [ ] **M3.4** Add API compatibility checks and client integration tests.

## M4 — Desktop migration and tray

- [ ] **M4.1** Move desktop state and actions to the shared API client.
- [ ] **M4.2** Preserve avatars, QR workflows, drafts, and ciphertext views.
- [ ] **M4.3** Add tray controls without stopping the daemon on window close.
- [ ] **M4.4** Port desktop tests to daemon-backed behavior.

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
