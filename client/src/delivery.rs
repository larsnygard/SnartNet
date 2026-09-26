//! Durable-first outbound delivery (M7.1–M7.2).
//!
//! An outbound object must have a durable copy before it is *pushed* to a recipient.
//! Pushing first would let a message look delivered while the only copy was in flight on
//! a connection that can die, and the old code did exactly that: it published at command
//! time, silently ignored publish failures, and then treated a TCP or iroh hand-off as
//! delivery regardless.
//!
//! [`DurableStore`] is the seam that makes the rule explicit and testable:
//!
//! * [`PublishOutcome::Stored`] — a durable copy exists locally and a pointer to it is
//!   published (torrent object plus a signed DHT descriptor), so the recipient can fetch
//!   it later, without us, even from another device or replica.
//! * [`PublishOutcome::Unsupported`] — this host has no durable transport at all (an
//!   ephemeral test bind, or a frontend with the swarm switched off). Direct delivery is
//!   then the only path there is, and it is reported as such instead of pretending a
//!   durable copy exists.
//! * [`PublishOutcome::Failed`] — a durable transport exists but the publication failed.
//!   The object stays queued: it is neither pushed nor reported as delivered, and the next
//!   sync retries. This is the case that used to be silently downgraded to "relayed".
use crate::transport::TcpSwarmTransport;
use snartnet_core::{SignedMessage, SignedPost, SignedProfile};

/// What happened to an attempt to publish an outbound object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    /// A durable copy is published and addressable.
    Stored,
    /// No durable transport exists on this host, so there is nothing to publish to.
    Unsupported,
    /// A durable transport exists but refused the object.
    Failed(String),
}

impl PublishOutcome {
    /// Whether a recipient can retrieve this object without us being online.
    pub fn is_durable(&self) -> bool {
        matches!(self, PublishOutcome::Stored)
    }

    /// Whether direct delivery may proceed for an object with this outcome.
    ///
    /// `Failed` is the only refusal: the object must not be pushed before its durable copy
    /// exists, because a push cannot be retried by the recipient.
    pub fn allows_direct_delivery(&self) -> bool {
        !matches!(self, PublishOutcome::Failed(_))
    }

    /// Short label for the snapshot and the progress log.
    pub fn label(&self) -> &'static str {
        match self {
            PublishOutcome::Stored => "stored",
            PublishOutcome::Unsupported => "direct-only",
            PublishOutcome::Failed(_) => "failed",
        }
    }
}

/// The result of one publish attempt: what happened, plus where the copy landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreOutcome {
    pub published: PublishOutcome,
    /// Magnet URI of the published object, when the durable path produced one.
    pub locator: Option<String>,
}

impl StoreOutcome {
    pub fn stored(locator: Option<String>) -> Self {
        Self {
            published: PublishOutcome::Stored,
            locator,
        }
    }

    pub fn unsupported() -> Self {
        Self {
            published: PublishOutcome::Unsupported,
            locator: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            published: PublishOutcome::Failed(error.into()),
            locator: None,
        }
    }
}

/// Publishes a durable, addressable copy of an outbound object.
///
/// Implementations must be cheap to call from the session thread and must not panic; the
/// session decides what to do with the outcome. The session holds this behind a trait object
/// so a host whose store is broken can be tested against the real delivery rules.
pub trait DurableStore: Send + Sync + 'static {
    /// Publish a signed profile under its fingerprint namespace.
    fn store_profile(&self, profile: &SignedProfile) -> StoreOutcome;

    /// Publish the author's feed snapshot under their fingerprint.
    fn store_posts(&self, fingerprint: &str, posts: &[SignedPost]) -> StoreOutcome;

    /// Publish one signed message into the recipient's mailbox.
    fn store_message(&self, message: &SignedMessage) -> StoreOutcome;
}

/// The production store: the transport's own torrent session and DHT (M7.1).
///
/// Both nodes belong to the peer bind (`crate::ports`), so there is exactly one copy of
/// each in a process and no derived port can collide.
pub struct SwarmStore {
    transport: TcpSwarmTransport,
}

impl SwarmStore {
    pub fn new(transport: TcpSwarmTransport) -> Self {
        Self { transport }
    }
}

impl DurableStore for SwarmStore {
    fn store_profile(&self, profile: &SignedProfile) -> StoreOutcome {
        match self.transport.publish_profile(profile) {
            Ok(magnet) => StoreOutcome::stored(Some(magnet)),
            Err(error) if self.transport.has_durable_transport() => StoreOutcome::failed(error),
            Err(_) => StoreOutcome::unsupported(),
        }
    }

    fn store_posts(&self, fingerprint: &str, posts: &[SignedPost]) -> StoreOutcome {
        match self.transport.publish_posts(fingerprint, posts) {
            Ok(locator) => StoreOutcome::stored(locator),
            Err(error) if self.transport.has_durable_transport() => StoreOutcome::failed(error),
            Err(_) => StoreOutcome::unsupported(),
        }
    }

    fn store_message(&self, message: &SignedMessage) -> StoreOutcome {
        match self.transport.publish_message(message) {
            Ok(locator) => StoreOutcome::stored(locator),
            Err(error) if self.transport.has_durable_transport() => StoreOutcome::failed(error),
            Err(_) => StoreOutcome::unsupported(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_failed_publication_blocks_direct_delivery() {
        assert!(PublishOutcome::Stored.allows_direct_delivery());
        // A host without the swarm still delivers, and says so instead of claiming a
        // durable copy exists.
        assert!(PublishOutcome::Unsupported.allows_direct_delivery());
        assert!(!PublishOutcome::Failed("no dht".into()).allows_direct_delivery());
    }

    #[test]
    fn outcomes_carry_a_stable_label_and_durability() {
        assert!(PublishOutcome::Stored.is_durable());
        assert!(!PublishOutcome::Unsupported.is_durable());
        assert_eq!(PublishOutcome::Unsupported.label(), "direct-only");
        assert_eq!(StoreOutcome::failed("x").published.label(), "failed");
        assert!(StoreOutcome::stored(Some("magnet:?xt=urn:btih:abc".into()))
            .locator
            .is_some());
    }
}
