//! Regression tests exercise real encrypted envelopes, independent storage roots,
//! loopback TCP, and the same state transitions used by the native interface.
use super::*;
use iced::futures::executor::block_on;

fn identity(name: &str) -> (KeyPair, SignedProfile) {
    block_on(create_profile_async(
        name.into(),
        Some(name.into()),
        None,
        None,
        None,
        None,
    ))
    .unwrap()
}

fn contact(profile: &SignedProfile) -> Contact {
    let mut contact = block_on(add_contact_async(
        profile.profile.fingerprint.clone(),
        profile.profile.username.clone(),
    ))
    .unwrap();
    contact.known_public_key = Some(profile.profile.public_key.clone());
    contact.known_encryption_public_key = profile.profile.encryption_public_key.clone();
    contact.verification = VerificationState::Verified;
    contact
}

fn encrypted(sender: &KeyPair, recipient: &KeyPair, content: &str) -> SignedMessage {
    block_on(create_message_async(
        sender.fingerprint.clone(),
        recipient.fingerprint.clone(),
        content.into(),
        Some(sender.clone()),
        recipient.enc_public_key.clone().unwrap(),
    ))
    .unwrap()
}

fn cache_profile(transport: &TcpSwarmTransport, profile: &SignedProfile) {
    transport
        .save_profile(
            &profile.profile.fingerprint,
            &SwarmProfileBlob {
                profile: profile.clone(),
                updated_at: unix_secs(),
            },
        )
        .unwrap();
}

fn app_at(root: &std::path::Path) -> App {
    App {
        panel: Panel::Messages,
        keypair: None,
        profile: None,
        local_posts: Vec::new(),
        contacts: Vec::new(),
        threads: Vec::new(),
        network: NetworkState::default(),
        forms: FormState::default(),
        storage: FileStorage::new(root.join("data")).unwrap(),
        transport: TcpSwarmTransport::for_test(&root.join("swarm")),
        lan_discovery: LanDiscovery::new(),
        gossip: None,
        discovered_peers: Vec::new(),
        gossip_peers: Vec::new(),
        revealed_message_ids: HashSet::new(),
        status_line: String::new(),
        syncing: false,
        loaded: true,
        saving_profile: false,
        sending: None,
        listener_error: None,
    }
}

#[test]
fn two_clients_exchange_encrypted_messages_and_refresh_cached_profiles() {
    let a_dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let a = TcpSwarmTransport::for_test(a_dir.path());
    let b = TcpSwarmTransport::for_test(b_dir.path());
    let a_addr = a.start_server().unwrap();
    let b_addr = b.start_server().unwrap();
    a.set_peers(vec![b_addr]);
    b.set_peers(vec![a_addr]);
    let (alice, alice_profile) = identity("alice");
    let (bob, bob_profile) = identity("bob");
    cache_profile(&a, &alice_profile);
    cache_profile(&b, &bob_profile);
    let first = encrypted(&alice, &bob, "Hello Bob 👋");
    let sent = sync::exchange(
        a.clone(),
        alice_profile.clone(),
        vec![],
        vec![contact(&bob_profile)],
        vec![first.clone()],
    );
    assert!(sent.relayed_ids.contains(&first.message.id));
    let received = sync::exchange(
        b.clone(),
        bob_profile.clone(),
        vec![],
        vec![contact(&alice_profile)],
        vec![],
    );
    assert_eq!(received.incoming.len(), 1);
    assert_eq!(
        decrypt_for_display(
            &received.incoming[0].1,
            Some(&bob),
            alice.enc_public_key.as_deref()
        )
        .unwrap(),
        "Hello Bob 👋"
    );
    let reply = encrypted(&bob, &alice, "Hi Alice!");
    sync::exchange(
        b.clone(),
        bob_profile.clone(),
        vec![],
        vec![contact(&alice_profile)],
        vec![reply],
    );
    let received = sync::exchange(
        a.clone(),
        alice_profile.clone(),
        vec![],
        vec![contact(&bob_profile)],
        vec![],
    );
    assert_eq!(
        decrypt_for_display(
            &received.incoming[0].1,
            Some(&alice),
            bob.enc_public_key.as_deref()
        )
        .unwrap(),
        "Hi Alice!"
    );
    // An existing cache must refresh when a peer edits their signed profile.
    let (_, updated) = block_on(create_profile_async(
        "bob".into(),
        Some("Bobby".into()),
        Some("New bio".into()),
        None,
        Some(bob),
        Some(bob_profile.clone()),
    ))
    .unwrap();
    cache_profile(&b, &updated);
    assert_eq!(
        a.load_profile(&updated.profile.fingerprint)
            .unwrap()
            .profile
            .profile
            .bio
            .as_deref(),
        Some("New bio")
    );
}

