//! Durable Android host state. Commands commit before changing the visible state.
use crate::{
    actions::*,
    delivery::{DurableStore, PublishOutcome, StoreOutcome, SwarmStore},
    device::{
        DeviceCertificate, DeviceKey, DEFAULT_CAPABILITIES, DEVICE_CERT_RENEW_SECS,
        DEVICE_CERT_TTL_SECS,
    },
    discovery::*,
    model::*,
    peer::{
        referral_from_object, ContactPolicy, PeerInbound, PeerNode, PeerOptions, PeerStatus,
        PeerTarget, UpdateKind,
    },
    relay::{
        community_relays_from_env, configured_relay_token, configured_relays_from_env, now_secs,
        relays_disabled_from_env, staging_relays_from_env, ReferredRelay, RelayGrant, RelayHealth,
        RelayInputs, RelayPlan, RelayReferral, MAX_REFERRALS, RELAY_REFERRAL_TTL_SECS,
    },
    repository::{CanonicalState as State, InboundSpool, IndexedStore},
    transport::*,
    *,
};
use futures::executor::block_on;
use serde_json::{json, Value};
use snartnet_core::{KeyPair, SignedProfile};
use std::{path::Path, sync::Arc};

pub struct Session {
    state: State,
    repository: IndexedStore,
    pub transport: TcpSwarmTransport,
    /// The durable publication seam (M7.1). Publishing happens *before* any direct push and
    /// its outcome is what decides whether a push may happen at all.
    store: Box<dyn DurableStore>,
    /// Accepted peer objects, stored durably before they are acknowledged (M7.3).
    spool: Arc<InboundSpool>,
    /// Authenticated, contact-scoped iroh endpoint (ADR 0003). Replaces the old global
    /// gossip topic: only contacts with a policy can exchange frames with us.
    pub peer: Option<Arc<PeerNode>>,
    /// This device's iroh secret. Kept out of [`State`] so it never reaches the mirror.
    device: Option<DeviceKey>,
    /// Why the peer endpoint could not be opened, shown next to the peer status.
    peer_error: Option<String>,
    /// Why the last durable publication failed. A frontend shows this next to the delivery
    /// state: "queued" is honest only when the reason is visible (M7.5).
    publish_error: Option<String>,
    /// Local relay observations, scored from the live endpoint (M8.4).
    relay_health: RelayHealth,
    /// The plan the running endpoint is reconciled against, for the snapshot (M8.2).
    relay_plan: RelayPlan,
    discovery: LanDiscovery,
    paused: bool,
    pub listener_error: Option<String>,
    last_sync: String,
}

