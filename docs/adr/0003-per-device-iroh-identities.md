# ADR 0003: Iroh identities are per device

## Status

Accepted — 2026-09-23

## Decision

Each device creates its own Iroh key and endpoint ID. A profile-signed device
certificate binds that endpoint ID to the profile, device capabilities, and an
expiry. The profile signing secret is never reused as an Iroh key.

## Consequences

- Multiple devices can operate concurrently without endpoint-discovery races.
- Device-specific revocation and routing are possible.
- Incoming peer traffic must validate the device certificate before application
  frames are accepted.
