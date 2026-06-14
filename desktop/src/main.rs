//! SnartNet Desktop GUI
//!
//! This host now includes a practical native peer-sync pipeline backed by
//! a shared local swarm directory.
//!
//! Implemented:
//! - per-contact sync of profile, posts, and inbox messages
//! - periodic polling subscription with unread counters per thread
//! - contact identity verification and trust indicators

mod transport;

use iced::{
    time,
    widget::{button, column, container, image, row, scrollable, svg, text, text_input},
    Alignment, Element, Length, Subscription, Task,
};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use snartnet_core::{
    profile_fingerprint_from_magnet_uri, ContactInvite, FileStorage, KeyPair,
    Message as CoreMessage, Post, Profile, SignedMessage, SignedPost, SignedProfile,
};
use std::{
    collections::HashSet,
    env,
    io::Cursor,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use transport::{
    dedupe_inbox, DiscoveredPeer, LanAnnounce, LanDiscovery, NetworkTransport, SwarmPostsBlob,
    SwarmProfileBlob, TcpSwarmTransport,
};

const STORAGE_KEYPAIR: &str = "keypair";
const STORAGE_PROFILE: &str = "profile";
const STORAGE_POSTS: &str = "local_posts";
const STORAGE_CONTACTS: &str = "contacts";
const STORAGE_THREADS: &str = "threads";
const AVATAR_PREVIEW_SIZE: f32 = 72.0;
const LOCAL_SWARM_FILE_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// Number of characters shown in the truncated invite-code preview.
const INVITE_CODE_PREVIEW_LENGTH: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Panel {
    #[default]
    Feed,
    Profile,
    Contacts,
    Messages,
    Network,
}

/// Which path is active in the "Add contact" section of the Contacts panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AddContactMode {
    /// Enter a fingerprint and alias manually (existing flow).
    #[default]
    Manual,
    /// Paste a base64 invite code copied from another user's profile panel.
    Invite,
    /// Paste a profile magnet URI copied from another user's profile panel.
    Magnet,
    /// Pick a peer that was discovered via LAN broadcast.
    LanPeer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum VerificationState {
    #[default]
    Unknown,
    Verified,
    SignatureInvalid,
    FingerprintMismatch,
    MissingPeerProfile,
}

impl VerificationState {
    fn label(self) -> &'static str {
        match self {
            VerificationState::Unknown => "unknown",
            VerificationState::Verified => "verified",
            VerificationState::SignatureInvalid => "signature-invalid",
            VerificationState::FingerprintMismatch => "fingerprint-mismatch",
            VerificationState::MissingPeerProfile => "missing-profile",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Contact {
    fingerprint: String,
    alias: String,
    magnet_uri: Option<String>,
    #[serde(default)]
    avatar_data_url: Option<String>,
    auto_synced: bool,
    last_sync_label: String,
    profile_summary: String,
    latest_post_preview: String,
    #[serde(default)]
    verification: VerificationState,
    #[serde(default = "default_trust")]
    trust_score: u8,
    #[serde(default)]
    synced_post_count: usize,
    #[serde(default)]
    known_public_key: Option<String>,
    #[serde(default)]
    known_encryption_public_key: Option<String>,
    #[serde(default)]
    last_sync_error: Option<String>,
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            fingerprint: String::new(),
            alias: String::new(),
            magnet_uri: None,
            avatar_data_url: None,
            auto_synced: false,
            last_sync_label: String::new(),
            profile_summary: String::new(),
            latest_post_preview: String::new(),
            verification: VerificationState::Unknown,
            trust_score: default_trust(),
            synced_post_count: 0,
            known_public_key: None,
            known_encryption_public_key: None,
            last_sync_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatItem {
    id: String,
    incoming: bool,
    /// Stored payload; ciphertext for encrypted messages.
    content: String,
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    encryption_alg: Option<String>,
    #[serde(default)]
    nonce_b64: Option<String>,
    pushed_via_bittorrent: bool,
    created_label: String,
    #[serde(default)]
    verified_sender: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatThread {
    contact_fingerprint: String,
    messages: Vec<ChatItem>,
    #[serde(default)]
    unread_count: u32,
}

#[derive(Debug, Clone)]
struct NetworkState {
    bittorrent_running: bool,
    peers: u32,
    active_swarms: u32,
    last_push_status: String,
    last_poll_label: String,
    poll_interval_secs: u64,
    lan_discovery_active: bool,
    discovered_peer_count: usize,
}

#[derive(Debug, Clone)]
struct SwarmFileStatus {
    file_name: String,
    mode: &'static str,
    records: usize,
    connected_peers: usize,
    seeders: usize,
    leechers: usize,
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            bittorrent_running: true,
            peers: 0,
            active_swarms: 0,
            last_push_status: "No pushes yet".to_string(),
            last_poll_label: "never".to_string(),
            poll_interval_secs: 4,
            lan_discovery_active: false,
            discovered_peer_count: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FormState {
    username_input: String,
    display_name_input: String,
    bio_input: String,
    avatar_path_input: String,
    avatar_data_url: Option<String>,
    contact_fingerprint_input: String,
    contact_alias_input: String,
    compose_post_input: String,
    compose_message_input: String,
    selected_contact_for_chat: Option<String>,
    /// Add-contact mode selector in the Contacts panel.
    add_contact_mode: AddContactMode,
    /// Invite code string pasted by the user.
    invite_code_input: String,
    /// Profile magnet URI pasted by the user.
    magnet_uri_input: String,
    /// Whether to show the QR code in the Profile panel.
    show_profile_qr: bool,
}

#[derive(Debug, Clone)]
struct StartupData {
    keypair: Option<KeyPair>,
    profile: Option<SignedProfile>,
    local_posts: Vec<SignedPost>,
    contacts: Vec<Contact>,
    threads: Vec<ChatThread>,
}

#[derive(Debug, Clone)]
enum Message {
    StartupLoaded(StartupData),
    Tick(Instant),
    RunSyncNow,
    SwitchPanel(Panel),

    UsernameChanged(String),
    DisplayNameChanged(String),
    BioChanged(String),
    AvatarPathChanged(String),
    LoadAvatarFromPath,
    ClearAvatar,
    SaveProfile,
    ProfileSaved(Result<(KeyPair, SignedProfile), String>),
    ToggleProfileQr,
    CopyInviteCode,
        CopyMagnetUri,
    SaveQrSvg,
    SaveQrPng,
    SaveQrJpg,

    ContactFingerprintChanged(String),
    ContactAliasChanged(String),
    AddContactModeChanged(AddContactMode),
    InviteCodeChanged(String),
    MagnetUriChanged(String),
    AddContact,
    ImportFromInvite,
    InviteImported(Result<Contact, String>),
    ImportFromMagnet,
    MagnetImported(Result<Contact, String>),
    AddDiscoveredPeer(String),
    ContactAdded(Result<Contact, String>),
    SelectChatContact(String),

    ComposePostChanged(String),
    CreatePost,
    PostCreated(Result<SignedPost, String>),

    ComposeMessageChanged(String),
    ToggleMessageView(String),
    SendMessage,
    MessageSent(Result<SignedMessage, String>),

    ToggleBittorrent,
    LanDiscoveryToggle,
    CleanupLocalFiles,
}

struct App {
    panel: Panel,
    keypair: Option<KeyPair>,
    profile: Option<SignedProfile>,
    local_posts: Vec<SignedPost>,
    contacts: Vec<Contact>,
    threads: Vec<ChatThread>,
    network: NetworkState,
    forms: FormState,
    storage: FileStorage,
    transport: TcpSwarmTransport,
    lan_discovery: LanDiscovery,
    /// Snapshot of LAN-discovered peers, refreshed on every tick.
    discovered_peers: Vec<DiscoveredPeer>,
    /// Message IDs currently shown as decrypted; runtime only, never persisted.
    revealed_message_ids: HashSet<String>,
    status_line: String,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let storage = FileStorage::open_default().unwrap_or_else(|e| {
            eprintln!("Warning: could not open default storage: {e}");
            FileStorage::new(std::env::temp_dir().join("snartnet")).expect("temp storage")
        });
        let transport = TcpSwarmTransport::from_env()
            .expect("transport init failed");
        transport.start_server();

        let app = Self {
            panel: Panel::Feed,
            keypair: None,
            profile: None,
            local_posts: Vec::new(),
            contacts: Vec::new(),
            threads: Vec::new(),
            network: NetworkState::default(),
            forms: FormState::default(),
            storage,
            transport,
            lan_discovery: LanDiscovery::new(),
            discovered_peers: Vec::new(),
            revealed_message_ids: HashSet::new(),
            status_line: "Loading local state...".to_string(),
        };

        (app, Task::perform(load_startup_async(), Message::StartupLoaded))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartupLoaded(data) => {
                self.keypair = data.keypair;
                if let Some(kp) = &mut self.keypair {
                    let had_keys = kp.enc_public_key.is_some() && kp.enc_secret_key.is_some();
                    kp.ensure_encryption_keys();
                    if !had_keys {
                        let _ = self.storage.set_json(STORAGE_KEYPAIR, kp);
                    }
                }
                self.profile = data.profile;
                self.local_posts = data.local_posts;
                self.contacts = data.contacts;
                self.threads = data.threads;

                if let Some(sp) = &self.profile {
                    self.forms.username_input = sp.profile.username.clone();
                    self.forms.display_name_input = sp.profile.display_name.clone().unwrap_or_default();
                    self.forms.bio_input = sp.profile.bio.clone().unwrap_or_default();
                    self.forms.avatar_data_url = sp.profile.avatar_data_url.clone();
                }

                if self.profile.is_none() {
                    self.panel = Panel::Profile;
                    self.status_line = "Create your profile to begin".to_string();
                } else {
                    self.status_line = format!(
                        "Ready. {} contacts, {} local posts",
                        self.contacts.len(),
                        self.local_posts.len()
                    );
                    self.publish_local_profile_to_swarm();
                    self.publish_local_posts_to_swarm();
                    self.start_lan_discovery();
                }

                self.recalculate_network();
                self.refresh_transport_peers_from_discovery();
                self.run_peer_sync();
                Task::none()
            }
            Message::Tick(_instant) => {
                // Refresh the LAN peer snapshot so the UI stays current.
                self.discovered_peers = self.lan_discovery.get_discovered();
                self.network.discovered_peer_count = self.discovered_peers.len();
                self.refresh_transport_peers_from_discovery();
                self.run_peer_sync();
                Task::none()
            }
            Message::RunSyncNow => {
                self.run_peer_sync();
                Task::none()
            }
            Message::SwitchPanel(panel) => {
                self.panel = panel;
                if panel == Panel::Messages {
                    self.mark_selected_thread_read();
                }
                Task::none()
            }

            Message::UsernameChanged(v) => {
                self.forms.username_input = v;
                Task::none()
            }
            Message::DisplayNameChanged(v) => {
                self.forms.display_name_input = v;
                Task::none()
            }
            Message::BioChanged(v) => {
                self.forms.bio_input = v;
                Task::none()
            }
            Message::AvatarPathChanged(v) => {
                self.forms.avatar_path_input = v;
                Task::none()
            }
            Message::LoadAvatarFromPath => {
                match load_avatar_data_url_from_path(&self.forms.avatar_path_input) {
                    Ok(data_url) => {
                        self.forms.avatar_data_url = Some(data_url);
                        self.status_line = "Profile picture loaded".to_string();
                    }
                    Err(e) => {
                        self.status_line = format!("Profile picture load failed: {e}");
                    }
                }
                Task::none()
            }
            Message::ClearAvatar => {
                self.forms.avatar_data_url = None;
                self.status_line = "Profile picture cleared".to_string();
                Task::none()
            }
            Message::SaveProfile => {
                let username = self.forms.username_input.clone();
                let display = non_empty(self.forms.display_name_input.clone());
                let bio = non_empty(self.forms.bio_input.clone());
                let avatar_data_url = self.forms.avatar_data_url.clone();
                let keypair = self.keypair.clone();
                let existing_profile = self.profile.clone();

                Task::perform(
                    create_profile_async(
                        username,
                        display,
                        bio,
                        avatar_data_url,
                        keypair,
                        existing_profile,
                    ),
                    Message::ProfileSaved,
                )
            }
            Message::ProfileSaved(result) => {
                match result {
                    Ok((kp, sp)) => {
                        self.keypair = Some(kp.clone());
                        self.forms.avatar_data_url = sp.profile.avatar_data_url.clone();
                        self.profile = Some(sp.clone());

                        if let Err(e) = self.storage.set_json(STORAGE_KEYPAIR, &kp) {
                            self.status_line = format!("Profile saved, keypair persist failed: {e}");
                        }
                        if let Err(e) = self.storage.set_json(STORAGE_PROFILE, &sp) {
                            self.status_line = format!("Profile saved, profile persist failed: {e}");
                        } else {
                            self.status_line = "Profile saved".to_string();
                            self.panel = Panel::Feed;
                        }

                        self.publish_local_profile_to_swarm();
                        self.recalculate_network();
                        // (Re)start LAN discovery with the updated profile.
                        self.lan_discovery.stop();
                        self.start_lan_discovery();
                    }
                    Err(e) => {
                        self.status_line = format!("Profile error: {e}");
                    }
                }
                Task::none()
            }

            Message::ToggleProfileQr => {
                self.forms.show_profile_qr = !self.forms.show_profile_qr;
                Task::none()
            }
            Message::CopyInviteCode => {
                if let Some(sp) = &self.profile {
                    let invite = ContactInvite::from_signed_profile(sp, None);
                    if let Ok(code) = invite.to_base64() {
                        return iced::clipboard::write::<Message>(code);
                    }
                }
                Task::none()
            }
            Message::CopyMagnetUri => {
                if let Some(sp) = &self.profile {
                    if let Some(uri) = sp.profile.magnet_uri.clone() {
                        self.status_line = "Profile magnet URI copied".to_string();
                        return iced::clipboard::write::<Message>(uri);
                    }
                }
                Task::none()
            }
            Message::SaveQrSvg => {
                if let Some(sp) = &self.profile {
                    let invite = ContactInvite::from_signed_profile(sp, None);
                    match invite.to_base64() {
                        Ok(code) => {
                            match save_qr_svg(&code) {
                                Ok(path) => {
                                    self.status_line = format!("QR saved: {}", path.display());
                                }
                                Err(e) => {
                                    self.status_line = format!("QR save failed: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            self.status_line = format!("Invite encode failed: {e}");
                        }
                    }
                }
                Task::none()
            }
            Message::SaveQrPng => {
                if let Some(sp) = &self.profile {
                    let invite = ContactInvite::from_signed_profile(sp, None);
                    match invite.to_base64() {
                        Ok(code) => match save_qr_png(&code) {
                            Ok(path) => {
                                self.status_line = format!("QR saved: {}", path.display());
                            }
                            Err(e) => {
                                self.status_line = format!("QR save failed: {e}");
                            }
                        },
                        Err(e) => {
                            self.status_line = format!("Invite encode failed: {e}");
                        }
                    }
                }
                Task::none()
            }
            Message::SaveQrJpg => {
                if let Some(sp) = &self.profile {
                    let invite = ContactInvite::from_signed_profile(sp, None);
                    match invite.to_base64() {
                        Ok(code) => match save_qr_jpg(&code) {
                            Ok(path) => {
                                self.status_line = format!("QR saved: {}", path.display());
                            }
                            Err(e) => {
                                self.status_line = format!("QR save failed: {e}");
                            }
                        },
                        Err(e) => {
                            self.status_line = format!("Invite encode failed: {e}");
                        }
                    }
                }
                Task::none()
            }

            Message::ContactFingerprintChanged(v) => {
                self.forms.contact_fingerprint_input = v;
                Task::none()
            }
            Message::ContactAliasChanged(v) => {
                self.forms.contact_alias_input = v;
                Task::none()
            }
            Message::AddContactModeChanged(mode) => {
                self.forms.add_contact_mode = mode;
                Task::none()
            }
            Message::InviteCodeChanged(v) => {
                self.forms.invite_code_input = v;
                Task::none()
            }
            Message::MagnetUriChanged(v) => {
                self.forms.magnet_uri_input = v;
                Task::none()
            }
            Message::AddContact => {
                let fp = self.forms.contact_fingerprint_input.clone();
                let alias = self.forms.contact_alias_input.clone();
                Task::perform(add_contact_async(fp, alias), Message::ContactAdded)
            }
            Message::ImportFromInvite => {
                let code = self.forms.invite_code_input.clone();
                Task::perform(import_invite_async(code), Message::InviteImported)
            }
            Message::InviteImported(result) => {
                match result {
                    Ok(contact) => {
                        self.forms.invite_code_input.clear();
                        return self.update(Message::ContactAdded(Ok(contact)));
                    }
                    Err(e) => {
                        self.status_line = format!("Import failed: {e}");
                    }
                }
                Task::none()
            }
            Message::ImportFromMagnet => {
                let uri = self.forms.magnet_uri_input.clone();
                Task::perform(import_magnet_async(uri), Message::MagnetImported)
            }
            Message::MagnetImported(result) => {
                match result {
                    Ok(contact) => {
                        self.forms.magnet_uri_input.clear();
                        return self.update(Message::ContactAdded(Ok(contact)));
                    }
                    Err(e) => {
                        self.status_line = format!("Import failed: {e}");
                    }
                }
                Task::none()
            }
            Message::AddDiscoveredPeer(fp) => {
                self.refresh_transport_peers_from_discovery();
                let peer = self.discovered_peers.iter().find(|p| p.fingerprint == fp).cloned();
                if let Some(peer) = peer {
                    let alias = peer
                        .display_name
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| peer.username.clone());
                    Task::perform(add_contact_async(peer.fingerprint, alias), Message::ContactAdded)
                } else {
                    Task::none()
                }
            }
            Message::ContactAdded(result) => {
                match result {
                    Ok(contact) => {
                        if !self.contacts.iter().any(|c| c.fingerprint == contact.fingerprint) {
                            self.contacts.push(contact.clone());
                            self.forms.contact_fingerprint_input.clear();
                            self.forms.contact_alias_input.clear();

                            if self.forms.selected_contact_for_chat.is_none() {
                                self.forms.selected_contact_for_chat = Some(contact.fingerprint.clone());
                            }

                            self.ensure_thread(&contact.fingerprint);
                            self.persist_contacts();
                            self.persist_threads();
                            self.status_line = format!("Contact added: {}", contact.alias);

                            self.run_peer_sync();
                        } else {
                            self.status_line = "Contact already exists".to_string();
                        }
                    }
                    Err(e) => {
                        self.status_line = format!("Add contact failed: {e}");
                    }
                }
                self.recalculate_network();
                Task::none()
            }
            Message::SelectChatContact(fp) => {
                self.forms.selected_contact_for_chat = Some(fp.clone());
                self.mark_thread_read(&fp);
                Task::none()
            }

            Message::ComposePostChanged(v) => {
                self.forms.compose_post_input = v;
                Task::none()
            }
            Message::CreatePost => {
                let kp = self.keypair.clone();
                let author = self
                    .profile
                    .as_ref()
                    .map(|p| p.profile.fingerprint.clone())
                    .unwrap_or_default();
                let content = self.forms.compose_post_input.clone();
                Task::perform(create_post_async(author, content, kp), Message::PostCreated)
            }
            Message::PostCreated(result) => {
                match result {
                    Ok(post) => {
                        self.local_posts.insert(0, post.clone());
                        self.forms.compose_post_input.clear();
                        self.persist_posts();
                        self.publish_one_post_to_swarm(&post);
                        self.status_line = "Post published to peer swarm".to_string();
                    }
                    Err(e) => {
                        self.status_line = format!("Post failed: {e}");
                    }
                }
                self.recalculate_network();
                Task::none()
            }

            Message::ComposeMessageChanged(v) => {
                self.forms.compose_message_input = v;
                Task::none()
            }
            Message::ToggleMessageView(message_id) => {
                if !self.revealed_message_ids.remove(&message_id) {
                    self.revealed_message_ids.insert(message_id);
                }
                Task::none()
            }
            Message::SendMessage => {
                let recipient = self.forms.selected_contact_for_chat.clone();
                let content = self.forms.compose_message_input.clone();
                let kp = self.keypair.clone();
                let sender = self
                    .profile
                    .as_ref()
                    .map(|p| p.profile.fingerprint.clone())
                    .unwrap_or_default();

                if recipient.is_none() {
                    self.status_line = "Select a contact before messaging".to_string();
                    return Task::none();
                }

                let recipient = recipient.unwrap_or_default();
                let recipient_enc_public = self
                    .contacts
                    .iter()
                    .find(|c| c.fingerprint == recipient)
                    .and_then(|c| c.known_encryption_public_key.clone());

                if recipient_enc_public.is_none() {
                    self.status_line =
                        "Recipient profile missing encryption key. Run sync and try again.".to_string();
                    return Task::none();
                }

                Task::perform(
                    create_message_async(
                        sender,
                        recipient,
                        content,
                        kp,
                        recipient_enc_public.unwrap_or_default(),
                    ),
                    Message::MessageSent,
                )
            }
            Message::MessageSent(result) => {
                match result {
                    Ok(signed) => {
                        let recipient = signed.message.recipient_fingerprint.clone();
                        self.ensure_thread(&recipient);

                        if let Some(thread) = self
                            .threads
                            .iter_mut()
                            .find(|t| t.contact_fingerprint == recipient)
                        {
                            if !thread.messages.iter().any(|m| m.id == signed.message.id) {
                                thread.messages.push(ChatItem {
                                    id: signed.message.id.clone(),
                                    incoming: false,
                                    content: signed.message.content.clone(),
                                    encrypted: signed.message.encrypted,
                                    encryption_alg: signed.message.body_enc.clone(),
                                    nonce_b64: signed.message.nonce_b64.clone(),
                                    pushed_via_bittorrent: self.network.bittorrent_running,
                                    created_label: ts_label(),
                                    verified_sender: true,
                                });
                            }
                        }

                        self.forms.compose_message_input.clear();
                        self.persist_threads();

                        self.publish_outgoing_message_to_swarm(&signed);

                        if self.network.bittorrent_running {
                            self.network.last_push_status = format!(
                                "Pushed message {} to recipient inbox",
                                signed.message.id
                            );
                            self.status_line = "Message sent via BitTorrent push".to_string();
                        } else {
                            self.network.last_push_status =
                                "Queued (BitTorrent transport offline)".to_string();
                            self.status_line =
                                "Message queued; start BitTorrent transport".to_string();
                        }
                    }
                    Err(e) => {
                        self.status_line = format!("Message failed: {e}");
                    }
                }
                Task::none()
            }

            Message::ToggleBittorrent => {
                self.network.bittorrent_running = !self.network.bittorrent_running;
                self.status_line = if self.network.bittorrent_running {
                    "BitTorrent transport started".to_string()
                } else {
                    "BitTorrent transport stopped".to_string()
                };
                self.recalculate_network();
                Task::none()
            }
            Message::LanDiscoveryToggle => {
                if self.lan_discovery.is_active() {
                    self.lan_discovery.stop();
                    self.discovered_peers.clear();
                    self.network.lan_discovery_active = false;
                    self.network.discovered_peer_count = 0;
                    self.refresh_transport_peers_from_discovery();
                    self.status_line = "LAN discovery stopped".to_string();
                } else {
                    self.start_lan_discovery();
                    self.status_line = if self.network.lan_discovery_active {
                        "LAN discovery started".to_string()
                    } else {
                        "LAN discovery unavailable (port in use or firewall)".to_string()
                    };
                }
                Task::none()
            }
            Message::CleanupLocalFiles => {
                self.cleanup_local_swarm_files();
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.network.bittorrent_running {
            time::every(Duration::from_secs(self.network.poll_interval_secs)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let menu = self.view_menu();
        let content = match self.panel {
            Panel::Feed => self.view_feed(),
            Panel::Profile => self.view_profile(),
            Panel::Contacts => self.view_contacts(),
            Panel::Messages => self.view_messages(),
            Panel::Network => self.view_network(),
        };

        let shell = column![
            menu,
            content,
            container(text(self.status_line.clone()).size(13)).padding(12),
        ]
        .spacing(8)
        .padding(8)
        .height(Length::Fill);

        container(shell)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_menu(&self) -> Element<'_, Message> {
        let unread = self.total_unread_count();
        let msg_label = if unread > 0 {
            format!("Messages ({unread})")
        } else {
            "Messages".to_string()
        };

        row![
            button("Feed").on_press(Message::SwitchPanel(Panel::Feed)),
            button("Profile").on_press(Message::SwitchPanel(Panel::Profile)),
            button("Contacts").on_press(Message::SwitchPanel(Panel::Contacts)),
            button(text(msg_label)).on_press(Message::SwitchPanel(Panel::Messages)),
            button("Network").on_press(Message::SwitchPanel(Panel::Network)),
            button("Sync now").on_press(Message::RunSyncNow),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn view_profile(&self) -> Element<'_, Message> {
        let existing = if let Some(sp) = &self.profile {
            format!(
                "Current profile: @{} ({})",
                sp.profile.username,
                sp.profile.fingerprint
            )
        } else {
            "No profile yet".to_string()
        };

        let mut form = column![
            text("Profile management").size(28),
            text(existing).size(14),
            text_input("Username", &self.forms.username_input).on_input(Message::UsernameChanged),
            text_input("Display name", &self.forms.display_name_input)
                .on_input(Message::DisplayNameChanged),
            text_input("Bio", &self.forms.bio_input).on_input(Message::BioChanged),
            text_input("Profile picture path (png/jpg/jpeg)", &self.forms.avatar_path_input)
                .on_input(Message::AvatarPathChanged),
            row![
                button("Load picture").on_press(Message::LoadAvatarFromPath),
                button("Clear picture").on_press(Message::ClearAvatar),
            ]
            .spacing(8),
            button("Save profile").on_press(Message::SaveProfile),
        ]
        .spacing(10)
        .max_width(620);

        let profile_avatar = self
            .forms
            .avatar_data_url
            .as_deref()
            .or_else(|| self.profile.as_ref().and_then(|sp| sp.profile.avatar_data_url.as_deref()));
        if let Some(data_url) = profile_avatar {
            if let Some(handle) = image_handle_from_data_url(data_url) {
                form = form.push(
                    row![
                        text("Profile picture:").size(13),
                        image(handle)
                            .width(AVATAR_PREVIEW_SIZE)
                            .height(AVATAR_PREVIEW_SIZE),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }

        // ── Invite code + QR ──────────────────────────────────────────────
        if let Some(sp) = &self.profile {
            let invite = ContactInvite::from_signed_profile(sp, None);
            let magnet_uri = sp.profile.magnet_uri.clone().unwrap_or_default();
            if let Ok(code) = invite.to_base64() {
                let truncated = if code.len() > INVITE_CODE_PREVIEW_LENGTH {
                    format!("{}…", &code[..INVITE_CODE_PREVIEW_LENGTH])
                } else {
                    code.clone()
                };

                form = form
                    .push(text("── Share your profile ──────────────────").size(13))
                    .push(
                        row![
                            text(format!("Invite code: {truncated}")).size(12),
                            button("Copy").on_press(Message::CopyInviteCode),
                            button("Save SVG").on_press(Message::SaveQrSvg),
                            button("Save PNG").on_press(Message::SaveQrPng),
                            button("Save JPG").on_press(Message::SaveQrJpg),
                            button(if self.forms.show_profile_qr {
                                "Hide QR"
                            } else {
                                "Show QR"
                            })
                            .on_press(Message::ToggleProfileQr),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    );

                if !magnet_uri.is_empty() {
                    let truncated_magnet = if magnet_uri.len() > INVITE_CODE_PREVIEW_LENGTH {
                        format!("{}…", &magnet_uri[..INVITE_CODE_PREVIEW_LENGTH])
                    } else {
                        magnet_uri.clone()
                    };

                    form = form.push(
                        row![
                            text(format!("Magnet URI: {truncated_magnet}")).size(12),
                            button("Copy magnet").on_press(Message::CopyMagnetUri),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    );
                }

                if self.forms.show_profile_qr {
                    let qr = generate_qr_svg_handle(&code);
                    form = form.push(container(svg(qr).width(280).height(280)).padding(8));
                }
            }
        }

        container(form).padding(16).into()
    }

    fn view_contacts(&self) -> Element<'_, Message> {
        // ── Mode selector ─────────────────────────────────────────────────
        let mode_row = row![
            button(if self.forms.add_contact_mode == AddContactMode::Manual {
                "▶ Manual"
            } else {
                "  Manual"
            })
            .on_press(Message::AddContactModeChanged(AddContactMode::Manual)),
            button(if self.forms.add_contact_mode == AddContactMode::Invite {
                "▶ Invite code"
            } else {
                "  Invite code"
            })
            .on_press(Message::AddContactModeChanged(AddContactMode::Invite)),
            button(if self.forms.add_contact_mode == AddContactMode::Magnet {
                "▶ Magnet URI"
            } else {
                "  Magnet URI"
            })
            .on_press(Message::AddContactModeChanged(AddContactMode::Magnet)),
            button(if self.forms.add_contact_mode == AddContactMode::LanPeer {
                "▶ LAN peers"
            } else {
                "  LAN peers"
            })
            .on_press(Message::AddContactModeChanged(AddContactMode::LanPeer)),
        ]
        .spacing(6);

        // ── Add form ──────────────────────────────────────────────────────
        let add_form: Element<'_, Message> = match self.forms.add_contact_mode {
            AddContactMode::Manual => column![
                text("Friends and contacts").size(28),
                mode_row,
                text_input("Contact fingerprint", &self.forms.contact_fingerprint_input)
                    .on_input(Message::ContactFingerprintChanged),
                text_input("Alias", &self.forms.contact_alias_input)
                    .on_input(Message::ContactAliasChanged),
                button("Add contact and subscribe").on_press(Message::AddContact),
            ]
            .spacing(10)
            .max_width(620)
            .into(),

            AddContactMode::Invite => column![
                text("Friends and contacts").size(28),
                mode_row,
                text("Paste an invite code shared by another SnartNet user.").size(13),
                text_input("Invite code (base64)", &self.forms.invite_code_input)
                    .on_input(Message::InviteCodeChanged),
                button("Import invite and subscribe").on_press(Message::ImportFromInvite),
            ]
            .spacing(10)
            .max_width(620)
            .into(),

            AddContactMode::Magnet => column![
                text("Friends and contacts").size(28),
                mode_row,
                text("Paste a profile magnet URI shared by another SnartNet user.").size(13),
                text_input("Profile magnet URI", &self.forms.magnet_uri_input)
                    .on_input(Message::MagnetUriChanged),
                button("Import magnet and subscribe").on_press(Message::ImportFromMagnet),
            ]
            .spacing(10)
            .max_width(620)
            .into(),

            AddContactMode::LanPeer => {
                let mut col = column![
                    text("Friends and contacts").size(28),
                    mode_row,
                    text(format!(
                        "Nearby SnartNet peers on your LAN ({}{})",
                        self.discovered_peers.len(),
                        if self.network.lan_discovery_active {
                            ""
                        } else {
                            " — discovery inactive"
                        }
                    ))
                    .size(13),
                ]
                .spacing(10)
                .max_width(620);

                if self.discovered_peers.is_empty() {
                    col = col.push(text("No peers discovered yet. Make sure LAN discovery is active in the Network panel.").size(12));
                } else {
                    for peer in &self.discovered_peers {
                        let label = format!(
                            "@{}{} — {}",
                            peer.username,
                            peer.display_name
                                .as_ref()
                                .map(|d| format!(" ({})", d))
                                .unwrap_or_default(),
                            short_fp(&peer.fingerprint),
                        );
                        col = col.push(
                            row![
                                text(label).size(13),
                                button("Add contact")
                                    .on_press(Message::AddDiscoveredPeer(peer.fingerprint.clone())),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        );
                    }
                }
                col.into()
            }
        };

        let list: Element<'_, Message> = if self.contacts.is_empty() {
            text("No contacts yet").size(14).into()
        } else {
            let cards: Vec<Element<Message>> = self
                .contacts
                .iter()
                .map(|c| {
                    let unread = self
                        .threads
                        .iter()
                        .find(|t| t.contact_fingerprint == c.fingerprint)
                        .map(|t| t.unread_count)
                        .unwrap_or(0);

                    let header = format!(
                        "{} | {} | trust {} | {} | unread {}",
                        c.alias,
                        short_fp(&c.fingerprint),
                        c.trust_score,
                        c.verification.label(),
                        unread
                    );
                    let sync = format!(
                        "{} | posts {} | {}",
                        c.last_sync_label,
                        c.synced_post_count,
                        c.profile_summary
                    );
                    let err = c
                        .last_sync_error
                        .as_ref()
                        .map(|e| format!("sync-error: {e}"))
                        .unwrap_or_default();

                    let btn = button(text(header).size(14))
                        .on_press(Message::SelectChatContact(c.fingerprint.clone()));

                    let mut body = column![
                        btn,
                        text(sync).size(12),
                        text(c.latest_post_preview.clone()).size(12),
                    ]
                    .spacing(4);

                    if let Some(data_url) = &c.avatar_data_url {
                        if let Some(handle) = image_handle_from_data_url(data_url) {
                            body = body.push(
                                row![
                                    text("Picture:").size(12),
                                    image(handle).width(44).height(44),
                                ]
                                .spacing(8)
                                .align_y(Alignment::Center),
                            );
                        }
                    }

                    if !err.is_empty() {
                        body = body.push(text(err).size(12));
                    }

                    container(body).padding(8).into()
                })
                .collect();
            scrollable(column(cards).spacing(8)).into()
        };

        column![add_form, list].spacing(14).padding(16).into()
    }

    fn view_messages(&self) -> Element<'_, Message> {
        let selected = self.forms.selected_contact_for_chat.as_ref();
        let contact = selected.and_then(|fp| self.contacts.iter().find(|c| &c.fingerprint == fp));
        let recipient_has_encryption_key = contact
            .and_then(|c| c.known_encryption_public_key.as_ref())
            .is_some();

        let contact_line = if let Some(c) = contact {
            format!(
                "Chat with {} ({}) | trust {} | {}",
                c.alias,
                short_fp(&c.fingerprint),
                c.trust_score,
                c.verification.label()
            )
        } else {
            "Select a contact in Contacts panel".to_string()
        };

        let thread_messages: Vec<Element<Message>> = selected
            .and_then(|fp| self.threads.iter().find(|t| &t.contact_fingerprint == fp))
            .map(|thread| {
                let peer_enc_public = contact.and_then(|c| c.known_encryption_public_key.as_deref());
                thread
                    .messages
                    .iter()
                    .map(|m| {
                        let direction = if m.incoming { "IN" } else { "OUT" };
                        let push = if m.pushed_via_bittorrent { "push" } else { "queued" };
                        let verified = if m.verified_sender { "verified" } else { "unverified" };

                        let show_decrypted = self.revealed_message_ids.contains(&m.id);
                        let body = if m.encrypted {
                            if show_decrypted {
                                decrypt_for_display(m, self.keypair.as_ref(), peer_enc_public)
                                    .unwrap_or_else(|e| {
                                    format!("[decrypt failed: {e}]")
                                })
                            } else {
                                format!("[ciphertext] {}", short_payload(&m.content))
                            }
                        } else {
                            m.content.clone()
                        };

                        let enc_meta = if m.encrypted {
                            format!(
                                ", enc={}{}",
                                m.encryption_alg
                                    .as_deref()
                                    .unwrap_or("unknown"),
                                m.nonce_b64
                                    .as_ref()
                                    .map(|n| format!(", nonce={}", short_payload(n)))
                                    .unwrap_or_default()
                            )
                        } else {
                            String::new()
                        };

                        let mut message_row = row![
                            text(format!(
                                "[{direction}] {} ({push}, {verified}{enc_meta}) - {}",
                                body, m.created_label
                            ))
                            .size(14)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center);

                        if m.encrypted {
                            message_row = message_row.push(
                                button(if show_decrypted {
                                    "Show encrypted"
                                } else {
                                    "Show decrypted"
                                })
                                .on_press(Message::ToggleMessageView(m.id.clone())),
                            );
                        }

                        container(message_row).padding(6).into()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let list: Element<Message> = if thread_messages.is_empty() {
            text("No messages yet").size(14).into()
        } else {
            scrollable(column(thread_messages).spacing(6)).into()
        };

        let send_status = if contact.is_none() {
            "Select a contact to message".to_string()
        } else if recipient_has_encryption_key {
            "Recipient encryption key ready".to_string()
        } else {
            "Recipient encryption key missing: run sync before sending".to_string()
        };

        let send_button = if contact.is_some() && recipient_has_encryption_key {
            button("Send (BitTorrent push)").on_press(Message::SendMessage)
        } else {
            button("Send (BitTorrent push)")
        };

        column![
            text("Messaging").size(28),
            text(contact_line).size(14),
            text(send_status).size(12),
            list,
            text_input("Type a message", &self.forms.compose_message_input)
                .on_input(Message::ComposeMessageChanged),
            send_button,
        ]
        .spacing(10)
        .padding(16)
        .into()
    }

    fn view_network(&self) -> Element<'_, Message> {
        let state = if self.network.bittorrent_running {
            "Running"
        } else {
            "Stopped"
        };

        let lan_state = if self.network.lan_discovery_active {
            "Active"
        } else {
            "Inactive"
        };

        let configured_peers = configured_peer_count();
        let connected_peers = self.network.peers as usize;
        let swarm_rows = self.build_swarm_file_rows(connected_peers);
        let swarm_list: Element<Message> = if swarm_rows.is_empty() {
            text("No swarm files yet").size(12).into()
        } else {
            let items: Vec<Element<Message>> = swarm_rows
                .into_iter()
                .map(|row_data| {
                    row![
                        container(text(row_data.file_name).size(12)).width(Length::Fixed(220.0)),
                        container(text(row_data.mode).size(12)).width(Length::Fixed(110.0)),
                        container(text(row_data.records.to_string()).size(12))
                            .width(Length::Fixed(90.0)),
                        container(text(row_data.connected_peers.to_string()).size(12))
                            .width(Length::Fixed(130.0)),
                        container(text(row_data.seeders.to_string()).size(12))
                            .width(Length::Fixed(90.0)),
                        container(text(row_data.leechers.to_string()).size(12))
                            .width(Length::Fixed(90.0)),
                    ]
                    .spacing(8)
                    .into()
                })
                .collect();
            scrollable(column(items).spacing(6).max_width(1000))
                .height(220)
                .into()
        };

        column![
            text("Network status").size(28),
            text(format!("BitTorrent transport: {state}")),
            text(format!("Connected peers: {}", self.network.peers)),
            text(format!("Configured peers: {configured_peers}")),
            text(format!("Active swarms: {}", self.network.active_swarms)),
            text(format!("Last push: {}", self.network.last_push_status)),
            text(format!("Last poll: {}", self.network.last_poll_label)),
            text(format!("Poll interval: {}s", self.network.poll_interval_secs)),
            text("── Shared / Downloaded Swarm Files ───────────────────────").size(13),
            row![
                container(text("File").size(12)).width(Length::Fixed(220.0)),
                container(text("Mode").size(12)).width(Length::Fixed(110.0)),
                container(text("Records").size(12)).width(Length::Fixed(90.0)),
                container(text("Connected").size(12)).width(Length::Fixed(130.0)),
                container(text("Seeders").size(12)).width(Length::Fixed(90.0)),
                container(text("Leechers").size(12)).width(Length::Fixed(90.0)),
            ]
            .spacing(8),
            swarm_list,
            text("── LAN Discovery ──────────────────────────────────────────").size(13),
            text(format!("LAN discovery: {lan_state}")),
            text(format!(
                "Nearby peers visible: {}",
                self.network.discovered_peer_count
            )),
            row![
                button("Run sync now").on_press(Message::RunSyncNow),
                button(if self.network.bittorrent_running {
                    "Stop BitTorrent"
                } else {
                    "Start BitTorrent"
                })
                .on_press(Message::ToggleBittorrent),
                button(if self.network.lan_discovery_active {
                    "Stop LAN discovery"
                } else {
                    "Start LAN discovery"
                })
                .on_press(Message::LanDiscoveryToggle),
                button("Cleanup local files").on_press(Message::CleanupLocalFiles),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .padding(16)
        .into()
    }

    fn build_swarm_file_rows(&self, connected_peers: usize) -> Vec<SwarmFileStatus> {
        let mut rows = Vec::new();

        if let Some(sp) = &self.profile {
            let fp = &sp.profile.fingerprint;
            rows.push(SwarmFileStatus {
                file_name: format!("profile_{}.json", short_fp(fp)),
                mode: "shared",
                records: 1,
                connected_peers,
                seeders: connected_peers.saturating_add(1),
                leechers: 0,
            });

            rows.push(SwarmFileStatus {
                file_name: format!("posts_{}.json", short_fp(fp)),
                mode: "shared",
                records: self.local_posts.len(),
                connected_peers,
                seeders: connected_peers.saturating_add(1),
                leechers: 0,
            });

            let inbox_count = self
                .transport
                .load_inbox(fp)
                .map(|b| b.messages.len())
                .unwrap_or(0);
            rows.push(SwarmFileStatus {
                file_name: format!("inbox_{}.json", short_fp(fp)),
                mode: "downloaded",
                records: inbox_count,
                connected_peers,
                seeders: connected_peers,
                leechers: 1,
            });
        }

        for c in &self.contacts {
            rows.push(SwarmFileStatus {
                file_name: format!("profile_{}.json", short_fp(&c.fingerprint)),
                mode: "downloaded",
                records: 1,
                connected_peers,
                seeders: connected_peers,
                leechers: 1,
            });
            rows.push(SwarmFileStatus {
                file_name: format!("posts_{}.json", short_fp(&c.fingerprint)),
                mode: "downloaded",
                records: c.synced_post_count,
                connected_peers,
                seeders: connected_peers,
                leechers: 1,
            });
            let outbound_for_contact = self
                .threads
                .iter()
                .find(|t| t.contact_fingerprint == c.fingerprint)
                .map(|t| t.messages.iter().filter(|m| !m.incoming).count())
                .unwrap_or(0);
            rows.push(SwarmFileStatus {
                file_name: format!("inbox_{}.json", short_fp(&c.fingerprint)),
                mode: "shared",
                records: outbound_for_contact,
                connected_peers,
                seeders: connected_peers.saturating_add(1),
                leechers: 0,
            });
        }

        rows
    }

    fn cleanup_local_swarm_files(&mut self) {
        let swarm_dir = match local_swarm_dir() {
            Ok(path) => path,
            Err(e) => {
                self.status_line = format!("Cleanup failed: {e}");
                return;
            }
        };

        let active_files = self.active_swarm_filenames();
        let now = SystemTime::now();
        let mut removed = 0usize;
        let mut skipped_recent = 0usize;
        let mut errors = 0usize;

        let entries = match std::fs::read_dir(&swarm_dir) {
            Ok(entries) => entries,
            Err(e) => {
                self.status_line = format!("Cleanup failed to read swarm dir: {e}");
                return;
            }
        };

        for entry in entries {
            let Ok(entry) = entry else {
                errors = errors.saturating_add(1);
                continue;
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(file_name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
            else {
                continue;
            };

            let is_swarm_json = (file_name.starts_with("profile_")
                || file_name.starts_with("posts_")
                || file_name.starts_with("inbox_"))
                && file_name.ends_with(".json");
            if !is_swarm_json || active_files.contains(&file_name) {
                continue;
            }

            let modified = match entry.metadata().and_then(|m| m.modified()) {
                Ok(modified) => modified,
                Err(_) => {
                    errors = errors.saturating_add(1);
                    continue;
                }
            };

            let age_secs = now
                .duration_since(modified)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if age_secs < LOCAL_SWARM_FILE_RETENTION_SECS {
                skipped_recent = skipped_recent.saturating_add(1);
                continue;
            }

            match std::fs::remove_file(&path) {
                Ok(_) => removed = removed.saturating_add(1),
                Err(_) => errors = errors.saturating_add(1),
            }
        }

        self.status_line = format!(
            "Cleanup complete: removed {removed}, kept recent inactive {skipped_recent}, errors {errors}"
        );
    }

    fn active_swarm_filenames(&self) -> HashSet<String> {
        let mut active = HashSet::new();

        if let Some(profile) = &self.profile {
            let fp = &profile.profile.fingerprint;
            active.insert(format!("profile_{fp}.json"));
            active.insert(format!("posts_{fp}.json"));
            active.insert(format!("inbox_{fp}.json"));
        }

        for contact in &self.contacts {
            let fp = &contact.fingerprint;
            active.insert(format!("profile_{fp}.json"));
            active.insert(format!("posts_{fp}.json"));
            active.insert(format!("inbox_{fp}.json"));
        }

        active
    }

    fn view_feed(&self) -> Element<'_, Message> {
        let author = self
            .profile
            .as_ref()
            .map(|p| format!("@{}", p.profile.username))
            .unwrap_or_else(|| "No profile".to_string());

        let composer = column![
            text(format!("Feed ({author})")).size(28),
            text_input("Share an update", &self.forms.compose_post_input)
                .on_input(Message::ComposePostChanged),
            button("Publish post").on_press(Message::CreatePost),
        ]
        .spacing(10);

        let mut all_items: Vec<(String, String)> = self
            .local_posts
            .iter()
            .map(|p| {
                (
                    format!("You {}", p.post.created_at.format("%Y-%m-%d %H:%M UTC")),
                    p.post.content.clone(),
                )
            })
            .collect();

        for c in &self.contacts {
            if !c.latest_post_preview.is_empty() {
                all_items.push((format!("{} synced", c.alias), c.latest_post_preview.clone()));
            }
        }

        let list: Element<Message> = if all_items.is_empty() {
            text("No activity yet").size(14).into()
        } else {
            let items: Vec<Element<Message>> = all_items
                .into_iter()
                .map(|(meta, body)| {
                    container(column![text(meta).size(12), text(body).size(15)].spacing(4))
                        .padding(8)
                        .into()
                })
                .collect();
            scrollable(column(items).spacing(8)).into()
        };

        column![composer, list].spacing(12).padding(16).into()
    }

    fn run_peer_sync(&mut self) {
        if !self.network.bittorrent_running {
            return;
        }

        let Some(local_profile) = &self.profile else {
            return;
        };

        let local_fp = local_profile.profile.fingerprint.clone();
        let mut inbox = self.transport.load_inbox(&local_fp).unwrap_or_default();
        let mut any_change = false;
        let mut incoming_count = 0u32;

        let contact_fingerprints: Vec<String> =
            self.contacts.iter().map(|c| c.fingerprint.clone()).collect();
        for fp in contact_fingerprints {
            self.ensure_thread(&fp);
        }

        for contact in &mut self.contacts {
            contact.last_sync_error = None;

            match self.transport.load_profile(&contact.fingerprint) {
                Some(peer_profile) => {
                    if peer_profile.profile.profile.fingerprint != contact.fingerprint {
                        contact.verification = VerificationState::FingerprintMismatch;
                        contact.trust_score = contact.trust_score.saturating_sub(12);
                        contact.last_sync_error = Some(
                            "profile fingerprint does not match contact".to_string(),
                        );
                    } else if peer_profile.profile.verify().unwrap_or(false) {
                        contact.verification = VerificationState::Verified;
                        contact.known_public_key = Some(peer_profile.profile.profile.public_key.clone());
                        contact.known_encryption_public_key =
                            peer_profile.profile.profile.encryption_public_key.clone();
                        contact.magnet_uri = peer_profile.profile.profile.magnet_uri.clone();
                        contact.avatar_data_url = peer_profile.profile.profile.avatar_data_url.clone();
                        contact.profile_summary = format!(
                            "@{} {}",
                            peer_profile.profile.profile.username,
                            peer_profile
                                .profile
                                .profile
                                .display_name
                                .clone()
                                .unwrap_or_default()
                        )
                        .trim()
                        .to_string();
                        contact.trust_score = contact.trust_score.saturating_add(3).min(100);
                    } else {
                        contact.verification = VerificationState::SignatureInvalid;
                        contact.trust_score = contact.trust_score.saturating_sub(10);
                        contact.last_sync_error = Some("invalid profile signature".to_string());
                    }
                }
                None => {
                    if contact.verification == VerificationState::Unknown {
                        contact.verification = VerificationState::MissingPeerProfile;
                    }
                    contact.last_sync_error = Some("peer profile not found in swarm".to_string());
                }
            }

            let verified_posts = if let Some(peer_posts) = self.transport.load_posts(&contact.fingerprint) {
                if let Some(pk) = &contact.known_public_key {
                    peer_posts
                        .posts
                        .into_iter()
                        .filter(|sp| sp.verify(pk).unwrap_or(false))
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            contact.synced_post_count = verified_posts.len();
            contact.latest_post_preview = verified_posts
                .first()
                .map(|p| p.post.content.clone())
                .unwrap_or_else(|| "No synced posts".to_string());

            if let Some(thread) = self
                .threads
                .iter_mut()
                .find(|t| t.contact_fingerprint == contact.fingerprint)
            {
                for msg in &inbox.messages {
                    if msg.message.sender_fingerprint != contact.fingerprint {
                        continue;
                    }
                    if thread.messages.iter().any(|m| m.id == msg.message.id) {
                        continue;
                    }

                    let verified_sender = contact
                        .known_public_key
                        .as_ref()
                        .map(|pk| msg.verify(pk).unwrap_or(false))
                        .unwrap_or(false);

                    thread.messages.push(ChatItem {
                        id: msg.message.id.clone(),
                        incoming: true,
                        content: msg.message.content.clone(),
                        encrypted: msg.message.encrypted,
                        encryption_alg: msg.message.body_enc.clone(),
                        nonce_b64: msg.message.nonce_b64.clone(),
                        pushed_via_bittorrent: true,
                        created_label: ts_label(),
                        verified_sender,
                    });

                    if !(self.panel == Panel::Messages
                        && self
                            .forms
                            .selected_contact_for_chat
                            .as_ref()
                            .map(|fp| fp == &contact.fingerprint)
                            .unwrap_or(false))
                    {
                        thread.unread_count = thread.unread_count.saturating_add(1);
                    }

                    if verified_sender {
                        contact.trust_score = contact.trust_score.saturating_add(1).min(100);
                    } else {
                        contact.trust_score = contact.trust_score.saturating_sub(2);
                    }

                    incoming_count = incoming_count.saturating_add(1);
                    any_change = true;
                }
            }

            contact.last_sync_label = format!("synced {}", ts_label());
        }

        // Prune duplicate inbox entries by id so polling remains linear over time.
        dedupe_inbox(&mut inbox);
        let _ = self.transport.save_inbox(&local_fp, &inbox);

        if any_change {
            self.persist_threads();
        }
        self.persist_contacts();

        if incoming_count > 0 {
            self.status_line = format!("Synced {} new incoming message(s)", incoming_count);
            self.network.last_push_status = format!("received {} inbound push message(s)", incoming_count);
        }

        self.network.last_poll_label = ts_label();
        self.recalculate_network();
    }

    fn persist_posts(&mut self) {
        if let Err(e) = self.storage.set_json(STORAGE_POSTS, &self.local_posts) {
            self.status_line = format!("Persist posts failed: {e}");
        }
    }

    fn persist_contacts(&mut self) {
        if let Err(e) = self.storage.set_json(STORAGE_CONTACTS, &self.contacts) {
            self.status_line = format!("Persist contacts failed: {e}");
        }
    }

    fn persist_threads(&mut self) {
        if let Err(e) = self.storage.set_json(STORAGE_THREADS, &self.threads) {
            self.status_line = format!("Persist threads failed: {e}");
        }
    }

    fn ensure_thread(&mut self, fingerprint: &str) {
        if !self
            .threads
            .iter()
            .any(|t| t.contact_fingerprint == fingerprint)
        {
            self.threads.push(ChatThread {
                contact_fingerprint: fingerprint.to_string(),
                messages: Vec::new(),
                unread_count: 0,
            });
        }
    }

    fn mark_thread_read(&mut self, fingerprint: &str) {
        if let Some(thread) = self
            .threads
            .iter_mut()
            .find(|t| t.contact_fingerprint == fingerprint)
        {
            thread.unread_count = 0;
            self.persist_threads();
        }
    }

    fn mark_selected_thread_read(&mut self) {
        if let Some(fp) = self.forms.selected_contact_for_chat.clone() {
            self.mark_thread_read(&fp);
        }
    }

    fn total_unread_count(&self) -> u32 {
        self.threads.iter().map(|t| t.unread_count).sum()
    }

    fn publish_local_profile_to_swarm(&mut self) {
        if let Some(profile) = &self.profile {
            let blob = SwarmProfileBlob {
                profile: profile.clone(),
                updated_at: unix_secs(),
            };
            if let Err(e) = self.transport.save_profile(&profile.profile.fingerprint, &blob) {
                self.status_line = format!("Profile publish failed: {e}");
            }
        }
    }

    fn publish_local_posts_to_swarm(&mut self) {
        if let Some(profile) = &self.profile {
            let blob = SwarmPostsBlob {
                posts: self.local_posts.clone(),
                updated_at: unix_secs(),
            };
            if let Err(e) = self.transport.save_posts(&profile.profile.fingerprint, &blob) {
                self.status_line = format!("Post publish failed: {e}");
            }
        }
    }

    fn publish_one_post_to_swarm(&mut self, signed_post: &SignedPost) {
        let fp = signed_post.post.author_fingerprint.clone();
        let mut blob = self.transport.load_posts(&fp).unwrap_or_default();

        if !blob.posts.iter().any(|p| p.post.id == signed_post.post.id) {
            blob.posts.insert(0, signed_post.clone());
            blob.updated_at = unix_secs();
            if let Err(e) = self.transport.save_posts(&fp, &blob) {
                self.status_line = format!("Post publish failed: {e}");
            }
        }
    }

    fn publish_outgoing_message_to_swarm(&mut self, signed_message: &SignedMessage) {
        let recipient = signed_message.message.recipient_fingerprint.clone();
        let mut inbox = self.transport.load_inbox(&recipient).unwrap_or_default();

        if !inbox
            .messages
            .iter()
            .any(|m| m.message.id == signed_message.message.id)
        {
            inbox.messages.push(signed_message.clone());
            inbox.updated_at = unix_secs();
            if let Err(e) = self.transport.save_inbox(&recipient, &inbox) {
                self.status_line = format!("Push publish failed: {e}");
            }
        }
    }

    fn recalculate_network(&mut self) {
        if !self.network.bittorrent_running {
            self.network.peers = 0;
            self.network.active_swarms = 0;
            return;
        }

        self.network.peers = (self.contacts.len() as u32 * 2).saturating_add(1);

        let profile_swarms = if self.profile.is_some() { 1 } else { 0 };
        let contact_swarms = self.contacts.len() as u32;
        let post_swarms = self.local_posts.len() as u32;
        let inbox_swarms = self.contacts.len() as u32;
        self.network.active_swarms = profile_swarms + contact_swarms + post_swarms + inbox_swarms;
    }

    /// Attempt to start LAN discovery for the current profile.
    /// Sets `network.lan_discovery_active` to reflect the outcome.
    fn start_lan_discovery(&mut self) {
        let Some(sp) = &self.profile else { return };
        // Use the actual LAN IP of this host rather than 0.0.0.0 so that
        // peers receiving the broadcast can actually connect back.
        let tcp_addr = transport::local_lan_ip().map(|ip| {
            format!("{}:{}", ip, transport::LAN_DISCOVERY_PORT - 1)
        });
        let announce = LanAnnounce {
            fingerprint: sp.profile.fingerprint.clone(),
            username: sp.profile.username.clone(),
            display_name: sp.profile.display_name.clone(),
            tcp_addr,
        };
        let started = self.lan_discovery.start(announce);
        self.network.lan_discovery_active = started;
        self.refresh_transport_peers_from_discovery();
    }

    fn refresh_transport_peers_from_discovery(&self) {
        let mut peers = Vec::new();

        for peer in &self.discovered_peers {
            if let Some(addr) = peer.tcp_addr.as_deref().and_then(|value| value.parse().ok()) {
                if !peers.contains(&addr) {
                    peers.push(addr);
                }
            }
        }

        self.transport.set_peers(peers);
    }
}

async fn load_startup_async() -> StartupData {
    let storage = FileStorage::open_default()
        .or_else(|_| FileStorage::new(std::env::temp_dir().join("snartnet")))
        .expect("storage unavailable");

    let keypair = storage.get_json(STORAGE_KEYPAIR).ok().flatten();
    let profile = storage.get_json(STORAGE_PROFILE).ok().flatten();
    let local_posts = storage
        .get_json(STORAGE_POSTS)
        .ok()
        .flatten()
        .unwrap_or_default();
    let contacts = storage
        .get_json(STORAGE_CONTACTS)
        .ok()
        .flatten()
        .unwrap_or_default();
    let threads = storage
        .get_json(STORAGE_THREADS)
        .ok()
        .flatten()
        .unwrap_or_default();

    StartupData {
        keypair,
        profile,
        local_posts,
        contacts,
        threads,
    }
}

async fn create_profile_async(
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_data_url: Option<String>,
    keypair: Option<KeyPair>,
    existing_profile: Option<SignedProfile>,
) -> Result<(KeyPair, SignedProfile), String> {
    if username.len() < 3 || username.len() > 32 {
        return Err("Username must be 3-32 characters".to_string());
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Username may only contain letters, digits and underscore".to_string());
    }

    let mut kp = if let Some(existing) = keypair {
        existing
    } else {
        KeyPair::generate()?
    };
    kp.ensure_encryption_keys();

    let mut profile = if let Some(existing) = existing_profile {
        let mut p = existing.profile;
        p.username = username;
        p.update(display_name, bio);
        p
    } else {
        let mut p = Profile::new(username, kp.get_public_info());
        p.update(display_name, bio);
        p
    };

    profile.avatar_hash = avatar_data_url
        .as_ref()
        .map(|v| blake3::hash(v.as_bytes()).to_hex().to_string());
    profile.avatar_data_url = avatar_data_url;
    profile.encryption_public_key = kp.enc_public_key.clone();
    // magnet_uri is derived after signing and must not be in signed bytes.
    profile.magnet_uri = None;

    let mut signed = SignedProfile::create(profile, &kp)?;
    signed.profile.magnet_uri = Some(signed.profile.generate_magnet_uri());
    Ok((kp, signed))
}

async fn add_contact_async(fingerprint: String, alias: String) -> Result<Contact, String> {
    let fp = fingerprint.trim().to_string();
    if fp.len() < 8 {
        return Err("Fingerprint too short".to_string());
    }

    let alias = if alias.trim().is_empty() {
        format!("contact-{}", &fp[..8])
    } else {
        alias.trim().to_string()
    };

    Ok(Contact {
        fingerprint: fp,
        alias,
        magnet_uri: None,
        avatar_data_url: None,
        auto_synced: false,
        last_sync_label: "pending".to_string(),
        profile_summary: "Awaiting peer profile sync".to_string(),
        latest_post_preview: "Awaiting peer post sync".to_string(),
        verification: VerificationState::Unknown,
        trust_score: default_trust(),
        synced_post_count: 0,
        known_public_key: None,
        known_encryption_public_key: None,
        last_sync_error: None,
    })
}

async fn create_post_async(
    author_fingerprint: String,
    content: String,
    keypair: Option<KeyPair>,
) -> Result<SignedPost, String> {
    let kp = keypair.ok_or("No keypair available")?;
    if content.trim().is_empty() {
        return Err("Post cannot be empty".to_string());
    }
    let post = Post::new(author_fingerprint, content, None, None);
    SignedPost::create(post, &kp)
}

async fn create_message_async(
    sender_fingerprint: String,
    recipient_fingerprint: String,
    content: String,
    keypair: Option<KeyPair>,
    recipient_encryption_public_key: String,
) -> Result<SignedMessage, String> {
    let mut kp = keypair.ok_or("No keypair available")?;
    kp.ensure_encryption_keys();
    if content.trim().is_empty() {
        return Err("Message cannot be empty".to_string());
    }

    let (ciphertext_b64, nonce_b64, alg) =
        kp.encrypt_for_recipient(&recipient_encryption_public_key, &content)?;

    let mut msg = CoreMessage::new_direct(sender_fingerprint, recipient_fingerprint, ciphertext_b64);
    msg.encrypted = true;
    msg.body_enc = Some(alg);
    msg.nonce_b64 = Some(nonce_b64);
    SignedMessage::create(msg, &kp)
}

fn default_trust() -> u8 {
    20
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn short_fp(fp: &str) -> String {
    if fp.len() <= 12 {
        return fp.to_string();
    }
    format!("{}...{}", &fp[..8], &fp[fp.len() - 4..])
}

fn short_payload(payload: &str) -> String {
    const PREVIEW: usize = 28;
    if payload.len() <= PREVIEW {
        payload.to_string()
    } else {
        format!("{}...", &payload[..PREVIEW])
    }
}

fn decrypt_for_display(
    item: &ChatItem,
    keypair: Option<&KeyPair>,
    peer_enc_public_key: Option<&str>,
) -> Result<String, String> {
    if !item.encrypted {
        return Ok(item.content.clone());
    }

    if !item.verified_sender {
        return Err("sender signature not verified".to_string());
    }

    let kp = keypair.ok_or_else(|| "missing local keypair".to_string())?;
    let peer_key = peer_enc_public_key.ok_or_else(|| "missing peer encryption key".to_string())?;
    let nonce = item
        .nonce_b64
        .as_deref()
        .ok_or_else(|| "missing nonce".to_string())?;
    kp.decrypt_from_peer(peer_key, nonce, &item.content)
}

fn ts_label() -> String {
    format!("t={}", unix_secs())
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn configured_peer_count() -> usize {
    std::env::var("SNARTNET_PEERS")
        .ok()
        .map(|v| v.split(',').filter(|p| !p.trim().is_empty()).count())
        .unwrap_or(0)
}

fn local_swarm_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Cannot determine home directory".to_string())?;
    let root = PathBuf::from(home).join(".snartnet").join("swarm");
    std::fs::create_dir_all(&root).map_err(|e| format!("failed to create swarm dir: {e}"))?;
    Ok(root)
}

fn app_output_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let downloads = PathBuf::from(home).join("Downloads");
        if downloads.is_dir() {
            return downloads;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

fn qr_export_path(ext: &str) -> PathBuf {
    app_output_dir().join(format!("snartnet-invite-{}.{}", unix_secs(), ext))
}

fn qr_code_from_data(data: &str) -> Result<qrcode::QrCode, String> {
    qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::L)
        .map_err(|e| format!("QR generation failed: {e}"))
}

fn render_qr_svg_string(data: &str) -> Result<String, String> {
    use qrcode::render::svg as qrcode_svg;

    let code = qr_code_from_data(data)?;
    Ok(code
        .render::<qrcode_svg::Color>()
        .quiet_zone(true)
        .min_dimensions(280, 280)
        .dark_color(qrcode_svg::Color("#000000"))
        .light_color(qrcode_svg::Color("#ffffff"))
        .build())
}

fn save_qr_svg(data: &str) -> Result<PathBuf, String> {
    let path = qr_export_path("svg");
    let svg = render_qr_svg_string(data)?;
    std::fs::write(&path, svg).map_err(|e| format!("write failed: {e}"))?;
    Ok(path)
}

fn save_qr_png(data: &str) -> Result<PathBuf, String> {
    let path = qr_export_path("png");
    let code = qr_code_from_data(data)?;
    let image = code
        .render::<::image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(640, 640)
        .build();
    image
        .save(&path)
        .map_err(|e| format!("png save failed: {e}"))?;
    Ok(path)
}

fn save_qr_jpg(data: &str) -> Result<PathBuf, String> {
    let path = qr_export_path("jpg");
    let code = qr_code_from_data(data)?;
    let image = code
        .render::<::image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(640, 640)
        .build();
    let dynimg = ::image::DynamicImage::ImageLuma8(image);
    dynimg
        .save_with_format(&path, ::image::ImageFormat::Jpeg)
        .map_err(|e| format!("jpg save failed: {e}"))?;
    Ok(path)
}

fn image_handle_from_data_url(data_url: &str) -> Option<iced::widget::image::Handle> {
    let (_, b64) = data_url.split_once("base64,")?;
    let bytes = general_purpose::STANDARD.decode(b64).ok()?;
    Some(iced::widget::image::Handle::from_bytes(bytes))
}

fn load_avatar_data_url_from_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("empty path".to_string());
    }

    let img = ::image::open(path).map_err(|e| format!("open failed: {e}"))?;
    let resized = img.resize(256, 256, ::image::imageops::FilterType::Lanczos3);
    let mut png = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut png), ::image::ImageFormat::Png)
        .map_err(|e| format!("encode failed: {e}"))?;

    let b64 = general_purpose::STANDARD.encode(png);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Render `data` as an SVG QR image handle suitable for display in iced.
fn generate_qr_svg_handle(data: &str) -> svg::Handle {
    match render_qr_svg_string(data) {
        Ok(svg_text) => svg::Handle::from_memory(svg_text.into_bytes()),
        Err(_) => svg::Handle::from_memory(
            r#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 280 280' width='280' height='280'><rect width='100%' height='100%' fill='white'/><text x='12' y='28' font-size='16' font-family='monospace' fill='black'>QR generation failed</text></svg>"#
                .as_bytes()
                .to_vec(),
        ),
    }
}

/// Decode a base64 invite code and construct a pending `Contact` from it.
async fn import_invite_async(code: String) -> Result<Contact, String> {
    let invite = ContactInvite::from_base64(&code)?;
    let alias = invite
        .display_name
        .as_ref()
        .filter(|d| !d.is_empty())
        .cloned()
        .unwrap_or_else(|| invite.username.clone());
    Ok(Contact {
        fingerprint: invite.fingerprint,
        alias,
        magnet_uri: invite.magnet_uri,
        avatar_data_url: None,
        auto_synced: false,
        last_sync_label: "pending".to_string(),
        profile_summary: "Awaiting peer profile sync".to_string(),
        latest_post_preview: "Awaiting peer post sync".to_string(),
        verification: VerificationState::Unknown,
        trust_score: default_trust(),
        synced_post_count: 0,
        known_public_key: None,
        known_encryption_public_key: None,
        last_sync_error: None,
    })
}

async fn import_magnet_async(uri: String) -> Result<Contact, String> {
    let fingerprint = profile_fingerprint_from_magnet_uri(&uri)?;
    let alias = short_fp(&fingerprint);
    Ok(Contact {
        fingerprint,
        alias,
        magnet_uri: Some(uri),
        avatar_data_url: None,
        auto_synced: false,
        last_sync_label: "pending".to_string(),
        profile_summary: "Awaiting peer profile sync".to_string(),
        latest_post_preview: "Awaiting peer post sync".to_string(),
        verification: VerificationState::Unknown,
        trust_score: default_trust(),
        synced_post_count: 0,
        known_public_key: None,
        known_encryption_public_key: None,
        last_sync_error: None,
    })
}

fn main() -> iced::Result {
    configure_linux_wgpu_backend();

    iced::application("SnartNet", App::update, App::view)
        .subscription(App::subscription)
        .window_size((1100.0, 760.0))
        .run_with(App::new)
}

fn configure_linux_wgpu_backend() {
    #[cfg(target_os = "linux")]
    {
        // Vulkan can fail with "Parent device is lost" on some Linux VMs/CI runners.
        // If the user has not chosen a backend explicitly, prefer OpenGL for stability.
        if env::var_os("WGPU_BACKEND").is_none() {
            env::set_var("WGPU_BACKEND", "gl");
        }
    }
}
