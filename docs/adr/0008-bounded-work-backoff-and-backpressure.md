# ADR 0008: Bounded work, bounded queues, and retry backoff

## Status

Accepted — 2026-09-26

## Decision

A sync round is bounded work, the queues that feed it are bounded, and a round that
fails is retried with growing patience rather than at a fixed cadence.

Three rules carry that:

1. **One round spends a budget.** Publishing, resolving contacts, folding spooled inbound
   objects, and handing over storage records are each capped per round
   (`client/src/limits.rs`). Work that does not fit stays in its queue and is attempted by
   the next round, so a budget is a delay and never a loss. Contact resolution rotates
   through the list by a cursor, so a per-round cap delays everyone instead of permanently
   excluding whoever sits last.
2. **A queue bound is a refusal, not a discard.** The durable inbound spool and the
   endpoint's in-memory inbox both refuse new objects when full. Neither drops something
   already accepted: an entry in the spool is an object we already acknowledged, and
   discarding it would turn that acknowledgement into a lie. A refusal means no
   acknowledgement, so the sender keeps the object queued and retries. Counts of refusals
   are reported instead of being swallowed.
3. **A failing round backs off.** The daemon's scheduler waits `base * 2^failures`
   (10 s, 20 s, 40 s … capped at 10 minutes) after a round that failed, and resets on a
   round that completed. A round that had to refuse inbound is scored as a failure. A mode
   change wakes the scheduler immediately, so a phone returning to the foreground never
   waits out a backoff.

The numbers live next to the queue they bound, and one `Limits` value gathers them so a
host can narrow them; the caps and the spend are reported in the snapshot, so a frontend
shows the real bounds rather than repeating them in a string.

## Consequences

- A frontend never waits on an unbounded round: a backlog of a day's messages, a few
  hundred contacts, or a flooded inbox all cost a bounded amount of time per round, and the
  rest is deferred with its queue depth visible.
- The acknowledgement contract is preserved under pressure. A peer is told "stored" only
  when the object is durable, which is why the spool refuses instead of trimming; the
  sender's next attempt is what closes the gap.
- A broken store, a full disk, or a full spool no longer produces a ten-second retry loop
  forever: a persistent failure settles at a ten-minute retry, and the reason is visible in
  `/v1/health` and in the snapshot's `sync` block.
- A full spool is a local condition that inbound traffic cannot fix by itself: refusing
  objects is the only way to stop a host from filling the volume its canonical store lives
  on, and the sender's retry is what eventually succeeds once the round has drained the
  spool.
- Backoff is deterministic (no jitter): one daemon retries at a time, and the delay a
  frontend shows has to match what the daemon does.

## Implementation (M11.1, 2026-09-26)

- `client/src/limits.rs` holds `Category`, `Limits` (per-round caps plus the spool and
  inbox bounds), `RoundBudget` (what a round may spend, what it spent, and its JSON report),
  and `Backoff`. `Limits::production()` takes its numbers from the subsystem that enforces
  them (`MAX_SPOOLED_INBOUND`, `MAX_SPOOLED_BYTES`, `MAX_PEER_INBOX`), so there is one place
  to change each bound.
- `Session` spends the budget: `publish_pending_outbound` stops at the publish cap,
  `sync_distributed` resolves a rotating window of contacts, `ingest_spooled_inbound`
  truncates its drain, and `pending_storage_records` leaves an unsent lease unmarked so the
  next round sends it. The pending push list is capped by `Limits::pushes`, because a backlog
  handed over in one round is one frame per message per contact.
- `IndexedStore::spool_inbound` checks the bounds before inserting (entries and bytes) and
  counts refusals; `MAX_SPOOLED_BYTES` exists because 512 maximum-size frames would be half a
  gigabyte. `PeerInner::push_inbound` bounds the in-memory inbox, and a full inbox only drops
  a copy when a durable sink already stored the object — without a sink it refuses the
  acknowledgement instead, because the inbox is then the storage.
- `Session::overloaded()` reports a round that refused inbound or ended with a full spool.
  The daemon's `Scheduler` (cadence per mode, `Backoff`, and a `Notify` that a mode change
  wakes) scores every round with it and reports `failures`, `nextSyncMs`, and `lastError` in
  `Health.sync` and in the snapshot's `sync` block.
- The snapshot's `limits` block reports the round caps and spend, the queue depths
  (outbound, awaiting publication, spooled with its bytes, inbox), the queue caps, and the
  refusal counts. The desktop and terminal Network panels render it.
