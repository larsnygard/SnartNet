//! Tests for the daemon-backed frontend (ADR 0001).
//!
//! The desktop owns no identity, database, torrent session, or peer listener, so
//! these tests drive the real daemon over its loopback API and then assert on
//! the view models and reducer transitions the window actually uses. Nothing
//! here reaches into the daemon's files directly: the SDK is the only door.
use super::*;
use crate::state::NearbyPeer;
use snartnet_core::{KeyPair, Post, Profile, SignedPost};
use snartnet_sdk::{Client, DaemonPaths, Snapshot, API_VERSION};
use std::time::{Duration, Instant};

/// One identity plus the JSON shape the daemon persists and republishes.
fn identity(username: &str) -> (KeyPair, Profile) {
    let keypair = KeyPair::generate().unwrap();
    let mut profile = Profile::new(username.to_string(), keypair.get_public_info());
    profile.display_name = Some(username.to_uppercase());
    profile.bio = Some(format!("{username} tests the desktop"));
    (keypair, profile)
}

/// Runtime paths for a throwaway data directory, like the CLI resolves them.
fn paths_at(root: &std::path::Path) -> DaemonPaths {
    let data = root.join("data");
    DaemonPaths::from_data_dir(Some(data.to_str().expect("UTF-8 temp path"))).unwrap()
}

/// A frontend pointed at `paths` that has received one snapshot through the
/// normal startup path, so `loaded` and the status line are exactly what the
/// window shows after its first poll.
fn app_after_snapshot(root: &std::path::Path, state: DaemonState) -> App {
    let backend = Backend::connect_to(paths_at(root)).unwrap();
    let (mut app, _startup) = App::with_backend(backend, None);
    app.dispatch(Message::Started(Ok(state)));
    app
}

/// A populated snapshot in exactly the shape `Session::snapshot` produces,
/// including the status extras the Network panel renders.
fn populated_snapshot() -> (DaemonState, Profile, Profile) {
    let (keypair, profile) = identity("alice");
    let (_, bob) = identity("bob");
    let (_, carol) = identity("carol");
    let post = SignedPost::create(
        Post::new(
            profile.fingerprint.clone(),
            "Haloj from the daemon".to_string(),
            Some(vec!["intro".to_string()]),
            None,
        ),
        &keypair,
    )
    .unwrap();
    let contact = Contact {
        fingerprint: bob.fingerprint.clone(),
        alias: "Bob".into(),
        auto_synced: true,
        last_sync_label: "2 minutes ago".into(),
        profile_summary: "bob tests the desktop".into(),
        latest_post_preview: "Bob's latest post".into(),
        verification: VerificationState::Verified,
        trust_score: 80,
        synced_post_count: 2,
        known_public_key: Some(bob.public_key.clone()),
        known_encryption_public_key: bob.encryption_public_key.clone(),
        ..Contact::default()
    };
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "apiVersion": API_VERSION,
        "revision": 7,
        "state": {
            "profile": serde_json::to_value(&profile).unwrap(),
            "identityUri": profile.identity_uri(),
            "posts": [serde_json::to_value(&post).unwrap()],
            "contacts": [serde_json::to_value(&contact).unwrap()],
            "threads": [{
                "fingerprint": bob.fingerprint.clone(),
                "unread": 2,
                "messages": [
                    {
                        "id": "message-1",
                        "incoming": true,
                        "encrypted": true,
                        "ciphertext": "ciphertext-1",
                        "text": "Hej Bob",
                        "delivery": "relayed",
                        "time": "09:14"
                    },
                    {
                        "id": "message-2",
                        "incoming": false,
                        "encrypted": true,
                        "ciphertext": "ciphertext-2",
                        "error": "missing peer encryption key",
                        "delivery": "available",
                        "time": "09:15"
                    },
                    {
                        "id": "message-3",
                        "incoming": false,
                        "encrypted": true,
                        "ciphertext": "ciphertext-3",
                        "error": "missing peer encryption key",
                        "delivery": "queued",
                        "deliveryError": "disk is full",
                        "time": "09:16"
                    }
                ]
            }],
            "nearby": [{
                "fingerprint": carol.fingerprint.clone(),
                "alias": "carol@laptop",
                "address": "10.0.0.9:47470"
            }],
            "listening": "192.168.1.5:47470",
            "address": "192.168.1.5:47470",
            "peers": 3,
            "dht": {
                "bootstrapped": true,
                "last_lookup": "1 minute ago",
                "last_publish": null,
                "last_error": null
            },
            "torrent": {
                "listening": true,
                "reachability": "direct",
                "peer_count": 5,
                "last_fetch": null,
                "last_publish": null,
                "last_error": null
            },
            "peer": {
                "active": true,
                "node_id": "node-1",
                "peer_count": 4,
                "discovery": "dns-pkarr",
                "last_error": null
            },
            "delivery": {
                "durable": true,
                "failed": "disk is full",
                "spooled": 2
            },
            "storage": {
                "platform": "desktop",
                "hosting": true,
                "quotaBytes": 1073741824,
                "usedBytes": 4096,
                "freeBytes": 8_589_934_592u64,
                "held": 2,
                "stored": 1,
                "issued": 3,
                "receipts": 2,
                "note": "stored 1 replica(s)"
            },
            "relay": {
                "plan": "referral (1): https://relay.example.com/",
                "source": "referral",
                "disabled": false,
                "planned": ["https://relay.example.com/"],
                "active": ["https://relay.example.com/"],
                "health": [
                    {
                        "url": "https://relay.example.com/",
                        "connected": true,
                        "score": 101,
                        "failures": 0,
                        "lastError": null
                    }
                ]
            },
            "paused": true,
            "discovery": true,
            "lastSync": "10:02",
            "listenerError": null
        }
    }))
    .unwrap();
    let state = DaemonState::from_snapshot(&snapshot).expect("the fixture parses");
    (state, profile, bob)
}

