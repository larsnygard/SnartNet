# ADR 0007: Storage is local policy, leases are encrypted, receipts are signed

## Status

Accepted — 2026-09-26

## Decision

A device may hold replicas of a contact's published objects, but only on its own terms.
The settings that bound that are resolved locally, most specific last: a platform
default (a desktop volunteers space, a phone does not), the user's saved overrides, the
deployment's environment, and a per-contact rule that may only narrow what is already
allowed. Nothing a contact sends can widen them.

The request itself is a `ReplicaLease`: an envelope naming the owner, the host, a lease
id, and a validity window, signed with the owner's profile key, whose payload (object id,
kind, size, fetch pointer) is encrypted to the host. The answer is a `StorageReceipt`:
the host's own profile-key-signed promise that it holds the object until a stated time.

Space is bounded locally and honestly. Admission refuses a replica that would cross the
quota, exceed the per-replica cap, or eat the free-space headroom, and eviction drops
expired leases first and then the oldest live ones, so a host never fills a disk to obey
a lease. The lease is a request, not an obligation.

## Consequences

- An author can keep an object available while offline, without trusting the replica
  host with its contents: a mailbox object is opaque ciphertext, and a profile or feed
  is public but signed.
- A receipt is evidence, not a claim: it is signed by the host, names a lease the owner
  actually issued, and expires with that lease, which is what makes `replica-stored`
  (M7.5) an honest delivery state.
- A host cannot be made to store more than it agreed to, and cannot be trapped by a
  queue of leases: refusal is local, immediate, and explained.
- Replication does not require the author to be online: the host resolves the DHT
  pointer the lease names and fetches the bytes from the torrent swarm.

## Implementation (M9, 2026-09-26)

- `client/src/replica.rs` owns the policy and the records. `StorageSettings::resolve`
  applies the platform default, then the saved user policy, then `SNARTNET_STORAGE_*`,
  then a per-contact rule through `apply_narrowing`, and `clamp` keeps the result usable
  (a zero quota or a zero lease would be a trap). `Platform::from_env` reads
  `SNARTNET_PLATFORM` rather than a compile-time target, so a desktop build standing in
  for a phone behaves like the phone.
- `ReplicaLease` is signed over a canonical body that includes a hash of its sealed
  payload, so replacing the payload invalidates the envelope; `verify` checks version,
  the addressed host, the window (with `LEASE_CLOCK_SKEW_SECS` against a lease from the
  future), and the signature. `SealedPayload` reuses the chat construction under its own
  algorithm label, so a lease is readable only by the host it names.
- `StorageReceipt::issue`/`verify` are the other half: the host's promise, keyed to a
  lease id and owner, checked against the host's profile key, and refused when expired or
  edited. The owner only accepts a receipt for a lease it issued, and only from the
  contact that lease named.
- M9.3: the host fetches the object from the DHT pointer the lease kind implies
  (`snartnet/profile`, `snartnet/feed`, `snartnet/mailbox`), re-checks the fetched size
  against what the lease declared, stores the bytes under the data directory with a row
  in the `replicas` table, and answers with a receipt. The owner offers its profile, its
  feed snapshot, and its outbound messages (as opaque mailbox objects), one lease per
  contact per object, until `copies` live receipts exist for that object.
- M9.4: `admit` and `at_capacity` are the admission rules; `eviction_plan` orders the
  drop list (expired first, then oldest stored) and counts space freed by the plan
  itself while it builds it, so a healthy host evicts nothing. `free_bytes` reads `df`
  and returns `None` where that is not supported, in which case the quota alone governs
  rather than every replica being refused for a missing tool.
- M9.5: the `storage` command sets the user overrides field by field, `cleanupStorage`
  evicts on demand, and the snapshot exposes a `storage` block (platform, settings,
  hosting, quota, used, free, held, stored, issued, receipts, and the note explaining the
  last decision). The desktop and terminal frontends render it, and the terminal binds
  `h` to toggle replica hosting and `C` to drop expired or over-quota replicas.
- Verified: six `replica.rs` tests cover policy precedence and clamping, lease
  verification (wrong host, foreign key, expired, future, tampered payload, unreadable by
  a third party), receipt verification (expired, edited, foreign key), admission
  (not hosting, oversized, quota, low disk, unknown free space), and eviction ordering;
  two `session.rs` tests cover the full lease → store → receipt round trip through the
  durable spool (including a tampered receipt and lease expiry) and a host that refuses
  with a reason because the platform or a per-contact rule says so. The frontend tests
  assert the storage block reaches both view models.