impl Session {
    pub fn open(root: &Path, bind: std::net::SocketAddr) -> Result<Self, String> {
        let repository = IndexedStore::open(root)?;
        let state = repository.load_state()?;
        // One owner for the auxiliary ports (crate::ports): the transport opens the torrent
        // node for a concrete bind, and the DHT node once an identity is attached. The
        // session no longer opens a second node on a derived port or falls back to a fixed
        // one, which is what used to make the two binders collide.
        let transport = TcpSwarmTransport::new(&root.join("swarm"), bind)?;
        // The device secret is a long-lived identity of its own (ADR 0003): generated once,
        // stored in the identity table, and never part of the mirror-visible state.
        let device = match repository.device_key()? {
            Some(secret) => Some(DeviceKey::from_secret_base64(&secret)?),
            None => {
                let key = DeviceKey::generate();
                repository.save_device_key(&key.secret_base64())?;
                Some(key)
            }
        };
        Ok(Self {
            state,
            repository: repository.clone(),
            store: Box::new(SwarmStore::new(transport.clone())),
            spool: Arc::new(InboundSpool::new(repository)),
            transport,
            peer: None,
            device,
            peer_error: None,
            publish_error: None,
            relay_health: RelayHealth::default(),
            relay_plan: RelayPlan::default(),
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
            self.publish_profile_durable();
        }
        self.start_discovery();
    }

    fn start_distributed(&mut self) {
        // The transport owns the sockets, so all that is left here is giving it the signing
        // identity the signed DHT records need, and opening the device endpoint.
        if let Some(keypair) = self.state.keypair.clone() {
            self.transport.set_identity(&keypair);
        }
        self.ensure_peer();
    }

    /// Open the device endpoint on first use.
    ///
    /// A certificate binds the endpoint to a profile fingerprint, so there is nothing to
    /// open before a profile exists. A stored certificate is reused until it expires, which
    /// keeps `issued_at` (and therefore every contact's pin) stable across restarts.
    fn ensure_peer(&mut self) {
        if self.peer.is_some() {
            return;
        }
        let (Some(device), Some(profile), Some(keypair)) = (
            self.device.clone(),
            self.state.profile.clone(),
            self.state.keypair.clone(),
        ) else {
            return;
        };
        match self.device_certificate(&profile, &keypair, &device) {
            Ok(certificate) => {
                let plan = self.build_relay_plan();
                let options = PeerOptions {
                    discovery: peer::discovery_mode_from_env(),
                    relay: plan.clone(),
                };
                match PeerNode::open_with(&device, certificate, options) {
                    Ok(node) => {
                        let node = Arc::new(node);
                        // Install the durable sink before anything can connect: an object the
                        // peer handler acknowledged has to already be storable (M7.3).
                        node.set_inbound_persist(self.spool.clone());
                        node.set_contacts(self.peer_policies());
                        self.peer = Some(node);
                        self.peer_error = None;
                        self.relay_plan = plan;
                    }
                    Err(error) => self.peer_error = Some(error),
                }
            }
            Err(error) => self.peer_error = Some(error),
        }
    }

    /// The certificate to present to contacts: the stored one while it is still valid for
    /// this endpoint, otherwise a freshly issued one, persisted immediately.
    fn device_certificate(
        &self,
        profile: &SignedProfile,
        keypair: &KeyPair,
        device: &DeviceKey,
    ) -> Result<DeviceCertificate, String> {
        let now = unix_secs();
        if let Some(stored) = self.repository.device_certificate()? {
            let same_device = stored.endpoint_id == device.endpoint_id_string();
            let fresh = stored.expires_at > now.saturating_add(DEVICE_CERT_RENEW_SECS);
            let certified = stored
                .verify_for_profile(&profile.profile.fingerprint, &device.endpoint_id(), now)
                .is_ok();
            if same_device && fresh && certified {
                return Ok(stored);
            }
        }
        let certificate = DeviceCertificate::issue(
            profile,
            keypair,
            device,
            &DEFAULT_CAPABILITIES,
            now,
            DEVICE_CERT_TTL_SECS,
        )?;
        self.repository.save_device_certificate(&certificate)?;
        Ok(certificate)
    }

    /// Policies for every verified contact, carrying the pins learned from earlier
    /// handshakes so a replayed certificate cannot be accepted after a restart.
    fn peer_policies(&self) -> Vec<ContactPolicy> {
        self.state
            .contacts
            .iter()
            .filter(|contact| contact.verification == VerificationState::Verified)
            .filter_map(|contact| {
                let mut policy =
                    ContactPolicy::new(&contact.fingerprint, contact.known_public_key.as_ref()?);
                policy.pinned_endpoint = contact.peer_endpoint_id.clone();
                policy.pinned_issued_at = contact.peer_certificate_issued_at;
                Some(policy)
            })
            .collect()
    }

    /// Dial targets for contacts whose device endpoint we already learned.
    ///
    /// The addresses are the device's iroh addresses (learned from a descriptor or a LAN
    /// announcement), never the TCP sync port in `transport_addr`.
    fn peer_targets(&self) -> Vec<PeerTarget> {
        self.state
            .contacts
            .iter()
            .filter_map(|contact| {
                Some(PeerTarget {
                    fingerprint: contact.fingerprint.clone(),
                    endpoint_id: contact.peer_endpoint_id.clone()?,
                    addrs: crate::peer::bounded_addrs(contact.peer_addrs.clone())
                        .iter()
                        .filter_map(|addr| addr.parse().ok())
                        .collect(),
                })
            })
            .collect()
    }

    /// Publish the local profile durably and return what happened (M7.1).
    ///
    /// The magnet only enters state once the copy is addressable, and a failure is recorded
    /// instead of being swallowed: the profile is what a contact needs before any message
    /// can be decrypted, so "published" has to mean something.
    fn publish_profile_durable(&mut self) -> StoreOutcome {
        let Some(profile) = self.state.profile.clone() else {
            return StoreOutcome::unsupported();
        };
        let outcome = self.store.store_profile(&profile);
        let durable = outcome.published.is_durable();
        self.record_publish(&outcome);
        if let Some(magnet) = outcome.locator.clone() {
            let mut next = self.state.clone();
            if let Some(local) = next.profile.as_mut() {
                local.profile.magnet_uri = Some(magnet);
            }
            let _ = self.commit(next);
        }
        if durable {
            if let Some(peer) = self.peer.as_ref() {
                peer.announce(UpdateKind::Profile, self.peer_targets());
            }
        }
        outcome
    }

    /// Publish the whole feed snapshot under the author fingerprint (M7.1).
    ///
    /// A feed is a mutable pointer, so the snapshot is what the DHT pointer names; a contact
    /// resolves the pointer and fetches the bytes from torrent peers.
    fn publish_feed_durable(&mut self) -> StoreOutcome {
        let Some(fingerprint) = self
            .state
            .profile
            .as_ref()
            .map(|p| p.profile.fingerprint.clone())
        else {
            return StoreOutcome::unsupported();
        };
        let outcome = self.store.store_posts(&fingerprint, &self.state.posts);
        let durable = outcome.published.is_durable();
        self.record_publish(&outcome);
        if durable {
            if let Some(peer) = self.peer.as_ref() {
                peer.announce(UpdateKind::Post, self.peer_targets());
            }
        }
        outcome
    }

    /// Publish one outbound message durably, before it is pushed anywhere (M7.1/M7.2).
    ///
    /// The caller must not push this message when the outcome refuses direct delivery: the
    /// item stays `Queued` and the next sync retries, which is the difference between a
    /// silent loss and a visible retry.
    fn publish_message_durable(&mut self, message: &SignedMessage) -> StoreOutcome {
        let outcome = self.store.store_message(message);
        self.record_publish(&outcome);
        outcome
    }

    /// Remember, or clear, why the last durable publication failed.
    fn record_publish(&mut self, outcome: &StoreOutcome) {
        self.publish_error = match &outcome.published {
            PublishOutcome::Failed(error) => Some(error.clone()),
            _ => None,
        };
    }

    /// Publish every outbound message that has no durable copy yet (M7.1).
    ///
    /// This runs before the push worker, because the outcome decides both the visible state
    /// and whether the message may be pushed at all. A failure leaves the message `Queued`
    /// with the reason in `publish_error`, so the next sync retries instead of the message
    /// silently looking delivered.
    fn publish_pending_outbound(&mut self) -> Result<usize, String> {
        let pending: Vec<SignedMessage> = self
            .state
            .threads
            .iter()
            .flat_map(|t| &t.messages)
            .filter(|m| !m.incoming && m.delivery == DeliveryState::Queued)
            .filter_map(|m| m.envelope.clone())
            .collect();
        let mut published = 0;
        for message in pending {
            let outcome = self.publish_message_durable(&message);
            if outcome.published.is_durable() {
                published += 1;
            }
            self.record_message_delivery(&message.message.id, &outcome)?;
        }
        Ok(published)
    }

    /// Fold one publication outcome into the visible state of that message (M7.1/M7.5).
    ///
    /// `Available` means exactly what it claims: a durable, addressable copy exists. A
    /// failure keeps the message `Queued` *and* records the reason, which is what stops the
    /// push worker from handing over a copy that does not exist.
    fn record_message_delivery(
        &mut self,
        message_id: &str,
        outcome: &StoreOutcome,
    ) -> Result<(), String> {
        let error = match &outcome.published {
            PublishOutcome::Failed(error) => Some(error.clone()),
            _ => None,
        };
        let durable = outcome.published.is_durable();
        let mut next = self.state.clone();
        let mut changed = false;
        for item in next.threads.iter_mut().flat_map(|t| &mut t.messages) {
            if item.incoming || item.id != message_id {
                continue;
            }
            changed |= item.delivery_error != error;
            item.delivery_error = error.clone();
            if durable {
                let before = item.delivery;
                item.delivery = item.delivery.strongest(DeliveryState::Available);
                changed |= item.delivery != before;
            }
        }
        if changed {
            self.commit(next)?;
        }
        Ok(())
    }

    /// Resolve contact profile torrents and encrypted mailbox descriptors.
    pub fn sync_distributed(&mut self) -> Result<usize, String> {
        self.start_distributed();
        // A LAN announcement may be the only thing that tells us where a contact's device
        // is, so hints are folded in before the peer channel is dialed.
        self.learn_lan_hints()?;
        // Publishing comes before pushing (M7.1): a message that has no durable copy yet is
        // published here, and only what this produced as addressable may be pushed.
        self.publish_pending_outbound()?;
        // Relay health is observed and the plan applied before anything is dialed (M8.4).
        self.refresh_relay_selection();
        // Every arrival path funnels through the same deduplicating intake (M7.4). The spool
        // is drained first because it is the durable record of what the peer handler already
        // acknowledged; the in-memory inbox then only adds objects whose persist was refused,
        // and a message id already in the thread is skipped either way.
        let mut received = self.ingest_spooled_inbound()?;
        received += self.ingest_peer_events()?;
        let Some(torrent) = self.transport.torrent() else {
            return Ok(received);
        };
        let local_fp = self
            .state
            .profile
            .as_ref()
            .map(|p| p.profile.fingerprint.clone())
            .unwrap_or_default();
        let mut next = self.state.clone();
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
            for signed in self
                .transport
                .load_distributed_messages(&contact, &local_fp)
            {
                let thread = thread_mut(&mut next, &fingerprint);
                if thread.messages.iter().any(|m| m.id == signed.message.id) {
                    continue;
                }
                // `from_signed` with `incoming` is already `Received`: we resolved it from a
                // signed pointer and fetched and verified the bytes (M7.5).
                let mut item = ChatItem::from_signed(
                    signed,
                    true,
                    contact.known_encryption_public_key.clone(),
                );
                item.pushed_via_bittorrent = true;
                thread.messages.push(item);
                received += 1;
            }
        }
        if received > 0 {
            self.commit(next)?;
        }
        Ok(received)
    }

