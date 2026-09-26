# ADR 0006: Relay selection is local policy with signed referrals

## Status

Accepted — 2026-09-26

## Decision

Which Iroh relay server a device uses is resolved locally from four sources, most
trusted first: relays the operator configured, relays a verified contact referred the
device to with a signed and expiring referral, a community list the operator opted
into, and finally Iroh's own production relays. The first source that yields a usable
relay decides the plan; the sources are never mixed into one anonymous pool. An empty
source list means n0's production relays, and only an explicit
`SNARTNET_IROH_RELAY=off` disables relaying.

A referral names a relay. It never carries the relay's authorization token: the token
travels as an encrypted grant addressed to one recipient's X25519 key, inside the
signed referral. A relay's local health is observed and scored on the device that uses
it, and a plan change is applied to the running endpoint rather than requiring a new
device identity.

## Consequences

- A relay outage or a bad referral cannot leave a device unreachable: selection always
  falls back to n0 unless the operator switched relaying off, and direct paths are
  unaffected throughout.
- Relay choices can be shared between contacts without a central authority and without
  publishing a private relay's secret.
- Health is per device and in memory, so no relay reputation is global or permanent.
- Relaying stays infrastructure this project does not operate; it selects, it does not
  serve.

## Implementation (M8, 2026-09-26)

- `client/src/relay.rs` owns the policy. `RelayPlan::build` resolves
  `RelaySource::{Configured, Referral, Community, N0}` from `RelayInputs`, normalizes
  every URL, deduplicates, and bounds the map to `MAX_ACTIVE_RELAYS`; a plan with no
  explicit relay becomes `RelayMode::Default` (n0 production) and reports source `n0`,
  so an empty configuration can never silently disable relaying. Relay URLs are
  validated before they are used or shared: `http(s)` only, a host is required, and
  credentials in the URL are refused because a URL is stored, mirrored, and logged.
- M8.1: the default is now the production relay set. `staging_relays_from_env` only
  selects n0's test relays on an explicit `SNARTNET_IROH_RELAY=staging`, so a
  deployment cannot end up on test infrastructure because a variable was unset.
- M8.2: sources come from `SNARTNET_RELAY_URLS` (the operator's own relay),
  `SNARTNET_COMMUNITY_RELAYS` (a shared list the operator opted into), verified
  referrals, and n0. A community list is deployment data rather than protocol data:
  relay servers come and go, and a third party's URL compiled into the binary would
  imply an endorsement this project cannot make.
- M8.3: `RelayReferral` is signed with the referrer's *profile* key over a canonical
  body that includes a hash of the grant, so a swapped grant, a changed relay URL, or a
  different referrer name invalidates the signature. Verification checks the version,
  the referrer, the URL, the validity window (with `REFERRAL_CLOCK_SKEW_SECS` against a
  referral from the future), and the signature; the session additionally requires the
  sender to be a verified contact whose own key produced the signature. Referrals are
  stored as the signed record — the grant inside stays ciphertext — and are bounded at
  `MAX_REFERRALS`. A referral travels as a `Frame::Object` under the
  `relay_referral` key over the authenticated peer channel, so an older peer ignores it
  instead of failing, and it is throttled by `REFERRAL_REFRESH_SECS` per contact so an
  unchanged recommendation costs no connection.
- M8.3: `RelayGrant::seal`/`open` reuse the chat construction (X25519 +
  ChaCha20-Poly1305, `snartnet_core::encrypt_message`) under its own algorithm label.
  Only the operator's *own* configured relay is referred, and only its own token
  (`SNARTNET_RELAY_TOKEN`) is sealed into grants: a token that arrived in someone
  else's referral is theirs to give, not ours to re-share.
- M8.4: `RelayHealth` scores each relay from the endpoint's own home-relay status
  (`PeerNode::relay_status`). A connected relay outranks a disconnected one, successes
  count for one and failures against two, and `DEMOTE_AFTER_FAILURES` consecutive
  failures mark a relay demoted, which moves it behind an untried one and eventually
  out of a bounded plan. A success clears the failures, so a relay that recovered can
  win its place back. `PeerNode::apply_relay_map` reconciles the live map with
  `insert_relay`/`remove_relay`, touching only the relays this client added (tracked in
  `PeerInner::applied_relays`), so a plan change does not touch the device key and every
  contact's certificate pin stays valid. When every planned relay is demoted, iroh's
  production relays are added back as a floor. Switching relaying off entirely still
  needs a new endpoint, because iroh binds that mode: the plan reports it instead of
  pretending otherwise.
- Verified: nine `relay.rs` tests cover URL and list validation, source precedence,
  a plan that always reports n0 rather than an empty map, referral verification
  (forged, expired, future, tampered URL, swapped grant, wrong version), grant
  addressing and refusal for a third party, newest-expiry referral selection, health
  ordering/demotion/recovery, and the n0 floor; three `peer.rs` tests cover a dead
  relay that does not block a direct delivery, a live map reconciliation that keeps the
  endpoint id (and therefore every pin), and referral throttling; one `session.rs` test
  covers a referral that arrives spooled before acknowledgement, is verified against
  the sender's key, has its grant opened for the plan, and is refused when forged or
  expired. The frontends render the plan, its source, and each relay's local score.
