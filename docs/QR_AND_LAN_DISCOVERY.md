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

## Contact devices over iroh

Alongside the LAN broadcast, the app opens an [iroh](https://iroh.computer) endpoint for the devices of contacts you already have. There is no global topic and no internet-wide peer list: the endpoint only accepts connections from contacts, and it only dials device endpoints it already knows, so a stranger cannot appear in it at all. Iroh's public relay and discovery infrastructure is used to establish direct, end-to-end encrypted connections whenever possible, and SnartNet does not operate a relay of its own.

Each device holds its own iroh key, kept apart from your profile signing key (ADR 0003). A profile-signed device certificate binds that key to your profile fingerprint, capabilities, and expiry. The other side validates the certificate against the endpoint identity iroh's TLS handshake already proved before it reads a single application frame, so a device that is not a contact is closed immediately, an older certificate replayed after a newer one was accepted is refused against the pin stored with the contact, and a recorded handshake cannot be replayed onto a new connection.

Device addresses are looked up through iroh's n0 DNS/Pkarr services by default; set `SNARTNET_IROH_DISCOVERY=off` to disable lookups, or **Turn off discovery** in **Connection**, which stops the LAN broadcast and the contact presence loop together. Before giving up on a device that moved, the endpoint also uses a signed device descriptor published to the DHT under the `snartnet/device` namespace. Either way, a device is only dialed once your contact's signed profile has been verified.

Only change notices go to idle contacts: after saving a profile or publishing a post, a small notice (kind plus fingerprint) is sent to the devices of the contacts you know, as a signal that new content is available rather than a data channel. Bulk data (profiles, posts) still travels over the direct TCP connections and BitTorrent/DHT swarm described below. **Across networks** in **Connection** shows the authenticated endpoint as `active (discovery …, N peers)`, `idle`, or `disabled`.

### Direct chat over iroh (NAT-to-NAT)

Chat messages need to reach one specific contact promptly, so they get an additional path: when a queued message cannot be relayed over TCP or BitTorrent, the endpoint above dials the recipient's device, completes the certificate handshake, and delivers the signed message as an object that the recipient acknowledges before the sender treats it as relayed. Iroh attempts to hole-punch a direct path through NAT and, if that fails, falls back to relaying the connection through an iroh relay server. Nothing is trusted because it arrived this way: the recipient still checks the message signature, the sender fingerprint, and the recipient fingerprint. This lets two clients that are both behind restrictive/unreachable NAT still start a chat with no VPN, port forwarding, or manual endpoint sharing, as long as both processes are online.

By default this uses n0's production relay infrastructure. Set the `SNARTNET_IROH_RELAY` environment variable to change the relay *policy*: `staging` to use n0's test relays, or `off`/`disabled` to rely only on directly reachable/hole-punched paths. Which relays are actually offered also comes from `SNARTNET_RELAY_URLS` (your own relay) and `SNARTNET_COMMUNITY_RELAYS` (a community list you joined), both of which outrank the fallback and are preferred in that order; a relay a verified contact referred you to sits between them (ADR 0006). A relay that needs a bearer token gets it from `SNARTNET_RELAY_TOKEN`, and that token is only ever shared as an encrypted grant addressed to one contact, never in the referral itself.

## Messaging and retry

Keep both clients open for the initial profile exchange. Once the recipient's signed profile has been verified, the composer becomes available. Type a message and press Enter or **Send**.

The signed ciphertext is persisted before the draft is cleared. Outgoing messages remain **Queued** until a peer accepts them, whether over TCP, BitTorrent, or the direct iroh chat channel above. Sync retries queued messages every four seconds when enabled, including after restart. **Relayed** acknowledges peer storage/acknowledgement only; it does not mean the recipient read the message. Peers can also pull the inbox from the sender.

Inbox updates merge under a write lock and replace the cache atomically. Duplicate envelopes do not produce repeated chat entries or unread counts. Received messages must match both the selected contact's signing identity and the local recipient fingerprint. Invalid signatures are excluded.

Plaintext is derived for display and is not stored in newly created chat records. Each message retains the peer encryption key used for that message so later profile changes do not make its history unreadable. Drafts are kept in memory per conversation and do not survive quitting the app.

**Pause sync** stops outgoing retries and polling. The TCP listener continues to accept inbound requests. It is not a network kill switch.

## Remote setup

The native protocol keeps direct TCP as a fallback, while profile and post objects use direct BitTorrent peers discovered through signed DHT descriptors. IPv6 and UPnP port forwarding are attempted automatically. Chat messages additionally fall back to a direct iroh connection (see [Direct chat over iroh](#direct-chat-over-iroh-nat-to-nat) above), so two clients behind unreachable NAT can still chat without a VPN or manual port forwarding; SnartNet itself does not operate a relay, but relies on iroh's public relay infrastructure for that fallback path.

```bash
# First instance; choose another root and port for the second instance.
SNARTNET_HOME=/tmp/snartnet-a SNARTNET_BIND=127.0.0.1:47570 cargo run -p snartnet-desktop
```

`SNARTNET_PEERS=127.0.0.1:47571` optionally supplies additional endpoints. These settings currently accept literal IP addresses, not DNS names. Endpoint errors and TCP bind failures are shown or reported at startup.

An app launched with an invitation URI as its first positional argument pre-fills Contacts. Native installers do not yet register the `snartnet://` scheme with operating systems, so clicking a link in another app may require copying and pasting it.
