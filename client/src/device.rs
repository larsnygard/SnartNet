//! Per-device Iroh identity and the profile-signed certificate that binds it.
//!
//! ADR 0003 makes the device, not the profile, the unit that holds an Iroh key: the
//! profile signing secret is never reused as an endpoint key. This module owns both
//! halves of that decision.
//!
//! * [`DeviceKey`] is a per-device Ed25519 secret that exists only inside the daemon.
//!   It is persisted through the canonical identity store (never in a snapshot) and is
//!   the only key ever handed to [`iroh::Endpoint`].
//! * [`DeviceCertificate`] is issued by the profile key and proves that an endpoint id
//!   belongs to a profile, which capabilities it may use, and until when. Peers validate
//!   it before any application frame is accepted.
//!
//! Certificate validation is deliberately independent of the live connection except for
//! one input: the endpoint id that iroh's TLS handshake already proved the remote party
//! holds. Everything else is a pure function of the certificate bytes, so replay,
//! expiry, and mismatch cases are unit-testable without a network.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use iroh::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use snartnet_core::{fingerprint_for_public_key, verify_signature, KeyPair, SignedProfile};
use std::fmt;

/// Certificate wire version understood by this build.
pub const DEVICE_CERT_VERSION: u16 = 1;

/// Default certificate lifetime. A device refreshes its own certificate on every start,
/// so a month is a comfortable margin that still bounds a stolen profile-key window.
pub const DEVICE_CERT_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Clock skew tolerated between an issuing and a validating device.
pub const DEVICE_CERT_SKEW_SECS: u64 = 300;

/// Seconds of remaining lifetime below which a device certificate is renewed on startup.
///
/// Renewing early keeps a long-lived installation from presenting a nearly-expired
/// certificate, while still reusing one certificate for most of its lifetime so that
/// contacts' pins stay stable.
pub const DEVICE_CERT_RENEW_SECS: u64 = 7 * 24 * 60 * 60;

/// Capability allowing a device to open the peer protocol and exchange updates.
pub const CAPABILITY_PEER: &str = "peer";

/// Capability allowing a device to send and receive direct messages.
pub const CAPABILITY_CHAT: &str = "chat";

/// Capabilities a device issued by this build requests by default.
pub const DEFAULT_CAPABILITIES: [&str; 2] = [CAPABILITY_PEER, CAPABILITY_CHAT];

/// A per-device Iroh secret key. Cheap to clone, never logged, never sent to a frontend.
#[derive(Clone)]
pub struct DeviceKey {
    key: SecretKey,
}

impl DeviceKey {
    /// Create a fresh random device identity.
    pub fn generate() -> Self {
        Self {
            key: SecretKey::generate(),
        }
    }

    /// Rebuild a device identity from its base64 secret, as stored by the daemon.
    pub fn from_secret_base64(secret: &str) -> Result<Self, String> {
        let decoded = STANDARD
            .decode(secret.trim())
            .map_err(|e| format!("invalid device secret encoding: {e}"))?;
        let bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|_| "invalid device secret length".to_string())?;
        Ok(Self {
            key: SecretKey::from_bytes(&bytes),
        })
    }

    /// The base64 secret to persist. Callers keep it inside the daemon's identity store.
    pub fn secret_base64(&self) -> String {
        STANDARD.encode(self.key.to_bytes())
    }

    /// The underlying Iroh secret, for the one caller allowed to bind an endpoint.
    pub fn secret_key(&self) -> &SecretKey {
        &self.key
    }

    /// This device's endpoint id, which is also its Ed25519 public key.
    pub fn endpoint_id(&self) -> PublicKey {
        self.key.public()
    }

    /// [`DeviceKey::endpoint_id`] as the hex string used by certificates and snapshots.
    pub fn endpoint_id_string(&self) -> String {
        self.endpoint_id().to_string()
    }
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceKey")
            .field("endpoint_id", &self.endpoint_id_string())
            .finish_non_exhaustive()
    }
}

/// A profile-signed statement that one Iroh endpoint belongs to one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCertificate {
    /// Certificate format version.
    pub v: u16,
    /// Fingerprint of the profile that issued the certificate.
    pub profile: String,
    /// Base64 Ed25519 public key of that profile, re-derived and checked against `profile`.
    pub profile_key: String,
    /// Hex endpoint id of the device. Always the device's Ed25519 public key.
    pub endpoint_id: String,
    /// Capabilities the profile grants this device.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Unix-epoch seconds at which the certificate was issued.
    pub issued_at: u64,
    /// Unix-epoch seconds after which the certificate must be refused.
    pub expires_at: u64,
    /// Base64 Ed25519 signature by `profile_key` over [`DeviceCertificate::canonical_json`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Why a certificate was refused. Each variant maps to one rejection rule, so tests can
