//! Relay selection: configured, referred, community, and n0 fallback relays (M8).
//!
//! Iroh connects two devices directly when it can and falls back to a relay server when
//! hole punching fails. Which relay that is, is a policy decision that belongs to the
//! operator and to contacts that already trust each other, not to a constant in the binary,
//! so this module resolves it from four sources in a fixed order:
//!
//! | source | where it comes from | trust |
//! | --- | --- | --- |
//! | configured | `SNARTNET_RELAY_URLS` | the operator's own choice, always used |
//! | referral | a profile-key-signed, expiring recommendation from a verified contact (M8.3) | the referrer's profile key |
//! | community | `SNARTNET_COMMUNITY_RELAYS` | the operator opted into a shared list |
//! | n0 | iroh's own production relays | the library default and the last resort |
//!
//! Two properties are deliberate. First, a relay list is never a *necessity*: when every
//! source is empty the plan falls back to iroh's n0 production relays, and only an explicit
//! `SNARTNET_IROH_RELAY=off` disables relaying, so a wrong or expired referral can never
//! leave a device unreachable. Second, an authorization token is never part of a referral:
//! the referral names the relay (public, signed, expiring), and the token travels as an
//! encrypted grant that only the recipient's X25519 key can open (M8.3).
//!
//! Local observation closes the loop (M8.4): [`RelayHealth`] scores each relay from the
//! endpoint's own home-relay status, and [`RelayPlan::build`] orders by that score, so a
//! relay that keeps failing moves behind one that works and is eventually dropped, bounded
//! by [`MAX_ACTIVE_RELAYS`]. Applying a plan to a live endpoint is [`crate::peer`]'s job and
//! uses iroh's `insert_relay`/`remove_relay`, which only touch the relays this client added.
use iroh::{RelayConfig, RelayMap, RelayMode, RelayUrl};
use serde::{Deserialize, Serialize};
use snartnet_core::{verify_signature, KeyPair};
use std::{
    collections::BTreeMap,
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Environment variable listing operator-configured relay URLs (comma separated).
pub const ENV_RELAY_URLS: &str = "SNARTNET_RELAY_URLS";

/// Environment variable listing community relay URLs (comma separated).
///
/// A community list is deployment data, not protocol data: relay servers come and go, and
/// baking a third party's URL into the binary would imply an endorsement this project cannot
/// make. The list is supplied by the deployment that joins the community.
pub const ENV_COMMUNITY_RELAYS: &str = "SNARTNET_COMMUNITY_RELAYS";

/// Environment variable that switches relaying off entirely (`off`/`disabled`).
///
/// Shared with [`crate::peer::ENV_RELAY_MODE`]; a missing value means relays are on.
pub const ENV_RELAY_MODE: &str = "SNARTNET_IROH_RELAY";

/// Version of the referral record format.
pub const RELAY_REFERRAL_VERSION: u8 = 1;

/// How long a referral stays usable. A referral is a claim about *current* infrastructure,
/// so it expires and has to be re-issued rather than lasting forever.
pub const RELAY_REFERRAL_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Clock skew allowed on a referral's `issued_at`, so two machines cannot disagree about a
/// referral that was just issued.
pub const REFERRAL_CLOCK_SKEW_SECS: u64 = 300;

/// Most relays one plan may use. Iroh races its home relays, so a long list costs latency
/// without adding reachability.
pub const MAX_ACTIVE_RELAYS: usize = 4;

/// Most referrals kept in state. Newest expiry wins when the bound is reached.
pub const MAX_REFERRALS: usize = 8;

/// Longest accepted relay URL, so a referral cannot smuggle a large blob past the frame bound.
pub const MAX_RELAY_URL_BYTES: usize = 200;

/// Consecutive failed observations before a relay is demoted behind the next source (M8.4).
pub const DEMOTE_AFTER_FAILURES: u32 = 3;

/// Environment variable carrying the bearer token for the operator's own configured relay.
///
/// The token is never written into a referral: it is sealed into a grant for each recipient
/// (M8.3), which is also why it is read from the environment rather than from state.
pub const ENV_RELAY_TOKEN: &str = "SNARTNET_RELAY_TOKEN";

/// The bearer token for the configured relays, when the operator set one.
pub fn configured_relay_token() -> Option<String> {
    std::env::var(ENV_RELAY_TOKEN)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Current unix time in seconds.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Where a relay in a plan came from, in trust order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelaySource {
    /// A URL the operator configured. Always used when present.
    Configured,
    /// A verified, unexpired referral from a contact.
    Referral,
    /// A URL from the community list the operator opted into.
    Community,
    /// Iroh's own production relays, the fallback when nothing else is available.
    N0,
}

impl RelaySource {
    /// Short label for the snapshot and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            RelaySource::Configured => "configured",
            RelaySource::Referral => "referral",
            RelaySource::Community => "community",
            RelaySource::N0 => "n0",
        }
    }
}

