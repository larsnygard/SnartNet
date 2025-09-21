# PeerSocial CLI Specification

## Overview

The PeerSocial CLI (`ps`) provides a command-line interface for all protocol operations including profile management, messaging, content creation, and network administration. This specification defines the complete command structure, options, and expected behaviors.

## Command Structure

```
ps [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS] [ARGS...]
```

### Global Options

```
-c, --config <PATH>     Path to configuration file (default: ~/.peersocial/config.toml)
-v, --verbose           Enable verbose output
-q, --quiet             Suppress non-error output
-h, --help              Show help information
--version               Show version information
--no-color              Disable colored output
--json                  Output in JSON format where applicable
--log-level <LEVEL>     Set log level (error, warn, info, debug, trace)
```

## Commands Overview

### Identity & Profile Management
- `ps init` - Initialize a new PeerSocial profile
- `ps profile` - Profile management operations
- `ps keys` - Cryptographic key operations
- `ps contacts` - Contact and Ring of Trust management

### Content Operations
- `ps post` - Create and manage posts
- `ps sync` - Synchronize content with the network
- `ps search` - Search for content and users

### Messaging
- `ps message` - Direct and group messaging
- `ps notifications` - Manage notifications

### Network Operations
- `ps swarm` - Swarm and torrent operations
- `ps peers` - Peer discovery and management
- `ps relay` - Relay node operations

### Verification & Trust
- `ps verify` - Identity verification operations
- `ps trust` - Trust and reputation management

### Utilities
- `ps config` - Configuration management
- `ps daemon` - Background daemon operations
- `ps export` - Data export operations
- `ps import` - Data import operations

---

## Detailed Command Specifications

### Identity & Profile Management

#### `ps init`
Initialize a new PeerSocial profile and generate cryptographic keys.

```bash
ps init [OPTIONS] <username>
```

**Arguments:**
- `<username>` - Desired username (3-32 characters, alphanumeric + underscore)

**Options:**
```
-e, --email <EMAIL>         Email address for identity verification
-n, --name <NAME>           Full display name
-b, --bio <TEXT>            Profile biography
-a, --avatar <PATH>         Path to avatar image
-k, --keysize <BITS>        Key size in bits (default: 256)
--no-upload                 Don't automatically upload profile to DHT
--recovery-threshold <N>    Ring of Trust recovery threshold (default: 3)
--output-dir <PATH>         Output directory (default: ~/.peersocial/)
```

**Examples:**
```bash
ps init alice
ps init bob --email bob@example.com --name "Bob Smith"
ps init --no-upload charlie  # Create profile but don't publish
```

**Output:**
- Creates profile directory structure
- Generates Ed25519 keypair
- Creates initial profile.json
- Outputs magnet URI for profile torrent

#### `ps profile`
Manage profile information and operations.

##### Subcommands

**`ps profile show [username|magnet-uri]`**
Display profile information.
```bash
ps profile show                    # Show own profile
ps profile show alice              # Show alice's profile
ps profile show magnet:?xt=...     # Show profile by magnet URI
```

**`ps profile edit [OPTIONS]`**
Edit profile information.
```bash
ps profile edit --name "New Name"
ps profile edit --bio "Updated biography"
ps profile edit --avatar /path/to/new/avatar.jpg
```

**`ps profile publish`**
Publish profile updates to the network.
```bash
ps profile publish                 # Publish all pending changes
ps profile publish --force         # Force republish even without changes
```

**`ps profile backup <path>`**
Create encrypted backup of profile.
```bash
ps profile backup /path/to/backup.ps
ps profile backup --no-encrypt backup.json
```

**`ps profile restore <path>`**
Restore profile from backup.
```bash
ps profile restore /path/to/backup.ps
ps profile restore --password-file pass.txt backup.ps
```

#### `ps keys`
Cryptographic key management operations.

##### Subcommands

**`ps keys generate [OPTIONS]`**
Generate new cryptographic keys.
```bash
ps keys generate --type signing     # Generate new signing key
ps keys generate --type encryption  # Generate new encryption key
ps keys generate --ephemeral        # Generate ephemeral key pair
```

**`ps keys list`**
List all keys with their fingerprints and purposes.
```bash
ps keys list
ps keys list --show-private         # Include private key information
```

**`ps keys export <fingerprint> [path]`**
Export public key.
```bash
ps keys export ABC123DEF456         # Export to stdout
ps keys export ABC123DEF456 key.asc # Export to file
```