/// A frontend that has already received one full snapshot.
fn app_with_daemon_state(root: &std::path::Path) -> App {
    let (state, _, _) = populated_snapshot();
    app_after_snapshot(root, state)
}

#[test]
fn a_snapshot_becomes_the_view_models_the_window_renders() {
    let (state, profile, bob) = populated_snapshot();
    let expected_uri = profile.identity_uri();
    assert_eq!(
        state
            .profile
            .as_ref()
            .map(|profile| profile.username.as_str()),
        Some("alice")
    );
    assert_eq!(state.identity_uri.as_deref(), Some(expected_uri.as_str()));
    assert_eq!(state.posts.len(), 1);
    assert_eq!(state.posts[0].post.content, "Haloj from the daemon");
    assert_eq!(state.posts[0].post.author_fingerprint, profile.fingerprint);

    // Contacts keep the daemon's verification verdict and trust score instead of
    // re-deriving them from keys this process does not hold.
    let contact = state.contact(&bob.fingerprint).expect("bob is a contact");
    assert_eq!(contact.alias, "Bob");
    assert_eq!(contact.verification, VerificationState::Verified);
    assert_eq!(contact.trust_score, 80);
    assert_eq!(contact.synced_post_count, 2);
    assert!(contact.auto_synced);
    assert_eq!(contact.last_sync_label, "2 minutes ago");

    let thread = state.thread(&bob.fingerprint).expect("bob has a thread");
    assert_eq!(thread.unread_count, 2);
    assert_eq!(state.total_unread(), 2);
    // The daemon decrypts for this frontend, so plaintext arrives with the state.
    assert_eq!(thread.messages[0].plaintext, Ok("Hej Bob".to_string()));
    assert_eq!(thread.messages[0].delivery, DeliveryState::Relayed);
    assert_eq!(thread.messages[0].ciphertext, "ciphertext-1");
    assert_eq!(thread.messages[0].created_label, "09:14");
    assert!(thread.messages[0].incoming && thread.messages[0].encrypted);
    // Unreadable messages keep the daemon's explanation and stay queued.
    let error = thread.messages[1]
        .plaintext
        .as_ref()
        .expect_err("the second message is not decrypted");
    assert!(error.contains("missing peer encryption key"), "{error}");
    assert_eq!(thread.messages[1].delivery, DeliveryState::Available);
    assert_eq!(thread.messages[1].ciphertext, "ciphertext-2");
    assert_eq!(thread.messages[1].delivery.label(), "Available");
    // A publication failure is shown with the state instead of the message looking queued for
    // no reason (M7.5).
    assert_eq!(thread.messages[2].delivery, DeliveryState::Queued);
    assert!(thread.messages[2].delivery.is_pending());
    assert_eq!(
        thread.messages[2].delivery_error.as_deref(),
        Some("disk is full")
    );

    assert_eq!(state.nearby.len(), 1);
    assert_eq!(state.nearby[0].alias, "carol@laptop");
    assert_eq!(state.nearby[0].address.as_deref(), Some("10.0.0.9:47470"));

    let network = &state.network;
    assert_eq!(network.listening.as_deref(), Some("192.168.1.5:47470"));
    assert_eq!(network.address_override, "192.168.1.5:47470");
    assert_eq!(network.peers, 3);
    assert!(network.paused);
    assert!(network.discovery);
    assert_eq!(network.last_sync, "10:02");
    assert!(network.listener_error.is_none());
    assert!(network.dht.as_ref().is_some_and(|dht| dht.bootstrapped));
    assert!(network
        .torrent
        .as_ref()
        .is_some_and(|session| session.listening));
    assert_eq!(network.peer.as_ref().map(|peer| peer.peer_count), Some(4));
    // The durable summary explains why a message is still queued (M7.5).
    assert!(network.delivery.durable);
    assert_eq!(network.delivery.failed.as_deref(), Some("disk is full"));
    assert_eq!(network.delivery.spooled, 2);
    // Relay selection names its source and scores each relay locally (M8.2/M8.4).
    assert_eq!(network.relay.source.as_deref(), Some("referral"));
    assert!(!network.relay.disabled);
    assert_eq!(network.relay.active, vec!["https://relay.example.com/"]);
    assert_eq!(network.relay.health.len(), 1);
    assert!(network.relay.health[0].connected);
    assert_eq!(network.relay.health[0].score, 101);
    assert!(network.relay.plan.contains("relay.example.com"));
    // Replication and storage state is named, not guessed (M9.5).
    assert!(network.storage.hosting);
    assert_eq!(network.storage.platform, "desktop");
    assert_eq!(network.storage.quota_bytes, 1024 * 1024 * 1024);
    assert_eq!(network.storage.used_bytes, 4096);
    assert_eq!(network.storage.held, 2);
    assert_eq!(network.storage.stored, 1);
    assert_eq!(network.storage.issued, 3);
    assert_eq!(network.storage.receipts, 2);
    assert_eq!(network.storage.free_bytes, Some(8_589_934_592));
    assert_eq!(network.storage.note.as_deref(), Some("stored 1 replica(s)"));
}

