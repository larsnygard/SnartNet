# Terminal client (`snartnet-tui`)

`snartnet-tui` is the Ratatui/Crossterm frontend delivered in milestone M5. It is
a pure client of the local daemon per
[ADR 0001](adr/0001-daemon-owns-local-state-and-networking.md): it owns no
identity, no SQLite database, no torrent session, and no peer listener. Every
frame is drawn from the last daemon snapshot, every keystroke that changes state
becomes at most one `snartnet-sdk` call, and quitting the view never interrupts
background work.

## Run it

```bash
cargo run -p snartnet-tui
# attach to a specific daemon home instead of the default one
cargo run -p snartnet-tui -- --data-dir /tmp/snartnet-alice
```

`--data-dir` falls back to `SNARTNET_DATA_DIR` and then to the same
`SNARTNET_HOME`/`~/.snartnet` resolution the CLI uses, so
`cargo run -p snartnet-cli -- daemon status` always describes the daemon this
view is talking to.

If no daemon is running, the view says so and keeps reconnecting; `D` asks the
SDK to auto-start the daemon, which requires an installed `snartnet` binary
beside `snartnet-tui`. During development, start the daemon yourself with
`cargo run -p snartnet-cli -- daemon start`.

## Screen

- **Tabs 1–5**: Messages, Contacts, Feed, Profile, Network. The tab bar shows the
  unread total next to Messages.
- **Status line**: the last daemon answer, including failures verbatim. It reports
  `The SnartNet daemon is not running for this data directory.` instead of
  pretending the view is empty when the connection is lost.
- **Hint bar**: the keys that matter for the focused tab, or for the focused field.
- **Help overlay**: `?` or `F1`, dismissed with `?` or `Esc`.

## Keys

| Key | Meaning |
| --- | --- |
| `1`–`5`, `Tab`, `Shift+Tab` | Switch tabs |
| `?`, `F1` | Toggle the help overlay |
| `i` / `e` | Start writing in the current tab's primary field |
| `Esc` | Stop editing without submitting |
| `Enter` | Submit the focused field, or open the highlighted row |
| `↑` `↓` / `j` `k` | Move the cursor |
| `PgUp` `PgDn`, `g` `G` | Move a page, or jump to the first/last row |
| `←` `→` / `[` `]` | Previous or next conversation |
| `x` | Show the stored payload of the selected message |
| `r` | Refresh the view from the daemon |
| `y` | Sync with peers now |
| `m` | Cycle the sync mode: always on, balanced, paused |
| `d` | Toggle LAN discovery |
| `c` | Clean up local file caches |
| `h` | Toggle replica hosting for contacts (M9) |
| `C` | Drop expired or over-quota replicas (M9) |
| `u` | Build a fresh invitation link |
| `D` / `S` | Start or stop the daemon (with confirmation for stop) |
| `q`, `Ctrl+C` | Quit this view; the daemon keeps running |

While a field has focus, plain characters type. Movement stays on the arrow,
page, and `Tab` keys, so a shortcut can never steal a character the user meant to
type; submit with `Enter` or leave with `Esc`.

## Workflows

### Create an identity and publish the profile

1. Open **Profile**, press `i`, and fill in at least a username (`Tab` moves
   between username, display name, bio, and advertised address).
2. Press `Enter`. Without an identity the daemon creates one; afterwards the same
   key publishes the edited profile.
3. The tab then shows the fingerprint, the published encryption key, the
   `snartnet://profile/...` identity URI, and the magnet once the daemon has
   published the profile torrent.

Set the advertised address there when friends must reach a port-forwarded or
fixed IP; invitations generated afterwards carry it.

### Invite someone and import a contact

1. Press `u` in **Profile** (or anywhere) and an invite URI plus magnet appear as
   selectable text. Use the terminal's own copy to share it; the encryption key
   never leaves the daemon.
2. In **Contacts**, press `i`, paste an invite link, a magnet link, or a
   fingerprint (`ContactInputMode` detects which it is), optionally type an alias,
   and press `Enter`.

The Contacts tab lists each contact's verification state, trust score, unread
count, and the reason sending may still be blocked. Nearby peers discovered on
the local network are listed on the Network tab.

### Send a message

1. Press `Enter` on a contact in **Contacts** to open its conversation, or use
   `←` `→` in **Messages**.
2. Press `i`, type, and press `Enter`.

Messages need the contact's verified encryption key; if it is missing, the status
line says to sync that contact first. Opening a conversation asks the daemon to
mark it read. Drafts are kept per conversation in the view, and a rejected send
keeps the text so nothing is lost.

Delivery states come straight from the daemon: **Queued** means it is stored
locally and will be retried by automatic sync, **Relayed** means at least one peer
acknowledged storing the envelope. Neither is a read receipt.

### Post to the feed

Open **Feed**, press `i`, type, and press `Enter`. The daemon signs, stores, and
publishes the post; the Feed tab renders the objects the daemon reports.

### Sync, modes, and cleanup

`y` starts a sync round and reports how many objects arrived. `m` cycles the
scheduler's mode (always on, balanced, paused), and the Network tab shows the mode
the daemon is really in, including a paused scheduler. `d` toggles LAN discovery,
`c` frees local file caches, and `r` reloads the view from a fresh snapshot.

Whether LAN discovery can actually start depends on the daemon having an identity
and a usable interface; the daemon's own answer is what the status line shows.

### Recovery

- The view follows daemon-pushed snapshots, so a change made elsewhere (another
  frontend, the CLI, a peer) appears without user action.
- If the daemon stops, the status line explains it, the Network tab reports
  `Daemon: unreachable`, and the last known state stays on screen. Press `D` to
  auto-start an installed daemon, or start one and press `r`.
- Nothing that fails is invented locally: a rejected command shows the daemon's
  message and leaves the form intact, and terminal errors (authentication,
  protocol version) are shown rather than triggering an automatic restart.

## What this client never does

- It never opens the database, decodes BitTorrent objects, or talks to peers.
- It never holds private keys; invitations and signatures are daemon work.
- It never stops the daemon implicitly: `q` and `Ctrl+C` end this process only,
  and `S` is the single explicit shutdown path.

## Tests

`tui/src/tests.rs` covers the reducer (one message in, at most one daemon call
out), the key map (typing is never stolen by a shortcut), rendering from daemon
state alone for every tab, narrow terminals down to a few cells, and API failures.
Two integration tests start a real daemon with
`snartnet_daemon::run_with` on an OS-assigned port inside a temporary home and run
the same action-to-daemon-call mapping `main` uses, so the client is exercised
against the real local API rather than a mock.
