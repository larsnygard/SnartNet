//! Terminal UI state and reducer.
//!
//! The reducer is pure: it turns semantic messages into at most one daemon
//! action, so every transition can be tested without a terminal or a socket.
//! `main` executes the action against the daemon and feeds the result back in.

use crate::daemon::Invite;
use crate::state::{Contact, DaemonState, MessageView, ThreadView};
use snartnet_sdk::{Command, SyncMode};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tab {
    #[default]
    Messages,
    Contacts,
    Feed,
    Profile,
    Network,
}

impl Tab {
    pub(crate) const ALL: [Tab; 5] = [
        Tab::Messages,
        Tab::Contacts,
        Tab::Feed,
        Tab::Profile,
        Tab::Network,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Tab::Messages => "Messages",
            Tab::Contacts => "Contacts",
            Tab::Feed => "Feed",
            Tab::Profile => "Profile",
            Tab::Network => "Network",
        }
    }

    pub(crate) fn index(self) -> usize {
        Tab::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    pub(crate) fn from_digit(digit: char) -> Option<Tab> {
        match digit {
            '1' => Some(Tab::Messages),
            '2' => Some(Tab::Contacts),
            '3' => Some(Tab::Feed),
            '4' => Some(Tab::Profile),
            '5' => Some(Tab::Network),
            _ => None,
        }
    }

    pub(crate) fn cycle(self, step: i8) -> Tab {
        let count = Tab::ALL.len() as i8;
        let index = (self.index() as i8 + step).rem_euclid(count) as usize;
        Tab::ALL[index]
    }
}

/// Text fields and lists share one focus slot, so typing never fights shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Focus {
    #[default]
    None,
    MessageCompose,
    ContactInput,
    ContactAlias,
    PostContent,
    ProfileUsername,
    ProfileDisplayName,
    ProfileBio,
    ProfileAddress,
}

impl Focus {
    /// Fields in tab order, so Tab and Shift+Tab walk the form predictably.
    pub(crate) fn fields(tab: Tab) -> &'static [Focus] {
        match tab {
            Tab::Messages => &[Focus::MessageCompose],
            Tab::Contacts => &[Focus::ContactInput, Focus::ContactAlias],
            Tab::Feed => &[Focus::PostContent],
            Tab::Profile => &[
                Focus::ProfileUsername,
                Focus::ProfileDisplayName,
                Focus::ProfileBio,
                Focus::ProfileAddress,
            ],
            Tab::Network => &[],
        }
    }

    pub(crate) fn primary(tab: Tab) -> Focus {
        Focus::fields(tab).first().copied().unwrap_or(Focus::None)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Focus::None => "",
            Focus::MessageCompose => "message",
            Focus::ContactInput => "invite, magnet, or fingerprint",
            Focus::ContactAlias => "alias (optional)",
            Focus::PostContent => "post",
            Focus::ProfileUsername => "username",
            Focus::ProfileDisplayName => "display name",
            Focus::ProfileBio => "bio",
            Focus::ProfileAddress => "advertised address",
        }
    }

    /// Walks the fields of `tab`, wrapping at both ends.
    pub(crate) fn cycle(self, tab: Tab, step: i8) -> Focus {
        let fields = Focus::fields(tab);
        if fields.is_empty() {
            return Focus::None;
        }
        let next = match fields.iter().position(|field| *field == self) {
            Some(index) => (index as i8 + step).rem_euclid(fields.len() as i8) as usize,
            None if step < 0 => fields.len() - 1,
            None => 0,
        };
        fields[next]
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FormState {
    pub message: String,
    pub contact_input: String,
    pub contact_alias: String,
    pub post: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    pub address: String,
    /// The daemon owns the avatar; the terminal UI round-trips it untouched.
    pub avatar: String,
}

/// Which add-contact path the pasted input selects, matching the daemon's op modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContactInputMode {
    Invite,
    Magnet,
    Manual,
}

