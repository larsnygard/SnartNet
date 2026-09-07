//! Validated profile, contact, and encrypted-message creation.
use super::*;

pub(crate) async fn load_startup_async() -> Result<StartupData, String> {
    let storage = FileStorage::open_default().map_err(|e| e.to_string())?;
    load_startup(&storage)
}

pub(crate) fn load_startup(storage: &FileStorage) -> Result<StartupData, String> {
    let keypair: Option<KeyPair> = storage
        .get_json(STORAGE_KEYPAIR)
        .map_err(|e| e.to_string())?;
    let profile: Option<SignedProfile> = storage
        .get_json(STORAGE_PROFILE)
        .map_err(|e| e.to_string())?;
    if let Some(profile) = &profile {
        let kp = keypair.as_ref().ok_or(
            "Profile exists but its keypair is missing. Restore a backup before continuing.",
        )?;
        if kp.fingerprint != profile.profile.fingerprint || !profile.verify().unwrap_or(false) {
            return Err("Stored profile does not match a valid local identity. Restore a backup before continuing.".into());
        }
    }
    Ok(StartupData {
        keypair,
        profile,
        local_posts: storage
            .get_json(STORAGE_POSTS)
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
        contacts: storage
            .get_json(STORAGE_CONTACTS)
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
        threads: storage
            .get_json(STORAGE_THREADS)
            .map_err(|e| e.to_string())?
            .unwrap_or_default(),
    })
}

pub(crate) async fn create_profile_async(
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_data_url: Option<String>,
    keypair: Option<KeyPair>,
    existing_profile: Option<SignedProfile>,
) -> Result<(KeyPair, SignedProfile), String> {
    let username = username.trim().to_string();
    if username.len() < 3 || username.len() > 32 {
        return Err("Username must be 3-32 characters".to_string());
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Username may only contain letters, digits and underscore".to_string());
    }

    let mut kp = if let Some(existing) = keypair {
        existing
    } else {
        KeyPair::generate()?
    };
    kp.ensure_encryption_keys();

    let mut profile = if let Some(existing) = existing_profile {
        let mut p = existing.profile;
        p.username = username;
        p.update(display_name.clone(), bio.clone());
        p.display_name = display_name;
        p.bio = bio;
        p
    } else {
        let mut p = Profile::new(username, kp.get_public_info());
        p.update(display_name.clone(), bio.clone());
        p.display_name = display_name;
        p.bio = bio;
        p
    };

    profile.avatar_hash = avatar_data_url
        .as_ref()
        .map(|v| blake3::hash(v.as_bytes()).to_hex().to_string());
    profile.avatar_data_url = avatar_data_url;
    profile.encryption_public_key = kp.enc_public_key.clone();
    // magnet_uri is derived after signing and must not be in signed bytes.
    profile.magnet_uri = None;

    let mut signed = SignedProfile::create(profile, &kp)?;
    signed.profile.magnet_uri = Some(signed.profile.generate_magnet_uri());
    Ok((kp, signed))
}

pub(crate) async fn add_contact_async(
    fingerprint: String,
    alias: String,
) -> Result<Contact, String> {
    let fp = fingerprint.trim().to_string();
    if general_purpose::STANDARD
        .decode(&fp)
        .map(|bytes| bytes.len() != 16)
        .unwrap_or(true)
    {
        return Err("Paste the complete 24-character contact fingerprint".into());
    }

    let alias = if alias.trim().is_empty() {
        format!("contact-{}", &fp[..8])
    } else {
        alias.trim().to_string()
    };

    Ok(Contact {
        fingerprint: fp,
        alias,
        magnet_uri: None,
        transport_addr: None,
        avatar_data_url: None,
        auto_synced: false,
        last_sync_label: "pending".to_string(),
        profile_summary: "Awaiting peer profile sync".to_string(),
        latest_post_preview: "Awaiting peer post sync".to_string(),
        verification: VerificationState::Unknown,
        trust_score: default_trust(),
        synced_post_count: 0,
        known_public_key: None,
        known_encryption_public_key: None,
        last_sync_error: None,
    })
}

pub(crate) async fn create_post_async(
    author_fingerprint: String,
    content: String,
    keypair: Option<KeyPair>,
) -> Result<SignedPost, String> {
    let kp = keypair.ok_or("No keypair available")?;
    if content.trim().is_empty() {
        return Err("Post cannot be empty".to_string());
    }
    let post = Post::new(author_fingerprint, content, None, None);
    SignedPost::create(post, &kp)
}

pub(crate) async fn create_message_async(
    sender_fingerprint: String,
    recipient_fingerprint: String,
    content: String,
    keypair: Option<KeyPair>,
    recipient_encryption_public_key: String,
) -> Result<SignedMessage, String> {
    let mut kp = keypair.ok_or("No keypair available")?;
    kp.ensure_encryption_keys();
    if content.trim().is_empty() {
        return Err("Message cannot be empty".to_string());
    }

    let (ciphertext_b64, nonce_b64, alg) =
        kp.encrypt_for_recipient(&recipient_encryption_public_key, &content)?;

    let mut msg =
        CoreMessage::new_direct(sender_fingerprint, recipient_fingerprint, ciphertext_b64);
    msg.encrypted = true;
    msg.body_enc = Some(alg);
    msg.nonce_b64 = Some(nonce_b64);
    SignedMessage::create(msg, &kp)
}

/// Decode a base64 invite code and construct a pending `Contact` from it.
pub(crate) async fn import_invite_async(code: String) -> Result<Contact, String> {
    let invite = ContactInvite::parse(&code)?;
    let alias = invite
        .display_name
        .as_ref()
        .filter(|d| !d.is_empty())
        .cloned()
        .unwrap_or_else(|| invite.username.clone());
    Ok(Contact {
        fingerprint: invite.fingerprint,
        alias,
        magnet_uri: invite.magnet_uri,
        transport_addr: invite.transport_addr,
        avatar_data_url: None,
        auto_synced: false,
        last_sync_label: "pending".to_string(),
        profile_summary: "Awaiting peer profile sync".to_string(),
        latest_post_preview: "Awaiting peer post sync".to_string(),
        verification: VerificationState::Unknown,
        trust_score: default_trust(),
        synced_post_count: 0,
        known_public_key: None,
        known_encryption_public_key: None,
        last_sync_error: None,
    })
}

pub(crate) async fn import_magnet_async(uri: String) -> Result<Contact, String> {
    let fingerprint = profile_fingerprint_from_magnet_uri(&uri)?;
    add_contact_async(fingerprint.clone(), String::new()).await?;
    let alias = short_fp(&fingerprint);
    Ok(Contact {
        fingerprint,
        alias,
        magnet_uri: Some(uri),
        transport_addr: None,
        avatar_data_url: None,
        auto_synced: false,
        last_sync_label: "pending".to_string(),
        profile_summary: "Awaiting peer profile sync".to_string(),
        latest_post_preview: "Awaiting peer post sync".to_string(),
        verification: VerificationState::Unknown,
        trust_score: default_trust(),
        synced_post_count: 0,
        known_public_key: None,
        known_encryption_public_key: None,
        last_sync_error: None,
    })
}
