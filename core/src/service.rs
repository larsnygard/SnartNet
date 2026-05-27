use crate::crypto::KeyPair;
use crate::profile::{Profile, SignedProfile};
use crate::post::{Post, SignedPost};
use crate::message::{Message, SignedMessage};
use crate::storage::{StorageBackend, StorageError};
use serde::{Serialize, Deserialize};
use std::marker::PhantomData;

// ----- Shared JSON API structs (additive, forward-compatible) -----
#[derive(Serialize, Deserialize)]
pub struct CreateProfileRequest {
    pub username: String,
    #[serde(rename = "displayName")] pub display_name: Option<String>,
    pub bio: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    #[serde(rename = "displayName")] pub display_name: Option<String>,
    pub bio: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProfileEnvelope {
    pub profile: Profile,
    pub signature: String,
    #[serde(rename = "magnetUri")] pub magnet_uri: String,
    pub api: String,
    pub version: u32,
}

#[derive(Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    #[serde(rename = "profileJsonApi")] pub profile_json_api: bool,
    #[serde(rename = "postJsonApi")] pub post_json_api: bool,
    #[serde(rename = "messageJsonApi")] pub message_json_api: bool,
    pub version: String,
}

/// Platform-neutral core service, generic over a `StorageBackend`.
///
/// All business logic lives here; platform hosts (WASM, desktop, …) own a
/// `CoreService<TheirStorage>` and call these methods directly.
pub struct CoreService<S: StorageBackend> {
    current_profile: Option<SignedProfile>,
    keypair: Option<KeyPair>,
    _storage: PhantomData<S>,
}

impl<S: StorageBackend> CoreService<S> {
    pub fn new() -> Self {
        Self {
            current_profile: None,
            keypair: None,
            _storage: PhantomData,
        }
    }

    /// Load persisted keypair and profile from storage.
    pub fn init(&mut self) -> Result<(), StorageError> {
        if let Some(keypair) = S::get_json::<KeyPair>("snartnet_keypair")? {
            self.keypair = Some(keypair);
        }
        if let Some(profile) = S::get_json::<SignedProfile>("snartnet_current_profile")? {
            self.current_profile = Some(profile);
        }
        Ok(())
    }

    /// Create (or replace) the user profile, persist keypair + profile, return magnet URI.
    pub fn create_profile(
        &mut self,
        username: &str,
        display_name: Option<String>,
        bio: Option<String>,
    ) -> Result<String, StorageError> {
        if self.keypair.is_none() {
            self.keypair = Some(
                KeyPair::generate()
                    .map_err(|e| StorageError::Backend(format!("keygen failed: {e}")))?,
            );
        }
        let keypair = self.keypair.as_ref().unwrap();

        let mut profile = Profile::new(username.to_string(), keypair.get_public_info());
        profile.update(display_name, bio);

        let mut signed_profile = SignedProfile::create(profile, keypair)
            .map_err(|e| StorageError::Backend(format!("sign failed: {e}")))?;

        let magnet_uri = signed_profile.profile.generate_magnet_uri();
        signed_profile.profile.magnet_uri = Some(magnet_uri.clone());

        S::set_json("snartnet_keypair", keypair)?;
        S::set_json("snartnet_current_profile", &signed_profile)?;

        self.current_profile = Some(signed_profile);
        Ok(magnet_uri)
    }

    /// Update display name / bio of the current profile, re-sign and persist.
    pub fn update_profile(
        &mut self,
        display_name: Option<String>,
        bio: Option<String>,
    ) -> Result<(), StorageError> {
        match (&mut self.current_profile, &self.keypair) {
            (Some(profile), Some(keypair)) => {
                profile.profile.update(display_name, bio);
                let mut new_signed =
                    SignedProfile::create(profile.profile.clone(), keypair)
                        .map_err(|e| StorageError::Backend(format!("sign failed: {e}")))?;
                let magnet_uri = new_signed.profile.generate_magnet_uri();
                new_signed.profile.magnet_uri = Some(magnet_uri);
                S::set_json("snartnet_current_profile", &new_signed)?;
                self.current_profile = Some(new_signed);
                Ok(())
            }
            _ => Err(StorageError::Backend("no current profile".into())),
        }
    }

    /// Return the current profile envelope (if any).
    pub fn get_profile(&self) -> Option<ProfileEnvelope> {
        self.current_profile.as_ref().map(|signed| {
            let magnet_uri = signed
                .profile
                .magnet_uri
                .clone()
                .unwrap_or_else(|| signed.profile.generate_magnet_uri());
            ProfileEnvelope {
                profile: signed.profile.clone(),
                signature: signed.signature.clone(),
                magnet_uri,
                api: "profile-json-v1".to_string(),
                version: signed.profile.version,
            }
        })
    }