/// assert the exact reason instead of "some error".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateError {
    /// The certificate declares a wire version this build cannot evaluate.
    UnsupportedVersion(u16),
    /// A structural problem: bad hex, bad base64, wrong key length, empty field.
    Malformed(String),
    /// The certificate names a different endpoint than the one that presented it.
    EndpointMismatch { expected: String, actual: String },
    /// `profile_key` does not hash to the fingerprint stored in `profile`.
    ProfileMismatch { claimed: String, derived: String },
    /// The signature is absent or does not verify under `profile_key`.
    SignatureInvalid,
    /// The certificate has expired relative to the validating clock.
    Expired { expires_at: u64, now: u64 },
    /// The certificate is dated too far in the future to trust.
    NotYetValid { issued_at: u64, now: u64 },
    /// `expires_at` does not follow `issued_at`.
    InvalidLifetime { issued_at: u64, expires_at: u64 },
    /// The certificate is older than one already accepted from this contact.
    Superseded {
        presented: u64,
        accepted: u64,
        endpoint_id: String,
    },
    /// The profile may not use the tested capability.
    MissingCapability(String),
}

impl fmt::Display for CertificateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported device certificate version {version}")
            }
            Self::Malformed(reason) => write!(f, "malformed device certificate: {reason}"),
            Self::EndpointMismatch { expected, actual } => write!(
                f,
                "certificate names endpoint {expected} but the connection is {actual}"
            ),
            Self::ProfileMismatch { claimed, derived } => write!(
                f,
                "certificate claims profile {claimed} but its key hashes to {derived}"
            ),
            Self::SignatureInvalid => write!(f, "certificate signature is invalid"),
            Self::Expired { expires_at, now } => {
                write!(f, "certificate expired at {expires_at} (now {now})")
            }
            Self::NotYetValid { issued_at, now } => {
                write!(f, "certificate issued at {issued_at} is in the future (now {now})")
            }
            Self::InvalidLifetime {
                issued_at,
                expires_at,
            } => write!(
                f,
                "certificate lifetime is invalid ({issued_at} to {expires_at})"
            ),
            Self::Superseded {
                presented,
                accepted,
                endpoint_id,
            } => write!(
                f,
                "certificate issued at {presented} is older than the accepted {accepted} for endpoint {endpoint_id}"
            ),
            Self::MissingCapability(capability) => {
                write!(f, "certificate does not grant the {capability} capability")
            }
        }
    }
}

impl std::error::Error for CertificateError {}

impl DeviceCertificate {
    /// Issue a certificate for `device` on behalf of `profile`.
    ///
    /// The profile keypair must actually own the profile: a certificate is only useful if
    /// it binds the endpoint id to the fingerprint its signature will be checked against.
    pub fn issue(
        profile: &SignedProfile,
        profile_keypair: &KeyPair,
        device: &DeviceKey,
        capabilities: &[&str],
        issued_at: u64,
        ttl_secs: u64,
    ) -> Result<Self, String> {
        if !profile.verify().unwrap_or(false) {
            return Err("refusing to certify a profile with an invalid signature".into());
        }
        if profile_keypair.public_key != profile.profile.public_key
            || profile_keypair.fingerprint != profile.profile.fingerprint
        {
            return Err("refusing to certify a profile with a different keypair".into());
        }
        let mut certificate = Self {
            v: DEVICE_CERT_VERSION,
            profile: profile.profile.fingerprint.clone(),
            profile_key: profile.profile.public_key.clone(),
            endpoint_id: device.endpoint_id_string(),
            capabilities: capabilities
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            issued_at,
            expires_at: issued_at.saturating_add(ttl_secs),
            signature: None,
        };
        certificate.sign(profile_keypair)?;
        Ok(certificate)
    }