**`ps keys import <path>`**
Import public key from another user.
```bash
ps keys import alice-public.asc
ps keys import --trust-level 3 bob-public.asc
```

**`ps keys revoke <fingerprint>`**
Generate revocation certificate for a key.
```bash
ps keys revoke ABC123DEF456
ps keys revoke --reason "compromised" ABC123DEF456
```

#### `ps contacts`
Manage contacts and Ring of Trust.

##### Subcommands

**`ps contacts add <username|magnet-uri> [OPTIONS]`**
Add a contact to your Ring of Trust.
```bash
ps contacts add alice
ps contacts add --trust-level 5 bob
ps contacts add --verify magnet:?xt=... charlie
```

**`ps contacts list [OPTIONS]`**
List contacts with trust levels.
```bash
ps contacts list
ps contacts list --trust-level 4   # Show only level 4+ contacts
ps contacts list --format json     # JSON output
```

**`ps contacts remove <username>`**
Remove contact from Ring of Trust.
```bash
ps contacts remove alice
ps contacts remove --reason "no longer trusted" bob
```

**`ps contacts trust <username> <level>`**
Update trust level for a contact.
```bash
ps contacts trust alice 5          # Set maximum trust
ps contacts trust bob 2            # Set low trust
```

**`ps contacts verify <username>`**
Verify contact's identity and profile integrity.
```bash
ps contacts verify alice
ps contacts verify --deep bob      # Deep verification with full history
```

### Content Operations

#### `ps post`
Create and manage posts.

##### Subcommands

**`ps post create [OPTIONS] [content]`**
Create a new post.
```bash
ps post create "Hello, PeerSocial!"
ps post create --file post.md
ps post create --attach image.jpg "Check out this photo"
ps post create --reply-to <post-hash> "Great post!"
```

**Options:**
```
-f, --file <PATH>           Read content from file
-a, --attach <PATH>         Attach media file
-t, --tags <TAG1,TAG2>      Add hashtags
-r, --reply-to <HASH>       Reply to another post
-p, --private               Create private post (followers only)
-s, --sign-only             Sign but don't publish immediately
--no-timestamp              Don't include timestamp
--expire-after <DURATION>   Set expiration time (e.g., "7d", "1h")
```

**`ps post list [OPTIONS]`**
List posts from timeline or specific user.
```bash
ps post list                        # Your timeline
ps post list alice                  # Alice's posts
ps post list --tag bitcoin          # Posts with #bitcoin
ps post list --since "2025-01-01"   # Posts since date
```

**`ps post show <post-hash>`**
Display a specific post with full details.
```bash
ps post show ABC123...
ps post show --with-replies ABC123...
```

**`ps post delete <post-hash>`**
Delete (revoke) a post.
```bash
ps post delete ABC123...
ps post delete --reason "incorrect information" ABC123...
```

#### `ps sync`
Synchronize content with the network.

##### Subcommands

**`ps sync all [OPTIONS]`**
Synchronize all content.
```bash
ps sync all
ps sync all --profiles-only         # Only sync profiles
ps sync all --posts-only           # Only sync posts
ps sync all --full                 # Full resynchronization
```

**`ps sync profile <username>`**
Synchronize specific profile.
```bash
ps sync profile alice
ps sync profile --force bob
```

**`ps sync posts [username]`**
Synchronize posts from timeline or user.
```bash
ps sync posts                      # Sync timeline
ps sync posts alice                # Sync Alice's posts
ps sync posts --since "1d"         # Sync last day's posts
```

#### `ps search`
Search for content and users.

##### Subcommands

**`ps search posts <query> [OPTIONS]`**
Search for posts.
```bash
ps search posts "bitcoin"
ps search posts --tag technology --author alice
ps search posts --since "1w" "decentralization"
```

**`ps search users <query>`**
Search for users by username or display name.
```bash
ps search users alice
ps search users "Bob Smith"
```

**`ps search hashtags <query>`**
Search for hashtags and topics.
```bash
ps search hashtags tech
ps search hashtags --trending      # Show trending hashtags
```

### Messaging

#### `ps message`
Direct and group messaging operations.

##### Subcommands

**`ps message send <recipient> [message]`**
Send direct message.
```bash
ps message send alice "Hello!"
ps message send alice --file message.txt
ps message send alice --encrypt-to bob "Secret for Bob too"
```

