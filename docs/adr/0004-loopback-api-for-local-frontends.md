# ADR 0004: Local frontends use an authenticated loopback API

## Status

Accepted — 2026-09-23

## Decision

The daemon exposes a versioned JSON HTTP API on loopback, defaulting to
`127.0.0.1:47469`, plus a server-sent event stream. A per-installation 256-bit
bearer token and runtime metadata are readable only by the local user.

## Consequences

- Desktop, TUI, CLI administration, and a future same-origin web frontend use
  one API contract.
- The API remains localhost-only in the initial release.
- API versions and event revisions make frontend reconnects recoverable.
