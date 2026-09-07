use super::*;
pub(crate) use snartnet_client::sync::*;

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
        if let Some(magnet) = result.profile_magnet.clone() {
            if let Some(profile) = self.profile.as_mut() {
                profile.profile.magnet_uri = Some(magnet);
                if let Err(error) = self.storage.set_json(STORAGE_PROFILE, profile) {
                    self.status_line = format!("Profile magnet could not be saved: {error}");
                }
            }
        }
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