/// Why a relay URL or a referral was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
    /// The referral carries a format version this build does not understand.
    Version(u8),
    /// The relay URL is not a usable `http(s)` relay URL.
    Url(String),
    /// The referral's `issued_at` is too far in the future.
    NotYetValid { issued_at: u64, now: u64 },
    /// The referral has expired.
    Expired { expires_at: u64, now: u64 },
    /// The signature does not match the referrer's key.
    Signature,
    /// The referral names no referrer.
    NoReferrer,
    /// The referral names no relay.
    NoRelay,
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::Version(version) => {
                write!(f, "unsupported relay referral version {version}")
            }
            RelayError::Url(reason) => write!(f, "unusable relay URL: {reason}"),
            RelayError::NotYetValid { issued_at, now } => write!(
                f,
                "relay referral issued at {issued_at} is ahead of the local clock ({now})"
            ),
            RelayError::Expired { expires_at, now } => {
                write!(f, "relay referral expired at {expires_at} (now {now})")
            }
            RelayError::Signature => write!(f, "relay referral signature is invalid"),
            RelayError::NoReferrer => write!(f, "relay referral names no referrer"),
            RelayError::NoRelay => write!(f, "relay referral names no relay"),
        }
    }
}

impl std::error::Error for RelayError {}

/// Validate a relay URL and return it in iroh's own type.
///
/// A relay is dialed over HTTPS (plain HTTP is allowed so a self-hosted relay on a LAN can be
/// tested), must name a host, and may not carry credentials: an embedded `user:password@`
/// would end up in a referral, in a snapshot, and in a log line.
pub fn parse_relay_url(url: &str) -> Result<RelayUrl, RelayError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(RelayError::Url("empty".into()));
    }
    if trimmed.len() > MAX_RELAY_URL_BYTES {
        return Err(RelayError::Url(format!(
            "longer than {MAX_RELAY_URL_BYTES} bytes"
        )));
    }
    let parsed = RelayUrl::from_str(trimmed).map_err(|e| RelayError::Url(e.to_string()))?;
    match parsed.scheme() {
        "https" | "http" => {}
        other => return Err(RelayError::Url(format!("unsupported scheme {other}"))),
    }
    if parsed.host_str().is_none() {
        return Err(RelayError::Url("no host".into()));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(RelayError::Url(
            "credentials belong in an encrypted grant, not in the URL".into(),
        ));
    }
    Ok(parsed)
}

/// Split a comma-separated relay list, dropping entries that are not usable URLs.
///
/// An unusable entry is dropped rather than made fatal: a typo in a community list must not
/// stop a node from binding an endpoint at all. The entries that survive are normalized, so
/// they match the keys health scores and plans use.
pub fn parse_relay_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| parse_relay_url(entry).ok())
        .map(|url| url.to_string())
        .collect()
}

/// The operator-configured relays.
pub fn configured_relays_from_env() -> Vec<String> {
    std::env::var(ENV_RELAY_URLS)
        .map(|value| parse_relay_list(&value))
        .unwrap_or_default()
}

/// The community relays the operator opted into.
pub fn community_relays_from_env() -> Vec<String> {
    std::env::var(ENV_COMMUNITY_RELAYS)
        .map(|value| parse_relay_list(&value))
        .unwrap_or_default()
}

/// Whether relaying is switched off entirely.
///
/// Anything but an explicit `off` keeps relays on: a device with no relay cannot be reached
/// behind a symmetric NAT, so "unset" must not mean "no relaying".
pub fn relays_disabled_from_env() -> bool {
    matches!(
        std::env::var(ENV_RELAY_MODE)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("off" | "disabled" | "none")
    )
}

/// Whether n0's *staging* (test) relays were asked for instead of the production ones.
///
/// Only an explicit `staging` selects them; that keeps a developer testing relay paths from
/// having to point the whole deployment at test infrastructure, and it keeps the default
/// (production) from being an accident of an unset variable.
pub fn staging_relays_from_env() -> bool {
    matches!(
        std::env::var(ENV_RELAY_MODE)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("staging" | "test" | "testing")
    )
}

// ---------------------------------------------------------------------------
// Referrals and grants (M8.3)
// ---------------------------------------------------------------------------

/// One relay's authorization token, encrypted to a single recipient (M8.3).
///
/// A private relay hands out bearer tokens. A referral is public data, so the token is not in
/// it: the referral carries this grant instead, which only the recipient's X25519 key can
/// open. Encryption is the same X25519 + ChaCha20-Poly1305 construction the chat uses, but
/// the *domain* is separate: a grant is addressed to a device the referrer already has a key
/// for, so a token cannot be replayed as a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayGrant {
    /// Encryption construction, so an old grant stays readable after an algorithm change.
    pub alg: String,
    /// base64 X25519 nonce.
    pub nonce_b64: String,
    /// base64 ChaCha20-Poly1305 ciphertext of the token.
    pub token_enc: String,
}

impl RelayGrant {
    /// Encrypt `token` so only the holder of `recipient_encryption_public_key` can read it.
    pub fn seal(
        keypair: &KeyPair,
        recipient_encryption_public_key: &str,
        token: &str,
    ) -> Result<Self, String> {
        if token.trim().is_empty() {
            return Err("refusing to seal an empty relay token".into());
        }
        let (token_enc, nonce_b64, alg) =
            keypair.encrypt_for_recipient(recipient_encryption_public_key, token)?;
        Ok(Self {
            alg,
            nonce_b64,
            token_enc,
        })
    }