    /// Ingest objects the peer handler stored before it acknowledged them (M7.3/M7.4).
    ///
    /// The spool outlives the process, so an object acknowledged just before a crash is not
    /// lost. Every entry is re-verified exactly like an in-memory arrival, and entries are
    /// dropped from the spool only after the canonical commit succeeded, which is what makes
    /// the spool a retry log instead of a loss.
    fn ingest_spooled_inbound(&mut self) -> Result<usize, String> {
        let spooled = self.repository.spooled_inbound()?;
        if spooled.is_empty() {
            return Ok(0);
        }
        let local_fp = self
            .state
            .profile
            .as_ref()
            .map(|p| p.profile.fingerprint.clone())
            .unwrap_or_default();
        let mut next = self.state.clone();
        let mut received = 0;
        let mut ingested = Vec::with_capacity(spooled.len());
        let mut referral_objects = Vec::new();
        for entry in &spooled {
            // An entry is removed from the spool even when it cannot be ingested: bytes that
            // can never become a valid object must not block the spool forever.
            ingested.push(entry.id.clone());
            let Ok(object) = serde_json::from_str::<Value>(&entry.object_json) else {
                continue;
            };
            if referral_from_object(&object).is_some() {
                referral_objects.push((entry.fingerprint.clone(), object));
                continue;
            }
            let Ok(signed) = serde_json::from_value::<SignedMessage>(object) else {
                continue;
            };
            let Some(contact) = next
                .contacts
                .iter()
                .find(|c| {
                    c.fingerprint == entry.fingerprint
                        && c.fingerprint == signed.message.sender_fingerprint
                })
                .cloned()
            else {
                continue;
            };
            // The peer channel proved the sender holds the profile key, but the object still
            // has to satisfy the same signature and recipient checks as any other path.
            if !sync::accepts_message(&signed, &contact, &local_fp) {
                continue;
            }
            let thread = thread_mut(&mut next, &contact.fingerprint);
            if thread.messages.iter().any(|m| m.id == signed.message.id) {
                continue;
            }
            let mut item =
                ChatItem::from_signed(signed, true, contact.known_encryption_public_key.clone());
            item.pushed_via_iroh = true;
            thread.messages.push(item);
            received += 1;
        }
        if received > 0 {
            self.commit(next)?;
        }
        self.repository.clear_spooled(&ingested)?;
        received += self.ingest_referrals(referral_objects)?;
        Ok(received)
    }