#[test]
fn inbox_merges_concurrent_writers_and_rejects_wrong_recipient() {
    let dir = tempfile::tempdir().unwrap();
    let transport = TcpSwarmTransport::for_test(dir.path());
    let (alice, alice_profile) = identity("alice");
    let (bob, _) = identity("bob");
    cache_profile(&transport, &alice_profile);
    let messages: Vec<_> = (0..8)
        .map(|n| encrypted(&alice, &bob, &format!("message {n}")))
        .collect();
    std::thread::scope(|scope| {
        for message in &messages {
            let transport = transport.clone();
            scope.spawn(move || {
                transport
                    .save_inbox(
                        &message.message.recipient_fingerprint,
                        &transport::SwarmInboxBlob {
                            messages: vec![message.clone()],
                            updated_at: 0,
                        },
                    )
                    .unwrap()
            });
        }
    });
    assert_eq!(
        transport
            .load_inbox(&bob.fingerprint)
            .unwrap()
            .messages
            .len(),
        8
    );
    let wrong = transport::SwarmInboxBlob {
        messages: vec![messages[0].clone()],
        updated_at: 0,
    };
    assert!(transport.save_inbox(&alice.fingerprint, &wrong).is_err());
    let mut tampered = messages[0].clone();
    tampered.message.content = "tampered".into();
    assert!(transport
        .save_inbox(
            &bob.fingerprint,
            &transport::SwarmInboxBlob {
                messages: vec![tampered],
                updated_at: 0
            }
        )
        .is_err());
    transport.save_inbox(&bob.fingerprint, &wrong).unwrap();
    assert_eq!(
        transport
            .load_inbox(&bob.fingerprint)
            .unwrap()
            .messages
            .len(),
        8
    );
}

#[test]
fn queued_ciphertext_survives_restart_and_retries_when_a_peer_appears() {
    let dir = tempfile::tempdir().unwrap();
    let b_dir = tempfile::tempdir().unwrap();
    let mut app = app_at(dir.path());
    let (alice, alice_profile) = identity("alice");
    let (bob, bob_profile) = identity("bob");
    app.keypair = Some(alice.clone());
    app.profile = Some(alice_profile.clone());
    app.contacts.push(contact(&bob_profile));
    app.select_chat(bob.fingerprint.clone());
    app.network.bittorrent_running = false;
    app.sending = Some((bob.fingerprint.clone(), "Keep this safe".into()));
    app.forms.compose_message_input = "Keep this safe".into();
    let message = encrypted(&alice, &bob, "Keep this safe");
    let _ = app.update(Message::ChatCreated(Ok(message.clone())));
    let disk = app.storage.get_item(STORAGE_THREADS).unwrap().unwrap();
    assert!(!disk.contains("Keep this safe"));
    let restored: Vec<ChatThread> = serde_json::from_str(&disk).unwrap();
    assert_eq!(restored[0].messages[0].delivery, DeliveryState::Queued);
    assert_eq!(app.forms.compose_message_input, "");
    assert!(!app.transport.relay_message(&message));
    let b = TcpSwarmTransport::for_test(b_dir.path());
    cache_profile(&b, &bob_profile);
    cache_profile(&app.transport, &alice_profile);
    app.transport.set_peers(vec![b.start_server().unwrap()]);
    let result = sync::exchange(
        app.transport.clone(),
        alice_profile,
        vec![],
        app.contacts.clone(),
        vec![restored[0].messages[0].envelope.clone().unwrap()],
    );
    let _ = app.apply_sync(result);
    assert_eq!(app.threads[0].messages[0].delivery, DeliveryState::Relayed);
    assert_eq!(b.load_inbox(&bob.fingerprint).unwrap().messages.len(), 1);
}

#[test]
fn drafts_and_async_send_completion_stay_with_their_contact() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_at(dir.path());
    let (alice, _) = identity("alice");
    let (bob, bob_profile) = identity("bob");
    let (carol, carol_profile) = identity("carol");
    app.contacts = vec![contact(&bob_profile), contact(&carol_profile)];
    app.select_chat(bob.fingerprint.clone());
    app.forms.compose_message_input = "For Bob".into();
    app.sending = Some((bob.fingerprint.clone(), "For Bob".into()));
    app.select_chat(carol.fingerprint.clone());
    app.forms.compose_message_input = "For Carol".into();
    let _ = app.update(Message::ChatCreated(Ok(encrypted(&alice, &bob, "For Bob"))));
    assert_eq!(app.forms.compose_message_input, "For Carol");
    app.select_chat(bob.fingerprint.clone());
    assert_eq!(app.forms.compose_message_input, "");
    app.select_chat(carol.fingerprint);
    assert_eq!(app.forms.compose_message_input, "For Carol");
}

#[test]
fn sync_delta_preserves_new_messages_and_counts_unread_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_at(dir.path());
    let (alice, _) = identity("alice");
    let (bob, bob_profile) = identity("bob");
    app.contacts.push(contact(&bob_profile));
    let inbound = ChatItem::from_signed(
        encrypted(&bob, &alice, "Incoming"),
        true,
        bob.enc_public_key.clone(),
    );
    let result = sync::SyncResult {
        incoming: vec![(bob.fingerprint.clone(), inbound)],
        ..Default::default()
    };
    let _ = app.apply_sync(result.clone());
    let _ = app.apply_sync(result);
    assert_eq!(app.threads[0].unread_count, 1);
    assert_eq!(app.threads[0].messages.len(), 1);
    app.select_chat(bob.fingerprint);
    assert_eq!(app.threads[0].unread_count, 0);
}