    /// Decrypt the token with the recipient's own key and the referrer's encryption key.
    pub fn open(
        &self,
        keypair: &KeyPair,
        referrer_encryption_public_key: &str,
    ) -> Result<String, String> {
        keypair.decrypt_from_peer(
            referrer_encryption_public_key,
            &self.nonce_b64,
            &self.token_enc,
        )
    }
}

/// The bytes a referral's signature covers: everything except the signature itself.
#[derive(Debug, Serialize)]
struct ReferralBody<'a> {
    v: u8,
    referrer: &'a str,
    relay: &'a str,
    issued_at: u64,
    expires_at: u64,
    /// Hash of the grant bytes, so a swapped grant cannot keep a valid signature.
    grant_hash: String,
}

/// A referral that passed verification: safe to store and to plan with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedReferral {
    pub referrer: String,
    pub relay: String,
    pub expires_at: u64,
}

/// A contact's signed, expiring recommendation of a relay server (M8.3).
///
/// The referral is signed with the referrer's *profile* key, so it can be stored, mirrored,
/// and re-checked later without trusting the transport it arrived on. It expires because it
/// describes infrastructure that can move, and it carries a grant rather than a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayReferral {
    pub v: u8,
    /// Profile fingerprint of the contact that issued the referral.
    pub referrer: String,
    /// The relay URL a recipient may dial.
    pub relay: String,
    pub issued_at: u64,
    pub expires_at: u64,
    /// Authorization grant for a relay that needs one. Never the token itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<RelayGrant>,
    /// base64 profile-key signature over [`RelayReferral::signed_body`].
    pub signature: String,
}

impl RelayReferral {
    /// Sign a recommendation of `relay`, valid for `ttl_secs` from `now`.
    pub fn issue(
        keypair: &KeyPair,
        relay: &str,
        now: u64,
        ttl_secs: u64,
        grant: Option<RelayGrant>,
    ) -> Result<Self, String> {
        let relay = match parse_relay_url(relay) {
            Ok(url) => url.to_string(),
            Err(error) => return Err(error.to_string()),
        };
        let mut referral = Self {
            v: RELAY_REFERRAL_VERSION,
            referrer: keypair.fingerprint.clone(),
            relay,
            issued_at: now,
            expires_at: now.saturating_add(ttl_secs),
            grant,
            signature: String::new(),
        };
        let body = referral.signed_body()?;
        referral.signature = keypair.sign(&body)?;
        Ok(referral)
    }

    /// The canonical string the signature covers.
    fn signed_body(&self) -> Result<String, String> {
        let grant_hash = match &self.grant {
            Some(grant) => blake3::hash(&serde_json::to_vec(grant).map_err(|e| e.to_string())?)
                .to_hex()
                .to_string(),
            None => String::new(),
        };
        serde_json::to_string(&ReferralBody {
            v: self.v,
            referrer: &self.referrer,
            relay: &self.relay,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            grant_hash,
        })
        .map_err(|e| e.to_string())
    }