    /// The exact bytes the profile key signs: this certificate without its signature.
    pub fn canonical_json(&self) -> Result<String, String> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        serde_json::to_string(&unsigned)
            .map_err(|e| format!("device certificate serialization failed: {e}"))
    }

    /// Sign the certificate in place with the profile key.
    pub fn sign(&mut self, profile_keypair: &KeyPair) -> Result<(), String> {
        let signature = profile_keypair.sign(&self.canonical_json()?)?;
        self.signature = Some(signature);
        Ok(())
    }

    /// The endpoint id this certificate binds, as a dialable [`PublicKey`].
    pub fn endpoint_public_key(&self) -> Result<PublicKey, CertificateError> {
        self.endpoint_id.parse::<PublicKey>().map_err(|e| {
            CertificateError::Malformed(format!("endpoint id is not a public key: {e}"))
        })
    }

    /// Whether the certificate grants `capability`.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|value| value == capability)
    }

    /// Validate the certificate against the endpoint that presented it and a clock.
    ///
    /// Order matters for the error a caller sees: structure, then binding to the
    /// connection, then the profile binding, then the signature, then the validity window.
    /// The signature is checked before the window so a forged certificate can never be
    /// reported as merely "expired".
    pub fn verify_at(&self, endpoint: &PublicKey, now: u64) -> Result<(), CertificateError> {
        if self.v != DEVICE_CERT_VERSION {
            return Err(CertificateError::UnsupportedVersion(self.v));
        }
        if self.profile.trim().is_empty() || self.profile_key.trim().is_empty() {
            return Err(CertificateError::Malformed(
                "profile identity fields are empty".into(),
            ));
        }
        let declared = self.endpoint_public_key()?;
        if declared != *endpoint {
            return Err(CertificateError::EndpointMismatch {
                expected: declared.to_string(),
                actual: endpoint.to_string(),
            });
        }
        let derived =
            fingerprint_for_public_key(&self.profile_key).map_err(CertificateError::Malformed)?;
        if derived != self.profile {
            return Err(CertificateError::ProfileMismatch {
                claimed: self.profile.clone(),
                derived,
            });
        }
        let Some(signature) = self.signature.as_deref() else {
            return Err(CertificateError::SignatureInvalid);
        };
        let canonical = self.canonical_json().map_err(CertificateError::Malformed)?;
        if !verify_signature(&canonical, signature, &self.profile_key)
            .map_err(CertificateError::Malformed)?
        {
            return Err(CertificateError::SignatureInvalid);
        }
        if self.expires_at <= self.issued_at {
            return Err(CertificateError::InvalidLifetime {
                issued_at: self.issued_at,
                expires_at: self.expires_at,
            });
        }
        if self.issued_at > now.saturating_add(DEVICE_CERT_SKEW_SECS) {
            return Err(CertificateError::NotYetValid {
                issued_at: self.issued_at,
                now,
            });
        }
        if self.expires_at <= now {
            return Err(CertificateError::Expired {
                expires_at: self.expires_at,
                now,
            });
        }
        Ok(())
    }

    /// Validate the certificate, its endpoint binding, and the profile it claims to belong to.
    pub fn verify_for_profile(
        &self,
        profile_fingerprint: &str,
        endpoint: &PublicKey,
        now: u64,
    ) -> Result<(), CertificateError> {
        if self.profile != profile_fingerprint {
            return Err(CertificateError::ProfileMismatch {
                claimed: self.profile.clone(),
                derived: profile_fingerprint.to_string(),
            });
        }
        self.verify_at(endpoint, now)
    }

    /// Validate and require a capability in one step.
    pub fn verify_with_capability(
        &self,
        profile_fingerprint: &str,
        endpoint: &PublicKey,
        capability: &str,
        now: u64,
    ) -> Result<(), CertificateError> {
        self.verify_for_profile(profile_fingerprint, endpoint, now)?;
        if !self.has_capability(capability) {
            return Err(CertificateError::MissingCapability(capability.to_string()));
        }
        Ok(())
    }
}

