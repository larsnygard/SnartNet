//! SnartNet desktop host: rendering, input, and daemon supervision only.
//!
//! Every piece of state and every action is served by the local daemon through
//! `snartnet-sdk` (ADR 0001, `docs/LOCAL_API.md`). This process owns no
//! identity, database, or peer listener, so closing the window never stops
//! background work.

mod backend;
mod design;
mod media;
mod model;
mod state;
mod tray;
mod views;

#[cfg(test)]
mod tests;

use backend::{Backend, Invite};
use base64::{engine::general_purpose, Engine as _};
use media::*;
use model::*;
use snartnet_core::ContactInvite;
use state::DaemonState;

use iced::{
    time,
    widget::{button, container, image, row, scrollable, svg, text, text_input},
    window, Alignment, Element, Length, Subscription, Task,
};
use snartnet_sdk::{Command, SyncMode};
use std::{
    collections::HashSet,
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Close enough to the daemon's scheduler to feel live without busy waiting.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
enum Message {
    /// First snapshot after launch; drives the loading screen.
    Started(Result<DaemonState, String>),
    /// Periodic and post-action refresh.
    Refreshed(Result<DaemonState, String>),
    /// Ticks carry no data; the poll interval is the whole schedule.
    Tick,
    StartDaemon,
    RunSyncNow,
    SyncFinished(Result<usize, String>),
    /// Result of one daemon command, labelled for the status line.
    ActionFinished {
        done: &'static str,
        failed: &'static str,
        result: Result<(), String>,
    },
    SyncModeChanged(SyncMode),

    SearchChanged(String),
    QrPathChanged(String),
    ImportQr,
    QrImported(Result<String, String>),
    AdvertiseAddrChanged(String),
    SwitchPanel(Panel),

    UsernameChanged(String),
    DisplayNameChanged(String),
    BioChanged(String),
    AvatarPathChanged(String),
    LoadAvatarFromPath,
    BrowseForAvatar,
    AvatarFileSelected(Result<Option<PathBuf>, String>),
    CaptureAvatarFromCamera,
    AvatarCaptured(Result<String, String>),
    ClearAvatar,
    SaveProfile,
    ProfileSaved(Result<(), String>),
    CopyInviteCode,
    CopyMagnetUri,
    /// Daemon-generated invitation link, cached for QR rendering.
    InviteRefreshed(Result<Invite, String>),
    InviteCopied(Result<Invite, String>),
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
    ImportFromMagnet,
    AddDiscoveredPeer(String),
    SelectChatContact(String),
    /// Result of one contact import, whichever flow started it.
    ContactAdded(Result<(), String>),

    ComposePostChanged(String),
    CreatePost,
    PostPublished(Result<(), String>),

    ComposeMessageChanged(String),
    ToggleMessageView(String),
    SendChat,
    ChatSent(Result<(), String>),

    LanDiscoveryToggle,
    CleanupLocalFiles,

    /// Window close button: hide instead of exiting; the daemon keeps running.
    CloseRequested(window::Id),
    /// Menu click forwarded from the tray's own thread.
    Tray(tray::TrayCommand),
    /// A tray request to stop the daemon first finished; `None` means clean.
    Stopped(Option<String>),
}

struct App {
    backend: Arc<Backend>,
    tray: Option<tray::TrayHandle>,
    state: DaemonState,
    panel: Panel,
    forms: FormState,
    /// Message IDs currently shown as stored ciphertext; runtime only.
    revealed_message_ids: HashSet<String>,
    status_line: String,
    syncing: bool,
    loaded: bool,
    /// The daemon is missing, so the loader offers to start it explicitly.
    daemon_unreachable: bool,
    refreshing: bool,
    saving_profile: bool,
    sending: Option<(String, String)>,
    /// Latest daemon invitation link; the key never reaches this process.
    invite: Option<Invite>,
    forms_seeded: bool,
    /// Contact whose import is in flight, so its chat opens when it lands.
    pending_contact: Option<String>,
    /// Main window id, needed to show the window again from the tray.
    window: Option<window::Id>,
}

impl App {
    fn with_backend(
        backend: Arc<Backend>,
        tray: Option<tray::TrayHandle>,
    ) -> (Self, Task<Message>) {
        let app = Self {
            backend: backend.clone(),
            state: DaemonState::default(),
            panel: Panel::Messages,
            forms: FormState::default(),
            revealed_message_ids: HashSet::new(),
            status_line: "Connecting to the SnartNet daemon…".to_string(),
            syncing: false,
            loaded: false,
            daemon_unreachable: false,
            refreshing: true,
            saving_profile: false,
            sending: None,
            invite: None,
            tray,
            forms_seeded: false,
            pending_contact: None,
            window: None,
        };
        (app, Task::perform(snapshot_task(backend), Message::Started))
    }

    /// Request the authoritative snapshot off the UI thread.
    fn refresh(&self) -> Task<Message> {
        Task::perform(snapshot_task(self.backend.clone()), Message::Refreshed)
    }

    /// The runtime consumes the `Task` a reducer returns; tests only assert on
    /// state, so they go through this to drop it deliberately.
    #[cfg(test)]
    pub(crate) fn dispatch(&mut self, message: Message) {
        let _ = self.handle_message(message);
    }

    /// Send one daemon command, then pick up whatever the daemon committed.
    fn run_command(
        &self,
        done: &'static str,
        failed: &'static str,
        command: Command,
    ) -> Task<Message> {
        let backend = self.backend.clone();
        Task::perform(
            async move { backend.command(command).map(|_| ()) },
            move |result| Message::ActionFinished {
                done,
                failed,
                result,
            },
        )
    }

    fn refresh_invite(&self) -> Task<Message> {
        let backend = self.backend.clone();
        Task::perform(async move { backend.invite() }, Message::InviteRefreshed)
    }

    fn apply_state(&mut self, state: DaemonState) {
        self.state = state;
        // Seed the profile form once so typing is never overwritten by a poll.
        if !self.forms_seeded {
            if let Some(profile) = &self.state.profile {
                self.forms.username_input = profile.username.clone();
                self.forms.display_name_input = profile.display_name.clone().unwrap_or_default();
                self.forms.bio_input = profile.bio.clone().unwrap_or_default();
                self.forms.avatar_data_url = profile.avatar_data_url.clone();
                self.forms.advertise_addr = self.state.network.address_override.clone();
                self.forms_seeded = true;
            }
        }
    }

    fn after_startup(&mut self) {
        self.loaded = true;
        self.status_line = if self.state.profile.is_some() {
            "Connected to the daemon.".to_string()
        } else {
            "Create a profile to start connecting.".to_string()
        };
    }

    /// Invitation links are daemon-generated; this only reports the cache.
    fn invite_uri(&self) -> Result<String, String> {
        self.invite
            .as_ref()
            .map(|invite| invite.uri.clone())
            .ok_or_else(|| {
                "The invitation link is not ready yet. Try again in a moment.".to_string()
            })
    }

    fn total_unread_count(&self) -> u32 {
        self.state.total_unread()
    }

    /// iced entry point: run the reducer, then mirror the status into the tray.
    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle_message(message);
        if let Some(tray) = &self.tray {
            tray.set_status(&self.status_line);
        }
        task
    }

    /// Every state change happens here, so the tray tooltip and the window can
    /// never disagree about what the daemon is doing.
    fn handle_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Started(result) | Message::Refreshed(result) => {
                self.refreshing = false;
                match result {
                    Ok(state) => {
                        let first = !self.loaded;
                        let had_profile = self.state.profile.is_some();
                        self.daemon_unreachable = false;
                        self.apply_state(state);
                        if first {
                            self.after_startup();
                        }
                        if self.state.profile.is_some() && (!had_profile || self.invite.is_none()) {
                            return self.refresh_invite();
                        }
                    }
                    Err(error) => {
                        self.status_line = if self.loaded {
                            error
                        } else {
                            self.daemon_unreachable = true;
                            format!("{error} Start it with the button below, then retry.")
                        };
                    }
                }
                Task::none()
            }
            Message::Tick => {
                if self.refreshing || !self.loaded {
                    return Task::none();
                }
                self.refreshing = true;
                self.refresh()
            }
            Message::StartDaemon => {
                self.status_line = "Starting the SnartNet daemon…".to_string();
                let backend = self.backend.clone();
                Task::perform(
                    async move {
                        backend.ensure_running()?;
                        backend
                            .snapshot()
                            .and_then(|snapshot| DaemonState::from_snapshot(&snapshot))
                    },
                    Message::Started,
                )
            }
            Message::RunSyncNow => {
                if self.syncing {
                    return Task::none();
                }
                self.syncing = true;
                self.status_line = "Syncing with peers…".to_string();
                let backend = self.backend.clone();
                Task::perform(async move { backend.sync() }, Message::SyncFinished)
            }
            Message::SyncFinished(result) => {
                self.syncing = false;
                match result {
                    Ok(received) => {
                        self.status_line =
                            format!("Sync complete. Received {received} new item(s).");
                        self.refresh()
                    }
                    Err(error) => {
                        self.status_line = format!("Sync failed: {error}");
                        Task::none()
                    }
                }
            }
            Message::ActionFinished {
                done,
                failed,
                result,
            } => match result {
                Ok(()) => {
                    self.status_line = done.to_string();
                    self.refresh()
                }
                Err(error) => {
                    self.status_line = format!("{failed}: {error}");
                    Task::none()
                }
            },
            Message::SyncModeChanged(mode) => self.set_sync_mode(
                mode,
                match mode {
                    SyncMode::AlwaysOn => "Sync mode: always on.",
                    SyncMode::Balanced => "Sync mode: balanced.",
                    SyncMode::Paused => {
                        "Sync paused. The daemon keeps seeding what it already has."
                    }
                },
            ),
            Message::SearchChanged(value) => {
                self.forms.search = value;
                Task::none()
            }
            Message::SwitchPanel(panel) => {
                self.panel = panel;
                Task::none()
            }
            Message::QrPathChanged(value) => {
                self.forms.qr_path = value;
                Task::none()
            }
            Message::ImportQr => {
                let path = self.forms.qr_path.trim().to_string();
                if path.is_empty() {
                    self.status_line = "Choose a QR image first".to_string();
                    return Task::none();
                }
                Task::perform(
                    async move { tokio::task::spawn_blocking(move || read_qr_file(&path)).await },
                    |joined| match joined {
                        Ok(result) => Message::QrImported(result),
                        Err(error) => Message::QrImported(Err(error.to_string())),
                    },
                )
            }
            Message::QrImported(result) => match result {
                Ok(code) => self.import_contact(
                    "Invitation imported. Syncing the new contact…",
                    code,
                    "invite",
                    String::new(),
                    String::new(),
                ),
                Err(error) => {
                    self.status_line = format!("Could not read that QR image: {error}");
                    Task::none()
                }
            },
            Message::AdvertiseAddrChanged(value) => {
                self.forms.advertise_addr = value;
                Task::none()
            }
            Message::UsernameChanged(value) => {
                self.forms.username_input = value;
                Task::none()
            }
            Message::DisplayNameChanged(value) => {
                self.forms.display_name_input = value;
                Task::none()
            }
            Message::BioChanged(value) => {
                self.forms.bio_input = value;
                Task::none()
            }
            Message::AvatarPathChanged(value) => {
                self.forms.avatar_path_input = value;
                Task::none()
            }
            Message::LoadAvatarFromPath => {
                let path = self.forms.avatar_path_input.trim().to_string();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || load_avatar_data_url_from_path(&path))
                            .await
                    },
                    |joined| match joined {
                        Ok(result) => Message::AvatarCaptured(result),
                        Err(error) => Message::AvatarCaptured(Err(error.to_string())),
                    },
                )
            }
            Message::BrowseForAvatar => Task::perform(
                async {
                    let selected = rfd::AsyncFileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                        .pick_file()
                        .await
                        .map(|handle| handle.path().to_path_buf());
                    // iced tasks need one concrete return type, and the picker
                    // itself cannot fail, so the error arm stays unused.
                    Ok::<_, String>(selected)
                },
                Message::AvatarFileSelected,
            ),
            Message::AvatarFileSelected(result) => match result {
                Ok(Some(path)) => {
                    let path = path.to_string_lossy().into_owned();
                    self.forms.avatar_path_input = path.clone();
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                load_avatar_data_url_from_path(&path)
                            })
                            .await
                        },
                        |joined| match joined {
                            Ok(result) => Message::AvatarCaptured(result),
                            Err(error) => Message::AvatarCaptured(Err(error.to_string())),
                        },
                    )
                }
                Ok(None) => Task::none(),
                Err(error) => {
                    self.status_line = format!("Could not open the image picker: {error}");
                    Task::none()
                }
            },
            Message::CaptureAvatarFromCamera => Task::perform(
                async { tokio::task::spawn_blocking(capture_avatar_from_default_camera).await },
                |joined| match joined {
                    Ok(result) => Message::AvatarCaptured(result),
                    Err(error) => Message::AvatarCaptured(Err(error.to_string())),
                },
            ),
            Message::AvatarCaptured(result) => {
                match result {
                    Ok(data_url) => {
                        self.forms.avatar_data_url = Some(data_url);
                        self.status_line = "Profile picture ready to save".to_string();
                    }
                    Err(error) => self.status_line = format!("Could not load that image: {error}"),
                }
                Task::none()
            }
            Message::ClearAvatar => {
                self.forms.avatar_data_url = None;
                self.status_line = "Profile picture cleared. Save to apply it.".to_string();
                Task::none()
            }
            Message::SaveProfile => {
                if self.saving_profile {
                    return Task::none();
                }
                let address = self.forms.advertise_addr.trim().to_string();
                if let Err(error) = validate_endpoint(&address) {
                    self.status_line = error;
                    return Task::none();
                }
                self.saving_profile = true;
                let command = Command::Profile {
                    username: self.forms.username_input.trim().to_string(),
                    display_name: self.forms.display_name_input.clone(),
                    bio: self.forms.bio_input.clone(),
                    avatar: self.forms.avatar_data_url.clone().unwrap_or_default(),
                    address,
                };
                let backend = self.backend.clone();
                Task::perform(
                    async move { backend.command(command).map(|_| ()) },
                    Message::ProfileSaved,
                )
            }
            Message::ProfileSaved(result) => {
                self.saving_profile = false;
                match result {
                    Ok(()) => {
                        self.status_line =
                            "Profile saved. Your invitation is ready to share.".to_string();
                        Task::batch([self.refresh(), self.refresh_invite()])
                    }
                    Err(error) => {
                        self.status_line = format!("Could not save the profile: {error}");
                        Task::none()
                    }
                }
            }
            Message::CopyInviteCode => {
                let backend = self.backend.clone();
                Task::perform(async move { backend.invite() }, Message::InviteCopied)
            }
            Message::InviteRefreshed(result) => {
                match result {
                    Ok(invite) => self.invite = Some(invite),
                    Err(error) => self.status_line = error,
                }
                Task::none()
            }
            Message::InviteCopied(result) => match result {
                Ok(invite) => {
                    self.status_line =
                        "Invitation link copied. Share it with a friend.".to_string();
                    self.invite = Some(invite.clone());
                    iced::clipboard::write(invite.uri)
                }
                Err(error) => {
                    self.status_line = error;
                    Task::none()
                }
            },
            Message::CopyMagnetUri => {
                let magnet = self
                    .invite
                    .as_ref()
                    .and_then(|invite| invite.magnet.clone())
                    .or_else(|| {
                        self.state
                            .profile
                            .as_ref()
                            .and_then(|profile| profile.magnet_uri.clone())
                    });
                match magnet {
                    Some(uri) => {
                        self.status_line = "Profile magnet copied".to_string();
                        iced::clipboard::write(uri)
                    }
                    None => {
                        self.status_line =
                            "No profile magnet yet. Save your profile and sync again.".to_string();
                        Task::none()
                    }
                }
            }
            Message::ContactFingerprintChanged(value) => {
                self.forms.contact_fingerprint_input = value;
                Task::none()
            }
            Message::ContactAliasChanged(value) => {
                self.forms.contact_alias_input = value;
                Task::none()
            }
            Message::AddContactModeChanged(mode) => {
                self.forms.add_contact_mode = mode;
                Task::none()
            }
            Message::InviteCodeChanged(value) => {
                self.forms.invite_code_input = value;
                Task::none()
            }
            Message::MagnetUriChanged(value) => {
                self.forms.magnet_uri_input = value;
                Task::none()
            }
            Message::AddContact => {
                let fingerprint = self.forms.contact_fingerprint_input.trim().to_string();
                let alias = self.forms.contact_alias_input.trim().to_string();
                if fingerprint.is_empty() {
                    self.status_line = "Enter the contact's fingerprint".to_string();
                    return Task::none();
                }
                self.import_contact(
                    "Contact added. Syncing their profile…",
                    fingerprint,
                    "manual",
                    alias,
                    String::new(),
                )
            }
            Message::ImportFromInvite => {
                let code = self.forms.invite_code_input.trim().to_string();
                if code.is_empty() {
                    self.status_line = "Paste an invitation code first".to_string();
                    return Task::none();
                }
                let alias = self.forms.contact_alias_input.trim().to_string();
                self.import_contact(
                    "Invitation imported. Syncing the new contact…",
                    code,
                    "invite",
                    alias,
                    String::new(),
                )
            }
            Message::ImportFromMagnet => {
                let uri = self.forms.magnet_uri_input.trim().to_string();
                if uri.is_empty() {
                    self.status_line = "Paste a profile magnet URI first".to_string();
                    return Task::none();
                }
                // The daemon recognises magnet inputs from their prefix.
                self.import_contact(
                    "Profile link imported. Syncing the new contact…",
                    uri,
                    "invite",
                    String::new(),
                    String::new(),
                )
            }
            Message::AddDiscoveredPeer(fingerprint) => {
                let address = self
                    .state
                    .nearby
                    .iter()
                    .find(|peer| peer.fingerprint == fingerprint)
                    .and_then(|peer| peer.address.clone())
                    .unwrap_or_default();
                self.import_contact(
                    "Contact added from discovery. Syncing their profile…",
                    fingerprint,
                    "manual",
                    String::new(),
                    address,
                )
            }
            Message::ContactAdded(result) => {
                let fingerprint = self.pending_contact.take();
                match result {
                    Ok(()) => {
                        if let Some(fingerprint) = fingerprint {
                            self.forms.contact_fingerprint_input.clear();
                            self.forms.contact_alias_input.clear();
                            self.forms.invite_code_input.clear();
                            self.forms.magnet_uri_input.clear();
                            self.select_chat(fingerprint);
                            return Task::batch([self.refresh(), scroll_to_latest()]);
                        }
                        self.refresh()
                    }
                    Err(error) => {
                        self.status_line = format!("Could not add the contact: {error}");
                        Task::none()
                    }
                }
            }
            Message::SelectChatContact(fingerprint) => {
                self.select_chat(fingerprint.clone());
                let backend = self.backend.clone();
                let mark_read = Task::perform(
                    async move {
                        backend
                            .command(Command::Read {
                                recipient: fingerprint,
                            })
                            .map(|_| ())
                    },
                    |result| Message::ActionFinished {
                        done: "",
                        failed: "Could not mark the conversation read",
                        result,
                    },
                );
                Task::batch([mark_read, scroll_to_latest()])
            }
            Message::ComposePostChanged(value) => {
                self.forms.compose_post_input = value;
                Task::none()
            }
            Message::CreatePost => {
                let content = self.forms.compose_post_input.clone();
                if content.trim().is_empty() {
                    return Task::none();
                }
                let backend = self.backend.clone();
                Task::perform(
                    async move { backend.command(Command::Post { content }).map(|_| ()) },
                    Message::PostPublished,
                )
            }
            Message::PostPublished(result) => match result {
                Ok(()) => {
                    self.forms.compose_post_input.clear();
                    self.status_line =
                        "Post published. Peers pick it up on the next sync.".to_string();
                    self.refresh()
                }
                Err(error) => {
                    self.status_line = format!("Post failed: {error}");
                    Task::none()
                }
            },
            Message::ComposeMessageChanged(value) => {
                self.forms.compose_message_input = value.clone();
                if let Some(fingerprint) = self.forms.selected_contact_for_chat.clone() {
                    self.forms.drafts.insert(fingerprint, value);
                }
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
                let Some(recipient) = self.forms.selected_contact_for_chat.clone() else {
                    self.status_line = "Select a contact before messaging".to_string();
                    return Task::none();
                };
                let content = self.forms.compose_message_input.clone();
                if content.trim().is_empty() {
                    return Task::none();
                }
                self.sending = Some((recipient.clone(), content.clone()));
                let backend = self.backend.clone();
                Task::perform(
                    async move {
                        backend
                            .command(Command::Message { recipient, content })
                            .map(|_| ())
                    },
                    Message::ChatSent,
                )
            }
            Message::ChatSent(result) => {
                let draft = self.sending.take();
                match result {
                    Ok(()) => {
                        if let Some((fingerprint, sent_text)) = draft {
                            if self.forms.selected_contact_for_chat.as_ref() == Some(&fingerprint)
                                && self.forms.compose_message_input == sent_text
                            {
                                self.forms.compose_message_input.clear();
                            }
                            if self.forms.drafts.get(&fingerprint) == Some(&sent_text) {
                                self.forms.drafts.remove(&fingerprint);
                            }
                        }
                        self.status_line =
                            "Message queued. It will retry until a peer accepts it.".into();
                        Task::batch([self.refresh(), scroll_to_latest()])
                    }
                    Err(error) => {
                        // The daemon committed nothing, so the draft stays editable.
                        self.status_line = format!("Message failed: {error}");
                        Task::none()
                    }
                }
            }
            Message::SaveQrSvg | Message::SaveQrPng | Message::SaveQrJpg => {
                let result = self.invite_uri().and_then(|uri| match message {
                    Message::SaveQrSvg => save_qr_svg(&uri),
                    Message::SaveQrPng => save_qr_png(&uri),
                    _ => save_qr_jpg(&uri),
                });
                self.status_line = match result {
                    Ok(path) => format!("QR code saved to {}", path.display()),
                    Err(error) => format!("Could not save the QR code: {error}"),
                };
                Task::none()
            }
            Message::LanDiscoveryToggle => {
                let enabled = !self.state.network.discovery;
                self.run_command(
                    if enabled {
                        "Discovery turned on."
                    } else {
                        "Discovery turned off."
                    },
                    "Could not change discovery",
                    Command::Discovery { enabled },
                )
            }
            Message::CleanupLocalFiles => self.run_command(
                "Local file caches cleaned up.",
                "Could not clean up local files",
                Command::Cleanup,
            ),
            Message::CloseRequested(id) => {
                self.window = Some(id);
                self.status_line = "SnartNet keeps syncing in the tray.".to_string();
                window::change_mode(id, window::Mode::Hidden)
            }
            Message::Tray(tray::TrayCommand::Show) => match self.window {
                Some(id) => Task::batch([
                    window::change_mode(id, window::Mode::Windowed),
                    window::gain_focus(id),
                ]),
                None => Task::none(),
            },
            Message::Tray(tray::TrayCommand::Quit) => iced::exit(),
            Message::Tray(tray::TrayCommand::StopDaemonAndQuit) => {
                self.status_line = "Stopping the SnartNet daemon…".to_string();
                let backend = self.backend.clone();
                Task::perform(
                    // Report the daemon's answer, then leave either way: a
                    // frontend must never trap the user in a broken shutdown.
                    async move { backend.stop().err() },
                    Message::Stopped,
                )
            }
            Message::Stopped(error) => {
                if let Some(error) = error {
                    eprintln!("SnartNet daemon did not stop cleanly: {error}");
                }
                iced::exit()
            }
        }
    }

    /// Pausing only changes how much the daemon does, never whether it runs.
    fn set_sync_mode(&self, mode: SyncMode, done: &'static str) -> Task<Message> {
        let backend = self.backend.clone();
        Task::perform(
            async move { backend.set_sync_mode(mode).map(|_| ()) },
            move |result| Message::ActionFinished {
                done,
                failed: "Could not change the sync mode",
                result,
            },
        )
    }

    /// Contacts belong to the daemon; this only reports the import result.
    fn import_contact(
        &mut self,
        started: &'static str,
        input: String,
        mode: &'static str,
        alias: String,
        address: String,
    ) -> Task<Message> {
        self.status_line = started.to_string();
        self.pending_contact = Some(input.clone());
        let backend = self.backend.clone();
        Task::perform(
            async move {
                backend
                    .command(Command::Contact {
                        input,
                        mode: mode.to_string(),
                        alias,
                        address,
                    })
                    .map(|_| ())
            },
            Message::ContactAdded,
        )
    }

    /// Drafts follow the conversation, so switching contacts never mixes them up.
    fn select_chat(&mut self, fingerprint: String) {
        self.forms.compose_message_input = self
            .forms
            .drafts
            .get(&fingerprint)
            .cloned()
            .unwrap_or_default();
        self.forms.selected_contact_for_chat = Some(fingerprint);
        self.panel = Panel::Messages;
    }

    /// Snapshots keep the window live; the daemon does all of the work.
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            time::every(POLL_INTERVAL).map(|_| Message::Tick),
            window::close_requests().map(Message::CloseRequested),
            Subscription::run(tray::commands).map(Message::Tray),
        ])
    }
}