#[test]
fn invite_link_and_qr_image_import_preserve_connection_address() {
    let (_, profile) = identity("alice");
    let uri = ContactInvite::from_signed_profile(&profile, Some("127.0.0.1:47470".into()))
        .to_uri()
        .unwrap();
    let imported = block_on(import_invite_async(format!("  {uri}\n"))).unwrap();
    assert_eq!(imported.transport_addr.as_deref(), Some("127.0.0.1:47470"));
    assert_eq!(imported.verification, VerificationState::Unknown);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invite.png");
    qr_code_from_data(&uri)
        .unwrap()
        .render::<::image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(640, 640)
        .build()
        .save(&path)
        .unwrap();
    assert_eq!(read_qr_file(path.to_str().unwrap()).unwrap(), uri);
}

#[test]
fn invalid_messages_never_enter_a_thread() {
    let (alice, _) = identity("alice");
    let (bob, bob_profile) = identity("bob");
    let (other, _) = identity("other");
    let peer = contact(&bob_profile);
    let message = encrypted(&bob, &alice, "Verified");
    assert!(sync::accepts_message(&message, &peer, &alice.fingerprint));
    assert!(!sync::accepts_message(&message, &peer, &other.fingerprint));
    let mut tampered = message.clone();
    tampered.message.content = "tampered".into();
    assert!(!sync::accepts_message(&tampered, &peer, &alice.fingerprint));
    let mut unknown = peer;
    unknown.verification = VerificationState::Unknown;
    assert!(!sync::accepts_message(
        &message,
        &unknown,
        &alice.fingerprint
    ));
}

#[test]
fn unreadable_storage_is_reported_instead_of_starting_a_new_identity() {
    let dir = tempfile::tempdir().unwrap();
    let storage = FileStorage::new(dir.path()).unwrap();
    storage.set_item(STORAGE_KEYPAIR, "broken json").unwrap();
    assert!(load_startup(&storage).is_err());
}

#[test]
fn failed_message_persistence_keeps_the_draft_and_does_not_queue() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_at(dir.path());
    let (alice, _) = identity("alice");
    let (bob, bob_profile) = identity("bob");
    app.contacts.push(contact(&bob_profile));
    app.select_chat(bob.fingerprint.clone());
    app.forms.compose_message_input = "Keep my draft".into();
    app.sending = Some((bob.fingerprint.clone(), "Keep my draft".into()));
    // Make the destination a directory to force atomic replacement to fail on every OS.
    std::fs::create_dir(dir.path().join("data/threads.json")).unwrap();
    let _ = app.update(Message::ChatCreated(Ok(encrypted(
        &alice,
        &bob,
        "Keep my draft",
    ))));
    assert_eq!(app.forms.compose_message_input, "Keep my draft");
    assert!(app.threads[0].messages.is_empty());
    assert!(app.status_line.starts_with("Message not queued"));
}

/// Opt-in reproducible visual fixture; never writes to the user's real profile.
#[test]
#[ignore = "writes demo data to an explicitly supplied empty SNARTNET_REVIEW_DIR"]
fn create_visual_review_fixture() {
    let root =
        PathBuf::from(std::env::var_os("SNARTNET_REVIEW_DIR").expect("Set SNARTNET_REVIEW_DIR"));
    assert!(
        !root.exists(),
        "Use a fresh directory for the visual fixture"
    );
    let mut app = app_at(&root);
    let (alice, profile) = identity("alex");
    app.storage.set_json(STORAGE_KEYPAIR, &alice).unwrap();
    app.storage.set_json(STORAGE_PROFILE, &profile).unwrap();
    cache_profile(&app.transport, &profile);
    for (username, alias) in [
        ("maya", "Maya Chen"),
        ("jonas", "Jonas Berg"),
        ("elin", "Elin Strand"),
    ] {
        let (peer, profile) = identity(username);
        cache_profile(&app.transport, &profile);
        let mut person = contact(&profile);
        person.alias = alias.into();
        app.contacts.push(person);
        let mut messages = Vec::new();
        if username == "maya" {
            for (incoming, body) in [
                (
                    true,
                    "Hey Alex! Found a lovely trail by the lake. Up for a walk this weekend?",
                ),
                (false, "That sounds perfect. Saturday morning?"),
                (
                    true,
                    "Let's do it. I'll bring coffee, you bring the good stories ☕",
                ),
            ] {
                let signed = if incoming {
                    encrypted(&peer, &alice, body)
                } else {
                    encrypted(&alice, &peer, body)
                };
                let mut item = ChatItem::from_signed(signed, incoming, peer.enc_public_key.clone());
                item.delivery = DeliveryState::Relayed;
                messages.push(item);
            }
        }
        app.threads.push(ChatThread {
            contact_fingerprint: peer.fingerprint,
            messages,
            unread_count: 0,
        });
    }
    app.storage
        .set_json(STORAGE_CONTACTS, &app.contacts)
        .unwrap();
    app.storage.set_json(STORAGE_THREADS, &app.threads).unwrap();
}
