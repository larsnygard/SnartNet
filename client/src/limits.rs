//! Resource limits, per-round budgets, and backoff (M11.1).
//!
//! A sync round used to be unbounded work. Every queued message was published, every contact
//! was resolved over the DHT, and every spooled object was folded into state in one pass. With
//! a few hundred contacts, or a mailbox that was offline for a day, a single round could hold
//! the session lock for minutes and turn a frontend command into an apparent hang.
//!
//! Two ideas keep that bounded:
//!
//! * [`RoundBudget`] — what one round may spend, per [`Category`]. Work that does not fit stays
//!   queued and is picked up by the next round, so a budget is a delay and never a loss.
//! * [`Backoff`] — how long to wait after a round that failed. A broken store, a full disk, or a
//!   spool that refuses inbound is retried with growing patience instead of every ten seconds
//!   forever.
//!
//! The numbers live next to the queue they bound, so there is one place to change them:
//! [`crate::peer::MAX_PEER_INBOX`] for the endpoint inbox,
//! [`crate::repository::MAX_SPOOLED_INBOUND`] and [`crate::repository::MAX_SPOOLED_BYTES`] for
//! the durable spool, and [`crate::replica::MAX_HELD_LEASES`] for replica storage. [`Limits`]
//! gathers them into one value a host can narrow, and reports them, so a frontend shows the real
//! numbers instead of repeating them in a UI string.
use std::time::Duration;

use serde_json::{json, Value};

use crate::peer::MAX_PEER_INBOX;
use crate::repository::{MAX_SPOOLED_BYTES, MAX_SPOOLED_INBOUND};

/// Outbound messages published per round. Each publication is a torrent add plus a signed DHT
/// pointer, so this is the slowest part of a round.
pub const MAX_PUBLISHES_PER_ROUND: usize = 64;
/// Outbound messages handed to the direct push paths per round.
///
/// A push copies an already-durable object to a reachable contact, so it is far cheaper than a
/// publication. It is still bounded: a backlog pushed in one round would be one frame per
/// message per contact on a single connection.
pub const MAX_PUSHES_PER_ROUND: usize = 64;
/// Contacts whose profile and mailbox are resolved over the network per round.
pub const MAX_FETCHES_PER_ROUND: usize = 16;
/// Spooled inbound objects folded into canonical state per round.
pub const MAX_SPOOL_DRAIN_PER_ROUND: usize = 128;
/// Replica leases and storage receipts handed to the peer channel per round.
pub const MAX_STORAGE_RECORDS_PER_ROUND: usize = 16;

/// The retry delay after the first failed round.
pub const BACKOFF_BASE: Duration = Duration::from_secs(10);
/// The longest a failing daemon waits between rounds.
pub const BACKOFF_MAX: Duration = Duration::from_secs(600);

/// One bounded kind of work in a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Publishing a durable copy of an outbound message.
    Publish,
    /// Resolving one contact's profile and reading its mailbox.
    Fetch,
    /// Folding one spooled inbound object into canonical state.
    SpoolDrain,
    /// Handing one replica lease or storage receipt to the peer channel.
    StorageRecord,
}

impl Category {
    /// Every category, in the order a round works through them.
    pub const ALL: [Category; 4] = [
        Category::Publish,
        Category::Fetch,
        Category::SpoolDrain,
        Category::StorageRecord,
    ];

    /// The key this category uses in the snapshot's `limits` block.
    pub fn label(self) -> &'static str {
        match self {
            Category::Publish => "publishes",
            Category::Fetch => "fetches",
            Category::SpoolDrain => "spoolDrain",
            Category::StorageRecord => "storageRecords",
        }
    }
}

/// The caps one session works under: per-round work, plus the queues that outlive a round.
///
/// Defaults are the production numbers of each subsystem. A test narrows them to reach a bound
/// with a handful of objects, and a host with a smaller footprint may do the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Outbound messages published per round.
    pub publishes: usize,
    /// Outbound messages handed to the direct push paths per round.
    pub pushes: usize,
    /// Contacts resolved per round.
    pub fetches: usize,
    /// Spooled objects folded into state per round.
    pub spool_drain: usize,
    /// Storage records handed over per round.
    pub storage_records: usize,
    /// Most objects the durable spool holds before it refuses new ones.
    pub spool_entries: usize,
    /// Most bytes the durable spool holds before it refuses new ones.
    pub spool_bytes: u64,
    /// Most accepted objects held in the endpoint's in-memory inbox.
    pub inbox_entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self::production()
    }
}

impl Limits {
    /// The caps a released daemon runs with.
    pub const fn production() -> Self {
        Self {
            publishes: MAX_PUBLISHES_PER_ROUND,
            pushes: MAX_PUSHES_PER_ROUND,
            fetches: MAX_FETCHES_PER_ROUND,
            spool_drain: MAX_SPOOL_DRAIN_PER_ROUND,
            storage_records: MAX_STORAGE_RECORDS_PER_ROUND,
            spool_entries: MAX_SPOOLED_INBOUND,
            spool_bytes: MAX_SPOOLED_BYTES,
            inbox_entries: MAX_PEER_INBOX,
        }
    }