impl ContactInputMode {
    pub(crate) fn detect(input: &str) -> Self {
        let input = input.trim();
        if input.starts_with("magnet:") {
            return ContactInputMode::Magnet;
        }
        if input.contains("://") {
            return ContactInputMode::Invite;
        }
        if input.len() >= 16
            && input
                .chars()
                .all(|character| character.is_ascii_hexdigit() || character == ':')
        {
            return ContactInputMode::Manual;
        }
        ContactInputMode::Invite
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ContactInputMode::Invite => "invite link",
            ContactInputMode::Magnet => "profile magnet",
            ContactInputMode::Manual => "manual fingerprint",
        }
    }

    fn as_op(self) -> &'static str {
        match self {
            ContactInputMode::Invite => "invite",
            ContactInputMode::Magnet => "magnet",
            ContactInputMode::Manual => "manual",
        }
    }
}

/// One daemon call the reducer can ask `main` to perform.
#[derive(Debug, Clone)]
pub(crate) enum Action {
    Refresh,
    Sync,
    SetMode(SyncMode),
    Command(Command),
    Invite,
    StartDaemon,
    StopDaemon,
}

/// Input and daemon results, already stripped of terminal and socket details.
#[derive(Debug)]
pub(crate) enum Message {
    /// Boxed because a snapshot is by far the largest thing on this channel.
    Snapshot(Result<Box<DaemonState>, String>),
    CommandFinished {
        command: Command,
        result: Result<(), String>,
    },
    Synced(Result<usize, String>),
    ModeChanged {
        requested: SyncMode,
        result: Result<SyncMode, String>,
    },
    Invitation(Result<Invite, String>),
    DaemonStarted(Result<(), String>),
    DaemonStopped(Result<(), String>),
    SwitchTab(Tab),
    CycleTab(i8),
    Focus(Focus),
    FocusNext,
    FocusPrevious,
    Unfocus,
    Insert(char),
    Backspace,
    Submit,
    Move(i32),
    SelectThread(String),
    ToggleCiphertext,
    ToggleHelp,
    Refresh,
    Sync,
    CycleMode,
    ToggleDiscovery,
    Cleanup,
    ToggleStorageHosting,
    CleanupStorage,
    Invite,
    StartDaemon,
    StopDaemon,
    Quit,
}

impl Message {
    /// Snapshots travel through a channel, so the payload is boxed to keep the
    /// message small; construction goes through one place.
    pub(crate) fn snapshot(result: Result<DaemonState, String>) -> Message {
        Message::Snapshot(result.map(Box::new))
    }
}

/// Everything the terminal draws, plus the user's uncommitted input.
#[derive(Debug, Default)]
pub(crate) struct App {
    /// False until the daemon has answered once, so the first frame is honest.
    pub(crate) loaded: bool,
    pub(crate) quit: bool,
    pub(crate) help: bool,
    pub(crate) state: DaemonState,
    pub(crate) tab: Tab,
    pub(crate) focus: Focus,
    pub(crate) status: String,
    pub(crate) invitation: Option<Invite>,
    pub(crate) form: FormState,
    /// Conversation the compose field targets.
    pub(crate) selected_contact: Option<String>,
    /// Cursor inside the open conversation.
    pub(crate) selected_message: usize,
    pub(crate) contacts_index: usize,
    pub(crate) feed_index: usize,
    /// Drafts belong to conversations, so switching threads never mixes them.
    drafts: BTreeMap<String, String>,
    /// Per-message ciphertext reveal, keyed by message id.
    reveal: BTreeMap<String, bool>,
    /// Set when the daemon stops answering, so the status line never lies.
    pub(crate) connection_lost: bool,
    confirm_stop: bool,
}

impl App {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn current_thread(&self) -> Option<&ThreadView> {
        self.selected_contact
            .as_deref()
            .and_then(|fingerprint| self.state.thread(fingerprint))
    }

    pub(crate) fn current_contact(&self) -> Option<&Contact> {
        self.selected_contact
            .as_deref()
            .and_then(|fingerprint| self.state.contact(fingerprint))
    }

    /// Contacts whose verified encryption key allows sending right now.
    pub(crate) fn can_message(&self, fingerprint: &str) -> bool {
        self.state.is_ready_to_message(fingerprint)
    }

    pub(crate) fn reveal(&self, message_id: &str) -> bool {
        self.reveal.get(message_id).copied().unwrap_or(false)
    }

    pub(crate) fn mode(&self) -> SyncMode {
        self.state.network.sync_mode
    }

    pub(crate) fn cursor_message(&self) -> Option<&MessageView> {
        self.current_thread()?.messages.get(self.selected_message)
    }

