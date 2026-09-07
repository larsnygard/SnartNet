//! Blocking transport work runs on a worker. The UI applies a delta, so messages and
//! contacts added while a sync is running cannot be replaced by an older snapshot.
use super::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct SyncResult {
    pub contacts: Vec<Contact>,
    pub incoming: Vec<(String, ChatItem)>,
    pub relayed_ids: HashSet<String>,
}

pub(crate) fn exchange(
    transport: TcpSwarmTransport,
    profile: SignedProfile,
    posts: Vec<SignedPost>,
    mut contacts: Vec<Contact>,
    pending: Vec<SignedMessage>,
) -> SyncResult {
    // Announce our signed key before sending envelopes. This also enables replies
    // when only one side has a reachable TCP endpoint.
    transport.relay_profile(&profile);
    transport.relay_posts(&profile.profile.fingerprint, posts);
    let mut result = SyncResult::default();
    for contact in &mut contacts {
        if let Some(blob) = transport.load_profile(&contact.fingerprint) {
            let peer = blob.profile;
            if peer.profile.fingerprint == contact.fingerprint && peer.verify().unwrap_or(false) {
                contact.verification = VerificationState::Verified;
                contact.known_public_key = Some(peer.profile.public_key);
                contact.known_encryption_public_key = peer.profile.encryption_public_key;
                contact.avatar_data_url = peer.profile.avatar_data_url;
                contact.profile_summary = peer.profile.bio.unwrap_or_default();
                contact.last_sync_error = None;
                contact.last_sync_label = ts_label();
            }
        } else {
            contact.last_sync_error =
                Some("Waiting for their profile. Check the invite address and sync again.".into());
        }
        if let Some(posts) = transport.load_posts(&contact.fingerprint) {
            contact.synced_post_count = posts.posts.len();
            contact.latest_post_preview = posts
                .posts
                .first()
                .map(|p| p.post.content.clone())
                .unwrap_or_default();
        }
    }
    for message in pending {
        // The local inbox also allows peers to pull messages if their inbound port is closed.
        let inbox = transport::SwarmInboxBlob {
            messages: vec![message.clone()],
            updated_at: unix_secs(),
        };
        if transport
            .save_inbox(&message.message.recipient_fingerprint, &inbox)
            .is_ok()
            && transport.relay_message(&message)
        {
            result.relayed_ids.insert(message.message.id);
        }
    }
    if let Some(inbox) = transport.load_inbox(&profile.profile.fingerprint) {
        for signed in inbox.messages {
            if let Some(contact) = contacts
                .iter()
                .find(|c| c.fingerprint == signed.message.sender_fingerprint)
            {
                if accepts_message(&signed, contact, &profile.profile.fingerprint) {
                    result.incoming.push((
                        contact.fingerprint.clone(),
                        ChatItem::from_signed(
                            signed,
                            true,
                            contact.known_encryption_public_key.clone(),
                        ),
                    ));
                }
            }
        }
    }
    result.contacts = contacts;
    result
}

/// Verify both routing identities before an envelope can enter a visible thread.
pub(crate) fn accepts_message(message: &SignedMessage, contact: &Contact, local_fp: &str) -> bool {
    contact.verification == VerificationState::Verified
        && message.message.sender_fingerprint == contact.fingerprint
        && message.message.recipient_fingerprint == local_fp
        && contact
            .known_public_key
            .as_ref()
            .is_some_and(|key| message.verify(key).unwrap_or(false))
}

impl App {
    pub(crate) fn run_peer_sync(&mut self) -> Task<Message> {
        if self.syncing || !self.network.bittorrent_running {
            return Task::none();
        }
        let Some(profile) = self.profile.clone() else {
            return Task::none();
        };
        self.syncing = true;
        self.refresh_transport_peers_from_discovery();
        let transport = self.transport.clone();
        let contacts = self.contacts.clone();
        let posts = self.local_posts.clone();
        let pending = self
            .threads
            .iter()
            .flat_map(|t| &t.messages)
            .filter(|m| !m.incoming && m.delivery == DeliveryState::Queued)
            .filter_map(|m| m.envelope.clone())
            .collect();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    exchange(transport, profile, posts, contacts, pending)
                })
                .await
                .map_err(|e| e.to_string())
            },
            Message::SyncFinished,
        )
    }

    pub(crate) fn apply_sync(&mut self, result: SyncResult) -> Task<Message> {
        for refreshed in result.contacts {
            if let Some(contact) = self
                .contacts
                .iter_mut()
                .find(|c| c.fingerprint == refreshed.fingerprint)
            {
                // Endpoint edits made during sync remain authoritative.
                let endpoint = contact.transport_addr.clone();
                *contact = refreshed;
                contact.transport_addr = endpoint;
            }
        }
        let mut received = 0;
        let mut scroll = false;
        for (fingerprint, item) in result.incoming {
            self.ensure_thread(&fingerprint);
            let selected = self.panel == Panel::Messages
                && self.forms.selected_contact_for_chat.as_ref() == Some(&fingerprint);
            let thread = self
                .threads
                .iter_mut()
                .find(|t| t.contact_fingerprint == fingerprint)
                .unwrap();
            if !thread.messages.iter().any(|m| m.id == item.id) {
                thread.messages.push(item);
                if !selected {
                    thread.unread_count = thread.unread_count.saturating_add(1);
                }
                scroll |= selected;
                received += 1;
            }
        }
        for thread in &mut self.threads {
            for message in &mut thread.messages {
                if !message.incoming && result.relayed_ids.contains(&message.id) {
                    message.delivery = DeliveryState::Relayed;
                }
            }
        }
        self.network.last_poll_label = ts_label();
        if received > 0 {
            self.status_line = format!("{received} new message(s)");
        }
        self.persist_contacts();
        self.persist_threads();
        if scroll {
            scroll_to_latest()
        } else {
            Task::none()
        }
    }
}