    /// Narrow the per-round caps.
    pub const fn with_round(
        mut self,
        publishes: usize,
        pushes: usize,
        fetches: usize,
        spool_drain: usize,
        storage_records: usize,
    ) -> Self {
        self.publishes = publishes;
        self.pushes = pushes;
        self.fetches = fetches;
        self.spool_drain = spool_drain;
        self.storage_records = storage_records;
        self
    }

    /// Narrow the durable inbound queue.
    pub const fn with_spool(mut self, entries: usize, bytes: u64) -> Self {
        self.spool_entries = entries;
        self.spool_bytes = bytes;
        self
    }

    /// Narrow the endpoint's in-memory inbox.
    pub const fn with_inbox(mut self, entries: usize) -> Self {
        self.inbox_entries = entries;
        self
    }

    /// The per-round caps as the snapshot reports them.
    pub fn round_caps(&self) -> Value {
        json!({
            Category::Publish.label(): self.publishes,
            "pushes": self.pushes,
            Category::Fetch.label(): self.fetches,
            Category::SpoolDrain.label(): self.spool_drain,
            Category::StorageRecord.label(): self.storage_records,
        })
    }

    /// The queue caps as the snapshot reports them.
    pub fn queue_caps(&self) -> Value {
        json!({
            "spooled": self.spool_entries,
            "spoolBytes": self.spool_bytes,
            "inbox": self.inbox_entries,
        })
    }
}

/// What one round has spent of its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundBudget {
    limits: Limits,
    publishes: usize,
    fetches: usize,
    spool_drain: usize,
    storage_records: usize,
}

impl RoundBudget {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            publishes: 0,
            fetches: 0,
            spool_drain: 0,
            storage_records: 0,
        }
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Whether one slot of `category` is still available, consuming it when it is.
    ///
    /// Callers check this *before* doing the work. A refusal is not an error: the work stays
    /// queued and the next round picks it up.
    pub fn take(&mut self, category: Category) -> bool {
        if self.remaining(category) == 0 {
            return false;
        }
        *self.counter(category) += 1;
        true
    }

    /// How many slots of `category` this round may still spend.
    pub fn remaining(&self, category: Category) -> usize {
        self.cap(category).saturating_sub(self.spent(category))
    }

    /// How much of `category` this round has spent.
    pub fn spent(&self, category: Category) -> usize {
        match category {
            Category::Publish => self.publishes,
            Category::Fetch => self.fetches,
            Category::SpoolDrain => self.spool_drain,
            Category::StorageRecord => self.storage_records,
        }
    }

    fn cap(&self, category: Category) -> usize {
        match category {
            Category::Publish => self.limits.publishes,
            Category::Fetch => self.limits.fetches,
            Category::SpoolDrain => self.limits.spool_drain,
            Category::StorageRecord => self.limits.storage_records,
        }
    }

    fn counter(&mut self, category: Category) -> &mut usize {
        match category {
            Category::Publish => &mut self.publishes,
            Category::Fetch => &mut self.fetches,
            Category::SpoolDrain => &mut self.spool_drain,
            Category::StorageRecord => &mut self.storage_records,
        }
    }

    /// The budget as the snapshot reports it: what each category allowed and what it spent.
    pub fn report(&self) -> Value {
        let mut caps = serde_json::Map::new();
        let mut spent = serde_json::Map::new();
        for category in Category::ALL {
            caps.insert(category.label().into(), json!(self.cap(category)));
            spent.insert(category.label().into(), json!(self.spent(category)));
        }
        json!({ "caps": caps, "spent": spent })
    }
}

/// Growing patience between rounds that fail.
///
/// Deliberately deterministic: no jitter, because one daemon retries at a time and a frontend
/// shows the resulting delay, so a value that changed run to run would be untestable without
/// buying anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backoff {
    base: Duration,
    max: Duration,
    failures: u32,
}

impl Backoff {
    /// A backoff that starts at `base` and never waits longer than `max`.
    pub fn new(base: Duration, max: Duration) -> Self {
        Self {
            base,
            max: max.max(base),
            failures: 0,
        }
    }

    pub fn base(&self) -> Duration {
        self.base
    }

    pub fn max(&self) -> Duration {
        self.max
    }

    /// How many rounds in a row have failed. Zero means the last round completed.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Whether the next round waits longer than the base interval.
    pub fn is_backing_off(&self) -> bool {
        self.failures > 0
    }

    /// How long to wait before the next attempt: `base * 2^failures`, capped at `max`.
    pub fn delay(&self) -> Duration {
        // The exponent is clamped before the shift: past the cap the value is clamped anyway,
        // and a saturating power is easier to reason about than a wrapping one.
        let factor = 2u32.saturating_pow(self.failures.min(20));
        self.base.saturating_mul(factor).min(self.max)
    }