    /// Fold what the peer endpoint authenticated since the last tick into canonical state.
    ///
    /// Two kinds of events arrive here: certificates our contacts presented (which become
    /// replay-protection pins) and already-verified objects they sent. Both are committed
    /// before returning, so a crash cannot lose a pin and re-open a replay window.
    fn ingest_peer_events(&mut self) -> Result<usize, String> {
        let Some(peer) = self.peer.clone() else {
            return Ok(0);
        };
        let accepted = peer.take_accepted();
        let inbound = peer.drain_inbox();
        if accepted.is_empty() && inbound.is_empty() {
            return Ok(0);
        }
        let local_fp = self
            .state
            .profile
            .as_ref()
            .map(|p| p.profile.fingerprint.clone())
            .unwrap_or_default();
        let mut next = self.state.clone();
        let mut received = 0;
        let mut referral_objects = Vec::new();
        for certificate in accepted {
            if let Some(contact) = next
                .contacts
                .iter_mut()
                .find(|c| c.fingerprint == certificate.fingerprint)
            {
                // Keep the newest certificate we ever accepted for this contact, so an
                // older (but still valid) one cannot be replayed later.
                if contact.peer_certificate_issued_at.unwrap_or(0) <= certificate.issued_at {
                    contact.peer_endpoint_id = Some(certificate.endpoint_id);
                    contact.peer_certificate_issued_at = Some(certificate.issued_at);
                }
            }
        }
        for event in inbound {
            let (fingerprint, object) = match event {
                PeerInbound::Object {
                    fingerprint,
                    object,
                    ..
                } => (fingerprint, object),
                // A notice only says "something changed"; the next sync pulls the content
                // over the verified torrent/DHT paths, so nothing is ingested here.
                PeerInbound::Notice { .. } => continue,
            };
            // A relay referral is a signed record rather than a message; it is verified
            // against the sender's own key after this commit, because accepting one commits
            // state of its own (M8.3).
            if referral_from_object(&object).is_some() {
                referral_objects.push((fingerprint, object));
                continue;
            }
            let Ok(signed) = serde_json::from_value::<SignedMessage>(object) else {
                continue;
            };
            let Some(contact) = next
                .contacts
                .iter()
                .find(|c| c.fingerprint == signed.message.sender_fingerprint)
                .cloned()
            else {
                continue;
            };
            // The transport proved the sender holds the profile key, but the message still
            // has to satisfy the same signature and recipient checks as any other path.
            if fingerprint != contact.fingerprint
                || !sync::accepts_message(&signed, &contact, &local_fp)
            {
                continue;
            }
            let thread = thread_mut(&mut next, &contact.fingerprint);
            if thread.messages.iter().any(|m| m.id == signed.message.id) {
                continue;
            }
            let mut item =
                ChatItem::from_signed(signed, true, contact.known_encryption_public_key.clone());
            item.pushed_via_iroh = true;
            thread.messages.push(item);
            received += 1;
        }
        self.commit(next)?;
        received += self.ingest_referrals(referral_objects)?;
        Ok(received)
    }

    /// Fold referral objects a peer sent into state (M8.3).
    ///
    /// Runs after the surrounding intake committed, because accepting a referral commits state
    /// of its own: two commits racing on one snapshot would drop one of them.
    fn ingest_referrals(&mut self, referrals: Vec<(String, Value)>) -> Result<usize, String> {
        let mut stored = 0;
        for (fingerprint, object) in referrals {
            if self.accept_referral(&fingerprint, &object)? {
                stored += 1;
            }
        }
        if stored > 0 {
            // A new referral can change the relay plan, and a plan change is applied to the
            // running endpoint in place (M8.4).
            self.refresh_relay_selection();
        }
        Ok(stored)
    }

    fn commit(&mut self, next: State) -> Result<(), String> {
        self.repository.save_state(&next)?;
        self.state = next;
        Ok(())
    }

    fn start_discovery(&mut self) {
        if let Some(p) = &self.state.profile {
            // Advertise the device endpoint next to the TCP address, so a contact that only
            // saw a LAN announcement can dial the authenticated peer channel (M7.2). An
            // announcement is unauthenticated: it only suggests a device, and the handshake
            // still validates the certificate against the profile key and the endpoint id.
            let (peer_endpoint_id, peer_addrs) = match self.peer.as_ref() {
                Some(peer) => (
                    Some(peer.node_id()),
                    peer.bound_sockets()
                        .iter()
                        .map(|addr| addr.to_string())
                        .collect(),
                ),
                None => (
                    self.device.as_ref().map(DeviceKey::endpoint_id_string),
                    Vec::new(),
                ),
            };
            self.discovery.start(LanAnnounce {
                fingerprint: p.profile.fingerprint.clone(),
                username: p.profile.username.clone(),
                display_name: p.profile.display_name.clone(),
                tcp_addr: self.transport.advertised_addr(),
                peer_endpoint_id,
                peer_addrs,
            });
            // Contacts and their pinned devices both move, so every start refreshes the
            // policies (which may not reach us) and the dial targets (which may).
            self.refresh_peer_contacts();
        }
    }

    /// Build the relay plan from every source the session holds (M8.2).
    ///
    /// A referral is usable here only if it came from a contact we verified, verifies again
    /// against that contact's profile key, has not expired, and — when it carries a grant —
    /// decrypts with our own key and that contact's encryption key. Anything else is ignored
    /// rather than failing the plan, because one bad referral must not cost us a relay.
    fn build_relay_plan(&self) -> RelayPlan {
        let now = now_secs();
        let referred: Vec<ReferredRelay> = self
            .state
            .relay_referrals
            .iter()
            .filter_map(|referral| self.referred_relay(referral, now))
            .collect();
        let configured = configured_relays_from_env();
        let community = community_relays_from_env();
        RelayPlan::build(
            RelayInputs {
                disabled: relays_disabled_from_env(),
                staging: staging_relays_from_env(),
                configured: &configured,
                referrals: &referred,
                community: &community,
            },
            &self.relay_health,
        )
    }

    /// One verified referral, with its grant opened, or `None` when it is not usable.
    fn referred_relay(&self, referral: &RelayReferral, now: u64) -> Option<ReferredRelay> {
        let contact = self
            .state
            .contacts
            .iter()
            .find(|contact| contact.fingerprint == referral.referrer)
            .filter(|contact| contact.verification == VerificationState::Verified)?;
        let verified = referral
            .verify(contact.known_public_key.as_deref()?, now)
            .ok()?;
        let token = match (&referral.grant, self.state.keypair.as_ref()) {
            (Some(grant), Some(keypair)) => grant
                .open(keypair, contact.known_encryption_public_key.as_deref()?)
                .ok(),
            _ => None,
        };
        Some(ReferredRelay {
            referrer: verified.referrer,
            relay: verified.relay,
            expires_at: verified.expires_at,
            token,
        })
    }

