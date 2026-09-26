# ADR 0002: BitTorrent stores data; Iroh connects peers

## Status

Accepted — 2026-09-23

## Decision

Signed profiles, posts, and encrypted messages are durable immutable torrent
objects. DHT records provide signed discovery pointers and mailbox descriptors.
Iroh delivers the same already-persisted encrypted objects to online peers and
sends small availability hints. It selects direct, hole-punched, or relayed
paths transparently.

## Consequences

- A realtime failure cannot lose a queued message.
- Torrent and Iroh arrivals deduplicate by signed object ID.
- Iroh transport relays are distinct from persistent SnartNet replicas.

## Implementation (M7, 2026-09-26)

- Durable-first delivery. `client/src/delivery.rs` defines the rule that the rest of
  the client now follows: an outbound object gets a durable, addressable copy
  (torrent bytes plus a signed DHT pointer) *before* it is pushed to anyone.
  `PublishOutcome` distinguishes `Stored` (published and addressable),
  `Unsupported` (this host has no durable transport, so direct delivery is the only
  path there is), and `Failed` (a durable transport exists but refused the object).
  Only `Failed` blocks direct delivery, and the reason is stored on the message, so a
  failed publish cannot be silently downgraded to "sent" the way it was before.
- One owner per socket. `client/src/ports.rs` resolves the auxiliary ports from the
  peer bind once: torrent on `base + 3` (the layout skips LAN discovery's UDP
  `47471`) and DHT on `base + 4`, each moving to an OS-assigned port when the derived
  one is taken, and an ephemeral bind (`:0`) owning neither. `TcpSwarmTransport` is
  the only opener of both nodes and `Session` uses `transport.torrent()`/`dht()`, so
  `47472`/`47473` are no longer fixed fallbacks and a second binder can no longer
  silently end up without a node.
- Store before acknowledging (M7.3). `peer::InboundPersist` is the sink the accept
  handler writes through before it counts a frame in `Ack { frames }`.
  `repository::InboundSpool` implements it against the canonical SQLite store and is
  bounded at `MAX_SPOOLED_INBOUND` entries, dropping the oldest first. A refused
  persist is neither acknowledged nor queued, so the sender keeps the object instead
  of believing it was delivered, and an acknowledged object survives a crash between
  the acknowledgement and the next sync.
- One deduplicating intake (M7.4). Objects enter canonical state through either the
  spool drain or the in-memory inbox, both of which verify the sender's signature,
  recipient, and contact identity before writing, and both of which skip a message id
  already present in the thread. `repository::object_id_of` gives the spool its key:
  the signed object id when there is one, a fingerprint-and-version key for a
  profile, and a byte hash otherwise.
- Five delivery states (M7.5). `queued` (persisted locally, no durable copy yet),
  `available` (published and addressable), `replica-stored` (a contact signed a
  storage receipt, M9), `relayed` (handed to the recipient over torrent or iroh), and
  `received` (an inbound object we stored and verified before acknowledging). States
  only ever strengthen, so a second arrival path cannot downgrade what an earlier one
  proved, and the snapshot exposes the state label, the publication reason, and which
  paths carried the message.
- Both direct paths are attempted for every pending message rather than the second
  being a fallback for the first: a message that arrives twice is deduplicated on
  arrival, and one that reaches the recipient over a single path is still delivered.
  Messages with an unresolved publication failure are left out of the push list
  entirely.