    /// Verify the record against the referrer's profile key and the clock.
    ///
    /// Every rule is checked here, in one place, because a referral is stored and later used
    /// to dial a server: the version, the referrer's own fingerprint, the URL, the validity
    /// window, and the signature. What cannot be checked here is *whether the referrer is a
    /// contact* — that is the session's job, and it is the caller contract of this function.
    pub fn verify(
        &self,
        referrer_public_key: &str,
        now: u64,
    ) -> Result<VerifiedReferral, RelayError> {
        if self.v != RELAY_REFERRAL_VERSION {
            return Err(RelayError::Version(self.v));
        }
        if self.referrer.trim().is_empty() {
            return Err(RelayError::NoReferrer);
        }
        if self.relay.trim().is_empty() {
            return Err(RelayError::NoRelay);
        }
        let relay = parse_relay_url(&self.relay)?;
        if self.issued_at > now.saturating_add(REFERRAL_CLOCK_SKEW_SECS) {
            return Err(RelayError::NotYetValid {
                issued_at: self.issued_at,
                now,
            });
        }
        if self.expires_at <= now {
            return Err(RelayError::Expired {
                expires_at: self.expires_at,
                now,
            });
        }
        let body = self.signed_body().map_err(RelayError::Url)?;
        let valid = verify_signature(&body, &self.signature, referrer_public_key)
            .map_err(|_| RelayError::Signature)?;
        if !valid {
            return Err(RelayError::Signature);
        }
        Ok(VerifiedReferral {
            referrer: self.referrer.clone(),
            relay: relay.to_string(),
            expires_at: self.expires_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Local health (M8.4)
// ---------------------------------------------------------------------------

/// What we have observed locally about one relay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayScore {
    /// Whether the endpoint currently has a home relay connection through this URL.
    pub connected: bool,
    /// Successful observations tracked since this process started.
    pub successes: u32,
    /// Consecutive failed observations. Reset by a success, because a relay that recovered
    /// should be able to win its place back.
    pub failures: u32,
    /// Most recent connection error, for the diagnostics panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl RelayScore {
    /// Negative when the relay is trouble, so higher is better.
    ///
    /// A connected relay outranks any disconnected one, then successes count for one and
    /// failures against two, so a relay with an occasional hiccup is not thrown away while a
    /// relay that keeps failing loses quickly.
    pub fn score(&self) -> i64 {
        if self.connected {
            return 100 + i64::from(self.successes.min(20));
        }
        i64::from(self.successes.min(20)) - 2 * i64::from(self.failures.min(20))
    }

    /// Whether this relay has failed often enough to move behind the next source.
    pub fn is_demoted(&self) -> bool {
        !self.connected && self.failures >= DEMOTE_AFTER_FAILURES
    }
}

/// Local relay observations, keyed by relay URL.
///
/// Deliberately in-memory: health is an observation about *this* device's path to a relay
/// right now, worth nothing after a restart and not something to persist or to share.
#[derive(Debug, Clone, Default)]
pub struct RelayHealth {
    scores: BTreeMap<String, RelayScore>,
}

impl RelayHealth {
    /// The key a score is stored under.
    ///
    /// Normalized, so a configured URL, a referred URL, and the endpoint's own report of the
    /// same relay all land on one score.
    fn key(relay: &str) -> String {
        parse_relay_url(relay)
            .map(|url| url.to_string())
            .unwrap_or_else(|_| relay.trim().to_owned())
    }

    pub fn score(&self, relay: &str) -> RelayScore {
        self.scores
            .get(&Self::key(relay))
            .cloned()
            .unwrap_or_default()
    }

    /// Record one observation of a relay and return the updated score.
    pub fn observe(&mut self, relay: &str, connected: bool, error: Option<String>) -> RelayScore {
        let score = self.scores.entry(Self::key(relay)).or_default();
        score.connected = connected;
        if connected {
            score.successes = score.successes.saturating_add(1);
            score.failures = 0;
            score.last_error = None;
        } else {
            score.failures = score.failures.saturating_add(1);
            score.last_error = error;
        }
        score.clone()
    }

    /// Order URLs best first, then alphabetically so the result is deterministic.
    pub fn order(&self, urls: &[String]) -> Vec<String> {
        let mut ordered = urls.to_vec();
        ordered.sort_by(|left, right| {
            self.score(right)
                .score()
                .cmp(&self.score(left).score())
                .then_with(|| left.cmp(right))
        });
        ordered
    }

    /// The relays worth keeping in the active map: the healthy ones first, up to `limit`.
    ///
    /// A relay is only dropped once it has been *demoted* and better options exist, so an
    /// outage of every relay still leaves the previous list active instead of an empty one.
    pub fn select(&self, urls: &[String], limit: usize) -> Vec<String> {
        let ordered = self.order(urls);
        let (healthy, demoted): (Vec<String>, Vec<String>) = ordered
            .into_iter()
            .partition(|url| !self.score(url).is_demoted());
        healthy
            .into_iter()
            .chain(demoted)
            .take(limit.max(1))
            .collect()
    }

    /// Observed relays, worst first is not how an operator reads it: best first, then URL.
    pub fn snapshot(&self) -> Vec<(String, RelayScore)> {
        let urls: Vec<String> = self.scores.keys().cloned().collect();
        self.order(&urls)
            .into_iter()
            .filter_map(|url| self.scores.get(&url).map(|score| (url, score.clone())))
            .collect()
    }

    /// Worst score among the given relays, or `None` when nothing is known about them.
    pub fn worst(&self, urls: &[String]) -> Option<RelayScore> {
        urls.iter()
            .map(|url| self.score(url))
            .min_by_key(|score| score.score())
    }
}

// ---------------------------------------------------------------------------
// The plan (M8.2)
// ---------------------------------------------------------------------------

/// One relay the plan will use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRelay {
    /// Which source the relay came from.
    pub source: RelaySource,
    /// The relay URL, already validated.
    pub url: String,
    /// Authorization token for a relay that needs one, decrypted from a grant (M8.3).
    pub token: Option<String>,
}

/// A verified referral plus the token its grant decrypted to (if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferredRelay {
    pub referrer: String,
    pub relay: String,
    pub expires_at: u64,
    pub token: Option<String>,
}

/// What a plan is built from.
///
/// Referrals arrive here as [`ReferredRelay`] values whose referral the session already
/// verified (signature, expiry, referrer) and whose grant it already opened, so the plan never
/// has to trust an unverified record.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelayInputs<'a> {
    /// Relays are switched off entirely (`SNARTNET_IROH_RELAY=off`).
    pub disabled: bool,
    /// n0's staging (test) relays were asked for instead of the production ones.
    pub staging: bool,
    /// Operator-configured relays.
    pub configured: &'a [String],
    /// Verified referrals from contacts.
    pub referrals: &'a [ReferredRelay],
    /// Community relays.
    pub community: &'a [String],
}

/// The relay set an endpoint binds with.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelayPlan {
    relays: Vec<PlannedRelay>,
    disabled: bool,
    /// Use n0's staging relays rather than the production ones when no explicit relay is
    /// planned (`SNARTNET_IROH_RELAY=staging`).
    staging: bool,
}

