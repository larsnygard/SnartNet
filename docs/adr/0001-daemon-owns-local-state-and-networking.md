# ADR 0001: The daemon owns local state and networking

## Status

Accepted — 2026-09-23

## Decision

`snartnet` runs one persistent daemon for each `SNARTNET_HOME`. The daemon is
the exclusive owner of identity storage, indexed state, torrent/DHT sessions,
Iroh connectivity, replication, and background sync. Desktop and terminal UIs
are API clients only. Android uses the same backend service in-process through
JNI until a platform daemon is appropriate.

## Consequences

- Closing a frontend does not stop seeding or synchronization.
- One daemon avoids storage races and competing network listeners.
- Frontends can run together and receive the same authoritative state.
