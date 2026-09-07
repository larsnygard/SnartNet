# Invitations and connectivity

The desktop client supports invitation links, saved QR images, manual fingerprints, SnartNet profile magnets, and nearby peer discovery.

## Share an invitation

Save your identity in **My profile**. The invitation card shows a QR code immediately. **Copy invitation link** writes a `snartnet://invite/z1_…` URI to the clipboard. **Save PNG**, **SVG**, and **JPG** export that same URI as a QR image, including a white quiet zone. PNG is recommended for sharing and scanning.

Exports go to your home `Downloads` directory when it exists, otherwise to the current working directory. The status line shows the exact saved path. No upload or external QR service is involved.

An invitation contains your fingerprint, username, display name, a `snartnet://profile/<url-safe-fingerprint>` identity URI, an optional standard BitTorrent profile magnet, and a TCP endpoint. The magnet is emitted only after the profile torrent has been published; the identity URI remains useful for identifying a contact while publication is pending. It contains no private key. It is a distribution hint: the app still verifies the signed profile before trusting a contact's encryption key.

The default endpoint uses the local network address and the port from `SNARTNET_BIND`. To connect through a VPN or public endpoint, enter a reachable `IP:port` under **Connection address**, save, and share a fresh link. IPv6 uses `[address]:port`. Reimporting an invitation for an existing contact updates its endpoint without duplicating the contact.

## Add a contact

Open **Contacts → Invitation** and paste the entire link or a compressed/legacy invite code. Press Enter or **Add contact & start chatting**. The app stores the endpoint, opens the conversation, and starts syncing.

For a QR image, enter the saved PNG/JPG path and choose **Import QR**. QR import searches the image for a valid SnartNet invitation. Invalid or oversized images and malformed invitations produce an error without creating a contact. Live camera scanning is not included.

**Fingerprint** and **Magnet** remain available for advanced use. They identify the contact but need discovery or a configured reachable peer to obtain the signed profile. Adding your own invitation is rejected. Send your invitation back so the other person can add you too.

## Nearby discovery

After a profile is saved or loaded, the app announces its fingerprint, username, display name, and TCP address over UDP port **47471** every 30 seconds. **Contacts → Nearby** lists discovered peers. Entries expire after 120 seconds and are not automatically added as contacts. Choosing **Connect** persists that peer's endpoint.

**Connection → Turn off discovery** stops announcements and clears the list. A listening socket may remain reserved until the app exits to support restarting discovery. Discovery depends on local broadcast support and firewall settings; a blocked or occupied port is reported as inactive. It does not traverse routers. Share an invitation directly if broadcast discovery is unavailable.

## Messaging and retry

Keep both clients open for the initial profile exchange. Once the recipient's signed profile has been verified, the composer becomes available. Type a message and press Enter or **Send**.

The signed ciphertext is persisted before the draft is cleared. Outgoing messages remain **Queued** until a peer accepts them. Sync retries queued messages every four seconds when enabled, including after restart. **Relayed** acknowledges peer storage only; it does not mean the recipient read or received the message. Peers can also pull the inbox from the sender.

Inbox updates merge under a write lock and replace the cache atomically. Duplicate envelopes do not produce repeated chat entries or unread counts. Received messages must match both the selected contact's signing identity and the local recipient fingerprint. Invalid signatures are excluded.

Plaintext is derived for display and is not stored in newly created chat records. Each message retains the peer encryption key used for that message so later profile changes do not make its history unreadable. Drafts are kept in memory per conversation and do not survive quitting the app.

**Pause sync** stops outgoing retries and polling. The TCP listener continues to accept inbound requests. It is not a network kill switch.

## Remote setup

The native protocol keeps direct TCP as a fallback, while profile, post, and message objects use direct BitTorrent peers discovered through signed DHT descriptors. IPv6 and UPnP port forwarding are attempted automatically. If both participants are behind unreachable NAT, a VPN or manual port forwarding may be needed; there is no managed SnartNet relay service.

```bash
# First instance; choose another root and port for the second instance.
SNARTNET_HOME=/tmp/snartnet-a SNARTNET_BIND=127.0.0.1:47570 cargo run -p snartnet-desktop
```

`SNARTNET_PEERS=127.0.0.1:47571` optionally supplies additional endpoints. These settings currently accept literal IP addresses, not DNS names. Endpoint errors and TCP bind failures are shown or reported at startup.

An app launched with an invitation URI as its first positional argument pre-fills Contacts. Native installers do not yet register the `snartnet://` scheme with operating systems, so clicking a link in another app may require copying and pasting it.