    /// Record a failed round and return the new delay.
    pub fn record_failure(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        self.delay()
    }

    /// Record a completed round: the next attempt waits the base interval again.
    pub fn record_success(&mut self) -> Duration {
        self.failures = 0;
        self.delay()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_spends_each_category_only_up_to_its_cap() {
        let limits = Limits::production().with_round(2, 2, 1, 3, 0);
        let mut budget = RoundBudget::new(limits);
        assert!(budget.take(Category::Publish));
        assert!(budget.take(Category::Publish));
        assert!(!budget.take(Category::Publish));
        assert!(budget.take(Category::Fetch));
        assert!(!budget.take(Category::Fetch));
        for _ in 0..3 {
            assert!(budget.take(Category::SpoolDrain));
        }
        assert!(!budget.take(Category::SpoolDrain));
        // A category capped at zero is refused immediately, which is what makes an empty
        // budget a valid configuration instead of a special case.
        assert!(!budget.take(Category::StorageRecord));
    }

    #[test]
    fn remaining_and_spent_track_the_cap() {
        let limits = Limits::production().with_round(3, 3, 3, 3, 3);
        let mut budget = RoundBudget::new(limits);
        assert_eq!(budget.remaining(Category::Publish), 3);
        budget.take(Category::Publish);
        assert_eq!(budget.spent(Category::Publish), 1);
        assert_eq!(budget.remaining(Category::Publish), 2);
        assert_eq!(budget.spent(Category::Fetch), 0);
        assert_eq!(budget.limits(), limits);
    }

    #[test]
    fn the_report_names_every_category() {
        let mut budget = RoundBudget::new(Limits::production().with_round(4, 4, 0, 0, 0));
        budget.take(Category::Publish);
        let report = budget.report();
        for category in Category::ALL {
            assert!(
                report["caps"].get(category.label()).is_some(),
                "{} is missing from the caps",
                category.label()
            );
            assert!(report["spent"].get(category.label()).is_some());
        }
        assert_eq!(report["caps"]["publishes"], 4);
        assert_eq!(report["spent"]["publishes"], 1);
        assert_eq!(report["caps"]["fetches"], 0);
    }

    #[test]
    fn backoff_doubles_to_the_cap_and_resets() {
        let mut backoff = Backoff::new(Duration::from_secs(10), Duration::from_secs(60));
        assert_eq!(backoff.delay(), Duration::from_secs(10));
        assert!(!backoff.is_backing_off());
        assert_eq!(backoff.record_failure(), Duration::from_secs(20));
        assert_eq!(backoff.record_failure(), Duration::from_secs(40));
        assert!(backoff.is_backing_off());
        // Never longer than the cap, however many rounds keep failing.
        for _ in 0..20 {
            backoff.record_failure();
        }
        assert_eq!(backoff.delay(), Duration::from_secs(60));
        assert_eq!(backoff.failures(), 22);
        assert_eq!(backoff.record_success(), Duration::from_secs(10));
        assert_eq!(backoff.failures(), 0);
        assert!(!backoff.is_backing_off());
    }

    #[test]
    fn a_cap_below_the_base_still_waits_the_base() {
        // A misconfigured pair must not produce a busy loop: the base wins, and the cap is
        // raised to it rather than the delay dropping to zero.
        let backoff = Backoff::new(Duration::from_secs(30), Duration::from_secs(5));
        assert_eq!(backoff.max(), Duration::from_secs(30));
        assert_eq!(backoff.delay(), Duration::from_secs(30));
    }

    #[test]
    fn production_limits_are_the_subsystem_defaults() {
        let limits = Limits::production();
        assert_eq!(limits.spool_entries, MAX_SPOOLED_INBOUND);
        assert_eq!(limits.spool_bytes, MAX_SPOOLED_BYTES);
        assert_eq!(limits.inbox_entries, MAX_PEER_INBOX);
        assert_eq!(limits.publishes, MAX_PUBLISHES_PER_ROUND);
        assert_eq!(limits.pushes, MAX_PUSHES_PER_ROUND);
        assert_eq!(limits.fetches, MAX_FETCHES_PER_ROUND);
        assert_eq!(limits.spool_drain, MAX_SPOOL_DRAIN_PER_ROUND);
        assert_eq!(limits.storage_records, MAX_STORAGE_RECORDS_PER_ROUND);
        assert_eq!(BACKOFF_BASE, Duration::from_secs(10));
        assert!(BACKOFF_MAX >= BACKOFF_BASE);
    }

    #[test]
    fn caps_reported_to_a_frontend_are_json_objects() {
        let limits = Limits::production();
        assert_eq!(limits.queue_caps()["spoolBytes"], MAX_SPOOLED_BYTES);
        assert_eq!(limits.queue_caps()["inbox"], MAX_PEER_INBOX);
        assert_eq!(limits.round_caps()["fetches"], MAX_FETCHES_PER_ROUND);
        assert_eq!(limits.round_caps()["pushes"], MAX_PUSHES_PER_ROUND);
    }
}
