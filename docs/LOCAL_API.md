# Local frontend API and Rust SDK

`snartnet-sdk` is the frontend dependency. It owns no identity, SQLite database,
or peer network session. `snartnet-client` remains the native backend library;
desktop migration to the SDK was M4, and `snartnet-tui` (M5) is the terminal
frontend built on this API.

The SDK is blocking: call it on a UI worker thread (or in `spawn_blocking` from
an async host). Drop it outside async runtime workers too, because its HTTP
client owns a blocking runtime. It supports health, snapshots, typed commands,
manual sync, mode changes, graceful stop, subscriptions, and explicit auto-start.

```rust,no_run
use snartnet_sdk::{Client, DaemonPaths, Command};

fn example() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(DaemonPaths::from_data_dir(None)?)?;
    // The host supplies the installed daemon executable, e.g. beside the frontend.
    client.ensure_running(std::path::Path::new("/path/to/snartnet"))?;
    let snapshot = client.snapshot()?;
    println!("revision {}", snapshot.revision);
    client.command(&Command::Post { content: "Hello!".into() })?;
    let mut subscription = client.subscribe();
    loop {
        let snapshot = subscription.next_snapshot()?;
        // Replace the frontend's state with snapshot.state.
        println!("revision {}", snapshot.revision);
    }
}
```

## Version 1 contract

All paths require `Authorization: Bearer <token>`. The token is never included
in URLs. Runtime metadata and the token are reread for each request, allowing
restart and token replacement without rebuilding a client. Only loopback
addresses are accepted; redirects and HTTP proxies are disabled.

| Method/path | Request | Response |
| --- | --- | --- |
| GET `/v1/health` | none | `Health`: apiVersion, revision, syncMode, sync |
| GET `/v1/snapshot` | none | `Snapshot`: apiVersion, revision, state |
| POST `/v1/command` | tagged `Command` | `CommandResponse`: apiVersion, revision, result |
| POST `/v1/sync` | empty object | `SyncResponse`: received, revision |
| POST `/v1/sync-mode` | mode: always-on/balanced/paused | `Health` |
| GET `/v1/events` | none | SSE `state` events with apiVersion, revision, kind |
| POST `/v1/stop` | empty object | stopping: true (acknowledgment, not exit confirmation) |

Commands are profile, contact, post, message, read, discovery, storage,
cleanupStorage, cleanup, and invite. Required fields and accepted variants are defined in `sdk/src/types.rs`.
Unknown command fields are rejected. The public state has typed top-level
collections; existing signed records and network status retain their JSON
representation. This avoids coupling frontends to backend storage/network crates.
Private identity keys are absent from snapshots; decrypted message text may be
present for the authenticated local frontend.

A message carries a delivery state and, when publication failed, the reason:
`delivery` is `queued`, `available`, `replica-stored`, `relayed`, or `received`,
with `deliveryLabel` as the display form, `deliveryError` as the publication
failure, and `viaBittorrent`/`viaIroh` recording the paths that accepted it (M7).
The top-level `delivery` key summarises the durable path: `durable` (can this host
publish at all), `failed` (why the last publication failed), and `spooled`
(inbound objects stored before acknowledgement and not yet ingested).

The `storage` key reports replication (M9): `platform`, the resolved `settings`, whether
this device `hosting` replicas for contacts, `quotaBytes`, `usedBytes`, `freeBytes`,
counts of `held`/`stored` replicas and `issued` leases, live `receipts` for its own
objects, and a `note` explaining the last storage decision. `storage` sends the user's
overrides (`replicate`, `quotaMiB`, `leaseDays`, `copies`, `minFreeMiB`; absent fields
keep their value) and `cleanupStorage` evicts expired or over-quota replicas on demand.

API major version 1 is checked in runtime metadata and health before writes.
Versioned responses and events are checked too. Additive response fields are
accepted. Incompatible versions, invalid runtime metadata, and authentication
failures are terminal; they never trigger auto-start. Hosts should surface them.

The `limits` key reports bounded work (M11.1): `round` carries the per-round `caps` and
`spent` counts for publications, contact fetches, and spooled objects folded into state;
`queues` reports how deep the queues are (`outbound` awaiting delivery, `publishing`
awaiting a durable copy, `spooled` with `spoolBytes`, and `inbox`), `caps` repeats the
bounds those queues are held to, `refused` counts objects the host had to turn away
(each one is an acknowledgement that was not written, so the sender retries), and
`overloaded` marks a round that could not accept more inbound. Frontends render these
numbers rather than restating the bounds.

`sync` appears in health and in the snapshot (M11.1) as the scheduler's retry state:
`failures` (consecutive failed rounds, zero when the last round completed),
`nextSyncMs` (milliseconds until the next scheduled round), and, when something went
wrong, `lastError`. Absent `sync` means the daemon is older than the block, not that
nothing is wrong. A failing round backs off from 10 seconds to at most 10 minutes.

## Recovery and delivery semantics

GET requests retry connection failures up to three attempts with 100/200 ms
backoff. Writes are never replayed automatically: a lost response can follow a
committed command. Fetch a fresh snapshot to resolve an uncertain outcome.
Requests have timeouts (2 seconds for health, 20 seconds otherwise) and JSON
responses are capped at 8 MiB. Large datasets need pagination in a future API.

A subscription opens SSE before fetching its initial snapshot, then fetches a
snapshot for each invalidation. EOF, transport failure, or the 20-second stream
timeout causes bounded reconnect and a full snapshot refresh. This deliberately
does not depend on event history or Last-Event-ID. Revisions may reset after a
daemon restart, and duplicate snapshot notifications are allowed. A lagging
server subscription closes and follows the same recovery path. Events are
bounded to 64 KiB. After retry exhaustion, the caller can retry or surface the
error. Dropping the subscription closes its stream; an in-flight blocking read
can take up to its timeout to return.

Auto-start is explicit through `ensure_running(executable)` and waits for
authenticated readiness with bounded polling. Racing starters are resolved by
the daemon's OS-held lock, which releases on process exit even after a crash.
The persistent lock file must not be deleted while a daemon is running.
Runtime files are under the parent of the selected data directory in `runtime/`,
matching the M2 layout; use separate parent directories for separate homes.

## Administration and migration

`snartnet` now accepts only `daemon run`, `daemon start`, `daemon status`, and
`daemon stop`. The old `init`, `profile`, `post`, and `keys` commands are removed;
use the current desktop workflows or the SDK. `--data-dir` / `SNARTNET_DATA_DIR`
select the data directory; otherwise `SNARTNET_HOME/data` or `~/.snartnet/data`
is used. Foreground run is the diagnostic path for startup failures.

Desktop (M4) and Android (M10) are both frontends of this service: the desktop
starts a separate daemon process with `ensure_running`, and Android starts the
same daemon inside the app process and forwards to it over the loopback API, so
one writer owns local state either way. Do not run the legacy desktop and daemon
against the same data directory. Runtime token and metadata permissions are 0600
on Unix; Windows inherits directory ACLs, whose hardening is part of the release
platform audit.