    /// Score the live endpoint's relays and apply the resulting plan (M8.4).
    ///
    /// The map is reconciled in place with iroh's own add/remove API, so a relay that keeps
    /// failing moves behind a working one without touching the device key: every contact's
    /// certificate pin stays valid, and a plan change is not a reconnection event.
    fn refresh_relay_selection(&mut self) {
        let Some(peer) = self.peer.clone() else {
            return;
        };
        for observation in peer.relay_status() {
            self.relay_health.observe(
                &observation.url,
                observation.connected,
                observation.last_error,
            );
        }
        let plan = self.build_relay_plan();
        match peer.apply_relay_map(&plan, &self.relay_health) {
            Ok(_) => {
                self.relay_plan = plan;
                if self.peer_error.is_some() {
                    self.peer_error = None;
                }
            }
            Err(error) => self.peer_error = Some(error),
        }
    }

    /// The referral to hand one contact: our configured relay, with its token sealed to them.
    ///
    /// Only the operator's own configured relay is referred, and only its own token is
    /// attached. A token that arrived in someone else's referral is theirs to give, not ours to
    /// pass on, and re-sealing it would spread one relay secret across a whole contact list.
    fn referral_for(&self, contact: &Contact) -> Option<RelayReferral> {
        let keypair = self.state.keypair.as_ref()?;
        let relay = configured_relays_from_env().into_iter().next()?;
        let recipient_key = contact.known_encryption_public_key.as_deref()?;
        let grant = match configured_relay_token() {
            Some(token) => Some(RelayGrant::seal(keypair, recipient_key, &token).ok()?),
            None => None,
        };
        RelayReferral::issue(keypair, &relay, now_secs(), RELAY_REFERRAL_TTL_SECS, grant).ok()
    }

    /// Every referral to send this round, one per contact that can read a grant (M8.3).
    fn referral_offers(&self) -> Vec<(String, RelayReferral)> {
        self.state
            .contacts
            .iter()
            .filter(|contact| contact.verification == VerificationState::Verified)
            .filter_map(|contact| Some((contact.fingerprint.clone(), self.referral_for(contact)?)))
            .collect()
    }

    /// Accept a referral a contact sent over the peer channel (M8.3).
    ///
    /// A referral is only accepted from the contact whose profile key verifies the signature,
    /// so a peer cannot recommend relays on someone else's behalf. The signed record is what is
    /// stored; a grant inside it stays ciphertext until a plan needs the token.
    fn accept_referral(&mut self, sender: &str, object: &Value) -> Result<bool, String> {
        let Some(referral) = referral_from_object(object) else {
            return Ok(false);
        };
        if referral.referrer != sender {
            return Ok(false);
        }
        let Some(contact) = self
            .state
            .contacts
            .iter()
            .find(|contact| contact.fingerprint == sender)
            .cloned()
        else {
            return Ok(false);
        };
        let Some(public_key) = contact.known_public_key.as_deref() else {
            return Ok(false);
        };
        if referral.verify(public_key, now_secs()).is_err() {
            return Ok(false);
        }
        let mut next = self.state.clone();
        next.relay_referrals
            .retain(|held| held.relay != referral.relay || held.referrer != referral.referrer);
        next.relay_referrals.insert(0, referral);
        // Bounded: the newest referrals describe the infrastructure that is up now, so the
        // oldest are the ones to drop.
        next.relay_referrals.truncate(MAX_REFERRALS);
        self.commit(next)?;
        Ok(true)
    }

    /// Take device hints from LAN announcements for contacts we already know (M7.2).
    ///
    /// Without this, a contact that met us only on the LAN has no pinned device and is never
    /// dialed over the peer channel. A hint stays a hint: the handshake rejects a wrong
    /// endpoint id, a wrong certificate, and a replayed one.
    fn learn_lan_hints(&mut self) -> Result<(), String> {
        let discovered = self.discovery.get_discovered();
        if discovered.is_empty() {
            return Ok(());
        }
        let mut next = self.state.clone();
        let mut changed = false;
        for peer in discovered {
            let Some(contact) = next
                .contacts
                .iter_mut()
                .find(|c| c.fingerprint == peer.fingerprint)
            else {
                continue;
            };
            if let Some(endpoint_id) = peer.peer_endpoint_id.filter(|id| !id.is_empty()) {
                if contact.peer_endpoint_id.as_deref() != Some(endpoint_id.as_str()) {
                    contact.peer_endpoint_id = Some(endpoint_id);
                    changed = true;
                }
            }
            let addrs = crate::peer::bounded_addrs(peer.peer_addrs);
            if !addrs.is_empty() && contact.peer_addrs != addrs {
                contact.peer_addrs = addrs;
                changed = true;
            }
        }
        if changed {
            self.commit(next)?;
            self.refresh_peer_contacts();
        }
        Ok(())
    }

    /// Push the current contacts into the running endpoint and restart dialing.
    fn refresh_peer_contacts(&self) {
        let policies = self.peer_policies();
        let targets = self.peer_targets();
        if let Some(peer) = &self.peer {
            peer.set_contacts(policies);
            peer.start(targets);
        }
    }

