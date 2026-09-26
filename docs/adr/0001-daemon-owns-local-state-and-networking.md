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

## Implementation (M10, 2026-09-26)

- Android is a frontend like every other one (M10.1): `android-bridge` starts
  `snartnet-daemon` inside the app process, waits until its loopback API answers, and
  forwards every UI request to it. The UI no longer holds a session, so one writer owns
  local state and an activity can be recreated at any moment without losing queued work.
- The device reports itself as a phone (`SNARTNET_PLATFORM=mobile`) before the service
  opens its state, which selects the mobile storage defaults (ADR 0007, M10.3) — no
  replica hosting, a 64 MiB budget, 7-day leases — instead of the desktop ones.
- Lifecycle and power become a sync mode (M10.2): on screen is `Balanced`, hidden while
  saving battery and not charging is `Paused`, hidden otherwise stays `Balanced`, and
  `AlwaysOn` is never chosen for a phone. Only a change is sent to the service, and a
  `dataSync` foreground service keeps the process alive so the mode, not the activity,
  decides what network work happens.
- Recovery does not depend on the process surviving: the service imports its store on the
  next start, the outbox retries, and the inbound spool is drained then too. A lifecycle
  round trip (foreground → background → foreground) is a mode change, which the daemon
  contract test already covers end to end.
- Limitation: the service still runs in the app process, so Android may reclaim it under
  memory pressure. Running the daemon as its own process is a packaging change, not an API
  change.