#[test]
fn an_empty_snapshot_still_produces_renderable_defaults() {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "apiVersion": API_VERSION,
        "revision": 0,
        "state": {}
    }))
    .unwrap();
    let state = DaemonState::from_snapshot(&snapshot).unwrap();
    assert!(state.profile.is_none());
    assert!(state.posts.is_empty());
    assert!(state.contacts.is_empty());
    assert!(state.threads.is_empty());
    assert!(state.nearby.is_empty());
    assert_eq!(state.total_unread(), 0);
    // A daemon that has never synced says so rather than showing a blank label.
    assert_eq!(state.network.last_sync, "never");
    assert!(!state.network.paused);
    assert!(!state.network.discovery);
    assert!(state.network.listening.is_none());
    assert!(state.network.dht.is_none());
    assert!(state.network.torrent.is_none());
    assert!(state.network.peer.is_none());
    // A daemon that reports no delivery block is treated as having no durable path rather
    // than as a silent success.
    assert!(!state.network.delivery.durable);
    assert!(state.network.delivery.failed.is_none());
}

#[test]
fn unreadable_snapshot_entries_are_reported_instead_of_rendered_blank() {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "apiVersion": API_VERSION,
        "revision": 3,
        "state": {"posts": [{"not": "a post"}]}
    }))
    .unwrap();
    let error = DaemonState::from_snapshot(&snapshot).unwrap_err();
    assert!(error.contains("posts"), "{error}");
    assert!(error.contains("unreadable"), "{error}");
}

