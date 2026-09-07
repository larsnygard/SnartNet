//! Durable Android host state. Commands commit before changing the visible state.
use crate::{
    actions::*, dht::DhtNode, discovery::*, model::*, protocol::*, torrent::TorrentNode,
    transport::*, *,
};
use futures::executor::block_on;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use snartnet_core::{Message, MessageType};
use std::{path::Path, sync::Arc};

#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    keypair: Option<KeyPair>,
    profile: Option<SignedProfile>,
    posts: Vec<SignedPost>,
    contacts: Vec<Contact>,
    threads: Vec<ChatThread>,
    #[serde(default)]
    address: String,
}

pub struct Session {
    state: State,
    storage: FileStorage,
    pub transport: TcpSwarmTransport,
    pub torrent: Option<Arc<TorrentNode>>,
    pub dht: Option<Arc<DhtNode>>,
    discovery: LanDiscovery,
    paused: bool,
    pub listener_error: Option<String>,
    last_sync: String,
}

impl Session {
    pub fn open(root: &Path, bind: std::net::SocketAddr) -> Result<Self, String> {
        let storage = FileStorage::new(root.join("data")).map_err(|e| e.to_string())?;
        let state = match storage
            .get_json::<State>("client_state")
            .map_err(|e| e.to_string())?
        {
            Some(state) => state,
            None => {
                let old = load_startup(&storage)?;
                State {
                    keypair: old.keypair,
                    profile: old.profile,
                    posts: old.local_posts,
                    contacts: old.contacts,
                    threads: old.threads,
                    address: String::new(),
                }
            }
        };
        if let Some(profile) = &state.profile {
            if !profile.verify().unwrap_or(false)
                || state.keypair.as_ref().map(|k| &k.fingerprint)
                    != Some(&profile.profile.fingerprint)
            {
                return Err(
                    "Stored identity is invalid; restore a backup before continuing".into(),
                );
            }
        }
        let transport = TcpSwarmTransport::new(&root.join("swarm"), bind)?;
        let torrent = TorrentNode::open(root.join("torrent"), bind.port().saturating_add(1))
            .ok()
            .map(Arc::new);
        let dht = state
            .keypair
            .as_ref()
            .and_then(|keypair| DhtNode::open(keypair, bind.port().saturating_add(2)).ok())
            .map(Arc::new);
        Ok(Self {
            state,
            storage,
            transport,
            torrent,
            dht,
            discovery: LanDiscovery::new(),
            paused: false,
            listener_error: None,
            last_sync: "never".into(),
        })
    }

    pub fn start(&mut self) {
        self.listener_error = self.transport.start_server().err();
        self.start_distributed();
        if self.state.profile.is_some() {
            let _ = self.publish_profile_torrent();
        }
        self.start_discovery();
    }

    fn start_distributed(&mut self) {
        if self.torrent.is_none() {
            self.torrent = TorrentNode::open(self.transport.swarm_dir().join("torrent"), 47472)
                .ok()
                .map(Arc::new);
        }
        if self.dht.is_none() {
            self.dht = self
                .state
                .keypair
                .as_ref()
                .and_then(|keypair| DhtNode::open(keypair, 47473).ok())
                .map(Arc::new);
        }
    }

    fn publish_profile_torrent(&mut self) -> Result<(), String> {
        let Some(profile) = self.state.profile.clone() else {
            return Ok(());
        };
        let Some(torrent) = self.torrent.as_ref() else {
            return Ok(());
        };
        let object_id = format!("profile-{}", profile.profile.fingerprint);
        let bytes = serde_json::to_vec(&profile).map_err(|e| e.to_string())?;
        let magnet = torrent.publish(&object_id, &bytes)?;
        let mut next = self.state.clone();
        if let Some(local) = next.profile.as_mut() {
            local.profile.magnet_uri = Some(magnet.clone());
        }
        self.commit(next)?;
        if let Some(dht) = self.dht.as_ref() {
            let value = serde_json::to_vec(&json!({"magnet": magnet, "object_id": object_id}))
                .map_err(|e| e.to_string())?;
            dht.publish("snartnet/profile", &[&profile.profile.fingerprint], &value)?;
        }
        Ok(())
    }