**`ps message list [OPTIONS]`**
List messages and conversations.
```bash
ps message list                    # All conversations
ps message list alice              # Conversation with Alice
ps message list --unread          # Unread messages only
```

**`ps message group`**
Group messaging operations.

**`ps message group create <name> <member1> <member2> ...`**
Create new group.
```bash
ps message group create "Dev Team" alice bob charlie
```

**`ps message group add <group-id> <member>`**
Add member to group.
```bash
ps message group add ABC123 david
```

**`ps message group send <group-id> [message]`**
Send message to group.
```bash
ps message group send ABC123 "Team meeting at 3pm"
```

#### `ps notifications`
Manage notifications and alerts.

##### Subcommands

**`ps notifications list [OPTIONS]`**
List notifications.
```bash
ps notifications list
ps notifications list --unread
ps notifications list --type mentions
```

**`ps notifications mark-read [id]`**
Mark notifications as read.
```bash
ps notifications mark-read         # Mark all as read
ps notifications mark-read ABC123  # Mark specific notification
```

### Network Operations

#### `ps swarm`
Swarm and torrent operations.

##### Subcommands

**`ps swarm status`**
Show swarm connection status.
```bash
ps swarm status
ps swarm status --verbose          # Detailed connection info
```

**`ps swarm list`**
List active torrents and swarms.
```bash
ps swarm list
ps swarm list --seeding            # Only torrents being seeded
ps swarm list --downloading        # Only downloading torrents
```

**`ps swarm add <magnet-uri>`**
Join a swarm.
```bash
ps swarm add "magnet:?xt=urn:btih:..."
```

**`ps swarm remove <hash>`**
Leave a swarm.
```bash
ps swarm remove ABC123DEF456
```

#### `ps peers`
Peer discovery and management.

##### Subcommands

**`ps peers list`**
List connected peers.
```bash
ps peers list
ps peers list --trusted            # Only trusted peers
ps peers list --format table       # Tabular format
```

**`ps peers connect <address>`**
Connect to specific peer.
```bash
ps peers connect 192.168.1.100:6881
ps peers connect alice.example.com:6881
```

**`ps peers ban <peer-id>`**
Ban a problematic peer.
```bash
ps peers ban ABC123DEF456
ps peers ban --reason spam ABC123DEF456
```

#### `ps relay`
Relay node operations for mobile optimization.

##### Subcommands

**`ps relay start [OPTIONS]`**
Start relay node functionality.
```bash
ps relay start
ps relay start --port 8080
ps relay start --trusted-only      # Only relay for trusted peers
```

**`ps relay list`**
List available relay nodes.
```bash
ps relay list
ps relay list --my-relays          # Only your configured relays
```

**`ps relay add <peer-id>`**
Add peer as relay node.
```bash
ps relay add ABC123DEF456
```

### Verification & Trust

#### `ps verify`
Identity verification operations.

##### Subcommands

**`ps verify domain <domain>`**
Verify domain ownership.
```bash
ps verify domain example.com
ps verify domain --method dns example.com
```

**`ps verify credential <credential-file>`**
Verify a credential or certificate.
```bash
ps verify credential alice.vc
ps verify credential --issuer bob alice.vc
```

**`ps verify profile <username|magnet-uri>`**
Verify profile integrity and authenticity.
```bash
ps verify profile alice
ps verify profile --deep magnet:?xt=...
```

#### `ps trust`
Trust and reputation management.

##### Subcommands

**`ps trust score <username>`**
Show trust score for a user.
```bash
ps trust score alice
ps trust score --explain bob       # Show score calculation
```

**`ps trust path <username>`**
Show trust path to a user through Ring of Trust.
```bash
ps trust path alice
ps trust path --max-hops 5 charlie
```

### Utilities

#### `ps config`
Configuration management.

##### Subcommands

**`ps config get [key]`**
Get configuration values.
```bash
ps config get                      # Show all config
ps config get network.port         # Show specific value
```

**`ps config set <key> <value>`**
Set configuration values.
```bash
ps config set network.port 6881
ps config set profile.auto_sync true
```

**`ps config reset [key]`**
Reset configuration to defaults.
```bash
ps config reset                    # Reset all
ps config reset network.port       # Reset specific key
```

#### `ps daemon`
Background daemon operations.

##### Subcommands