    /// One full sync round: inbound intake, durable publication, then both direct push paths.
    ///
    /// The daemon and the Android bridge both drive their cadence through this, so the order
    /// that makes delivery honest — publish, then push what may be pushed — exists in one
    /// place instead of once per frontend.
    pub fn sync_once(&mut self) -> Result<usize, String> {
        let received = self.sync_distributed()?;
        if let Some(work) = self.prepare_sync() {
            let result = work();
            self.apply_sync(result)?;
        }
        Ok(received)
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
        // A message may be pushed once it has no unresolved publication failure (M7.1). On a
        // host without a durable transport at all — an ephemeral test bind, or a frontend with
        // the swarm switched off — every publish reports `Unsupported`, so direct delivery is
        // the only path there is. A *failed* publication records a reason, and that message
        // stays out of this list until a later sync publishes it.
        let pending = self
            .state
            .threads
            .iter()
            .flat_map(|t| &t.messages)
            .filter(|m| !m.incoming && m.delivery_error.is_none())
            .filter(|m| matches!(m.delivery, DeliveryState::Queued | DeliveryState::Available))
            .filter_map(|m| m.envelope.clone())
            .collect();
        let peer = self.peer.clone();
        let referrals = self.referral_offers();
        Some(move || {
            sync::exchange(
                transport, profile, posts, contacts, pending, peer, referrals,
            )
        })
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
                let peer_endpoint_id = c.peer_endpoint_id.clone();
                let peer_addrs = c.peer_addrs.clone();
                let issued_at = c.peer_certificate_issued_at;
                *c = refreshed;
                c.transport_addr = endpoint;
                c.alias = alias;
                c.peer_endpoint_id = peer_endpoint_id;
                c.peer_addrs = peer_addrs;
                c.peer_certificate_issued_at = issued_at;
            }
        }
        for (fp, item) in result.incoming {
            let thread = thread_mut(&mut next, &fp);
            if !thread.messages.iter().any(|m| m.id == item.id) {
                thread.messages.push(item);
                thread.unread_count = thread.unread_count.saturating_add(1);
            }
        }
        // The push worker reports the paths that accepted each message; recording them keeps
        // the strongest state instead of downgrading a message that already had a replica.
        for (id, paths) in result.delivered {
            for m in next.threads.iter_mut().flat_map(|t| &mut t.messages) {
                if !m.incoming && m.id == id {
                    m.record_path(paths.bittorrent, paths.iroh);
                }
            }
        }
        self.commit(next)?;
        self.last_sync = ts_label();
        Ok(())
    }

    /// Public snapshot deliberately excludes secret keys. Plaintext exists only in memory.
    pub fn snapshot(&self) -> Value {
        let (dht, torrent) = self.transport.distributed_status();
        let threads: Vec<Value> = self.state.threads.iter().map(|t| {
            let contact = self.state.contacts.iter().find(|c| c.fingerprint == t.contact_fingerprint);
            let messages: Vec<Value> = t.messages.iter().map(|m| {
                let plaintext = decrypt_for_display(m, self.state.keypair.as_ref(),
                    contact.and_then(|c| c.known_encryption_public_key.as_deref()));
                json!({"id": m.id, "incoming": m.incoming, "ciphertext": m.content,
                    "text": plaintext.as_ref().ok(), "error": plaintext.as_ref().err(),
                    "encrypted": m.encrypted, "delivery": m.delivery,
                    "deliveryLabel": m.delivery.label(),
                    "deliveryError": m.delivery_error,
                    "viaBittorrent": m.pushed_via_bittorrent, "viaIroh": m.pushed_via_iroh,
                    "time": m.created_label})
            }).collect();
            json!({"fingerprint": t.contact_fingerprint, "unread": t.unread_count, "messages": messages})
        }).collect();
        let nearby = self.discovery.get_discovered();
        let nearby: Vec<Value> = nearby.iter().map(|p| json!({
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
            "dht": dht,
            "torrent": torrent,
            // What the durable path is doing right now (M7.5): whether this host can publish
            // at all, why the last publication failed, and how much inbound is waiting.
            "delivery": json!({
                "durable": self.transport.has_durable_transport(),
                "failed": self.publish_error,
                "spooled": self.repository.spooled_count().unwrap_or(0),
            }),
            // Relay selection: which sources decided the map, what is applied right now, and
            // how each relay has behaved locally (M8.2/M8.4).
            "relay": json!({
                "plan": self.relay_plan.summary(),
                "source": self.relay_plan.source().map(|source| source.label()),
                "disabled": self.relay_plan.is_disabled(),
                "planned": self.relay_plan.active_urls(),
                "active": self.peer.as_ref().map(|peer| peer.applied_relays()).unwrap_or_default(),
                "health": self
                    .relay_health
                    .snapshot()
                    .into_iter()
                    .map(|(url, score)| json!({
                        "url": url,
                        "connected": score.connected,
                        "score": score.score(),
                        "failures": score.failures,
                        "lastError": score.last_error,
                    }))
                    .collect::<Vec<Value>>(),
            }),
            "peer": self.peer_status()})
    }

    /// The peer subsystem as the snapshot's `peer` key.
    ///
    /// A node that never opened still reports why, so a frontend can distinguish
    /// "no profile yet" from a real failure instead of showing a silent blank.
    fn peer_status(&self) -> Value {
        match self.peer.as_ref() {
            Some(node) => json!(node.status()),
            None => json!(PeerStatus {
                active: false,
                node_id: self.device.as_ref().map(|d| d.endpoint_id_string()),
                peer_count: 0,
                discovery: peer::discovery_label(PeerOptions::from_env().discovery).to_string(),
                last_error: self.peer_error.clone(),
            }),
        }
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
                    if let Some(peer) = &self.peer {
                        peer.stop();
                    }
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
            self.publish_profile_durable();
        }
        if operation == "post" {
            self.start_distributed();
            self.publish_feed_durable();
        }
        if operation == "contact" {
            // A new contact, or a new address for one we already know, changes both the
            // policies we enforce and the devices we dial.
            self.start_distributed();
            self.refresh_peer_contacts();
        }
        if operation == "message" {
            self.start_distributed();
            // Publish before anything can push: a failed publication leaves the message
            // queued and the sync worker will retry it (M7.1).
            self.publish_pending_outbound()?;
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
    use crate::peer::referral_object;
    use crate::relay::RelaySource;
    fn client(root: &Path, name: &str) -> Session {
        let mut s = Session::open(root, "127.0.0.1:0".parse().unwrap()).unwrap();
        s.command(json!({"op":"profile", "username":name})).unwrap();
        s
    }
    fn sync(s: &mut Session) {
        let result = s.prepare_sync().unwrap()();
        s.apply_sync(result).unwrap();
    }
    /// A store that refuses messages while profiles and feeds keep working, standing in for a
    /// full disk or an unreachable swarm.
    struct FailingStore;

    impl DurableStore for FailingStore {
        fn store_profile(&self, _: &SignedProfile) -> StoreOutcome {
            StoreOutcome::unsupported()
        }
        fn store_posts(&self, _: &str, _: &[SignedPost]) -> StoreOutcome {
            StoreOutcome::unsupported()
        }
        fn store_message(&self, _: &SignedMessage) -> StoreOutcome {
            StoreOutcome::failed("disk is full")
        }
    }

    /// Bring two sessions together so each has the other's profile and a reachable address.
    ///
    /// Both listeners start before the invitations are built, which is what keeps the tests on
    /// the direct paths instead of the public DHT/torrent path (M7.6).
    fn pair(a: &mut Session, b: &mut Session) {
        b.transport.start_server().unwrap();
        a.transport.start_server().unwrap();
        a.command(json!({"op":"contact", "input":b.invitation().unwrap()}))
            .unwrap();
        b.command(json!({"op":"contact", "input":a.invitation().unwrap()}))
            .unwrap();
        sync(a);
        sync(b);
        sync(a);
        assert_eq!(
            a.state.contacts[0].verification,
            VerificationState::Verified
        );
        assert_eq!(
            b.state.contacts[0].verification,
            VerificationState::Verified
        );
    }

    /// Queue offline, retry until a peer accepts, survive a restart, and take the reply over
    /// the same direct paths (M7.2/M7.6).
    #[test]
    fn android_session_interoperates_with_desktop_exchange_and_recovers_outbox() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut a = client(a_dir.path(), "alice");
        let mut b = client(b_dir.path(), "bob");
        let address = b.transport.start_server().unwrap();
        assert_eq!(b.transport.advertised_addr().unwrap(), address.to_string());
        // Alice listens too, so neither direction has to fall back to the public swarm.
        a.transport.start_server().unwrap();
        let bob = b.state.profile.clone().unwrap();
        let alice = a.state.profile.clone().unwrap();
        a.command(json!({"op":"contact", "input":b.invitation().unwrap()}))
            .unwrap();
        b.command(json!({"op":"contact", "input":a.invitation().unwrap()}))
            .unwrap();
        sync(&mut a); // publish Alice and verify Bob using the desktop exchange
        assert_eq!(
            a.state.contacts[0].verification,
            VerificationState::Verified
        );
        a.command(json!({"op":"pause", "paused":true})).unwrap();
        a.command(json!({"op":"message", "recipient":bob.profile.fingerprint, "content":"Secret hello 👋"})).unwrap();
        let state_file =
            std::fs::read_to_string(a_dir.path().join("data/client_state.json")).unwrap();
        assert!(!state_file.contains("Secret hello"));
        assert!(!a.snapshot().to_string().contains("secret_key"));
        let id = a.state.threads[0].messages[0].id.clone();
        drop(a);
        // The restart listens again, which is what lets a reply reach it at all.
        let mut a = Session::open(a_dir.path(), "127.0.0.1:0".parse().unwrap()).unwrap();
        a.transport.start_server().unwrap();
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Queued
        );
        sync(&mut a);
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Relayed
        );
        assert!(a.state.threads[0].messages[0].pushed_via_bittorrent);
        sync(&mut b);
        sync(&mut b);
        assert_eq!(b.state.threads[0].messages.len(), 1);
        assert_eq!(b.state.threads[0].unread_count, 1);
        assert_eq!(
            b.snapshot()["threads"][0]["messages"][0]["text"],
            "Secret hello 👋"
        );
        assert_eq!(
            b.state.threads[0].messages[0].delivery,
            DeliveryState::Received
        );
        assert_eq!(b.state.threads[0].messages[0].id, id);
        b.command(json!({"op":"read", "recipient":alice.profile.fingerprint}))
            .unwrap();
        assert_eq!(b.state.threads[0].unread_count, 0);
        // A restarted client binds a new port, so Bob learns where Alice went the same way a
        // user re-shares an invitation. His stored address would otherwise be stale.
        b.command(json!({"op":"contact", "input":a.invitation().unwrap()}))
            .unwrap();
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
        assert!(a.state.threads[0].messages[1].pushed_via_bittorrent);
    }

    /// A publication failure is not papered over by a direct push (M7.1/M7.6).
    #[test]
    fn a_failed_publication_keeps_the_message_queued_and_unpushed() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut a = client(a_dir.path(), "alice");
        let mut b = client(b_dir.path(), "bob");
        pair(&mut a, &mut b);
        let bob = b.state.profile.clone().unwrap();
        // The durable path now refuses every message, which must block the push as well.
        a.store = Box::new(FailingStore);
        a.command(
            json!({"op":"message", "recipient":bob.profile.fingerprint, "content":"held back"}),
        )
        .unwrap();
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Queued
        );
        assert_eq!(
            a.state.threads[0].messages[0].delivery_error.as_deref(),
            Some("disk is full")
        );
        assert_eq!(
            a.snapshot()["threads"][0]["messages"][0]["deliveryError"],
            "disk is full"
        );
        sync(&mut a);
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Queued
        );
        assert_eq!(a.snapshot()["delivery"]["failed"], "disk is full");
        sync(&mut b);
        assert!(b.state.threads.is_empty());
        // With the store working again the next full round publishes what is addressable and
        // hands a copy over, so the failure left nothing behind once it was fixed.
        a.store = Box::new(SwarmStore::new(a.transport.clone()));
        a.sync_once().unwrap();
        assert!(a.state.threads[0].messages[0].delivery_error.is_none());
        assert_eq!(
            a.state.threads[0].messages[0].delivery,
            DeliveryState::Relayed
        );
        sync(&mut b);
        sync(&mut b);
        assert_eq!(b.state.threads[0].messages.len(), 1);
        assert_eq!(
            b.snapshot()["threads"][0]["messages"][0]["text"],
            "held back"
        );
    }

    /// An object stored before its acknowledgement is ingested after a restart (M7.3/M7.6).
    ///
    /// The spool is what survives a crash between writing the acknowledgement and folding the
    /// object into canonical state, so this test writes one and never hands it to the inbox.
    #[test]
    fn a_spooled_object_is_ingested_after_a_restart() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut a = client(a_dir.path(), "alice");
        let mut b = client(b_dir.path(), "bob");
        pair(&mut a, &mut b);
        let alice = a.state.profile.clone().unwrap();
        let bob = b.state.profile.clone().unwrap();
        let reply = block_on(create_message_async(
            bob.profile.fingerprint.clone(),
            alice.profile.fingerprint.clone(),
            "written before the acknowledgement".into(),
            b.state.keypair.clone(),
            alice
                .profile
                .encryption_public_key
                .clone()
                .expect("a profile always carries an encryption key"),
        ))
        .unwrap();
        a.repository
            .spool_inbound(
                &bob.profile.fingerprint,
                "bob-device",
                &serde_json::to_value(&reply).unwrap(),
            )
            .unwrap();
        assert_eq!(a.repository.spooled_count().unwrap(), 1);
        drop(a);
        let mut a = Session::open(a_dir.path(), "127.0.0.1:0".parse().unwrap()).unwrap();
        assert_eq!(a.repository.spooled_count().unwrap(), 1);
        a.sync_distributed().unwrap();
        let item = &a.state.threads[0].messages[0];
        assert_eq!(item.id, reply.message.id);
        assert!(item.incoming);
        assert_eq!(item.delivery, DeliveryState::Received);
        assert!(item.pushed_via_iroh);
        // Ingested once, and the spool no longer holds it.
        assert_eq!(a.repository.spooled_count().unwrap(), 0);
        a.sync_distributed().unwrap();
        assert_eq!(a.state.threads[0].messages.len(), 1);
    }

    /// A referral from a contact is verified, stored, and becomes the relay plan (M8.3/M8.4).
    #[test]
    fn an_accepted_referral_changes_the_relay_plan_but_a_forged_one_does_not() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let mut a = client(a_dir.path(), "alice");
        let mut b = client(b_dir.path(), "bob");
        pair(&mut a, &mut b);
        let alice = a.state.profile.clone().unwrap();
        let bob = b.state.profile.clone().unwrap();
        let bob_keypair = b.state.keypair.clone().unwrap();
        // Bob refers Alice to his relay, with the bearer token sealed to her own key.
        let grant = RelayGrant::seal(
            &bob_keypair,
            alice.profile.encryption_public_key.as_deref().unwrap(),
            "relay-bearer-token",
        )
        .unwrap();
        let referral = RelayReferral::issue(
            &bob_keypair,
            "https://relay.example.com",
            now_secs(),
            RELAY_REFERRAL_TTL_SECS,
            Some(grant),
        )
        .unwrap();
        // It arrives the way the peer channel delivers it: spooled before acknowledgement.
        a.repository
            .spool_inbound(
                &bob.profile.fingerprint,
                "bob-device",
                &referral_object(&referral),
            )
            .unwrap();
        assert_eq!(a.sync_distributed().unwrap(), 1);
        assert_eq!(a.state.relay_referrals.len(), 1);

        // The session verifies it against Bob's own key and opens the grant to his token.
        let referred = a
            .referred_relay(&referral, now_secs())
            .expect("a verified referral with a decryptable grant");
        assert_eq!(referred.referrer, bob.profile.fingerprint);
        assert_eq!(referred.relay, "https://relay.example.com/");
        assert_eq!(referred.token.as_deref(), Some("relay-bearer-token"));
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &[],
                referrals: &[referred],
                community: &[],
            },
            &RelayHealth::default(),
        );
        assert_eq!(plan.source(), Some(RelaySource::Referral));
        // The session's own plan uses the referral when the operator configured no relay of
        // its own, which would outrank it by design.
        if configured_relays_from_env().is_empty() {
            let plan = a.build_relay_plan();
            assert_eq!(plan.source(), Some(RelaySource::Referral));
            assert_eq!(
                plan.relays()[0].token.as_deref(),
                Some("relay-bearer-token")
            );
        }

        // A referral signed by someone else cannot speak for Bob, even naming him.
        let mut carol = KeyPair::generate().unwrap();
        carol.ensure_encryption_keys();
        let mut forged = RelayReferral::issue(
            &carol,
            "https://attacker.example.com",
            now_secs(),
            RELAY_REFERRAL_TTL_SECS,
            None,
        )
        .unwrap();
        forged.referrer = bob.profile.fingerprint.clone();
        a.repository
            .spool_inbound(
                &bob.profile.fingerprint,
                "bob-device",
                &referral_object(&forged),
            )
            .unwrap();
        assert_eq!(a.sync_distributed().unwrap(), 0);
        assert_eq!(a.state.relay_referrals.len(), 1);

        // Nor can an expired referral re-enter the plan.
        let expired = RelayReferral::issue(
            &bob_keypair,
            "https://old.example.com",
            now_secs() - RELAY_REFERRAL_TTL_SECS - 60,
            RELAY_REFERRAL_TTL_SECS,
            None,
        )
        .unwrap();
        a.repository
            .spool_inbound(
                &bob.profile.fingerprint,
                "bob-device",
                &referral_object(&expired),
            )
            .unwrap();
        assert_eq!(a.sync_distributed().unwrap(), 0);
        assert_eq!(a.state.relay_referrals.len(), 1);
        assert!(a
            .build_relay_plan()
            .active_urls()
            .iter()
            .all(|url| !url.contains("old.example.com")));
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
    fn canonical_commit_survives_a_broken_legacy_mirror_and_keeps_new_contacts() {
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
        s.command(json!({"op":"post", "content":"Canonical state wins"}))
            .unwrap();
        assert_eq!(s.state.posts.len(), 1);
    }
}