impl RelayPlan {
    /// Relays switched off: only direct paths remain.
    pub fn disabled() -> Self {
        Self {
            relays: Vec::new(),
            disabled: true,
            staging: false,
        }
    }

    /// The plan for the environment's configured and community sources, without referrals.
    pub fn from_env(health: &RelayHealth) -> Self {
        let configured = configured_relays_from_env();
        let community = community_relays_from_env();
        Self::build(
            RelayInputs {
                disabled: relays_disabled_from_env(),
                staging: staging_relays_from_env(),
                configured: &configured,
                referrals: &[],
                community: &community,
            },
            health,
        )
    }

    /// Resolve the sources into one plan, most trusted source first (M8.2).
    ///
    /// The sources are tried in order and the first one that yields a usable relay wins, so a
    /// configured relay is never diluted by a referral, a referral is preferred over a shared
    /// community list, and n0 is left for when nothing else exists. Within a source,
    /// [`RelayHealth::select`] orders by local score and bounds the list.
    pub fn build(inputs: RelayInputs<'_>, health: &RelayHealth) -> Self {
        if inputs.disabled {
            return Self::disabled();
        }
        let chosen = Self::from_source(RelaySource::Configured, inputs.configured, health)
            .or_else(|| Self::from_referrals(inputs.referrals, health))
            .or_else(|| Self::from_source(RelaySource::Community, inputs.community, health));
        Self {
            relays: chosen.unwrap_or_default(),
            disabled: false,
            staging: inputs.staging,
        }
    }

    /// Plan one plain URL source (configured or community).
    ///
    /// URLs are stored normalized (the way iroh's own `RelayUrl` prints them) so a configured
    /// URL and a referral for the same relay produce the same key: health scores and the plan
    /// are keyed by that string.
    fn from_source(
        source: RelaySource,
        urls: &[String],
        health: &RelayHealth,
    ) -> Option<Vec<PlannedRelay>> {
        let mut usable: Vec<String> = urls
            .iter()
            .filter_map(|url| parse_relay_url(url).ok())
            .map(|url| url.to_string())
            .collect();
        usable.sort();
        usable.dedup();
        if usable.is_empty() {
            return None;
        }
        Some(
            health
                .select(&usable, MAX_ACTIVE_RELAYS)
                .into_iter()
                .map(|url| PlannedRelay {
                    source,
                    url,
                    token: None,
                })
                .collect(),
        )
    }

    /// Plan the referred relays: newest expiry wins when the same URL is referred twice.
    ///
    /// A token stays with the referral that carried it. Every URL planned here already passed
    /// [`RelayReferral::verify`] and came from a contact, because a referral from anyone else
    /// is never accepted at all.
    fn from_referrals(
        referrals: &[ReferredRelay],
        health: &RelayHealth,
    ) -> Option<Vec<PlannedRelay>> {
        let mut by_url: BTreeMap<String, (u64, Option<String>)> = BTreeMap::new();
        for referral in referrals {
            // Normalized, so two referrals for the same relay collapse onto one entry and the
            // health score of that relay is found under one key.
            let Ok(relay) = parse_relay_url(&referral.relay) else {
                continue;
            };
            let url = relay.to_string();
            let entry = by_url.entry(url).or_insert((referral.expires_at, None));
            if referral.expires_at >= entry.0 {
                let token = referral.token.clone().or_else(|| entry.1.clone());
                *entry = (referral.expires_at, token);
            }
        }
        if by_url.is_empty() {
            return None;
        }
        let offers: Vec<String> = by_url.keys().cloned().collect();
        Some(
            health
                .select(&offers, MAX_ACTIVE_RELAYS)
                .into_iter()
                .map(|url| PlannedRelay {
                    source: RelaySource::Referral,
                    token: by_url.get(&url).and_then(|(_, token)| token.clone()),
                    url,
                })
                .collect(),
        )
    }

    /// The relays in this plan, best first.
    pub fn relays(&self) -> &[PlannedRelay] {
        &self.relays
    }

    /// Whether relaying is switched off entirely.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// The source that decided this plan, or `None` when relaying is off.
    ///
    /// An empty enabled plan reports [`RelaySource::N0`]: that is what iroh will use.
    pub fn source(&self) -> Option<RelaySource> {
        if self.disabled {
            return None;
        }
        Some(
            self.relays
                .first()
                .map(|relay| relay.source)
                .unwrap_or(RelaySource::N0),
        )
    }

    /// The URLs this plan adds to the endpoint's relay map. Empty means "iroh's own relays".
    pub fn active_urls(&self) -> Vec<String> {
        self.relays.iter().map(|relay| relay.url.clone()).collect()
    }

    /// The iroh relay mode this plan maps to, used when the endpoint is bound.
    ///
    /// A plan with no explicit relays becomes [`RelayMode::Default`] (n0 production) rather
    /// than an empty map: relaying is the fallback that keeps a symmetric-NAT device
    /// reachable, so an empty source list must not silently disable it. Only an explicit
    /// `SNARTNET_IROH_RELAY=staging` selects n0's test relays instead.
    pub fn relay_mode(&self) -> RelayMode {
        if self.disabled {
            return RelayMode::Disabled;
        }
        match self.relay_map() {
            Some(map) => RelayMode::Custom(map),
            None if self.staging => RelayMode::Staging,
            None => RelayMode::Default,
        }
    }