/// The daemon needs a dialable address, not a hostname, to advertise itself.
fn validate_endpoint(value: &str) -> Result<(), String> {
    let endpoint: std::net::SocketAddr = value
        .parse()
        .map_err(|_| "Use an IP address and port, such as 192.168.1.5:47470")?;
    if endpoint.ip().is_unspecified() || endpoint.ip().is_multicast() || endpoint.port() == 0 {
        return Err("Use a reachable IP address and a nonzero port".into());
    }
    Ok(())
}

/// Fingerprints are long, so lists only show enough to tell contacts apart.
pub(crate) fn short_fp(fp: &str) -> String {
    if fp.len() <= 12 {
        fp.to_string()
    } else {
        format!("{}…{}", &fp[..8], &fp[fp.len() - 4..])
    }
}

/// Seconds since the Unix epoch, used for exported-file names and cache stamps.
pub(crate) fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One authoritative snapshot, decoded into view models off the UI thread.
async fn snapshot_task(backend: Arc<Backend>) -> Result<DaemonState, String> {
    tokio::task::spawn_blocking(move || {
        backend
            .snapshot()
            .and_then(|snapshot| DaemonState::from_snapshot(&snapshot))
    })
    .await
    .map_err(|error| format!("Snapshot task failed: {error}"))?
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
    match Backend::connect() {
        Ok(backend) => run(backend),
        // Without runtime paths there is no daemon to talk to and no state to
        // show, so say so in a dialog instead of opening a permanently empty window.
        Err(error) => {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("SnartNet")
                .set_description(&error)
                .show();
            std::process::exit(1)
        }
    }
}

fn run(backend: Arc<Backend>) -> iced::Result {
    iced::application("SnartNet", App::update, App::view)
        .subscription(App::subscription)
        .theme(|_| design::theme())
        // The window is disposable and the daemon is not (ADR 0001): closing it
        // hides the window and keeps background syncing alive.
        .exit_on_close_request(false)
        .window(iced::window::Settings {
            size: (1180.0, 800.0).into(),
            min_size: Some((1080.0, 680.0).into()),
            ..Default::default()
        })
        .run_with(move || App::with_backend(backend, tray::spawn()))
}