#[test]
fn message_readiness_needs_the_daemons_verification_not_just_a_key() {
    let mut state = DaemonState::default();
    assert!(!state.is_ready_to_message("nobody"));
    state.contacts.push(Contact {
        fingerprint: "fp-bob".into(),
        ..Contact::default()
    });
    state.contacts[0].known_encryption_public_key = Some("encryption-key".into());
    assert!(!state.is_ready_to_message("fp-bob"));
    state.contacts[0].verification = VerificationState::Verified;
    assert!(state.is_ready_to_message("fp-bob"));
    assert!(!state.is_ready_to_message("fp-carol"));
}

#[test]
fn a_daemon_without_a_profile_offers_onboarding() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_after_snapshot(dir.path(), DaemonState::default());
    assert!(app.loaded);
    assert_eq!(app.status_line, "Create a profile to start connecting.");
    assert!(app.state.identity_uri.is_none());
    assert!(app.invite.is_none());
}

#[test]
fn a_missing_daemon_is_explained_and_offers_an_explicit_start() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Backend::connect_to(paths_at(dir.path())).unwrap();
    let (mut app, _startup) = App::with_backend(backend, None);
    app.dispatch(Message::Started(Err(
        "The SnartNet daemon is not running for this data directory.".into(),
    )));
    assert!(!app.loaded);
    assert!(
        app.status_line.contains("daemon is not running"),
        "{}",
        app.status_line
    );
    assert!(
        app.status_line
            .ends_with("Start it with the button below, then retry."),
        "{}",
        app.status_line
    );
    // A later poll failure must not re-explain startup to a running window.
    app.loaded = true;
    app.dispatch(Message::Refreshed(Err(
        "The SnartNet daemon stopped responding.".into(),
    )));
    assert_eq!(app.status_line, "The SnartNet daemon stopped responding.");
}

#[test]
fn the_snapshot_seeds_the_profile_form_once_and_then_never_overwrites_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    assert_eq!(app.status_line, "Connected to the daemon.");
    assert_eq!(app.forms.username_input, "alice");
    assert_eq!(app.forms.display_name_input, "ALICE");
    assert_eq!(app.forms.bio_input, "alice tests the desktop");
    assert_eq!(app.forms.advertise_addr, "192.168.1.5:47470");

    // Typing must survive the two-second poll.
    app.dispatch(Message::UsernameChanged("alice2".into()));
    let (state, _, _) = populated_snapshot();
    app.dispatch(Message::Refreshed(Ok(state)));
    assert_eq!(app.forms.username_input, "alice2");
    assert_eq!(app.forms.display_name_input, "ALICE");
    // The mirrored state still follows the daemon.
    assert_eq!(
        app.state
            .profile
            .as_ref()
            .map(|profile| profile.username.as_str()),
        Some("alice")
    );
}

#[test]
fn an_undialable_advertised_address_never_reaches_the_daemon() {
    assert!(validate_endpoint("192.168.1.5:47470").is_ok());
    assert!(validate_endpoint("localhost:47470").is_err());
    assert!(validate_endpoint("0.0.0.0:47470").is_err());
    assert!(validate_endpoint("192.168.1.5:0").is_err());
    assert!(validate_endpoint("localhost").is_err());

    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    app.forms.advertise_addr = "localhost:47470".into();
    app.dispatch(Message::SaveProfile);
    assert_eq!(
        app.status_line,
        "Use an IP address and port, such as 192.168.1.5:47470"
    );
    assert!(!app.saving_profile, "no command should be in flight");
}

