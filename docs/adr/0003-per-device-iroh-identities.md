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

## Implementation (M6, 2026-09-26)

- `client/src/device.rs` owns both halves. `DeviceKey` is a per-device Ed25519
  secret stored in the canonical `identity_records` table: it never enters a
  snapshot, the legacy JSON mirror, or a frontend. `DeviceCertificate` carries a
  version, the profile fingerprint and public key, the endpoint id, capabilities,
  issue/expiry times, and the profile signature over the unsigned certificate.
- Validation takes exactly one input from the live connection: the endpoint id
  that Iroh's TLS handshake already proved the remote party holds. A genuine
  certificate for another device therefore fails with `EndpointMismatch`.
  Everything else is a pure function of the certificate bytes — profile and
  fingerprint agreement, signature, capability, lifetime, and clock skew — which
  is what makes replay, expiry, and mismatch cases testable without a network.
- `client/src/peer.rs` implements `snartnet/peer/1`: a 4-byte big-endian length
  prefix followed by internally tagged JSON frames (`hello`, `hello_ack`,
  `object`, `notice`, `ack`, `goodbye`). Frames are bounded at 1 MiB and a
  zero-length frame is refused before any allocation.
- The handshake is `hello`/`hello_ack`: each side presents its certificate and a
  fresh 16-byte connection nonce, and the answer must echo the nonce that was
  sent. A recorded handshake therefore cannot be replayed onto a new connection
  (`NonceMismatch`), and the dialer also checks that the endpoint it reached is
  the one it asked for.
- There is no topic. A connection whose certificate is not a known contact's is
  closed with `CLOSE_UNKNOWN_CONTACT` before any application frame is read, the
  same rule that ADR 0002 relies on for mailbox traffic.
- Replay protection is a per-contact pin: the newest accepted `issued_at` and its
  endpoint id are persisted with the contact, and any older certificate is
  refused with `CLOSE_STALE_CERTIFICATE` even though its signature verifies. A
  renewal of the same device is accepted, and a sync that re-sends pin-less
  policies cannot roll a pin backwards.
- `Ack { frames }` is written only after every received frame is queued, and the
  accept handler keeps the connection open until the sender hangs up: Iroh drops
  a connection as soon as its accept handler returns, so returning immediately
  would discard the acknowledgement.
- Certificate lifetime is 30 days with a 7-day renewal window: the stored
  certificate is reused while it is still valid for this endpoint, which keeps
  contacts' pins stable across restarts, and it is re-issued and persisted on
  startup once it enters the window.
- Lookups are explicit. `SNARTNET_IROH_DISCOVERY` (default `dns`) publishes the
  endpoint to Iroh's n0 DNS/Pkarr service and resolves contacts through it;
  `SNARTNET_IROH_RELAY` (default `staging`) selects the relay set. A
  profile-signed `DeviceDescriptor` under the `snartnet/device` DHT namespace is
  the fallback lookup, and it is only usable because it embeds a certificate that
  verifies for the profile the caller asked about.
- Delivery is contact-scoped: presence notices and post/profile change notices go
  to the devices we have pinned, and a queued message that plain TCP/BitTorrent
  could not relay is sent to the recipient's device as a signed object over the
  same authenticated channel. Ingestion still re-checks the object's signature,
  recipient, and contact identity, so the channel is an extra path rather than a
  bypass of the verification rules.
- The unsigned global gossip topic is gone: `client/src/gossip.rs` and the
  `iroh-gossip` dependency were removed, and the daemon's `peers` state key
  replaces `gossip` in the snapshot.

