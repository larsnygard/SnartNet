//! Blocking transport work runs on a worker. The UI applies a delta, so messages and
//! contacts added while a sync is running cannot be replaced by an older snapshot.
//!
//! The worker only *pushes*: durable publication already happened in the session (M7.1),
//! and its result is what decides whether a message may be pushed at all. Both direct
//! paths are attempted for every pending message (M7.2), and every arrival path funnels
//! through one deduplicating intake (M7.4).
use super::*;
use crate::peer::{PeerInbound, PeerNode, PeerTarget};
use std::sync::Arc;

/// Which direct paths carried one outbound message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeliveryPaths {
    /// Plain TCP: an inbox hand-off to a reachable peer.
    pub bittorrent: bool,
    /// The authenticated iroh peer channel.
    pub iroh: bool,
}

impl DeliveryPaths {
    pub fn any(self) -> bool {
        self.bittorrent || self.iroh
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub contacts: Vec<Contact>,
    pub incoming: Vec<(String, ChatItem)>,
    /// Message id to the paths that accepted it during this sync.
    pub delivered: Vec<(String, DeliveryPaths)>,
}

/// Add an inbound item, merging the paths of a duplicate instead of dropping the flags.
///
/// The same message can arrive over the torrent mailbox and the iroh channel in one sync;
/// collapsing the two onto one item is what makes the delivery state trustworthy (M7.4).
fn merge_incoming(incoming: &mut Vec<(String, ChatItem)>, fingerprint: &str, mut item: ChatItem) {
    if let Some((_, existing)) = incoming
        .iter_mut()
        .find(|(_, existing)| existing.id == item.id)
    {
        existing.pushed_via_bittorrent |= item.pushed_via_bittorrent;
        existing.pushed_via_iroh |= item.pushed_via_iroh;
        existing.delivery = existing.delivery.strongest(item.delivery);
        return;
    }
    item.delivery = item.delivery.strongest(DeliveryState::Received);
    incoming.push((fingerprint.to_string(), item));
}

pub fn exchange(
    transport: TcpSwarmTransport,
    profile: SignedProfile,
    posts: Vec<SignedPost>,
    mut contacts: Vec<Contact>,
    pending: Vec<SignedMessage>,
    peer: Option<Arc<PeerNode>>,
) -> SyncResult {
    // Announce our signed key before sending envelopes. This also enables replies when only
    // one side has a reachable TCP endpoint. Durable publication is the session's job
    // (M7.1); this worker only hands copies to reachable peers.
    transport.push_profile(&profile);
    transport.push_posts(&profile.profile.fingerprint, posts);
    let mut result = SyncResult::default();
    for contact in &mut contacts {
        if let Some(blob) = transport.load_profile_for_contact(contact) {
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
    // DHT/torrent mailbox polling is a direct peer-to-peer path. The DHT only
    // supplies signed manifest pointers; message bytes come from torrent peers.
    for contact in &contacts {
        for signed in transport.load_distributed_messages(contact, &profile.profile.fingerprint) {
            if accepts_message(&signed, contact, &profile.profile.fingerprint) {
                let mut item = ChatItem::from_signed(
                    signed,
                    true,
                    contact.known_encryption_public_key.clone(),
                );
                item.pushed_via_bittorrent = true;
                merge_incoming(&mut result.incoming, &contact.fingerprint, item);
            }
        }
    }
    for message in pending {
        // Both direct paths are attempted, not just the first one that works: a message that
        // reaches the recipient twice is deduplicated on arrival (M7.4), while a message
        // that reaches them over only one path is still delivered (M7.2).
        let paths = DeliveryPaths {
            bittorrent: transport.push_message(&message),
            iroh: peer
                .as_ref()
                .and_then(|peer| {
                    contacts
                        .iter()
                        .find(|c| c.fingerprint == message.message.recipient_fingerprint)
                        .and_then(peer_target_for)
                        .and_then(|target| {
                            let object = serde_json::to_value(&message).ok()?;
                            Some(peer.send_object(&target, &object))
                        })
                })
                .unwrap_or(false),
        };
        if paths.any() {
            result.delivered.push((message.message.id, paths));
        }
    }
    if let Some(inbox) = transport.load_inbox(&profile.profile.fingerprint) {
        for signed in inbox.messages {
            if let Some(contact) = contacts
                .iter()
                .find(|c| c.fingerprint == signed.message.sender_fingerprint)
            {
                if accepts_message(&signed, contact, &profile.profile.fingerprint) {
                    let mut item = ChatItem::from_signed(
                        signed,
                        true,
                        contact.known_encryption_public_key.clone(),
                    );
                    item.pushed_via_bittorrent = true;
                    merge_incoming(&mut result.incoming, &contact.fingerprint, item);
                }
            }
        }
    }
    // Chat messages that arrived over the authenticated iroh peer channel while we weren't
    // reachable any other way.
    if let Some(peer) = &peer {
        for event in peer.drain_inbox() {
            let PeerInbound::Object {
                fingerprint,
                object,
                ..
            } = event
            else {
                continue;
            };
            let Ok(signed) = serde_json::from_value::<SignedMessage>(object) else {
                continue;
            };
            let Some(contact) = contacts.iter().find(|c| {
                c.fingerprint == fingerprint && c.fingerprint == signed.message.sender_fingerprint
            }) else {
                continue;
            };
            if accepts_message(&signed, contact, &profile.profile.fingerprint) {
                let mut item = ChatItem::from_signed(
                    signed,
                    true,
                    contact.known_encryption_public_key.clone(),
                );
                item.pushed_via_iroh = true;
                merge_incoming(&mut result.incoming, &contact.fingerprint, item);
            }
        }
    }
    result.contacts = contacts;
    result
}

/// The dial target for a contact we already learned the device endpoint of.
///
/// Without a pinned endpoint id there is nothing to dial: the old gossip topic accepted
/// any node id derived from the profile key, which is exactly what ADR 0003 removes. The
/// addresses are the device's iroh addresses, never its TCP sync port.
fn peer_target_for(contact: &Contact) -> Option<PeerTarget> {
    Some(PeerTarget {
        fingerprint: contact.fingerprint.clone(),
        endpoint_id: contact.peer_endpoint_id.clone()?,
        addrs: contact
            .peer_addrs
            .iter()
            .filter_map(|addr| addr.parse().ok())
            .collect(),
    })
}

/// Verify both routing identities before an envelope can enter a visible thread.
pub fn accepts_message(message: &SignedMessage, contact: &Contact, local_fp: &str) -> bool {
    contact.verification == VerificationState::Verified
        && message.message.sender_fingerprint == contact.fingerprint
        && message.message.recipient_fingerprint == local_fp
        && contact
            .known_public_key
            .as_ref()
            .is_some_and(|key| message.verify(key).unwrap_or(false))
}
