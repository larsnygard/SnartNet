# QR Codes and LAN Discovery

This document describes two new onboarding paths available in the SnartNet
desktop client: sharing your profile as a **QR code / invite code**, and
**automatic discovery of nearby SnartNet peers on your local network**.

---

## Invite codes and QR codes

### What is an invite code?

An invite code is a compact, URL-safe string that bundles everything another
user needs to add you as a contact:

| Field | Description |
|---|---|
| `fingerprint` | Your Ed25519 public-key fingerprint (the durable identity anchor) |
| `username` | Your chosen username |
| `display_name` | Optional human-readable name |
| `magnet_uri` | Magnet link to your profile torrent (if available) |
| `transport_addr` | Optional LAN address for direct sync |

The code is the Base64url (no padding) encoding of a small JSON object.
Your **signed profile** remains the source of truth; the invite is only a
distribution wrapper.

### Showing your invite code / QR code

1. Open the **Profile** panel.
2. After saving your profile, a row appears at the bottom:
   - A truncated preview of your invite code.
   - **Copy** – writes the full invite code to your clipboard.
   - **Show QR / Hide QR** – toggles a monospace-rendered QR code that any
     QR-capable device can scan.

> **Tip:** Share the invite code via any channel (email, chat, printed paper).
> The QR code is intended for future mobile clients and any QR reader that
> understands the `snartnet://` scheme.

### Adding a contact via invite code

1. Open the **Contacts** panel.
2. Click **Invite code** in the mode selector at the top.
3. Paste the invite code you received into the text field.
4. Click **Import invite and subscribe**.

The contact is added through the normal contact workflow – the same sync,
verification, and trust-scoring pipeline used for manually added contacts.
The magnet URI from the invite is stored on the contact record so the sync
engine can locate the peer's profile torrent immediately.

---

## LAN peer discovery

### How it works

When you have a profile, SnartNet broadcasts a small UDP datagram to
`255.255.255.255:47471` every 30 seconds.  Each datagram contains your
fingerprint, username, and display name.  At the same time, SnartNet listens
on the same port for announcements from other peers on the same network
segment.

Discovered peers are kept in memory only (never written to your contacts file
automatically) and expire after 120 seconds of silence.

### Picking a peer from LAN discovery

1. Open the **Contacts** panel.
2. Click **LAN peers** in the mode selector.
3. Any nearby SnartNet users appear in the list with their username and a
   shortened fingerprint.
4. Click **Add contact** next to a peer to add them through the normal contact
   workflow.

The added contact is synced, verified, and stored exactly like a manually
added contact.  The LAN-presence metadata (broadcast address, last-seen time)
is kept separate and does not overwrite the durable contact record.

### Discovery state

The **Network** panel shows:

| Field | Meaning |
|---|---|
| **LAN discovery** | Active / Inactive |
| **Nearby peers visible** | Number of peers seen within the last 120 s |

Use the **Start / Stop LAN discovery** button to toggle broadcasting and
listening without restarting the application.  Discovery starts automatically
when you load or save a profile; it stops when you click the button or quit
the app.

### Firewall and privacy notes

- LAN discovery uses UDP port **47471** (one above the TCP sync port 47470).
- If the port is already in use or blocked by a firewall, discovery silently
  stays inactive; you can still add contacts manually or via invite code.
- Broadcasts are limited to your local network segment (they do not traverse
  routers).
- No data beyond your fingerprint, username, and display name is broadcast.
  Your posts and messages are never included in discovery packets.

---

## Summary of add-contact paths

| Path | How | When to use |
|---|---|---|
| **Manual fingerprint** | Enter fingerprint + alias in the Contacts panel | When you have someone's fingerprint from any out-of-band source |
| **Import invite code** | Paste a base64 invite code | When someone shares their code via text/email/chat |
| **LAN peer** | Pick from the discovered-peers list | When both users are on the same Wi-Fi / LAN |

---

## Remote Messaging (Windows ↔ Mac)

Use this when peers are in different cities and not on the same LAN.

### Network setup

Both peers run the desktop app with:

- `SNARTNET_BIND=<public_or_local_bind_ip>:47470`
- `SNARTNET_PEERS=<peer_ip_or_dns>:47470`

Example:

- Windows: `SNARTNET_BIND=0.0.0.0:47470`
- Mac: `SNARTNET_BIND=0.0.0.0:47470`
- Each side sets `SNARTNET_PEERS` to the other side's reachable endpoint.

Requirements:

- TCP port **47470** reachable through firewall/NAT (port-forwarding if behind home routers).
- At least one side must expose a reachable endpoint for direct peer sync.

### End-to-end test checklist

1. Each peer creates/saves profile and shares invite code/QR.
2. Import invite code in Contacts panel (`Import invite and subscribe`).
3. Run sync and confirm contact profile appears (name/fingerprint/avatar).
4. Open Messages panel and confirm status: `Recipient encryption key ready`.
5. Send message from peer A to peer B.
6. Receiver should see ciphertext preview by default (`[ciphertext] ...`).
7. Click `Show decrypted` to decrypt on demand.
8. Click `Show encrypted` to return to ciphertext view.

### Security behavior in current desktop client

- Messages are stored as ciphertext at rest.
- Decrypted text is computed on demand in UI only.
- Decryption is blocked unless sender signature is verified.
- If recipient encryption key is missing, send is disabled and UI prompts to sync first.