    fn field_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            Focus::None => None,
            Focus::MessageCompose => Some(&mut self.form.message),
            Focus::ContactInput => Some(&mut self.form.contact_input),
            Focus::ContactAlias => Some(&mut self.form.contact_alias),
            Focus::PostContent => Some(&mut self.form.post),
            Focus::ProfileUsername => Some(&mut self.form.username),
            Focus::ProfileDisplayName => Some(&mut self.form.display_name),
            Focus::ProfileBio => Some(&mut self.form.bio),
            Focus::ProfileAddress => Some(&mut self.form.address),
        }
    }

    /// Focusing a field the current tab cannot use explains why instead of failing silently.
    fn set_focus(&mut self, focus: Focus) {
        if focus == Focus::MessageCompose && self.selected_contact.is_none() {
            self.status = if self.state.contacts.is_empty() {
                "Add a contact (2) before writing a message.".to_string()
            } else {
                "Select a conversation first.".to_string()
            };
            return;
        }
        self.focus = focus;
    }

    /// Fresh daemon state replaces the view; the user's own typing is preserved.
    fn adopt(&mut self, state: DaemonState) {
        let first_load = !self.loaded;
        if let Some(profile) = &state.profile {
            self.form.avatar = profile.avatar_data_url.clone().unwrap_or_default();
            if first_load {
                self.form.username = profile.username.clone();
                self.form.display_name = profile.display_name.clone().unwrap_or_default();
                self.form.bio = profile.bio.clone().unwrap_or_default();
            }
        }
        if first_load {
            if self.form.address.is_empty() {
                self.form.address = state.network.address.clone();
            }
            self.selected_contact = state
                .threads
                .first()
                .map(|thread| thread.contact_fingerprint.clone());
        }
        self.loaded = true;
        self.state = state;
        self.clamp();
        if first_load {
            // Open on the newest message, the way every chat client does.
            self.selected_message = self
                .current_thread()
                .map_or(0, |thread| thread.messages.len().saturating_sub(1));
        }
    }

    /// Keeps the cursors and the open conversation valid after any snapshot.
    fn clamp(&mut self) {
        let stale = self
            .selected_contact
            .as_ref()
            .is_some_and(|fingerprint| self.state.contact(fingerprint).is_none());
        if stale {
            self.selected_contact = None;
        }
        if self.selected_contact.is_none() {
            self.selected_contact = self
                .state
                .threads
                .first()
                .map(|thread| thread.contact_fingerprint.clone());
        }
        let messages = self
            .current_thread()
            .map_or(0, |thread| thread.messages.len());
        self.selected_message = shift(self.selected_message, 0, messages);
        self.contacts_index = shift(self.contacts_index, 0, self.state.contacts.len());
        self.feed_index = shift(self.feed_index, 0, self.state.posts.len());
    }

    fn move_cursor(&mut self, step: i32) {
        match self.tab {
            Tab::Messages => {
                let messages = self
                    .current_thread()
                    .map_or(0, |thread| thread.messages.len());
                self.selected_message = shift(self.selected_message, step, messages);
            }
            Tab::Contacts => {
                self.contacts_index = shift(self.contacts_index, step, self.state.contacts.len());
            }
            Tab::Feed => {
                self.feed_index = shift(self.feed_index, step, self.state.posts.len());
            }
            Tab::Profile | Tab::Network => {}
        }
    }

    /// Opening a conversation also tells the daemon to clear its unread count.
    fn select_thread(&mut self, fingerprint: String, mark_read: bool) -> Option<Action> {
        if let Some(current) = self.selected_contact.clone() {
            if current != fingerprint {
                self.drafts
                    .insert(current, std::mem::take(&mut self.form.message));
            }
        }
        self.form.message = self.drafts.remove(&fingerprint).unwrap_or_default();
        self.selected_message = self
            .state
            .thread(&fingerprint)
            .map_or(0, |thread| thread.messages.len().saturating_sub(1));
        self.selected_contact = Some(fingerprint.clone());
        if mark_read && self.state.contact(&fingerprint).is_some() {
            Some(Action::Command(Command::Read {
                recipient: fingerprint,
            }))
        } else {
            None
        }
    }

    /// Enter with no field focused: start writing, or open the highlighted row.
    fn activate(&mut self) -> Option<Action> {
        match self.tab {
            Tab::Messages => {
                self.set_focus(Focus::MessageCompose);
                None
            }
            Tab::Contacts => match self
                .state
                .contacts
                .get(self.contacts_index)
                .map(|contact| contact.fingerprint.clone())
            {
                Some(fingerprint) => {
                    self.tab = Tab::Messages;
                    self.select_thread(fingerprint, true)
                }
                None => {
                    self.status = "Add a contact first.".to_string();
                    None
                }
            },
            Tab::Feed => {
                self.set_focus(Focus::PostContent);
                None
            }
            Tab::Profile => {
                self.set_focus(Focus::ProfileUsername);
                None
            }
            Tab::Network => None,
        }
    }
}