#[test]
fn contact_import_opens_the_new_chat_and_keeps_the_input_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());

    // Empty input is caught here; the daemon is never asked.
    app.dispatch(Message::AddContact);
    assert_eq!(app.status_line, "Enter the contact's fingerprint");
    assert!(app.pending_contact.is_none());

    // A failure from the daemon leaves the typo in the box so it can be fixed.
    app.forms.contact_fingerprint_input = "not-a-fingerprint".into();
    app.dispatch(Message::AddContact);
    assert_eq!(app.pending_contact.as_deref(), Some("not-a-fingerprint"));
    assert_eq!(app.status_line, "Contact added. Syncing their profile…");
    app.dispatch(Message::ContactAdded(Err("Invalid fingerprint".into())));
    assert_eq!(
        app.status_line,
        "Could not add the contact: Invalid fingerprint"
    );
    assert_eq!(app.forms.contact_fingerprint_input, "not-a-fingerprint");
    assert!(app.pending_contact.is_none());

    // Success clears the form and jumps straight into the new conversation.
    app.forms.contact_fingerprint_input = "fp-bob".into();
    app.forms.contact_alias_input = "Bob".into();
    app.dispatch(Message::AddContact);
    app.dispatch(Message::ContactAdded(Ok(())));
    assert!(app.forms.contact_fingerprint_input.is_empty());
    assert!(app.forms.contact_alias_input.is_empty());
    assert_eq!(
        app.forms.selected_contact_for_chat.as_deref(),
        Some("fp-bob")
    );
    assert_eq!(app.panel, Panel::Messages);
}

fn imported_contact() -> String {
    "magnet:?xt=snartnet:bob".to_string()
}

#[test]
fn every_import_route_reaches_the_same_daemon_command() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());

    app.dispatch(Message::ImportFromInvite);
    assert_eq!(app.status_line, "Paste an invitation code first");
    app.forms.invite_code_input = "invite-code".into();
    app.dispatch(Message::ImportFromInvite);
    assert_eq!(app.pending_contact.as_deref(), Some("invite-code"));

    app.dispatch(Message::ImportFromMagnet);
    assert_eq!(app.status_line, "Paste a profile magnet URI first");
    app.forms.magnet_uri_input = imported_contact();
    app.dispatch(Message::ImportFromMagnet);
    assert_eq!(
        app.pending_contact.as_deref(),
        Some(imported_contact().as_str())
    );

    // A peer picked from LAN discovery carries the address the daemon can dial.
    app.state.nearby = vec![NearbyPeer {
        fingerprint: "fp-carol".into(),
        alias: "carol@laptop".into(),
        address: Some("10.0.0.9:47470".into()),
    }];
    app.dispatch(Message::AddDiscoveredPeer("fp-carol".into()));
    assert_eq!(app.pending_contact.as_deref(), Some("fp-carol"));
    assert_eq!(
        app.status_line,
        "Contact added from discovery. Syncing their profile…"
    );
}

#[test]
fn a_failed_send_keeps_the_draft_and_a_committed_send_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    app.dispatch(Message::SendChat);
    assert_eq!(app.status_line, "Select a contact before messaging");

    app.select_chat("fp-bob".into());
    app.forms.compose_message_input = "Hej Bob".into();
    app.forms.drafts.insert("fp-bob".into(), "Hej Bob".into());
    app.dispatch(Message::SendChat);
    assert_eq!(
        app.sending
            .as_ref()
            .map(|(fingerprint, _)| fingerprint.as_str()),
        Some("fp-bob")
    );
    // The daemon refused, so the text stays where the user can retry it.
    app.dispatch(Message::ChatSent(Err(
        "The recipient is not a contact".into()
    )));
    assert_eq!(
        app.status_line,
        "Message failed: The recipient is not a contact"
    );
    assert_eq!(app.forms.compose_message_input, "Hej Bob");
    assert_eq!(
        app.forms.drafts.get("fp-bob").map(String::as_str),
        Some("Hej Bob")
    );
    assert!(app.sending.is_none());

    // A committed message clears only the conversation it belongs to.
    app.forms
        .drafts
        .insert("fp-carol".into(), "Hej Carol".into());
    app.dispatch(Message::SendChat);
    assert_eq!(
        app.sending.as_ref().map(|(_, draft)| draft.as_str()),
        Some("Hej Bob")
    );
    app.dispatch(Message::ChatSent(Ok(())));
    assert!(
        app.status_line.starts_with("Message queued"),
        "{}",
        app.status_line
    );
    assert!(app.forms.compose_message_input.is_empty());
    assert!(!app.forms.drafts.contains_key("fp-bob"));
    assert_eq!(
        app.forms.drafts.get("fp-carol").map(String::as_str),
        Some("Hej Carol")
    );
}

