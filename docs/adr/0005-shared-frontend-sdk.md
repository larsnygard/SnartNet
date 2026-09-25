# ADR 0005: Shared frontend SDK and snapshot recovery

## Status

Accepted — 2026-09-25

## Decision

Use a separate `snartnet-sdk` crate for the local API types and blocking Rust
HTTP client. Both daemon and frontends consume the contract, while the SDK has
no dependency on the native backend or secret storage. Run SDK operations on
frontend workers. CLI administration uses the same SDK.

SSE events invalidate snapshots. On every connect/reconnect, subscribe before
fetching the full snapshot. Revisions are scoped to a daemon lifetime; reconnect
never assumes replay history or that a smaller revision is stale. Typed envelopes
and major-version checks define compatibility; additive response fields remain
forward compatible. Retry reads, but never automatically replay mutations whose
outcome may be unknown after a disconnect.

## Consequences

- Desktop and terminal frontends share recovery and authentication behavior.
- Full snapshot recovery trades bandwidth for a simple authoritative state model.
- Blocking operations require worker dispatch and bounded timeouts.
- Backend records remain JSON inside typed state collections during migration.
- Public API changes that break the contract require a new major version.