    pub fn has_profile(&self) -> bool {
        self.current_profile.is_some()
    }

    /// Create and sign a post for the current user.
    pub fn create_post(
        &self,
        content: &str,
        tags: Option<Vec<String>>,
        reply_to: Option<String>,
    ) -> Result<SignedPost, StorageError> {
        let keypair = self
            .keypair
            .as_ref()
            .ok_or_else(|| StorageError::Backend("no keypair".into()))?;
        let profile = self
            .current_profile
            .as_ref()
            .ok_or_else(|| StorageError::Backend("no current profile".into()))?;

        let post = Post::new(
            profile.profile.fingerprint.clone(),
            content.to_string(),
            tags,
            reply_to,
        );
        SignedPost::create(post, keypair)
            .map_err(|e| StorageError::Backend(format!("sign post failed: {e}")))
    }

    /// Create and sign a direct message.
    pub fn create_message(
        &self,
        recipient_fingerprint: &str,
        content: &str,
    ) -> Result<SignedMessage, StorageError> {
        let keypair = self
            .keypair
            .as_ref()
            .ok_or_else(|| StorageError::Backend("no keypair".into()))?;
        let profile = self
            .current_profile
            .as_ref()
            .ok_or_else(|| StorageError::Backend("no current profile".into()))?;

        let message = Message::new_direct(
            profile.profile.fingerprint.clone(),
            recipient_fingerprint.to_string(),
            content.to_string(),
        );
        SignedMessage::create(message, keypair)
            .map_err(|e| StorageError::Backend(format!("sign message failed: {e}")))
    }

    pub fn get_public_key(&self) -> Option<&str> {
        self.keypair.as_ref().map(|kp| kp.public_key.as_str())
    }

    pub fn get_fingerprint(&self) -> Option<&str> {
        self.keypair.as_ref().map(|kp| kp.fingerprint.as_str())
    }

    /// Return a reference to the raw `SignedProfile` (for signature verification, etc.).
    pub fn get_signed_profile(&self) -> Option<&SignedProfile> {
        self.current_profile.as_ref()
    }

    pub fn capabilities() -> CapabilityDescriptor {
        CapabilityDescriptor {
            profile_json_api: true,
            post_json_api: false,
            message_json_api: false,
            version: "1".to_string(),
        }
    }
}

impl<S: StorageBackend> Default for CoreService<S> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[test]
    fn signature_verifies_after_magnet_uri_annotation() {
        let mut svc = CoreService::<MemoryStorage>::new();
        svc.create_profile("eve", None, None).unwrap();
        let signed = svc.current_profile.as_ref().unwrap();
        assert!(signed.verify().expect("verify failed"));
    }

    #[test]
    fn create_profile_roundtrip() {
        let mut svc = CoreService::<MemoryStorage>::new();
        let magnet = svc
            .create_profile("alice", Some("Alice A.".into()), None)
            .expect("create_profile failed");
        assert!(magnet.starts_with("magnet:?xt=urn:btih:"));
        assert!(svc.has_profile());

        let env = svc.get_profile().expect("no profile");
        assert_eq!(env.profile.username, "alice");
        assert_eq!(env.profile.display_name.as_deref(), Some("Alice A."));
    }

    #[test]
    fn init_reloads_persisted_profile() {
        let mut svc = CoreService::<MemoryStorage>::new();
        let magnet1 = svc
            .create_profile("bob", None, None)
            .expect("create_profile failed");

        // Re-initialise from same in-memory store
        let mut svc2 = CoreService::<MemoryStorage>::new();
        svc2.init().expect("init failed");
        assert!(svc2.has_profile());

        let magnet2 = svc2
            .get_profile()
            .unwrap()
            .magnet_uri;
        assert_eq!(magnet1, magnet2);
    }

    #[test]
    fn update_profile_persists() {
        let mut svc = CoreService::<MemoryStorage>::new();
        svc.create_profile("carol", None, None).unwrap();
        svc.update_profile(Some("Carol C.".into()), Some("bio".into()))
            .unwrap();
        let env = svc.get_profile().unwrap();
        assert_eq!(env.profile.display_name.as_deref(), Some("Carol C."));
    }

    #[test]
    fn create_post_and_message() {
        let mut svc = CoreService::<MemoryStorage>::new();
        svc.create_profile("dave", None, None).unwrap();

        let post = svc
            .create_post("Hello world", Some(vec!["test".into()]), None)
            .unwrap();
        assert_eq!(post.post.content, "Hello world");

        let msg = svc
            .create_message("other-fingerprint", "hi")
            .unwrap();
        assert_eq!(msg.message.content, "hi");
    }
}