#[test]
fn switching_conversations_swaps_drafts_instead_of_leaking_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    app.select_chat("fp-bob".into());
    app.dispatch(Message::ComposeMessageChanged("only for bob".into()));
    app.select_chat("fp-carol".into());
    assert_eq!(app.forms.compose_message_input, "");
    app.dispatch(Message::ComposeMessageChanged("only for carol".into()));
    app.select_chat("fp-bob".into());
    assert_eq!(app.forms.compose_message_input, "only for bob");
    assert_eq!(
        app.forms.drafts.get("fp-carol").map(String::as_str),
        Some("only for carol")
    );
}

#[test]
fn invitation_links_come_from_the_daemon_and_never_from_local_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    assert!(app.invite_uri().is_err(), "nothing is cached yet");

    let invite = Invite {
        uri: "snartnet://profile/abc".into(),
        magnet: Some("magnet:?xt=urn:btih:abc".into()),
    };
    app.dispatch(Message::InviteRefreshed(Ok(invite.clone())));
    assert_eq!(app.invite_uri().unwrap(), "snartnet://profile/abc");
    assert_eq!(
        app.invite
            .as_ref()
            .and_then(|invite| invite.magnet.as_deref()),
        Some("magnet:?xt=urn:btih:abc")
    );

    // A daemon failure while copying stays in the status line.
    app.dispatch(Message::InviteCopied(Err(
        "The SnartNet daemon is not running for this data directory.".into(),
    )));
    assert_eq!(
        app.status_line,
        "The SnartNet daemon is not running for this data directory."
    );

    // Exporting a QR code without an invitation is refused, not panicked on.
    app.invite = None;
    app.dispatch(Message::SaveQrPng);
    assert!(
        app.status_line
            .starts_with("Could not save the QR code: The invitation link is not ready"),
        "{}",
        app.status_line
    );
}

#[test]
fn closing_the_window_hides_it_and_the_tray_brings_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    let window = window::Id::unique();
    app.dispatch(Message::CloseRequested(window));
    assert_eq!(app.window, Some(window));
    assert_eq!(app.status_line, "SnartNet keeps syncing in the tray.");
    // The daemon and its state are untouched by closing the window.
    assert!(app.loaded);
    assert!(app.state.profile.is_some());

    app.dispatch(Message::Tray(tray::TrayCommand::Show));
    assert_eq!(app.window, Some(window));
    assert!(app.status_line.starts_with("SnartNet keeps syncing"));
}

#[test]
fn stopping_the_daemon_from_the_tray_exits_the_window_either_way() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    app.dispatch(Message::Tray(tray::TrayCommand::StopDaemonAndQuit));
    assert_eq!(app.status_line, "Stopping the SnartNet daemon…");
    // A clean stop leaves the status alone; a failure is only reported to the
    // console, because a frontend must never trap the user in a shutdown.
    app.dispatch(Message::Stopped(None));
    assert_eq!(app.status_line, "Stopping the SnartNet daemon…");
}

#[test]
fn every_panel_renders_from_daemon_state_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_daemon_state(dir.path());
    app.invite = Some(Invite {
        uri: "snartnet://profile/abc".into(),
        magnet: None,
    });
    let selected = app.state.contacts[0].fingerprint.clone();
    app.forms.selected_contact_for_chat = Some(selected);
    for panel in [
        Panel::Feed,
        Panel::Profile,
        Panel::Contacts,
        Panel::Messages,
        Panel::Network,
    ] {
        app.panel = panel;
        let _element = app.view();
    }
    // The same panels must still render before the daemon has answered.
    app.loaded = false;
    app.state = DaemonState::default();
    for panel in [Panel::Feed, Panel::Profile, Panel::Contacts, Panel::Network] {
        app.panel = panel;
        let _element = app.view();
    }
}

/// Metadata is published before the API answers, so poll until it does.
fn wait_for_daemon(client: &Client) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match client.health() {
            Ok(_) => return,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("the daemon never answered: {error}"),
        }
    }
}