**`ps daemon start [OPTIONS]`**
Start background daemon.
```bash
ps daemon start
ps daemon start --port 6881
ps daemon start --no-dht           # Disable DHT
```

**`ps daemon stop`**
Stop background daemon.
```bash
ps daemon stop
ps daemon stop --force             # Force stop
```

**`ps daemon status`**
Show daemon status.
```bash
ps daemon status
ps daemon status --json            # JSON output
```

#### `ps export`
Export data and configurations.

##### Subcommands

**`ps export profile [path]`**
Export profile data.
```bash
ps export profile backup.json
ps export profile --encrypt --password-file pass.txt secure-backup.ps
```

**`ps export posts [path]`**
Export posts.
```bash
ps export posts my-posts.json
ps export posts --since "2025-01-01" recent-posts.json
```

**`ps export keys [path]`**
Export keys (public keys only by default).
```bash
ps export keys public-keys.asc
ps export keys --include-private --encrypt private-backup.asc
```

#### `ps import`
Import data and configurations.

##### Subcommands

**`ps import profile <path>`**
Import profile data.
```bash
ps import profile backup.json
ps import profile --merge existing-profile.json
```

**`ps import posts <path>`**
Import posts.
```bash
ps import posts exported-posts.json
ps import posts --validate-signatures untrusted-posts.json
```

## Configuration File

The configuration file uses TOML format and is located at `~/.peersocial/config.toml` by default.

### Example Configuration

```toml
[profile]
username = "alice"
auto_sync = true
sync_interval = 300  # seconds

[network]
port = 6881
dht_enabled = true
max_peers = 200
bootstrap_nodes = [
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881"
]

[messaging]
max_message_size = 1048576  # 1MB
enable_forward_secrecy = true
auto_decrypt = true

[storage]
data_dir = "~/.peersocial/data"
cache_size = 536870912  # 512MB
cleanup_interval = 86400  # 24 hours

[security]
key_derivation_iterations = 100000
session_timeout = 3600  # 1 hour
require_signature_verification = true

[relay]
enabled = false
trusted_relays = []
relay_for_contacts = true
```

## Exit Codes

```
0   - Success
1   - General error
2   - Misuse of shell command
64  - Command line usage error
65  - Data format error
66  - Cannot open input file
67  - Addressee unknown
69  - Service unavailable
70  - Internal software error
73  - Cannot create output file
74  - Input/output error
75  - Temporary failure
77  - Permission denied
78  - Configuration error
```

## Environment Variables

```
PEERSOCIAL_CONFIG_DIR    - Override config directory (default: ~/.peersocial)
PEERSOCIAL_LOG_LEVEL     - Set log level (error, warn, info, debug, trace)
PEERSOCIAL_NO_COLOR      - Disable colored output (any value)
PEERSOCIAL_DHT_PORT      - Override DHT port
PEERSOCIAL_DAEMON_URL    - Daemon URL for client commands
```

## Output Formats

### JSON Output
When using `--json` flag, output follows this structure:

```json
{
  "success": true,
  "data": { /* command-specific data */ },
  "error": null,
  "timestamp": "2025-09-21T10:30:00Z"
}
```

### Error Format
```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "PROFILE_NOT_FOUND",
    "message": "Profile 'alice' not found in DHT",
    "details": {
      "searched_nodes": 45,
      "last_seen": "2025-09-20T15:30:00Z"
    }
  },
  "timestamp": "2025-09-21T10:30:00Z"
}
```

## Shell Completion

The CLI supports shell completion for bash, zsh, fish, and PowerShell:

```bash
# Bash
ps completion bash > ~/.local/share/bash-completion/completions/ps

# Zsh
ps completion zsh > ~/.zsh/completions/_ps

# Fish
ps completion fish > ~/.config/fish/completions/ps.fish
```

## Interactive Mode

Some commands support interactive mode for better user experience:

```bash
ps init --interactive           # Interactive profile creation
ps message --interactive        # Interactive messaging interface
ps search --interactive         # Interactive search with filters
```

## Batch Operations

Support for batch operations using JSON input:

```bash
ps post create --batch posts.jsonl     # Create multiple posts
ps contacts add --batch contacts.json  # Add multiple contacts
ps message send --batch messages.json  # Send multiple messages
```

This CLI specification provides comprehensive coverage of all PeerSocial protocol operations while maintaining usability and consistency with established command-line interface conventions.