/// Clamps a list cursor after movement or after the daemon shrinks a list.
fn shift(current: usize, step: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as i32 + step).clamp(0, len as i32 - 1) as usize
}

pub(crate) fn mode_label(mode: SyncMode) -> &'static str {
    match mode {
        SyncMode::AlwaysOn => "always on",
        SyncMode::Balanced => "balanced",
        SyncMode::Paused => "paused",
    }
}

/// Always-on, balanced, and paused form one cycle for the `m` shortcut.
pub(crate) fn next_mode(mode: SyncMode) -> SyncMode {
    match mode {
        SyncMode::AlwaysOn => SyncMode::Balanced,
        SyncMode::Balanced => SyncMode::Paused,
        SyncMode::Paused => SyncMode::AlwaysOn,
    }
}

impl App {
    /// Pure transition: input and daemon results in, at most one daemon call out.
    pub(crate) fn update(&mut self, message: Message) -> Option<Action> {
        match message {
            Message::Snapshot(Ok(state)) => {
                let first = !self.loaded;
                // The box is an artefact of the channel, not of the reducer.
                self.adopt(*state);
                if first {
                    self.status = if self.state.profile.is_some() {
                        "Connected to the SnartNet daemon.".to_string()
                    } else {
                        "This daemon has no identity yet. Press 4 to create your profile."
                            .to_string()
                    };
                } else if self.connection_lost {
                    self.connection_lost = false;
                    self.status = "Reconnected to the SnartNet daemon.".to_string();
                }
                None
            }
            Message::Snapshot(Err(error)) => {
                self.connection_lost = true;
                self.status = error;
                None
            }
            Message::CommandFinished { command, result } => match result {
                Ok(()) => {
                    self.status = success_line(&command);
                    self.clear_submitted(&command);
                    Some(Action::Refresh)
                }
                Err(error) => {
                    // The draft stays in its field, so nothing typed is lost.
                    self.status = format!("{} {error}", failure_prefix(&command));
                    None
                }
            },
            Message::Synced(Ok(received)) => {
                self.status = match received {
                    0 => "Sync finished: nothing new.".to_string(),
                    1 => "Sync finished: 1 item received.".to_string(),
                    count => format!("Sync finished: {count} items received."),
                };
                Some(Action::Refresh)
            }
            Message::Synced(Err(error)) => {
                self.status = format!("Sync failed: {error}");
                None
            }
            Message::ModeChanged { requested, result } => match result {
                Ok(mode) => {
                    self.state.network.sync_mode = mode;
                    self.status = format!("Sync mode: {}.", mode_label(mode));
                    Some(Action::Refresh)
                }
                Err(error) => {
                    // Never claim a mode the daemon refused.
                    self.status = format!(
                        "Could not set the sync mode to {}: {error}",
                        mode_label(requested)
                    );
                    None
                }
            },
            Message::Invitation(Ok(invite)) => {
                self.invitation = Some(invite);
                self.status = "Invitation link ready under the profile.".to_string();
                None
            }
            Message::Invitation(Err(error)) => {
                self.status = format!("Could not build the invitation link: {error}");
                None
            }
            Message::DaemonStarted(Ok(())) => {
                self.status = "Started the SnartNet daemon.".to_string();
                Some(Action::Refresh)
            }
            Message::DaemonStarted(Err(error)) => {
                self.status = format!("Could not start the daemon: {error}");
                None
            }
            Message::DaemonStopped(Ok(())) => {
                self.status = "Daemon shutdown requested. Press D to start it again.".to_string();
                None
            }
            Message::DaemonStopped(Err(error)) => {
                self.status = format!("Could not stop the daemon: {error}");
                None
            }
            Message::SwitchTab(tab) => {
                self.tab = tab;
                self.focus = Focus::None;
                None
            }
            Message::CycleTab(step) => {
                self.tab = self.tab.cycle(step);
                self.focus = Focus::None;
                None
            }
            Message::Focus(focus) => {
                self.set_focus(focus);
                None
            }
            Message::FocusNext => {
                let next = self.focus.cycle(self.tab, 1);
                self.set_focus(next);
                None
            }
            Message::FocusPrevious => {
                let next = self.focus.cycle(self.tab, -1);
                self.set_focus(next);
                None
            }
            Message::Unfocus => {
                self.focus = Focus::None;
                None
            }
            Message::Insert(character) => {
                if let Some(field) = self.field_mut() {
                    field.push(character);
                }
                None
            }
            Message::Backspace => {
                if let Some(field) = self.field_mut() {
                    field.pop();
                }
                None
            }
            Message::Submit => {
                if self.focus == Focus::None {
                    self.activate()
                } else {
                    self.submit()
                }
            }
            Message::Move(step) => {
                self.move_cursor(step);
                None
            }
            Message::SelectThread(fingerprint) => self.select_thread(fingerprint, true),
            Message::ToggleCiphertext => {
                match self.cursor_message() {
                    Some(message) => {
                        let id = message.id.clone();
                        let revealed = self.reveal(&id);
                        self.reveal.insert(id, !revealed);
                    }
                    None => self.status = "No message to reveal; open a conversation.".to_string(),
                }
                None
            }
            Message::ToggleHelp => {
                self.help = !self.help;
                None
            }
            Message::Refresh => Some(Action::Refresh),
            Message::Sync => Some(Action::Sync),
            Message::CycleMode => Some(Action::SetMode(next_mode(self.mode()))),
            Message::ToggleDiscovery => {
                let enabled = !self.state.network.discovery;
                self.status = if enabled {
                    "Turning discovery on…".to_string()
                } else {
                    "Turning discovery off…".to_string()
                };
                Some(Action::Command(Command::Discovery { enabled }))
            }
            Message::Cleanup => Some(Action::Command(Command::Cleanup)),
            Message::ToggleStorageHosting => {
                // One key for the setting that matters most on a phone: whether this device
                // holds copies for contacts at all (M9.1).
                let replicate = !self.state.network.storage.hosting;
                self.status = if replicate {
                    "Hosting replicas for contacts…".to_string()
                } else {
                    "Stopping replica hosting…".to_string()
                };
                Some(Action::Command(Command::Storage {
                    replicate: Some(replicate),
                    quota_mib: None,
                    lease_days: None,
                    copies: None,
                    min_free_mib: None,
                }))
            }
            Message::CleanupStorage => Some(Action::Command(Command::CleanupStorage)),
            Message::Invite => Some(Action::Invite),
            Message::StartDaemon => Some(Action::StartDaemon),
            Message::StopDaemon => {
                if self.confirm_stop {
                    self.confirm_stop = false;
                    self.status = "Stopping the SnartNet daemon…".to_string();
                    Some(Action::StopDaemon)
                } else {
                    // Quitting this view leaves the daemon running, so stopping is explicit.
                    self.confirm_stop = true;
                    self.status =
                        "Press S again to stop the SnartNet daemon; q only quits this view."
                            .to_string();
                    None
                }
            }
            Message::Quit => {
                self.quit = true;
                None
            }
        }
    }
}