/// The contract that matters most (ADR 0001): everything the window shows and
/// everything it changes goes through a real daemon over the local API, and the
/// frontend never owns identity or storage.
#[test]
fn the_frontend_reads_and_writes_only_through_a_real_daemon() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths_at(root.path());
    let server_paths = paths.clone();
    let server = std::thread::spawn(move || {
        let bind = "127.0.0.1:0".parse().expect("loopback bind");
        snartnet_daemon::run_with(server_paths, 0, bind)
    });
    let client = Client::new(paths.clone()).unwrap();
    wait_for_daemon(&client);
    // The window enters through the same door as these tests: the SDK.
    let backend = Backend::connect_to(paths.clone()).unwrap();

    // A fresh data directory has no identity yet, so the window shows onboarding.
    let fresh = DaemonState::from_snapshot(&backend.snapshot().unwrap()).unwrap();
    assert!(fresh.profile.is_none());
    assert!(fresh.posts.is_empty());
    assert!(fresh.contacts.is_empty());
    assert_eq!(fresh.network.last_sync, "never");
    assert!(!fresh.network.paused, "the daemon starts unpaused");

    // Onboarding is one daemon command; this process stores no key material.
    backend
        .command(Command::Profile {
            username: "alice".into(),
            display_name: "Alice".into(),
            bio: "Created by the desktop test".into(),
            avatar: String::new(),
            address: "127.0.0.1:47470".into(),
        })
        .unwrap();
    let state = DaemonState::from_snapshot(&backend.snapshot().unwrap()).unwrap();
    let profile = state.profile.as_ref().expect("the daemon owns the profile");
    assert_eq!(profile.username, "alice");
    assert_eq!(profile.display_name.as_deref(), Some("Alice"));
    let fingerprint = profile.fingerprint.clone();
    let uri = state
        .identity_uri
        .clone()
        .expect("the daemon builds the invite");
    assert!(uri.starts_with("snartnet://profile/"), "{uri}");

    // Invitation links, including the magnet, stay daemon-generated. Invites
    // carry an address and so are a different URI shape than the identity link.
    let invite = backend.invite().unwrap();
    assert!(
        invite.uri.starts_with("snartnet://invite/"),
        "{}",
        invite.uri
    );
    // The magnet only exists once the daemon has published the profile.
    if let Some(magnet) = invite.magnet.as_deref() {
        assert!(magnet.starts_with("magnet:?xt="), "{magnet}");
    }

    // Publishing is a command, and the snapshot is what the feed renders.
    backend
        .command(Command::Post {
            content: "Haloj from the desktop test".into(),
        })
        .unwrap();
    let state = DaemonState::from_snapshot(&backend.snapshot().unwrap()).unwrap();
    assert_eq!(state.posts.len(), 1);
    assert_eq!(state.posts[0].post.content, "Haloj from the desktop test");
    assert_eq!(state.posts[0].post.author_fingerprint, fingerprint);

    // Pausing changes how much the daemon does, never whether it answers, and
    // the frontend must be able to see the mode it is actually in.
    assert_eq!(fresh.network.sync_mode, SyncMode::Balanced);
    backend.set_sync_mode(SyncMode::Paused).unwrap();
    let state = DaemonState::from_snapshot(&backend.snapshot().unwrap()).unwrap();
    assert!(
        state.network.paused,
        "pausing must be visible in the snapshot"
    );
    assert_eq!(state.network.sync_mode, SyncMode::Paused);
    assert!(
        backend.sync().is_err(),
        "a paused daemon refuses explicit syncs"
    );
    backend.set_sync_mode(SyncMode::Balanced).unwrap();
    let state = DaemonState::from_snapshot(&backend.snapshot().unwrap()).unwrap();
    assert!(!state.network.paused, "resuming must clear the paused flag");
    assert_eq!(state.network.sync_mode, SyncMode::Balanced);

    // Unknown contacts are the daemon's answer to give, not the frontend's.
    let unknown = backend.command(Command::Contact {
        input: "not-a-fingerprint".into(),
        mode: "manual".into(),
        alias: String::new(),
        address: String::new(),
    });
    assert!(unknown.is_err(), "the daemon validates contact imports");

    // Shutting the daemon down is explicit, and it exits cleanly when asked.
    backend.stop().unwrap();
    server.join().unwrap().expect("the daemon exits cleanly");
}