    /// This plan as an iroh relay map, or `None` when no explicit relay is usable.
    pub fn relay_map(&self) -> Option<RelayMap> {
        let map = RelayMap::empty();
        for relay in &self.relays {
            let Ok(url) = parse_relay_url(&relay.url) else {
                continue;
            };
            let mut config = RelayConfig::from(url.clone());
            if let Some(token) = &relay.token {
                config = config.with_auth_token(token.clone());
            }
            map.insert(url, Arc::new(config));
        }
        if map.is_empty() {
            None
        } else {
            Some(map)
        }
    }

    /// Whether every relay in this plan is currently demoted (M8.4).
    ///
    /// Used to decide whether iroh's own production relays should be added as a floor: a plan
    /// whose relays all keep failing must not be the only way to reach us.
    pub fn all_demoted(&self, health: &RelayHealth) -> bool {
        !self.relays.is_empty()
            && self
                .relays
                .iter()
                .all(|relay| health.score(&relay.url).is_demoted())
    }

    /// One line for the snapshot and the network panel.
    pub fn summary(&self) -> String {
        if self.disabled {
            return "disabled (direct paths only)".into();
        }
        if self.relays.is_empty() {
            return if self.staging {
                "n0 staging relays".into()
            } else {
                "n0 production relays".into()
            };
        }
        format!(
            "{} ({}): {}",
            self.source().map(RelaySource::label).unwrap_or("unknown"),
            self.relays.len(),
            self.active_urls().join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keypair with encryption keys, the way a profile always has them.
    fn keypair() -> KeyPair {
        let mut keypair = KeyPair::generate().unwrap();
        keypair.ensure_encryption_keys();
        keypair
    }

    /// The normalized form of each URL: what a plan stores and a health score is keyed by.
    fn urls(values: &[&str]) -> Vec<String> {
        values
            .iter()
            .map(|value| {
                parse_relay_url(value)
                    .expect("a usable test URL")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn relay_urls_are_validated_before_they_are_used_or_shared() {
        assert!(parse_relay_url("https://relay.example.com").is_ok());
        // A self-hosted relay on a LAN is often plain HTTP, which is why it is allowed.
        assert!(parse_relay_url("http://192.168.1.5:8080").is_ok());
        assert!(parse_relay_url("").is_err());
        assert!(parse_relay_url("   ").is_err());
        assert!(parse_relay_url("relay.example.com").is_err());
        assert!(parse_relay_url("ftp://relay.example.com").is_err());
        assert!(parse_relay_url("https://").is_err());
        // Credentials belong in a grant, never in a URL that gets stored and mirrored.
        assert!(parse_relay_url("https://user:pass@relay.example.com").is_err());
        assert!(parse_relay_url(&format!(
            "https://relay.example.com/{}",
            "a".repeat(MAX_RELAY_URL_BYTES)
        ))
        .is_err());

        // A list keeps what it can use and drops what it cannot, so one typo is not fatal.
        let list = parse_relay_list("https://one.example.com, ,nonsense,http://two.example.com");
        assert_eq!(
            list,
            urls(&["https://one.example.com", "http://two.example.com"])
        );
        assert!(parse_relay_list("  ").is_empty());
    }

    #[test]
    fn a_referral_is_refused_when_it_is_forged_expired_or_foreign() {
        let alice = keypair();
        let bob = keypair();
        let now = 1_000_000;
        let referral =
            RelayReferral::issue(&alice, "https://relay.example.com", now, 3_600, None).unwrap();
        assert_eq!(
            referral.verify(&alice.public_key, now + 60).unwrap().relay,
            "https://relay.example.com/"
        );
        // A referral any other key signed does not speak for Alice.
        assert_eq!(
            referral.verify(&bob.public_key, now).unwrap_err(),
            RelayError::Signature
        );
        assert_eq!(
            referral.verify(&alice.public_key, now + 3_600).unwrap_err(),
            RelayError::Expired {
                expires_at: now + 3_600,
                now: now + 3_600
            }
        );
        // A referral from the future is refused rather than trusted until it agrees with us.
        let ahead = RelayReferral::issue(
            &alice,
            "https://relay.example.com",
            now + REFERRAL_CLOCK_SKEW_SECS + 1,
            60,
            None,
        )
        .unwrap();
        assert!(matches!(
            ahead.verify(&alice.public_key, now).unwrap_err(),
            RelayError::NotYetValid { .. }
        ));
        // Every field the signature covers is load bearing.
        let mut tampered = referral.clone();
        tampered.relay = "https://attacker.example.com".into();
        assert_eq!(
            tampered.verify(&alice.public_key, now).unwrap_err(),
            RelayError::Signature
        );
        let mut swapped = referral.clone();
        swapped.grant = Some(RelayGrant {
            alg: "none".into(),
            nonce_b64: "nonce".into(),
            token_enc: "token".into(),
        });
        assert_eq!(
            swapped.verify(&alice.public_key, now).unwrap_err(),
            RelayError::Signature
        );
        let mut future_version = referral.clone();
        future_version.v = RELAY_REFERRAL_VERSION + 1;
        assert_eq!(
            future_version.verify(&alice.public_key, now).unwrap_err(),
            RelayError::Version(RELAY_REFERRAL_VERSION + 1)
        );
        // A structurally unusable record is refused before it reaches the plan.
        assert!(RelayReferral::issue(&alice, "not a url", now, 60, None).is_err());
        let mut nameless = referral.clone();
        nameless.relay = String::new();
        assert_eq!(
            nameless.verify(&alice.public_key, now).unwrap_err(),
            RelayError::NoRelay
        );
    }

    #[test]
    fn a_grant_only_opens_with_the_recipients_own_key() {
        let alice = keypair();
        let bob = keypair();
        let carol = keypair();
        let grant = RelayGrant::seal(
            &alice,
            bob.enc_public_key.as_deref().unwrap(),
            "relay-bearer-token",
        )
        .unwrap();
        // Bob decrypts with his own key and Alice's public encryption key: the grant is
        // addressed, not broadcast.
        assert_eq!(
            grant
                .open(&bob, alice.enc_public_key.as_deref().unwrap())
                .unwrap(),
            "relay-bearer-token"
        );
        assert!(grant
            .open(&carol, alice.enc_public_key.as_deref().unwrap())
            .is_err());
        // An empty token is a mistake, not a grant.
        assert!(RelayGrant::seal(&alice, bob.enc_public_key.as_deref().unwrap(), "  ").is_err());
    }

    #[test]
    fn a_referral_carries_its_grant_through_a_signed_record() {
        let alice = keypair();
        let bob = keypair();
        let grant =
            RelayGrant::seal(&alice, bob.enc_public_key.as_deref().unwrap(), "token-1").unwrap();
        let referral = RelayReferral::issue(
            &alice,
            "https://relay.example.com",
            now_secs(),
            RELAY_REFERRAL_TTL_SECS,
            Some(grant.clone()),
        )
        .unwrap();
        // A round trip through JSON is what a stored referral and a frame both do.
        let wire = serde_json::to_string(&referral).unwrap();
        let parsed: RelayReferral = serde_json::from_str(&wire).unwrap();
        assert_eq!(parsed, referral);
        parsed
            .verify(&alice.public_key, now_secs())
            .expect("the stored referral still verifies");
        assert_eq!(
            parsed
                .grant
                .unwrap()
                .open(&bob, alice.enc_public_key.as_deref().unwrap())
                .unwrap(),
            "token-1"
        );
    }

    #[test]
    fn a_repeated_referral_keeps_the_newest_expiry_and_its_token() {
        let health = RelayHealth::default();
        let referrals = [
            ReferredRelay {
                referrer: "alice".into(),
                relay: "https://relay.example.com".into(),
                expires_at: 1_000,
                token: Some("old-token".into()),
            },
            ReferredRelay {
                referrer: "bob".into(),
                relay: "https://relay.example.com".into(),
                expires_at: 2_000,
                token: None,
            },
        ];
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &[],
                referrals: &referrals,
                community: &[],
            },
            &health,
        );
        // One URL, the newer expiry, and the token from the referral that carried it.
        assert_eq!(plan.relays().len(), 1);
        assert_eq!(plan.relays()[0].token.as_deref(), Some("old-token"));
        assert_eq!(plan.relay_map().expect("a map").len(), 1);
        // A malformed referred URL is dropped instead of ending up in a map.
        let broken = [ReferredRelay {
            referrer: "alice".into(),
            relay: "nonsense".into(),
            expires_at: 2_000,
            token: None,
        }];
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &[],
                referrals: &broken,
                community: &[],
            },
            &health,
        );
        assert_eq!(plan.source(), Some(RelaySource::N0));
    }

    #[test]
    fn local_health_orders_demotes_and_recovers_relays() {
        let mut health = RelayHealth::default();
        let candidates = urls(&[
            "https://bad.example.com",
            "https://good.example.com",
            "https://slow.example.com",
        ]);
        // An unknown relay is neutral, so a fresh node does not reorder on guesses.
        assert_eq!(health.score("https://good.example.com").score(), 0);
        assert!(!health.score("https://good.example.com").is_demoted());
        for _ in 0..DEMOTE_AFTER_FAILURES {
            health.observe("https://bad.example.com", false, Some("timeout".into()));
        }
        health.observe("https://good.example.com", true, None);
        assert!(health.score("https://bad.example.com").is_demoted());
        assert!(health.score("https://bad.example.com").last_error.is_some());
        // The healthy relay is first and the demoted one is behind the untried relay.
        let ordered = health.order(&candidates);
        assert_eq!(ordered[0], urls(&["https://good.example.com"])[0]);
        assert_eq!(ordered[2], urls(&["https://bad.example.com"])[0]);
        // A demoted relay is kept while there is room and dropped only once better options
        // exist: an outage must not empty the map.
        assert_eq!(health.select(&candidates, 3).len(), 3);
        let limited = health.select(&candidates, 2);
        assert_eq!(limited.len(), 2);
        assert!(!limited.contains(&"https://bad.example.com".to_string()));
        // The floor is never zero: a plan of one bad relay keeps it rather than going empty.
        assert_eq!(
            health.select(&urls(&["https://bad.example.com"]), 1).len(),
            1
        );
        // A success clears the failure count, so a relay that recovered can win its place back.
        health.observe("https://bad.example.com", true, None);
        assert!(!health.score("https://bad.example.com").is_demoted());
        assert_eq!(health.score("https://bad.example.com").failures, 0);
        // Only observed relays are scored: ordering a candidate list adds no entries.
        assert_eq!(health.snapshot().len(), 2);
    }

    #[test]
    fn a_plan_reports_when_every_relay_it_holds_is_failing() {
        let mut health = RelayHealth::default();
        let referrals = [ReferredRelay {
            referrer: "alice".into(),
            relay: "https://relay.example.com".into(),
            expires_at: now_secs() + 600,
            token: None,
        }];
        let inputs = RelayInputs {
            disabled: false,
            staging: false,
            configured: &[],
            referrals: &referrals,
            community: &[],
        };
        let plan = RelayPlan::build(inputs, &health);
        assert!(!plan.all_demoted(&health));
        for _ in 0..DEMOTE_AFTER_FAILURES {
            health.observe("https://relay.example.com", false, Some("refused".into()));
        }
        let plan = RelayPlan::build(inputs, &health);
        // The failing relay stays in the plan (there is nothing better), and the caller is told
        // that everything it holds is failing so it can add the n0 floor.
        assert_eq!(plan.active_urls(), urls(&["https://relay.example.com"]));
        assert!(plan.all_demoted(&health));
        assert!(plan.summary().starts_with("referral (1)"));
    }

    #[test]
    fn a_configured_list_is_bounded_and_health_ordered() {
        let mut health = RelayHealth::default();
        for _ in 0..DEMOTE_AFTER_FAILURES {
            health.observe("https://slow.example.com", false, None);
        }
        health.observe("https://fast.example.com", true, None);
        let configured = urls(&[
            "https://slow.example.com",
            "https://fast.example.com",
            "https://three.example.com",
            "https://four.example.com",
            "https://five.example.com",
        ]);
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &configured,
                referrals: &[],
                community: &[],
            },
            &health,
        );
        assert_eq!(plan.relays().len(), MAX_ACTIVE_RELAYS);
        assert_eq!(plan.relays()[0].url, urls(&["https://fast.example.com"])[0]);
        // The demoted relay is the one that loses its place when the bound is reached.
        assert!(!plan
            .active_urls()
            .contains(&"https://slow.example.com".to_string()));
        assert_eq!(plan.source(), Some(RelaySource::Configured));
    }

    #[test]
    fn sources_are_used_in_trust_order() {
        let health = RelayHealth::default();
        let configured = urls(&["https://configured.example.com"]);
        let community = urls(&["https://community.example.com"]);
        let referrals = [ReferredRelay {
            referrer: "alice".into(),
            relay: "https://referred.example.com".into(),
            expires_at: now_secs() + 600,
            token: None,
        }];
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &configured,
                referrals: &referrals,
                community: &community,
            },
            &health,
        );
        // The operator's own relay wins, and the other sources are not mixed in.
        assert_eq!(plan.source(), Some(RelaySource::Configured));
        assert_eq!(plan.active_urls(), configured);
        // A referral is next, ahead of the shared community list.
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &[],
                referrals: &referrals,
                community: &community,
            },
            &health,
        );
        assert_eq!(plan.source(), Some(RelaySource::Referral));
        assert_eq!(plan.active_urls(), urls(&["https://referred.example.com"]));
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: false,
                staging: false,
                configured: &[],
                referrals: &[],
                community: &community,
            },
            &health,
        );
        assert_eq!(plan.source(), Some(RelaySource::Community));
        // With nothing left the plan is empty and reports n0, which is what iroh will use: an
        // empty source list must never silently disable relaying.
        let plan = RelayPlan::build(RelayInputs::default(), &health);
        assert_eq!(plan.source(), Some(RelaySource::N0));
        assert!(plan.relays().is_empty());
        assert_eq!(plan.relay_mode(), RelayMode::Default);
        assert_eq!(plan.summary(), "n0 production relays");
        // Only an explicit `off` leaves us with no relay at all.
        let plan = RelayPlan::build(
            RelayInputs {
                disabled: true,
                staging: false,
                configured: &configured,
                referrals: &referrals,
                community: &community,
            },
            &health,
        );
        assert!(plan.is_disabled());
        assert_eq!(plan.source(), None);
        assert_eq!(plan.relay_mode(), RelayMode::Disabled);
        assert!(plan.summary().contains("disabled"));

        // Production relays are the default, and only an explicit request selects staging.
        let plan = RelayPlan::build(
            RelayInputs {
                staging: true,
                ..RelayInputs::default()
            },
            &health,
        );
        assert_eq!(plan.relay_mode(), RelayMode::Staging);
        assert_eq!(plan.summary(), "n0 staging relays");
    }
}
