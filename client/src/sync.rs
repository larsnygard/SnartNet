//! Blocking transport work runs on a worker. The UI applies a delta, so messages and
//! contacts added while a sync is running cannot be replaced by an older snapshot.
use super::*;

#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub contacts: Vec<Contact>,
    pub incoming: Vec<(String, ChatItem)>,
    pub relayed_ids: HashSet<String>,
}

pub fn exchange(
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
pub fn accepts_message(message: &SignedMessage, contact: &Contact, local_fp: &str) -> bool {
    contact.verification == VerificationState::Verified
        && message.message.sender_fingerprint == contact.fingerprint
        && message.message.recipient_fingerprint == local_fp
        && contact
            .known_public_key
            .as_ref()
            .is_some_and(|key| message.verify(key).unwrap_or(false))
}