impl App {
    /// Enter inside a field: validate locally, then ask the daemon to commit.
    ///
    /// The field is only cleared once the daemon reports success, so a rejected
    /// submission never costs the user their typing.
    fn submit(&mut self) -> Option<Action> {
        match self.focus {
            Focus::MessageCompose => {
                let recipient = self.selected_contact.clone()?;
                let content = self.form.message.trim().to_string();
                if content.is_empty() {
                    self.status = "Write a message first.".to_string();
                    return None;
                }
                if !self.can_message(&recipient) {
                    self.status = format!(
                        "Sync {}'s verified encryption key before sending.",
                        self.state.contact_label(&recipient)
                    );
                    return None;
                }
                self.drafts.remove(&recipient);
                self.status = "Sending the message…".to_string();
                Some(Action::Command(Command::Message { recipient, content }))
            }
            Focus::ContactInput | Focus::ContactAlias => {
                let input = self.form.contact_input.trim().to_string();
                if input.is_empty() {
                    self.status =
                        "Paste an invite link, a magnet link, or a fingerprint first.".to_string();
                    return None;
                }
                let mode = ContactInputMode::detect(&input);
                self.status = format!("Adding the contact from that {}…", mode.label());
                Some(Action::Command(Command::Contact {
                    input,
                    mode: mode.as_op().to_string(),
                    alias: self.form.contact_alias.trim().to_string(),
                    address: String::new(),
                }))
            }
            Focus::PostContent => {
                let content = self.form.post.trim().to_string();
                if content.is_empty() {
                    self.status = "Write a post first.".to_string();
                    return None;
                }
                self.status = "Publishing the post…".to_string();
                Some(Action::Command(Command::Post { content }))
            }
            Focus::ProfileUsername
            | Focus::ProfileDisplayName
            | Focus::ProfileBio
            | Focus::ProfileAddress => {
                let username = self.form.username.trim().to_string();
                if username.is_empty() {
                    self.status = "Pick a username first.".to_string();
                    return None;
                }
                let identity_exists = self.state.profile.is_some();
                self.status = if identity_exists {
                    "Publishing the profile…".to_string()
                } else {
                    "Creating your profile…".to_string()
                };
                Some(Action::Command(Command::Profile {
                    username,
                    display_name: self.form.display_name.trim().to_string(),
                    bio: self.form.bio.trim().to_string(),
                    avatar: self.form.avatar.clone(),
                    address: self.form.address.trim().to_string(),
                }))
            }
            Focus::None => None,
        }
    }

