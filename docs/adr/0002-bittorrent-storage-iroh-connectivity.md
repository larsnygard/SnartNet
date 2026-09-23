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
