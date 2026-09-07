# Native chat review

This change focuses on the active desktop client and shared Rust contracts. The Android shell and archived PWA are not feature-complete chat clients.

## Findings addressed

| Finding | Change |
| --- | --- |
| Desktop events, screens, models, networking, and media shared one large file | Separate models, views, design, actions, media, sync, transport, and discovery modules |
| Profile signatures did not bind the claimed fingerprint to the signing key | Derive and check the fingerprint before accepting the profile |
| Invitation imports discarded TCP endpoints | Preserve endpoints across import, restart, and discovery refresh |
| QR exports carried bare codes and lacked image import | One URI format for clipboard and QR; bounded saved-image decoding |
| Cached profiles prevented subsequent refresh | Poll peers and accept valid newer signed profiles |
| Inbox writes replaced messages from other writers | Locked read/merge/write and atomic replacement |
| Signature-invalid messages could enter visible threads | Verify sender, recipient, and signature before thread insertion |
| UI polling performed blocking TCP operations | Worker performs exchange; UI applies a delta |
| Sending claimed success without delivery acknowledgement | Durable queued envelopes, retries, honest peer-storage acknowledgement |
| Switching threads could move or erase draft text | Per-conversation drafts and recipient-aware send completion |
| Storage failures could discard drafts or silently create a fresh identity | Persist before clearing; report unreadable startup state |
| Network screen displayed invented peer/seeder counts | Show configured endpoints and actual sync state |
| Root contained stale npm dependencies and runtime artifacts | Native-only root; retain web dependencies under legacy; ignore local runtime files |
| README described planned features as implemented | Document current behavior, commands, and remaining protocol work |

## Validation

Run formatting, strict Clippy, workspace tests, and a desktop build using the commands in the README. Regression tests exercise independent temporary roots, real loopback TCP, encrypted round trips, concurrent writes, offline restart/retry, failed storage, invitation limits, and actual QR encode/decode.

The opt-in visual fixture supports reviewing all screens without opening real user data. Real-device Windows/macOS interoperability, camera scanning, deep-link installer integration, NAT traversal, ratcheting, and mobile chat remain outside this change.