    /// Clears exactly the field a committed command came from.
    fn clear_submitted(&mut self, command: &Command) {
        match command {
            Command::Message { .. } => self.form.message.clear(),
            Command::Post { .. } => self.form.post.clear(),
            Command::Contact { .. } => {
                self.form.contact_input.clear();
                self.form.contact_alias.clear();
            }
            _ => {}
        }
    }
}

fn success_line(command: &Command) -> String {
    match command {
        Command::Profile { .. } => "Profile published by the daemon.",
        Command::Contact { .. } => "Contact saved by the daemon.",
        Command::Post { .. } => "Post published to your feed.",
        Command::Message { .. } => "Message queued by the daemon.",
        Command::Read { .. } => "Conversation marked read.",
        Command::Pause { paused: true } => "Sync paused.",
        Command::Pause { paused: false } => "Sync resumed.",
        Command::Discovery { enabled: true } => "Discovery turned on.",
        Command::Discovery { enabled: false } => "Discovery turned off.",
        Command::Storage {
            replicate: Some(true),
            ..
        } => "Replica hosting turned on.",
        Command::Storage {
            replicate: Some(false),
            ..
        } => "Replica hosting turned off.",
        Command::Storage { .. } => "Storage settings saved.",
        Command::CleanupStorage => "Expired and over-quota replicas dropped.",
        Command::Cleanup => "Local file caches cleaned up.",
        Command::Invite => "Invitation link ready.",
    }
    .to_string()
}

fn failure_prefix(command: &Command) -> &'static str {
    match command {
        Command::Profile { .. } => "Could not publish the profile:",
        Command::Contact { .. } => "Could not add the contact:",
        Command::Post { .. } => "Could not publish the post:",
        Command::Message { .. } => "Could not send the message:",
        Command::Read { .. } => "Could not mark the conversation read:",
        Command::Pause { .. } => "Could not change sync:",
        Command::Discovery { .. } => "Could not change discovery:",
        Command::Storage { .. } => "Could not save storage settings:",
        Command::CleanupStorage => "Could not drop replicas:",
        Command::Cleanup => "Could not clean up local files:",
        Command::Invite => "Could not build the invitation link:",
    }
}

#[cfg(test)]
impl App {
    /// Tests that only assert on state drop the returned action on purpose;
    /// tests that assert on the daemon call use `update` directly.
    pub(crate) fn dispatch(&mut self, message: Message) {
        let _ = self.update(message);
    }
}
