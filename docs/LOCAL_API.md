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
| GET `/v1/health` | none | `Health`: apiVersion, revision, syncMode |
| GET `/v1/snapshot` | none | `Snapshot`: apiVersion, revision, state |
| POST `/v1/command` | tagged `Command` | `CommandResponse`: apiVersion, revision, result |
| POST `/v1/sync` | empty object | `SyncResponse`: received, revision |
| POST `/v1/sync-mode` | mode: always-on/balanced/paused | `Health` |
| GET `/v1/events` | none | SSE `state` events with apiVersion, revision, kind |
| POST `/v1/stop` | empty object | stopping: true (acknowledgment, not exit confirmation) |

Commands are profile, contact, post, message, read, discovery, cleanup, and
invite. Required fields and accepted variants are defined in `sdk/src/types.rs`.
Unknown command fields are rejected. The public state has typed top-level
collections; existing signed records and network status retain their JSON
representation. This avoids coupling frontends to backend storage/network crates.
Private identity keys are absent from snapshots; decrypted message text may be
present for the authenticated local frontend.

API major version 1 is checked in runtime metadata and health before writes.
Versioned responses and events are checked too. Additive response fields are
accepted. Incompatible versions, invalid runtime metadata, and authentication
failures are terminal; they never trigger auto-start. Hosts should surface them.

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

Desktop and Android still use their existing backend paths until M4/M10. Do not
run the legacy desktop and daemon against the same data directory during this
transition. Runtime token and metadata permissions are 0600 on Unix; Windows
inherits directory ACLs, whose hardening is part of the release platform audit.
