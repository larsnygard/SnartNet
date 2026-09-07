//! SnartNet desktop host: event handling and persistence. Screens, transport,
//! invitation helpers, and background synchronization live in dedicated modules.

mod actions;
mod design;
mod discovery;
mod media;
mod model;
mod sync;
mod transport;
mod views;

use actions::*;
use media::*;
use model::*;

use base64::{engine::general_purpose, Engine as _};
use iced::{
    time,
    widget::{button, container, image, row, scrollable, svg, text, text_input},
    Alignment, Element, Length, Subscription, Task,
};
use snartnet_core::{
    profile_fingerprint_from_magnet_uri, ContactInvite, FileStorage, KeyPair,
    Message as CoreMessage, Post, Profile, SignedMessage, SignedPost, SignedProfile,
};
use std::{
    collections::HashSet,
    io::Cursor,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use transport::{NetworkTransport, SwarmPostsBlob, SwarmProfileBlob, TcpSwarmTransport};

use discovery::{DiscoveredPeer, LanAnnounce, LanDiscovery};

const STORAGE_KEYPAIR: &str = "keypair";
const STORAGE_PROFILE: &str = "profile";
const STORAGE_POSTS: &str = "local_posts";
const STORAGE_CONTACTS: &str = "contacts";
const STORAGE_THREADS: &str = "threads";
const LOCAL_SWARM_FILE_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
enum Message {
    StartupLoaded(Result<StartupData, String>),
    Tick(Instant),
    RunSyncNow,
    SyncFinished(Result<sync::SyncResult, String>),
    SearchChanged(String),
    QrPathChanged(String),
    ImportQr,
    AdvertiseAddrChanged(String),
    SwitchPanel(Panel),

    UsernameChanged(String),
    DisplayNameChanged(String),
    BioChanged(String),
    AvatarPathChanged(String),
    LoadAvatarFromPath,
    ClearAvatar,
    SaveProfile,
    ProfileSaved(Result<(KeyPair, SignedProfile), String>),
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
    SendChat,
    ChatCreated(Result<SignedMessage, String>),

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
    syncing: bool,
    loaded: bool,
    saving_profile: bool,
    sending: Option<(String, String)>,
    listener_error: Option<String>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let storage = FileStorage::open_default().expect(
            "Cannot open SnartNet storage; check directory permissions or set SNARTNET_HOME",
        );
        let transport = TcpSwarmTransport::from_env().expect("transport init failed");
        let listener_error = transport.start_server().err();

        let app = Self {
            panel: Panel::Messages,
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
            syncing: false,
            loaded: false,
            saving_profile: false,
            sending: None,
            listener_error,
        };

        (
            app,
            Task::perform(load_startup_async(), Message::StartupLoaded),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartupLoaded(result) => {
                let data = match result {
                    Ok(data) => data,
                    Err(error) => {
                        self.status_line = format!("Could not load local data: {error}");
                        return Task::none();
                    }
                };
                self.loaded = true;
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
                self.forms.advertise_addr = self
                    .storage
                    .get_json::<String>("advertise_addr")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                self.forms.selected_contact_for_chat =
                    self.contacts.first().map(|c| c.fingerprint.clone());
                // CLI arguments accept the exact same invitation links as paste and QR import.
                if let Some(invite) = std::env::args()
                    .skip(1)
                    .find(|arg| arg.starts_with("snartnet://invite/"))
                {
                    self.forms.invite_code_input = invite;
                    self.panel = Panel::Contacts;
                }

                if let Some(sp) = &self.profile {
                    self.forms.username_input = sp.profile.username.clone();
                    self.forms.display_name_input =
                        sp.profile.display_name.clone().unwrap_or_default();
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
                self.run_peer_sync()
            }
            Message::Tick(_instant) => {
                // Refresh the LAN peer snapshot so the UI stays current.
                self.discovered_peers = self.lan_discovery.get_discovered();
                self.network.discovered_peer_count = self.discovered_peers.len();
                self.refresh_transport_peers_from_discovery();
                self.run_peer_sync()
            }
            Message::RunSyncNow => self.run_peer_sync(),
            Message::SyncFinished(result) => {
                self.syncing = false;
                match result {
                    Ok(result) => self.apply_sync(result),
                    Err(error) => {
                        self.status_line = format!("Sync failed: {error}");
                        Task::none()
                    }
                }
            }
            Message::SearchChanged(value) => {
                self.forms.search = value;
                Task::none()
            }
            Message::QrPathChanged(value) => {
                self.forms.qr_path = value;
                Task::none()
            }
            Message::ImportQr => {
                let path = self.forms.qr_path.trim().to_string();
                Task::perform(
                    async move {
                        let code = tokio::task::spawn_blocking(move || read_qr_file(&path))
                            .await
                            .map_err(|e| e.to_string())??;
                        import_invite_async(code).await
                    },
                    Message::InviteImported,
                )
            }
            Message::AdvertiseAddrChanged(value) => {
                self.forms.advertise_addr = value;
                Task::none()
            }
            Message::SwitchPanel(panel) => {
                self.panel = panel;
                if panel == Panel::Messages {
                    self.mark_selected_thread_read();
                    return scroll_to_latest();
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
                if self.saving_profile {
                    return Task::none();
                }
                if !self.forms.advertise_addr.trim().is_empty() {
                    if let Err(error) = validate_endpoint(self.forms.advertise_addr.trim()) {
                        self.status_line = error;
                        return Task::none();
                    }
                }
                self.saving_profile = true;
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
                self.saving_profile = false;
                match result {
                    Ok((kp, sp)) => {
                        // Persist identity before publishing it or accepting chat input.
                        if let Err(error) = self
                            .storage
                            .set_json(STORAGE_KEYPAIR, &kp)
                            .and_then(|_| self.storage.set_json(STORAGE_PROFILE, &sp))
                        {
                            self.status_line = format!("Could not save profile: {error}");
                            return Task::none();
                        }
                        self.keypair = Some(kp.clone());
                        self.forms.avatar_data_url = sp.profile.avatar_data_url.clone();
                        self.profile = Some(sp.clone());

                        if let Err(error) = self
                            .storage
                            .set_json("advertise_addr", &self.forms.advertise_addr)
                        {
                            self.status_line = format!(
                                "Profile saved; connection address could not be saved: {error}"
                            );
                        } else {
                            self.status_line =
                                "Profile saved. Your invitation is ready to share.".into();
                        }

                        self.publish_local_profile_to_swarm();
                        self.recalculate_network();
                        self.start_lan_discovery();
                        return self.run_peer_sync();
                    }
                    Err(e) => {
                        self.status_line = format!("Profile error: {e}");
                    }
                }
                Task::none()
            }

            Message::CopyInviteCode => {
                match self.invite_uri() {
                    Ok(uri) => {
                        self.status_line = "Invitation link copied. Share it with a friend.".into();
                        return iced::clipboard::write(uri);
                    }
                    Err(error) => self.status_line = error,
                }
                Task::none()
            }
            Message::CopyMagnetUri => {
                if let Some(uri) = self
                    .profile
                    .as_ref()
                    .and_then(|p| p.profile.magnet_uri.clone())
                {
                    self.status_line = "Profile magnet copied".into();
                    return iced::clipboard::write(uri);
                }
                Task::none()
            }
            Message::SaveQrSvg | Message::SaveQrPng | Message::SaveQrJpg => {
                let result = self.invite_uri().and_then(|uri| match message {
                    Message::SaveQrSvg => save_qr_svg(&uri),
                    Message::SaveQrPng => save_qr_png(&uri),
                    _ => save_qr_jpg(&uri),
                });
                self.status_line = match result {
                    Ok(path) => format!("QR saved to {}", path.display()),
                    Err(error) => format!("Could not save QR: {error}"),
                };
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
                let peer = self
                    .discovered_peers
                    .iter()
                    .find(|p| p.fingerprint == fp)
                    .cloned();
                if let Some(peer) = peer {
                    let alias = peer
                        .display_name
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| peer.username.clone());
                    Task::perform(
                        async move {
                            let mut contact = add_contact_async(peer.fingerprint, alias).await?;
                            contact.transport_addr = peer.tcp_addr;
                            Ok(contact)
                        },
                        Message::ContactAdded,
                    )
                } else {
                    Task::none()
                }
            }
            Message::ContactAdded(result) => {
                match result {
                    Ok(contact) => {
                        if self
                            .profile
                            .as_ref()
                            .is_some_and(|p| p.profile.fingerprint == contact.fingerprint)
                        {
                            self.status_line =
                                "This is your own invitation. Share it with a friend.".into();
                            return Task::none();
                        }
                        let previous = self.contacts.clone();
                        let fp = contact.fingerprint.clone();
                        if let Some(existing) =
                            self.contacts.iter_mut().find(|c| c.fingerprint == fp)
                        {
                            if contact.transport_addr.is_some() {
                                existing.transport_addr = contact.transport_addr;
                            }
                            self.status_line =
                                format!("Opened existing contact: {}", existing.alias);
                        } else {
                            self.status_line = format!("Added {}. Connecting…", contact.alias);
                            self.contacts.push(contact);
                        }
                        if let Err(error) = self.storage.set_json(STORAGE_CONTACTS, &self.contacts)
                        {
                            self.contacts = previous;
                            self.status_line = format!("Could not save contact: {error}");
                            return Task::none();
                        }
                        self.forms.contact_fingerprint_input.clear();
                        self.forms.contact_alias_input.clear();
                        self.ensure_thread(&fp);
                        self.select_chat(fp);
                        return Task::batch([self.run_peer_sync(), scroll_to_latest()]);
                    }
                    Err(error) => self.status_line = format!("Could not add contact: {error}"),
                }
                Task::none()
            }
            Message::SelectChatContact(fp) => {
                self.select_chat(fp);
                scroll_to_latest()
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
                        self.publish_local_posts_to_swarm();
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
            Message::SendChat => {
                if self.sending.is_some() {
                    return Task::none();
                }
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
                    .find(|c| {
                        c.fingerprint == recipient && c.verification == VerificationState::Verified
                    })
                    .and_then(|c| c.known_encryption_public_key.clone());

                if recipient_enc_public.is_none() {
                    self.status_line =
                        "Recipient profile missing encryption key. Run sync and try again."
                            .to_string();
                    return Task::none();
                }

                if content.trim().is_empty() {
                    return Task::none();
                }
                self.sending = Some((recipient.clone(), content.clone()));
                Task::perform(
                    create_message_async(
                        sender,
                        recipient,
                        content,
                        kp,
                        recipient_enc_public.unwrap_or_default(),
                    ),
                    Message::ChatCreated,
                )
            }
            Message::ChatCreated(result) => {
                let draft = self.sending.take();
                match result {
                    Ok(signed) => {
                        let recipient = signed.message.recipient_fingerprint.clone();
                        self.ensure_thread(&recipient);
                        let peer_key = self
                            .contacts
                            .iter()
                            .find(|c| c.fingerprint == recipient)
                            .and_then(|c| c.known_encryption_public_key.clone());
                        let thread = self
                            .threads
                            .iter_mut()
                            .find(|t| t.contact_fingerprint == recipient)
                            .unwrap();
                        thread
                            .messages
                            .push(ChatItem::from_signed(signed, false, peer_key));
                        // Save the signed outbox before clearing the draft or touching the network.
                        if let Err(error) = self.storage.set_json(STORAGE_THREADS, &self.threads) {
                            self.threads
                                .iter_mut()
                                .find(|t| t.contact_fingerprint == recipient)
                                .unwrap()
                                .messages
                                .pop();
                            self.status_line = format!("Message not queued: {error}");
                            return Task::none();
                        }
                        if let Some((fp, sent_text)) = draft {
                            if self.forms.selected_contact_for_chat.as_ref() == Some(&fp)
                                && self.forms.compose_message_input == sent_text
                            {
                                self.forms.compose_message_input.clear();
                            }
                            if self.forms.drafts.get(&fp) == Some(&sent_text) {
                                self.forms.drafts.remove(&fp);
                            }
                        }
                        self.status_line =
                            "Message queued. It will retry until a peer accepts it.".into();
                        return Task::batch([self.run_peer_sync(), scroll_to_latest()]);
                    }
                    Err(error) => self.status_line = format!("Message failed: {error}"),
                }
                Task::none()
            }

            Message::ToggleBittorrent => {
                self.network.bittorrent_running = !self.network.bittorrent_running;
                self.status_line = if self.network.bittorrent_running {
                    "Sync resumed".to_string()
                } else {
                    "Sync paused. Messages remain queued; the listener still receives peers."
                        .to_string()
                };
                self.recalculate_network();
                self.run_peer_sync()
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
        time::every(Duration::from_secs(self.network.poll_interval_secs)).map(Message::Tick)
    }

    fn select_chat(&mut self, fingerprint: String) {
        if let Some(previous) = self.forms.selected_contact_for_chat.take() {
            self.forms.drafts.insert(
                previous,
                std::mem::take(&mut self.forms.compose_message_input),
            );
        }
        self.forms.compose_message_input = self
            .forms
            .drafts
            .get(&fingerprint)
            .cloned()
            .unwrap_or_default();
        self.forms.selected_contact_for_chat = Some(fingerprint.clone());
        self.panel = Panel::Messages;
        self.mark_thread_read(&fingerprint);
    }

    fn invite_uri(&self) -> Result<String, String> {
        let profile = self.profile.as_ref().ok_or("Create your profile first")?;
        let address = if self.forms.advertise_addr.trim().is_empty() {
            self.transport.advertised_addr()
        } else {
            let value = self.forms.advertise_addr.trim();
            validate_endpoint(value)?;
            Some(value.to_string())
        };
        ContactInvite::from_signed_profile(profile, address).to_uri()
    }

    fn cleanup_local_swarm_files(&mut self) {
        let swarm_dir = self.transport.swarm_dir().to_path_buf();

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
            let fp = transport::sanitize_component(&profile.profile.fingerprint);
            active.insert(format!("profile_{fp}.json"));
            active.insert(format!("posts_{fp}.json"));
            active.insert(format!("inbox_{fp}.json"));
        }

        for contact in &self.contacts {
            let fp = transport::sanitize_component(&contact.fingerprint);
            active.insert(format!("profile_{fp}.json"));
            active.insert(format!("posts_{fp}.json"));
            active.insert(format!("inbox_{fp}.json"));
        }

        active
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
            if let Err(e) = self
                .transport
                .save_profile(&profile.profile.fingerprint, &blob)
            {
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
            if let Err(e) = self
                .transport
                .save_posts(&profile.profile.fingerprint, &blob)
            {
                self.status_line = format!("Post publish failed: {e}");
            }
        }
    }

    fn recalculate_network(&mut self) {
        self.network.peers = self.transport.peer_snapshot().len() as u32;
    }

    /// Attempt to start LAN discovery for the current profile.
    /// Sets `network.lan_discovery_active` to reflect the outcome.
    fn start_lan_discovery(&mut self) {
        let Some(sp) = &self.profile else { return };
        // Use the actual LAN IP of this host rather than 0.0.0.0 so that
        // peers receiving the broadcast can actually connect back.
        let tcp_addr = self.transport.advertised_addr();
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

        for contact in &self.contacts {
            if let Some(addr) = contact
                .transport_addr
                .as_deref()
                .and_then(|value| value.parse().ok())
            {
                if !peers.contains(&addr) {
                    peers.push(addr);
                }
            }
        }
        for peer in &self.discovered_peers {
            if let Some(addr) = peer
                .tcp_addr
                .as_deref()
                .and_then(|value| value.parse().ok())
            {
                if !peers.contains(&addr) {
                    peers.push(addr);
                }
            }
        }

        self.transport.set_peers(peers);
    }
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
    if fp.chars().count() <= 12 {
        return fp.into();
    }
    format!(
        "{}…{}",
        fp.chars().take(8).collect::<String>(),
        fp.chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    )
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

    if item.encryption_alg.as_deref() != Some("chacha20poly1305-x25519-v1") {
        return Err("unsupported encryption format".into());
    }
    let kp = keypair.ok_or_else(|| "missing local keypair".to_string())?;
    let peer_key = item
        .peer_encryption_key
        .as_deref()
        .or(peer_enc_public_key)
        .ok_or_else(|| "missing peer encryption key".to_string())?;
    let nonce = item
        .nonce_b64
        .as_deref()
        .ok_or_else(|| "missing nonce".to_string())?;
    kp.decrypt_from_peer(peer_key, nonce, &item.content)
}

fn ts_label() -> String {
    chrono::Utc::now().format("%H:%M:%S UTC").to_string()
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn validate_endpoint(value: &str) -> Result<(), String> {
    let endpoint: std::net::SocketAddr = value
        .parse()
        .map_err(|_| "Use an IP address and port, such as 192.168.1.5:47470")?;
    if endpoint.ip().is_unspecified() || endpoint.ip().is_multicast() || endpoint.port() == 0 {
        return Err("Use a reachable IP address and a nonzero port".into());
    }
    Ok(())
}

fn scroll_to_latest() -> Task<Message> {
    scrollable::snap_to(
        scrollable::Id::new("chat-history"),
        scrollable::RelativeOffset::END,
    )
}

fn main() -> iced::Result {
    // Let wgpu auto-detect the best available backend (Vulkan/Metal/DX12, falling
    // back to GL) rather than forcing one. Forcing GL previously caused a panic in
    // `wgpu-core`'s surface creation on systems where the GL/EGL instance fails to
    // initialize (its GL instance ends up `None`, which the crate unconditionally
    // unwraps). Users who need a specific backend can still set `WGPU_BACKEND`.
    iced::application("SnartNet", App::update, App::view)
        .subscription(App::subscription)
        .theme(|_| design::theme())
        .window(iced::window::Settings {
            size: (1180.0, 800.0).into(),
            min_size: Some((1080.0, 680.0).into()),
            ..Default::default()
        })
        .run_with(App::new)
}

#[cfg(test)]
mod tests;