/// The newest certificate accepted from one contact, so a replayed older one is refused.
///
/// A contact may legitimately present the same certificate again after a reconnect, or a
/// newer one after rotating devices. Any certificate older than the accepted one is a
/// replay of superseded authority and must be refused even though its signature verifies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedCertificate {
    pub profile: String,
    pub endpoint_id: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl PinnedCertificate {
    /// Pin a certificate that has already been validated.
    pub fn from_certificate(certificate: &DeviceCertificate) -> Result<Self, CertificateError> {
        Ok(Self {
            profile: certificate.profile.clone(),
            endpoint_id: certificate.endpoint_public_key()?.to_string(),
            issued_at: certificate.issued_at,
            expires_at: certificate.expires_at,
        })
    }

    /// Whether `certificate` may replace this pin.
    ///
    /// Accepts a renewal of the same device (`issued_at` moved forward) or the very same
    /// certificate again. Refuses anything older, which is the replay case.
    pub fn accepts(&self, certificate: &DeviceCertificate) -> bool {
        match certificate.issued_at.cmp(&self.issued_at) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Equal => certificate.endpoint_id == self.endpoint_id,
            std::cmp::Ordering::Less => false,
        }
    }

    /// [`PinnedCertificate::accepts`] as a typed result.
    pub fn check(&self, certificate: &DeviceCertificate) -> Result<(), CertificateError> {
        if self.accepts(certificate) {
            Ok(())
        } else {
            Err(CertificateError::Superseded {
                presented: certificate.issued_at,
                accepted: self.issued_at,
                endpoint_id: self.endpoint_id.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snartnet_core::{KeyInfo, Profile, SignedProfile};

    fn profile() -> (SignedProfile, KeyPair) {
        let keypair = KeyPair::generate().unwrap();
        let info: KeyInfo = keypair.get_public_info();
        let signed = SignedProfile::create(Profile::new("alice".into(), info), &keypair).unwrap();
        (signed, keypair)
    }

    fn certificate() -> (SignedProfile, KeyPair, DeviceKey, DeviceCertificate) {
        let (signed, keypair) = profile();
        let device = DeviceKey::generate();
        let certificate = DeviceCertificate::issue(
            &signed,
            &keypair,
            &device,
            &DEFAULT_CAPABILITIES,
            1_000,
            DEVICE_CERT_TTL_SECS,
        )
        .expect("certificate");
        (signed, keypair, device, certificate)
    }

    #[test]
    fn a_device_key_round_trips_and_keeps_its_endpoint_id() {
        let device = DeviceKey::generate();
        let restored = DeviceKey::from_secret_base64(&device.secret_base64()).unwrap();
        assert_eq!(device.endpoint_id(), restored.endpoint_id());
        assert_eq!(device.endpoint_id_string(), restored.endpoint_id_string());
    }

    #[test]
    fn a_device_key_rejects_a_malformed_secret() {
        assert!(DeviceKey::from_secret_base64("not base64!!").is_err());
        assert!(DeviceKey::from_secret_base64("").is_err());
        let short = STANDARD.encode([0u8; 16]);
        assert!(DeviceKey::from_secret_base64(&short).is_err());
    }

    #[test]
    fn a_device_key_debug_never_prints_the_secret() {
        let device = DeviceKey::generate();
        let rendered = format!("{device:?}");
        assert!(!rendered.contains(&device.secret_base64()));
        assert!(rendered.contains(&device.endpoint_id_string()));
    }

    #[test]
    fn the_endpoint_id_is_not_the_profile_key() {
        let (signed, _, device, certificate) = certificate();
        assert_ne!(certificate.endpoint_id, signed.profile.public_key);
        assert_eq!(certificate.endpoint_id, device.endpoint_id_string());
    }

    #[test]
    fn a_certificate_round_trips_through_json() {
        let (_, _, _, certificate) = certificate();
        let json = serde_json::to_string(&certificate).unwrap();
        let parsed: DeviceCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, certificate);
    }

    #[test]
    fn a_certificate_fits_a_dht_descriptor() {
        // M6.5 publishes the certificate inside a BEP-44 record, which caps the value at
        // 1000 bytes. Keep growth visible here instead of at publish time.
        let (_, _, _, certificate) = certificate();
        let json = serde_json::to_string(&certificate).unwrap();
        assert!(json.len() < 700, "certificate grew to {} bytes", json.len());
    }

    #[test]
    fn a_certificate_verifies_against_its_own_endpoint() {
        let (signed, _, device, certificate) = certificate();
        certificate
            .verify_for_profile(&signed.profile.fingerprint, &device.endpoint_id(), 1_060)
            .expect("valid certificate");
    }

    #[test]
    fn a_certificate_is_refused_for_another_endpoint() {
        let (signed, _, _, certificate) = certificate();
        let other = DeviceKey::generate();
        let error = certificate
            .verify_for_profile(&signed.profile.fingerprint, &other.endpoint_id(), 1_060)
            .unwrap_err();
        assert!(matches!(error, CertificateError::EndpointMismatch { .. }));
    }

    #[test]
    fn a_certificate_is_refused_for_another_profile() {
        let (_, _, device, certificate) = certificate();
        let (other, _) = profile();
        let error = certificate
            .verify_for_profile(&other.profile.fingerprint, &device.endpoint_id(), 1_060)
            .unwrap_err();
        assert!(matches!(error, CertificateError::ProfileMismatch { .. }));
    }

    #[test]
    fn a_certificate_whose_key_does_not_match_its_fingerprint_is_refused() {
        let (_, keypair, device, certificate) = certificate();
        let mut broken = DeviceCertificate {
            profile: "another-fingerprint".into(),
            signature: None,
            ..certificate
        };
        broken.sign(&keypair).unwrap();
        let error = broken.verify_at(&device.endpoint_id(), 1_060).unwrap_err();
        assert!(matches!(error, CertificateError::ProfileMismatch { .. }));
    }

    #[test]
    fn tampering_with_a_certificate_invalidates_the_signature() {
        let (_, _, device, mut certificate) = certificate();
        certificate.expires_at += 86_400;
        let error = certificate
            .verify_at(&device.endpoint_id(), 1_060)
            .unwrap_err();
        assert_eq!(error, CertificateError::SignatureInvalid);
    }

    #[test]
    fn a_certificate_signed_by_another_profile_is_refused() {
        let (_, _, device, certificate) = certificate();
        let (_, attacker) = profile();
        let mut forged = DeviceCertificate {
            signature: None,
            ..certificate
        };
        forged.sign(&attacker).unwrap();
        let error = forged.verify_at(&device.endpoint_id(), 1_060).unwrap_err();
        assert_eq!(error, CertificateError::SignatureInvalid);
    }

    #[test]
    fn a_certificate_without_a_signature_is_refused() {
        let (_, _, device, certificate) = certificate();
        let unsigned = DeviceCertificate {
            signature: None,
            ..certificate
        };
        assert_eq!(
            unsigned
                .verify_at(&device.endpoint_id(), 1_060)
                .unwrap_err(),
            CertificateError::SignatureInvalid
        );
    }

    #[test]
    fn an_expired_certificate_is_refused() {
        let (_, _, device, certificate) = certificate();
        let just_after_expiry = certificate.expires_at;
        let error = certificate
            .verify_at(&device.endpoint_id(), just_after_expiry)
            .unwrap_err();
        assert!(matches!(error, CertificateError::Expired { .. }));
    }

    #[test]
    fn a_certificate_issued_in_the_future_is_refused() {
        let (_, _, device, certificate) = certificate();
        let before_issue = certificate.issued_at - DEVICE_CERT_SKEW_SECS - 1;
        let error = certificate
            .verify_at(&device.endpoint_id(), before_issue)
            .unwrap_err();
        assert!(matches!(error, CertificateError::NotYetValid { .. }));
    }

    #[test]
    fn clock_skew_inside_the_tolerance_still_verifies() {
        let (_, _, device, certificate) = certificate();
        certificate
            .verify_at(
                &device.endpoint_id(),
                certificate.issued_at - DEVICE_CERT_SKEW_SECS,
            )
            .expect("skew within tolerance");
    }

    #[test]
    fn a_lifetime_that_does_not_advance_is_refused() {
        let (_, keypair, device, certificate) = certificate();
        let mut broken = DeviceCertificate {
            expires_at: certificate.issued_at,
            signature: None,
            ..certificate
        };
        broken.sign(&keypair).unwrap();
        let error = broken.verify_at(&device.endpoint_id(), 1_060).unwrap_err();
        assert!(matches!(error, CertificateError::InvalidLifetime { .. }));
    }

    #[test]
    fn an_unsupported_version_is_refused() {
        let (_, keypair, device, certificate) = certificate();
        let mut future = DeviceCertificate {
            v: DEVICE_CERT_VERSION + 1,
            signature: None,
            ..certificate
        };
        future.sign(&keypair).unwrap();
        assert_eq!(
            future.verify_at(&device.endpoint_id(), 1_060).unwrap_err(),
            CertificateError::UnsupportedVersion(DEVICE_CERT_VERSION + 1)
        );
    }

    #[test]
    fn a_certificate_with_an_unparsable_endpoint_id_is_refused() {
        let certificate = DeviceCertificate {
            v: DEVICE_CERT_VERSION,
            profile: "profile".into(),
            profile_key: "key".into(),
            endpoint_id: "not-an-endpoint".into(),
            capabilities: vec![CAPABILITY_PEER.into()],
            issued_at: 1_000,
            expires_at: 2_000,
            signature: Some("signature".into()),
        };
        let device = DeviceKey::generate();
        assert!(matches!(
            certificate.verify_at(&device.endpoint_id(), 1_060),
            Err(CertificateError::Malformed(_))
        ));
    }

    #[test]
    fn capabilities_are_granted_explicitly() {
        let (signed, keypair) = profile();
        let device = DeviceKey::generate();
        let peer_only = DeviceCertificate::issue(
            &signed,
            &keypair,
            &device,
            &[CAPABILITY_PEER],
            1_000,
            DEVICE_CERT_TTL_SECS,
        )
        .unwrap();
        assert!(peer_only.has_capability(CAPABILITY_PEER));
        assert!(!peer_only.has_capability(CAPABILITY_CHAT));
        assert_eq!(
            peer_only
                .verify_with_capability(
                    &signed.profile.fingerprint,
                    &device.endpoint_id(),
                    CAPABILITY_CHAT,
                    1_060
                )
                .unwrap_err(),
            CertificateError::MissingCapability(CAPABILITY_CHAT.into())
        );
        peer_only
            .verify_with_capability(
                &signed.profile.fingerprint,
                &device.endpoint_id(),
                CAPABILITY_PEER,
                1_060,
            )
            .expect("peer capability");
    }

    #[test]
    fn a_certificate_cannot_be_issued_for_someone_elses_profile() {
        let (signed, _) = profile();
        let attacker = KeyPair::generate().unwrap();
        let device = DeviceKey::generate();
        let error = DeviceCertificate::issue(
            &signed,
            &attacker,
            &device,
            &DEFAULT_CAPABILITIES,
            1_000,
            DEVICE_CERT_TTL_SECS,
        )
        .unwrap_err();
        assert!(error.contains("different keypair"), "{error}");
    }

    #[test]
    fn a_replayed_older_certificate_is_refused() {
        let (signed, keypair, device, first) = certificate();
        let renewed = DeviceCertificate::issue(
            &signed,
            &keypair,
            &device,
            &DEFAULT_CAPABILITIES,
            first.issued_at + 60,
            DEVICE_CERT_TTL_SECS,
        )
        .unwrap();
        let pin = PinnedCertificate::from_certificate(&renewed).unwrap();
        let error = pin.check(&first).unwrap_err();
        assert!(matches!(error, CertificateError::Superseded { .. }));
    }

    #[test]
    fn the_same_certificate_may_be_presented_again() {
        let (_, _, _, certificate) = certificate();
        let pin = PinnedCertificate::from_certificate(&certificate).unwrap();
        pin.check(&certificate).expect("reconnect reuses the pin");
    }

    #[test]
    fn a_newer_certificate_replaces_the_pin_across_a_device_rotation() {
        let (signed, keypair, _, first) = certificate();
        let replacement = DeviceKey::generate();
        let rotated = DeviceCertificate::issue(
            &signed,
            &keypair,
            &replacement,
            &DEFAULT_CAPABILITIES,
            first.issued_at + 60,
            DEVICE_CERT_TTL_SECS,
        )
        .unwrap();
        let pin = PinnedCertificate::from_certificate(&first).unwrap();
        pin.check(&rotated).expect("newer certificate wins");
        assert_eq!(
            PinnedCertificate::from_certificate(&rotated)
                .unwrap()
                .endpoint_id,
            replacement.endpoint_id_string()
        );
    }

    #[test]
    fn a_pin_refuses_an_equal_dated_certificate_from_another_endpoint() {
        let (signed, keypair, _, first) = certificate();
        let replacement = DeviceKey::generate();
        let same_epoch = DeviceCertificate::issue(
            &signed,
            &keypair,
            &replacement,
            &DEFAULT_CAPABILITIES,
            first.issued_at,
            DEVICE_CERT_TTL_SECS,
        )
        .unwrap();
        let pin = PinnedCertificate::from_certificate(&first).unwrap();
        assert!(matches!(
            pin.check(&same_epoch),
            Err(CertificateError::Superseded { .. })
        ));
    }

    #[test]
    fn the_certificate_is_signed_over_every_field_but_the_signature() {
        let (_, _, _, certificate) = certificate();
        let canonical = certificate.canonical_json().unwrap();
        assert!(!canonical.contains(certificate.signature.as_deref().unwrap()));
        assert!(canonical.contains(&certificate.endpoint_id));
        assert!(canonical.contains(&certificate.profile));
        assert!(canonical.contains(&certificate.issued_at.to_string()));
    }
}
