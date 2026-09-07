//! Durable Android host state. Commands commit before changing the visible state.
use crate::{actions::*, discovery::*, model::*, transport::*, *};
use futures::executor::block_on;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

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
        Ok(Self {
            state,
            storage,
            transport,
            discovery: LanDiscovery::new(),
            paused: false,
            listener_error: None,
            last_sync: "never".into(),
        })
    }

    pub fn start(&mut self) {
        self.listener_error = self.transport.start_server().err();
        self.start_discovery();
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
        json!({"profile": self.state.profile.as_ref().map(|p| &p.profile), "posts": self.state.posts,
            "contacts": self.state.contacts, "threads": threads, "nearby": nearby,
            "address": self.state.address, "listening": self.transport.advertised_addr(),
            "listenerError": self.listener_error, "paused": self.paused,
            "discovery": self.discovery.is_active(), "lastSync": self.last_sync,
            "peers": self.transport.peer_snapshot().len()})
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
        if field("op") == "profile" {
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