    fn publish_post_torrent(&self, post: &SignedPost) -> Result<(), String> {
        let Some(torrent) = self.torrent.as_ref() else {
            return Ok(());
        };
        let object_id = format!("post-{}", post.post.id);
        let bytes = serde_json::to_vec(post).map_err(|e| e.to_string())?;
        let magnet = torrent.publish(&object_id, &bytes)?;
        if let Some(dht) = self.dht.as_ref() {
            let value = serde_json::to_vec(&json!({"magnet": magnet, "object_id": object_id}))
                .map_err(|e| e.to_string())?;
            dht.publish("snartnet/feed", &[&post.post.author_fingerprint], &value)?;
        }
        Ok(())
    }

    fn publish_message_torrent(
        &self,
        sender: &str,
        recipient: &str,
        message: &SignedMessage,
    ) -> Result<(), String> {
        let Some(torrent) = self.torrent.as_ref() else {
            return Ok(());
        };
        let Some(dht) = self.dht.as_ref() else {
            return Ok(());
        };
        let keypair = self.state.keypair.as_ref().ok_or("missing signing key")?;
        let legacy = SignedEnvelope::from_signed_message(message);
        let envelope = SignedEnvelope::sign(legacy.envelope, keypair)?;
        let object_id = format!("message-{}", message.message.id);
        let magnet = torrent.publish(&object_id, &envelope.to_json()?)?;
        let now = chrono::Utc::now();
        let batch = TorrentDescriptor {
            magnet,
            object_id,
            created_at: now,
            expires_at: Some(now + chrono::Duration::days(30)),
        };
        let parts = [sender, recipient];
        let current = dht
            .get_for(&keypair.public_key, "snartnet/mailbox", &parts)?
            .and_then(|value| serde_json::from_slice::<Value>(&value).ok())
            .and_then(|value| {
                Some((
                    value.get("magnet")?.as_str()?.to_owned(),
                    value.get("object_id")?.as_str()?.to_owned(),
                ))
            });
        let current_pointer = current.clone();
        let mut manifest = if let Some((manifest_magnet, manifest_id)) = current {
            torrent
                .fetch(&manifest_magnet, &manifest_id)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<MailboxManifest>(&bytes).ok())
                .filter(|manifest| {
                    manifest.sender == sender
                        && manifest.recipient == recipient
                        && manifest.verify(&keypair.public_key, now).unwrap_or(false)
                })
        } else {
            None
        }
        .unwrap_or(MailboxManifest {
            v: WIRE_VERSION,
            kind: ObjectType::MailboxManifest,
            sender: sender.to_owned(),
            recipient: recipient.to_owned(),
            sequence: 0,
            batches: Vec::new(),
            previous: None,
            signature: String::new(),
        });
        manifest.sequence = manifest.sequence.saturating_add(1);
        if manifest.batches.len() >= 4 {
            manifest.previous = current_pointer.map(|(magnet, object_id)| TorrentDescriptor {
                magnet,
                object_id,
                created_at: now,
                expires_at: Some(now + chrono::Duration::days(30)),
            });
            manifest.batches.clear();
        }
        manifest.batches.push(batch);
        let manifest = manifest.sign(keypair)?;
        let manifest_id = format!(
            "mailbox-{}-{}-{}",
            sender, recipient, manifest.sequence
        );
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
        let manifest_magnet = torrent.publish(&manifest_id, &manifest_bytes)?;
        let value = serde_json::to_vec(
            &json!({"magnet": manifest_magnet, "object_id": manifest_id}),
        )
        .map_err(|e| e.to_string())?;
        dht.publish("snartnet/mailbox", &[sender, recipient], &value)?;
        Ok(())
    }

    /// Resolve contact profile torrents and encrypted mailbox descriptors.
    pub fn sync_distributed(&mut self) -> Result<usize, String> {
        self.start_distributed();
        let Some(torrent) = self.torrent.clone() else {
            return Ok(0);
        };
        let local_fp = self
            .state
            .profile
            .as_ref()
            .map(|p| p.profile.fingerprint.clone())
            .unwrap_or_default();
        let mut next = self.state.clone();
        let mut received = 0;
        for index in 0..next.contacts.len() {
            let fingerprint = next.contacts[index].fingerprint.clone();
            if let Some(magnet) = next.contacts[index].magnet_uri.clone() {
                let object_id = format!("profile-{}", fingerprint);
                if let Ok(bytes) = torrent.fetch(&magnet, &object_id) {
                    if let Ok(profile) = serde_json::from_slice::<SignedProfile>(&bytes) {
                        if profile.profile.fingerprint == fingerprint
                            && profile.verify().unwrap_or(false)
                        {
                            let contact = &mut next.contacts[index];
                            contact.verification = VerificationState::Verified;
                            contact.known_public_key = Some(profile.profile.public_key.clone());
                            contact.known_encryption_public_key =
                                profile.profile.encryption_public_key.clone();
                            contact.profile_summary =
                                profile.profile.bio.clone().unwrap_or_default();
                            contact.avatar_data_url = profile.profile.avatar_data_url.clone();
                        }
                    }
                }
            }
            let contact = next.contacts[index].clone();
            for signed in self.load_distributed_messages(&torrent, &contact, &local_fp) {
                let thread = thread_mut(&mut next, &fingerprint);
                if thread.messages.iter().any(|m| m.id == signed.message.id) {
                    continue;
                }
                let mut item = ChatItem::from_signed(
                    signed,
                    true,
                    contact.known_encryption_public_key.clone(),
                );
                item.pushed_via_bittorrent = true;
                item.delivery = DeliveryState::Relayed;
                thread.messages.push(item);
                received += 1;
            }
        }
        if received > 0 {
            self.commit(next)?;
        }
        Ok(received)
    }

    fn load_distributed_messages(
        &self,
        torrent: &TorrentNode,
        sender: &Contact,
        recipient_fingerprint: &str,
    ) -> Vec<SignedMessage> {
        let Some(public_key) = sender.known_public_key.as_deref() else {
            return Vec::new();
        };
        let Some(dht) = self.dht.as_ref() else {
            return Vec::new();
        };
        let Ok(Some(value)) = dht.get_for(
            public_key,
            "snartnet/mailbox",
            &[&sender.fingerprint, recipient_fingerprint],
        ) else {
            return Vec::new();
        };
        let Ok(pointer) = serde_json::from_slice::<Value>(&value) else {
            return Vec::new();
        };
        let Some(mut magnet) = pointer
            .get("magnet")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
        else {
            return Vec::new();
        };
        let Some(mut object_id) = pointer
            .get("object_id")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
        else {
            return Vec::new();
        };
        let mut output = Vec::new();
        for _ in 0..16 {
            let Ok(bytes) = torrent.fetch(&magnet, &object_id) else {
                break;
            };
            let Ok(manifest) = serde_json::from_slice::<MailboxManifest>(&bytes) else {
                break;
            };
            if manifest.sender != sender.fingerprint
                || manifest.recipient != recipient_fingerprint
                || !manifest
                    .verify(public_key, chrono::Utc::now())
                    .unwrap_or(false)
            {
                break;
            }
            for batch in &manifest.batches {
                let Ok(message_bytes) = torrent.fetch(&batch.magnet, &batch.object_id) else {
                    continue;
                };
                let Ok(envelope) = SignedEnvelope::from_json(&message_bytes) else {
                    continue;
                };
                if !envelope
                    .verify(public_key, Some(recipient_fingerprint))
                    .unwrap_or(false)
                {
                    continue;
                }
                let Some(content) = envelope.envelope.body.as_str() else {
                    continue;
                };
                output.push(SignedMessage {
                    message: Message {
                        id: envelope.envelope.id,
                        sender_fingerprint: envelope.envelope.from,
                        recipient_fingerprint: recipient_fingerprint.to_owned(),
                        content: content.to_owned(),
                        created_at: envelope.envelope.created_at,
                        encrypted: true,
                        body_enc: envelope.envelope.body_enc,
                        nonce_b64: envelope.envelope.nonce,
                        message_type: MessageType::Direct,
                    },
                    signature: envelope.signature,
                });
            }
            let Some(previous) = manifest.previous else {
                break;
            };
            if previous.magnet.is_empty() {
                break;
            }
            magnet = previous.magnet;
            object_id = previous.object_id;
        }
        output
    }

    fn commit(&mut self, next: State) -> Result<(), String> {
        self.storage
            .set_json("client_state", &next)
            .map_err(|e| e.to_string())?;
        self.state = next;
        Ok(())
    }

    fn start_discovery(&mut self) {
        if let Some(p) = &self.state.profile {
            self.discovery.start(LanAnnounce {
                fingerprint: p.profile.fingerprint.clone(),
                username: p.profile.username.clone(),
                display_name: p.profile.display_name.clone(),
                tcp_addr: self.transport.advertised_addr(),
            });
        }
    }

    pub fn prepare_sync(&self) -> Option<impl FnOnce() -> sync::SyncResult> {
        if self.paused {
            return None;
        }
        let profile = self.state.profile.clone()?;
        let mut peers: Vec<std::net::SocketAddr> = self
            .state
            .contacts
            .iter()
            .filter_map(|c| c.transport_addr.as_ref()?.parse().ok())
            .collect();
        peers.extend(
            self.discovery
                .get_discovered()
                .iter()
                .filter_map(|p| p.tcp_addr.as_ref()?.parse::<std::net::SocketAddr>().ok()),
        );
        self.transport.set_peers(peers);
        let transport = self.transport.clone();
        let posts = self.state.posts.clone();
        let contacts = self.state.contacts.clone();
        let pending = self
            .state
            .threads
            .iter()
            .flat_map(|t| &t.messages)
            .filter(|m| !m.incoming && m.delivery == DeliveryState::Queued)
            .filter_map(|m| m.envelope.clone())
            .collect();
        Some(move || sync::exchange(transport, profile, posts, contacts, pending))
    }

    pub fn apply_sync(&mut self, result: sync::SyncResult) -> Result<(), String> {
        let mut next = self.state.clone();
        if let Some(magnet) = result.profile_magnet {
            if let Some(profile) = next.profile.as_mut() {
                profile.profile.magnet_uri = Some(magnet);
            }
        }
        for refreshed in result.contacts {
            if let Some(c) = next
                .contacts
                .iter_mut()
                .find(|c| c.fingerprint == refreshed.fingerprint)
            {
                let endpoint = c.transport_addr.clone();
                let alias = c.alias.clone();
                *c = refreshed;
                c.transport_addr = endpoint;
                c.alias = alias;
            }
        }
        for (fp, item) in result.incoming {
            let thread = thread_mut(&mut next, &fp);
            if !thread.messages.iter().any(|m| m.id == item.id) {
                thread.messages.push(item);
                thread.unread_count = thread.unread_count.saturating_add(1);
            }
        }
        for m in next.threads.iter_mut().flat_map(|t| &mut t.messages) {
            if !m.incoming && result.relayed_ids.contains(&m.id) {
                m.delivery = DeliveryState::Relayed;
            }
        }
        self.commit(next)?;
        self.last_sync = ts_label();
        Ok(())
    }

    /// Public snapshot deliberately excludes secret keys. Plaintext exists only in memory.
    pub fn snapshot(&self) -> Value {
        let threads: Vec<Value> = self.state.threads.iter().map(|t| {
            let contact = self.state.contacts.iter().find(|c| c.fingerprint == t.contact_fingerprint);
            let messages: Vec<Value> = t.messages.iter().map(|m| {
                let plaintext = decrypt_for_display(m, self.state.keypair.as_ref(),
                    contact.and_then(|c| c.known_encryption_public_key.as_deref()));
                json!({"id": m.id, "incoming": m.incoming, "ciphertext": m.content,
                    "text": plaintext.as_ref().ok(), "error": plaintext.as_ref().err(),
                    "encrypted": m.encrypted, "delivery": m.delivery, "time": m.created_label})
            }).collect();
            json!({"fingerprint": t.contact_fingerprint, "unread": t.unread_count, "messages": messages})
        }).collect();
        let nearby: Vec<Value> = self.discovery.get_discovered().iter().map(|p| json!({
            "fingerprint": p.fingerprint, "alias": p.display_name.as_ref().unwrap_or(&p.username), "address": p.tcp_addr
        })).collect();
        json!({"profile": self.state.profile.as_ref().map(|p| &p.profile),
            "identityUri": self.state.profile.as_ref().map(|p| p.profile.identity_uri()),
            "posts": self.state.posts,
            "contacts": self.state.contacts, "threads": threads, "nearby": nearby,
            "address": self.state.address, "listening": self.transport.advertised_addr(),
            "listenerError": self.listener_error, "paused": self.paused,
            "discovery": self.discovery.is_active(), "lastSync": self.last_sync,
            "peers": self.transport.peer_snapshot().len(),
            "dht": self.dht.as_ref().map(|node| node.status()),
            "torrent": self.torrent.as_ref().map(|node| node.status())})
    }

    pub fn invitation(&self) -> Result<String, String> {
        let profile = self
            .state
            .profile
            .as_ref()
            .ok_or("Create your profile first")?;
        let addr = if self.state.address.is_empty() {
            self.transport.advertised_addr()
        } else {
            Some(self.state.address.clone())
        };
        ContactInvite::from_signed_profile(profile, addr).to_uri()
    }

    pub fn command(&mut self, request: Value) -> Result<Value, String> {
        let field = |name: &str| request[name].as_str().unwrap_or("").to_string();
        let mut next = self.state.clone();
        match field("op").as_str() {
            "snapshot" => return Ok(self.snapshot()),
            "invite" => {
                return Ok(
                    json!({"uri": self.invitation()?, "magnet": self.state.profile.as_ref().and_then(|p| p.profile.magnet_uri.clone())}),
                )
            }
            "profile" => {
                let address = field("address").trim().to_string();
                validate_address(&address)?;
                let (kp, profile) = block_on(create_profile_async(
                    field("username"),
                    optional(field("displayName")),
                    optional(field("bio")),
                    optional(field("avatar")),
                    next.keypair.clone(),
                    next.profile.clone(),
                ))?;
                next.keypair = Some(kp);
                next.profile = Some(profile);
                next.address = address;
            }
            "contact" => {
                let input = field("input");
                let mut contact = if input.trim().starts_with("magnet:") {
                    block_on(import_magnet_async(input))?
                } else if field("mode") == "manual" {
                    block_on(add_contact_async(input, field("alias")))?
                } else {
                    block_on(import_invite_async(input))?
                };
                if next
                    .profile
                    .as_ref()
                    .is_some_and(|p| p.profile.fingerprint == contact.fingerprint)
                {
                    return Err("This is your own identity".into());
                }
                if !field("alias").trim().is_empty() {
                    contact.alias = field("alias").trim().into();
                }
                if !field("address").trim().is_empty() {
                    validate_address(field("address").trim())?;
                    contact.transport_addr = Some(field("address").trim().into());
                }
                if let Some(existing) = next
                    .contacts
                    .iter_mut()
                    .find(|c| c.fingerprint == contact.fingerprint)
                {
                    if contact.transport_addr.is_some() {
                        existing.transport_addr = contact.transport_addr;
                    }
                    if !field("alias").trim().is_empty() {
                        existing.alias = contact.alias;
                    }
                } else {
                    next.contacts.push(contact);
                }
            }
            "post" => {
                let fp = next
                    .profile
                    .as_ref()
                    .ok_or("Create your profile first")?
                    .profile
                    .fingerprint
                    .clone();
                next.posts.insert(
                    0,
                    block_on(create_post_async(
                        fp,
                        field("content"),
                        next.keypair.clone(),
                    ))?,
                );
            }
            "message" => {
                let fp = next
                    .profile
                    .as_ref()
                    .ok_or("Create your profile first")?
                    .profile
                    .fingerprint
                    .clone();
                let recipient = field("recipient");
                let peer = next
                    .contacts
                    .iter()
                    .find(|c| {
                        c.fingerprint == recipient && c.verification == VerificationState::Verified
                    })
                    .and_then(|c| c.known_encryption_public_key.clone())
                    .ok_or("Sync the contact's verified encryption key before sending")?;
                let signed = block_on(create_message_async(
                    fp,
                    recipient.clone(),
                    field("content"),
                    next.keypair.clone(),
                    peer.clone(),
                ))?;
                thread_mut(&mut next, &recipient)
                    .messages
                    .push(ChatItem::from_signed(signed, false, Some(peer)));
            }
            "read" => {
                thread_mut(&mut next, &field("recipient")).unread_count = 0;
            }
            "pause" => {
                self.paused = request["paused"].as_bool().unwrap_or(false);
                return Ok(self.snapshot());
            }
            "discovery" => {
                if request["enabled"].as_bool().unwrap_or(false) {
                    self.start_discovery();
                } else {
                    self.discovery.stop();
                }
                return Ok(self.snapshot());
            }
            "cleanup" => return self.cleanup(),
            _ => return Err("Unknown command".into()),
        }
        self.commit(next)?;
        let operation = field("op");
        if operation == "profile" {
            self.start_distributed();
            let _ = self.publish_profile_torrent();
        }
        if operation == "post" {
            self.start_distributed();
            if let Some(post) = self.state.posts.first() {
                let _ = self.publish_post_torrent(post);
            }
        }
        if operation == "message" {
            self.start_distributed();
            if let Some(thread) = self
                .state
                .threads
                .iter()
                .find(|t| t.contact_fingerprint == field("recipient"))
            {
                if let Some(item) = thread.messages.last().and_then(|m| m.envelope.as_ref()) {
                    let _ = self.publish_message_torrent(
                        &item.message.sender_fingerprint,
                        &item.message.recipient_fingerprint,
                        item,
                    );
                }
            }
        }
        if operation == "profile" {
            self.start_discovery();
        }
        // Publish only committed state. Failed network work can retry on the next sync.
        if let Some(profile) = &self.state.profile {
            let _ = self.transport.save_profile(
                &profile.profile.fingerprint,
                &SwarmProfileBlob {
                    profile: profile.clone(),
                    updated_at: unix_secs(),
                },
            );
            let _ = self.transport.save_posts(
                &profile.profile.fingerprint,
                &SwarmPostsBlob {
                    posts: self.state.posts.clone(),
                    updated_at: unix_secs(),
                },
            );
        }
        Ok(self.snapshot())
    }

    fn cleanup(&self) -> Result<Value, String> {
        let mut active = HashSet::new();
        for fp in self
            .state
            .contacts
            .iter()
            .map(|c| c.fingerprint.as_str())
            .chain(
                self.state
                    .profile
                    .iter()
                    .map(|p| p.profile.fingerprint.as_str()),
            )
        {
            for kind in ["profile", "posts", "inbox"] {
                active.insert(format!("{kind}_{}.json", sanitize_component(fp)));
            }
        }
        let mut removed = 0;
        for entry in std::fs::read_dir(self.transport.swarm_dir()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if active.contains(&name)
                || !["profile_", "posts_", "inbox_"]
                    .iter()
                    .any(|s| name.starts_with(s))
                || !name.ends_with(".json")
            {
                continue;
            }
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.is_file()
                && meta
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|t| t.as_secs() >= 7 * 86400)
            {
                std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                removed += 1;
            }
        }
        Ok(json!({"removed": removed}))
    }
}
fn optional(s: String) -> Option<String> {
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
fn validate_address(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Ok(());
    }
    let addr: std::net::SocketAddr = s
        .parse()
        .map_err(|_| "Use an IP address and port, e.g. 192.168.1.5:47470")?;
    if addr.ip().is_unspecified() || addr.ip().is_multicast() || addr.port() == 0 {
        return Err("Use a reachable IP and nonzero port".into());
    }
    Ok(())
}
fn thread_mut<'a>(state: &'a mut State, fp: &str) -> &'a mut ChatThread {
    if !state.threads.iter().any(|t| t.contact_fingerprint == fp) {
        state.threads.push(ChatThread {
            contact_fingerprint: fp.into(),
            messages: vec![],
            unread_count: 0,
        });
    }
    state
        .threads
        .iter_mut()
        .find(|t| t.contact_fingerprint == fp)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn client(root: &Path, name: &str) -> Session {
        let mut s = Session::open(root, "127.0.0.1:0".parse().unwrap()).unwrap();
        s.command(json!({"op":"profile", "username":name})).unwrap();
        s
    }
    fn sync(s: &mut Session) {
        let result = s.prepare_sync().unwrap()();
        s.apply_sync(result).unwrap();
    }
    #[test]
    fn android_session_interoperates_with_desktop_exchange_and_recovers_outbox() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut a = client(a_dir.path(), "alice");
        let mut b = client(b_dir.path(), "bob");
        let address = b.transport.start_server().unwrap();
        assert_eq!(b.transport.advertised_addr().unwrap(), address.to_string());
        let bob = b.state.profile.clone().unwrap();
        let alice = a.state.profile.clone().unwrap();
        a.command(json!({"op":"contact", "input":b.invitation().unwrap()}))
            .unwrap();
        b.command(json!({"op":"contact", "input":a.invitation().unwrap()}))
            .unwrap();
        sync(&mut a); // publish Alice and verify Bob using the desktop exchange
        a.command(json!({"op":"pause", "paused":true})).unwrap();
        a.command(json!({"op":"message", "recipient":bob.profile.fingerprint, "content":"Secret hello 👋"})).unwrap();
        let state_file =
            std::fs::read_to_string(a_dir.path().join("data/client_state.json")).unwrap();
        assert!(!state_file.contains("Secret hello"));
        assert!(!a.snapshot().to_string().contains("secret_key"));
        let id = a.state.threads[0].messages[0].id.clone();
        drop(a);
        let mut a = Session::open(a_dir.path(), "127.0.0.1:0".parse().unwrap()).unwrap();
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Queued
        );
        sync(&mut a);
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Relayed
        );
        sync(&mut b);
        sync(&mut b);
        assert_eq!(b.state.threads[0].messages.len(), 1);
        assert_eq!(b.state.threads[0].unread_count, 1);
        assert_eq!(
            b.snapshot()["threads"][0]["messages"][0]["text"],
            "Secret hello 👋"
        );
        assert_eq!(b.state.threads[0].messages[0].id, id);
        b.command(json!({"op":"read", "recipient":alice.profile.fingerprint}))
            .unwrap();
        assert_eq!(b.state.threads[0].unread_count, 0);
        b.command(
            json!({"op":"message", "recipient":alice.profile.fingerprint, "content":"Hello Alice"}),
        )
        .unwrap();
        sync(&mut b);
        sync(&mut a);
        assert_eq!(
            a.snapshot()["threads"][0]["messages"][1]["text"],
            "Hello Alice"
        );
    }
    #[test]
    fn invalid_contacts_and_unverified_messages_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = client(dir.path(), "alice");
        assert!(s
            .command(json!({"op":"contact", "input":"garbage"}))
            .is_err());
        assert!(s
            .command(json!({"op":"profile", "username":"bad name"}))
            .is_err());
        assert!(s
            .command(json!({"op":"message", "recipient":"unknown", "content":"hello"}))
            .is_err());
        assert!(s
            .command(json!({"op":"contact", "input":s.invitation().unwrap()}))
            .is_err());
        let before = s.state.profile.clone().unwrap();
        s.command(json!({"op":"profile", "username":"alice_new", "bio":"Updated", "address":"127.0.0.1:47470"})).unwrap();
        assert_eq!(
            s.state.profile.as_ref().unwrap().profile.fingerprint,
            before.profile.fingerprint
        );
        assert!(s.state.profile.as_ref().unwrap().verify().unwrap());
    }
    #[test]
    fn failed_commit_preserves_state_and_concurrent_sync_keeps_new_contacts() {
        let dir = tempfile::tempdir().unwrap();
        let peer_dir = tempfile::tempdir().unwrap();
        let mut s = client(dir.path(), "alice");
        let b = client(peer_dir.path(), "bob");
        let work = s.prepare_sync().unwrap();
        s.command(json!({"op":"contact", "input":b.invitation().unwrap()}))
            .unwrap();
        s.apply_sync(work()).unwrap();
        assert_eq!(s.state.contacts.len(), 1);
        std::fs::remove_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data"), "block writes").unwrap();
        assert!(s
            .command(json!({"op":"post", "content":"Must not appear"}))
            .is_err());
        assert!(s.state.posts.is_empty());
    }
}
