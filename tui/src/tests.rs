//! Tests for the daemon-backed terminal client (ADR 0001).
//!
//! The terminal client owns nothing but pixels and keystrokes: identity, storage,
//! torrents, and the peer listener all live in the daemon. These tests drive a
//! real daemon over its loopback API, then assert on the same view models,
//! reducer transitions, key mappings, and rendered frames the program uses.

use crate::app::{Action, App, Focus, Message, Tab};
use crate::daemon::Backend;
use crate::state::{short, Contact, DaemonState, DeliveryState, VerificationState};
use crate::{input, ui};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use snartnet_core::{KeyPair, Post, Profile, SignedPost};
use snartnet_sdk::{Client, Command, DaemonPaths, Snapshot, SyncMode, API_VERSION};
use std::time::{Duration, Instant};

/// One identity plus the JSON shape the daemon persists and republishes.
fn identity(username: &str) -> (KeyPair, Profile) {
    let keypair = KeyPair::generate().unwrap();
    let mut profile = Profile::new(username.to_string(), keypair.get_public_info());
    profile.display_name = Some(username.to_uppercase());
    profile.bio = Some(format!("{username} tests the terminal client"));
    (keypair, profile)
}

/// Runtime paths for a throwaway data directory, like the CLI resolves them.
fn paths_at(root: &std::path::Path) -> DaemonPaths {
    let data = root.join("data");
    DaemonPaths::from_data_dir(Some(data.to_str().expect("UTF-8 temp path"))).unwrap()
}

/// A frontend that has already received one snapshot, exactly like the first
/// frame after launch.
fn app_with(state: DaemonState) -> App {
    let mut app = App::new();
    app.dispatch(Message::snapshot(Ok(state)));
    app
}

/// A populated snapshot in the shape `Session::snapshot` produces, including the
/// status extras the Network tab renders.
fn populated_state() -> (DaemonState, Profile, Profile) {
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
        last_sync_label: "2 minutes ago".into(),
        profile_summary: "bob tests the terminal client".into(),
        latest_post_preview: "Bob's latest post".into(),
        verification: VerificationState::Verified,
        trust_score: 80,
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
            "peer": {"active": true, "node_id": "node-1", "peer_count": 4, "discovery": "dns-pkarr", "last_error": null},
            "delivery": {"durable": false, "failed": "disk is full", "spooled": 1},
            "syncMode": "paused",
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

#[test]
fn a_snapshot_becomes_the_view_models_the_terminal_renders() {
    let (state, profile, bob) = populated_state();
    assert_eq!(state.profile.as_ref().unwrap().username, "alice");
    let uri = state.identity_uri.as_deref().expect("the daemon builds it");
    assert!(uri.starts_with("snartnet://profile/"), "{uri}");
    assert_eq!(state.posts.len(), 1);
    assert_eq!(state.posts[0].post.content, "Haloj from the daemon");
    assert_eq!(state.contacts.len(), 1);
    assert_eq!(state.contact_label(&bob.fingerprint), "Bob");
    assert_eq!(state.contact_label("unknown"), short("unknown"));
    assert_eq!(state.total_unread(), 2);
    assert!(state.is_ready_to_message(&bob.fingerprint));
    assert_eq!(short(&profile.fingerprint).chars().count(), 13);

    let thread = state
        .thread(&bob.fingerprint)
        .expect("threads are keyed by fingerprint");
    assert!(state.thread("someone-else").is_none());
    assert_eq!(thread.messages[0].delivery, DeliveryState::Relayed);
    assert_eq!(thread.messages[1].delivery, DeliveryState::Available);
    assert_eq!(thread.messages[1].state_label(), "available");
    // A publication failure is part of the state a row shows, not a hidden detail (M7.5).
    assert_eq!(thread.messages[2].delivery, DeliveryState::Queued);
    assert!(thread.messages[2].delivery.is_pending());
    assert_eq!(thread.messages[2].state_label(), "queued (disk is full)");
    // The daemon decrypts for this frontend, and the stored payload stays visible.
    assert!(thread.messages[0].incoming && thread.messages[0].encrypted);
    assert_eq!(thread.messages[0].body(false), "Hej Bob");
    assert_eq!(thread.messages[0].body(true), "ciphertext-1");
    assert_eq!(
        thread.messages[1].body(false),
        "[cannot decrypt: missing peer encryption key]"
    );
    assert_eq!(thread.messages[0].created_label, "09:14");

    // The status extras the Network tab renders.
    let network = &state.network;
    assert_eq!(network.peers, 3);
    assert_eq!(network.sync_mode, SyncMode::Paused);
    assert!(network.paused, "pausing must be visible, not inferred");
    assert!(network.discovery);
    assert_eq!(network.last_sync, "10:02");
    assert_eq!(network.listening.as_deref(), Some("192.168.1.5:47470"));
    assert_eq!(network.subsystems.len(), 3);
    assert!(network.listener_error.is_none());
    // The delivery summary the Network tab renders explains a stuck message (M7.5).
    assert!(!network.delivery.durable);
    assert_eq!(network.delivery.failed.as_deref(), Some("disk is full"));
    assert_eq!(network.delivery.spooled, 1);
    assert_eq!(state.nearby[0].alias, "carol@laptop");
    assert_eq!(state.nearby[0].address.as_deref(), Some("10.0.0.9:47470"));
}

#[test]
fn the_first_snapshot_seeds_the_forms_and_later_ones_never_overwrite_typing() {
    let (state, profile, bob) = populated_state();
    let mut app = app_with(state.clone());
    assert!(app.loaded);
    assert_eq!(app.tab, Tab::Messages);
    assert_eq!(app.focus, Focus::None);
    assert_eq!(
        app.selected_contact.as_deref(),
        Some(bob.fingerprint.as_str())
    );
    assert_eq!(
        app.selected_message, 2,
        "the cursor opens on the newest message"
    );
    assert_eq!(app.form.username, profile.username);
    assert_eq!(app.form.display_name, "ALICE");
    assert_eq!(app.form.bio, "alice tests the terminal client");
    assert_eq!(app.form.address, "192.168.1.5:47470");
    assert!(app.status.contains("Connected"));

    // A later snapshot refreshes the view but never edits the user's fields.
    app.dispatch(Message::Focus(Focus::ProfileUsername));
    app.dispatch(Message::Insert('x'));
    app.dispatch(Message::snapshot(Ok(state.clone())));
    assert_eq!(app.form.username, "alicex");
    assert!(!app.connection_lost);

    // A daemon that goes away is reported, and recovery is visible too.
    app.dispatch(Message::snapshot(Err(
        "The SnartNet daemon is not running.".into(),
    )));
    assert!(app.connection_lost);
    assert!(app.status.contains("not running"));
    app.dispatch(Message::snapshot(Ok(state)));
    assert!(!app.connection_lost);
    assert!(app.status.contains("Reconnected"));
}

/// Two conversations and no profile, to exercise conversation switching.
fn two_conversations() -> DaemonState {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "apiVersion": API_VERSION,
        "revision": 1,
        "state": {
            "threads": [
                {"fingerprint": "aa", "unread": 0, "messages": []},
                {"fingerprint": "bb", "unread": 1, "messages": []}
            ]
        }
    }))
    .unwrap();
    DaemonState::from_snapshot(&snapshot).expect("the fixture parses")
}

#[test]
fn keys_never_steal_typing_from_a_focused_field() {
    let (state, _, bob) = populated_state();
    let mut app = app_with(state);
    let key = |code, modifiers| KeyEvent::new(code, modifiers);

    // With nothing focused, letters are shortcuts.
    assert!(matches!(
        input::translate(key(KeyCode::Char('q'), KeyModifiers::NONE), &app),
        Some(Message::Quit)
    ));
    assert!(matches!(
        input::translate(key(KeyCode::Char('3'), KeyModifiers::NONE), &app),
        Some(Message::SwitchTab(Tab::Feed))
    ));
    assert!(matches!(
        input::translate(key(KeyCode::Char('y'), KeyModifiers::NONE), &app),
        Some(Message::Sync)
    ));
    assert!(matches!(
        input::translate(key(KeyCode::Char('c'), KeyModifiers::CONTROL), &app),
        Some(Message::Quit)
    ));
    assert!(input::translate(key(KeyCode::Char('x'), KeyModifiers::CONTROL), &app).is_none());

    // With a field focused, the same letters are characters instead.
    app.focus = Focus::MessageCompose;
    for character in ['q', 'm', 'y', '1', '?'] {
        assert!(
            matches!(
                input::translate(key(KeyCode::Char(character), KeyModifiers::NONE), &app),
                Some(Message::Insert(inserted)) if inserted == character
            ),
            "{character} must be typed, not intercepted"
        );
    }
    assert!(matches!(
        input::translate(key(KeyCode::Char('c'), KeyModifiers::CONTROL), &app),
        Some(Message::Quit)
    ));
    assert!(matches!(
        input::translate(key(KeyCode::Enter, KeyModifiers::NONE), &app),
        Some(Message::Submit)
    ));
    assert!(matches!(
        input::translate(key(KeyCode::Esc, KeyModifiers::NONE), &app),
        Some(Message::Unfocus)
    ));

    // Help swallows everything except the keys that close it.
    app.help = true;
    app.focus = Focus::None;
    assert!(input::translate(key(KeyCode::Char('j'), KeyModifiers::NONE), &app).is_none());
    assert!(matches!(
        input::translate(key(KeyCode::Esc, KeyModifiers::NONE), &app),
        Some(Message::ToggleHelp)
    ));

    // Conversation keys walk the daemon's order and wrap at both ends.
    let mut app = app_with(two_conversations());
    assert_eq!(app.selected_contact.as_deref(), Some("aa"));
    assert!(matches!(
        input::translate(key(KeyCode::Char(']'), KeyModifiers::NONE), &app),
        Some(Message::SelectThread(next)) if next == "bb"
    ));
    app.dispatch(Message::SelectThread("bb".to_string()));
    assert!(matches!(
        input::translate(key(KeyCode::Right, KeyModifiers::NONE), &app),
        Some(Message::SelectThread(next)) if next == "aa"
    ));
    app.dispatch(Message::SelectThread("aa".to_string()));
    assert!(matches!(
        input::translate(key(KeyCode::Left, KeyModifiers::NONE), &app),
        Some(Message::SelectThread(next)) if next == "bb"
    ));
    // A tab without fields still publishes a focus target, just an empty one.
    app.tab = Tab::Network;
    assert!(matches!(
        input::translate(key(KeyCode::Char('i'), KeyModifiers::NONE), &app),
        Some(Message::Focus(Focus::None))
    ));
    assert!(!bob.fingerprint.is_empty());
}

#[test]
fn the_reducer_only_asks_the_daemon_for_what_a_key_means() {
    let (state, _, bob) = populated_state();
    let mut app = app_with(state);

    // Typing then Enter asks the daemon to send exactly what is in the field.
    app.dispatch(Message::Focus(Focus::MessageCompose));
    assert_eq!(app.focus, Focus::MessageCompose);
    for character in "Hej da".chars() {
        app.dispatch(Message::Insert(character));
    }
    let action = app
        .update(Message::Submit)
        .expect("the daemon must send it");
    match action {
        Action::Command(Command::Message { recipient, content }) => {
            assert_eq!(recipient, bob.fingerprint);
            assert_eq!(content, "Hej da");
        }
        other => panic!("unexpected action: {other:?}"),
    }
    // The field is cleared by the daemon's answer, not by the key press.
    assert_eq!(app.form.message, "Hej da");
    let refresh = app.update(Message::CommandFinished {
        command: Command::Message {
            recipient: bob.fingerprint.clone(),
            content: "Hej da".into(),
        },
        result: Ok(()),
    });
    assert!(matches!(refresh, Some(Action::Refresh)));
    assert!(app.status.contains("Message queued"));
    assert!(app.form.message.is_empty());

    // A rejected send keeps the draft and never claims success.
    app.dispatch(Message::Insert('!'));
    app.dispatch(Message::CommandFinished {
        command: Command::Message {
            recipient: bob.fingerprint.clone(),
            content: "!".into(),
        },
        result: Err("the daemon is not running".into()),
    });
    assert_eq!(app.form.message, "!");
    assert!(app.status.contains("Could not send the message"));

    // Opening a conversation clears its unread count through the daemon.
    assert!(matches!(
        app.update(Message::SelectThread(bob.fingerprint.clone())),
        Some(Action::Command(Command::Read { recipient })) if recipient == bob.fingerprint
    ));

    // Tabs, the help overlay, and quitting are local: no daemon call at all.
    assert!(app.update(Message::SwitchTab(Tab::Network)).is_none());
    assert_eq!(app.tab, Tab::Network);
    assert_eq!(app.focus, Focus::None);
    assert!(app.update(Message::ToggleHelp).is_none());
    assert!(app.help);
    assert!(app.update(Message::ToggleHelp).is_none());
    assert!(!app.help);
    assert!(app.update(Message::Quit).is_none());
    assert!(app.quit, "quitting the view must not stop the daemon");
}

#[test]
fn daemon_actions_map_one_to_one_and_stopping_needs_confirmation() {
    let (state, _, _) = populated_state();
    let mut app = app_with(state);

    assert!(matches!(
        app.update(Message::Refresh),
        Some(Action::Refresh)
    ));
    assert!(matches!(app.update(Message::Sync), Some(Action::Sync)));
    assert!(matches!(app.update(Message::Invite), Some(Action::Invite)));
    assert!(matches!(
        app.update(Message::StartDaemon),
        Some(Action::StartDaemon)
    ));
    assert!(matches!(
        app.update(Message::Cleanup),
        Some(Action::Command(Command::Cleanup))
    ));
    assert!(matches!(
        app.update(Message::ToggleDiscovery),
        Some(Action::Command(Command::Discovery { enabled: false }))
    ));

    // `m` cycles the mode the daemon reported, not a locally guessed one.
    assert_eq!(app.mode(), SyncMode::Paused);
    assert!(matches!(
        app.update(Message::CycleMode),
        Some(Action::SetMode(SyncMode::AlwaysOn))
    ));
    // A refused mode change must not show the mode that was asked for.
    app.dispatch(Message::ModeChanged {
        requested: SyncMode::AlwaysOn,
        result: Err("unknown mode".into()),
    });
    assert_eq!(app.mode(), SyncMode::Paused);
    assert!(app.status.contains("Could not set the sync mode"));
    app.dispatch(Message::ModeChanged {
        requested: SyncMode::AlwaysOn,
        result: Ok(SyncMode::AlwaysOn),
    });
    assert_eq!(app.mode(), SyncMode::AlwaysOn);

    // One press asks for confirmation, the second one stops the daemon.
    assert!(app.update(Message::StopDaemon).is_none());
    assert!(app.status.contains("Press S again"));
    assert!(matches!(
        app.update(Message::StopDaemon),
        Some(Action::StopDaemon)
    ));
    app.dispatch(Message::DaemonStopped(Ok(())));
    assert!(app.status.contains("Press D to start it again"));

    // Failures from the daemon are always visible, never silently swallowed.
    app.dispatch(Message::Synced(Err("the daemon is paused".into())));
    assert!(app.status.contains("Sync failed"));
    app.dispatch(Message::Invitation(Err("no identity yet".into())));
    assert!(app.status.contains("Could not build the invitation link"));
    app.dispatch(Message::DaemonStarted(Err(
        "no daemon beside this program".into()
    )));
    assert!(app.status.contains("Could not start the daemon"));
    app.dispatch(Message::DaemonStopped(Err("not running".into())));
    assert!(app.status.contains("Could not stop the daemon"));
    app.dispatch(Message::CommandFinished {
        command: Command::Profile {
            username: "alice".into(),
            display_name: String::new(),
            bio: String::new(),
            avatar: String::new(),
            address: String::new(),
        },
        result: Err("that username is already taken".into()),
    });
    assert!(app.status.contains("Could not publish the profile"));
}

/// The visible frame as text: exactly the characters the terminal shows.
fn rendered(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn every_tab_renders_from_daemon_state_alone() {
    let (state, _, _) = populated_state();
    let mut app = app_with(state);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    // Before the first snapshot the view says so instead of inventing content.
    app.loaded = false;
    app.state = DaemonState::default();
    for tab in Tab::ALL {
        app.tab = tab;
        terminal.draw(|frame| ui::render(frame, &app)).unwrap();
        let screen = rendered(&terminal);
        assert!(!screen.trim().is_empty(), "{tab:?} drew nothing");
    }

    let (state, _, _) = populated_state();
    app.dispatch(Message::snapshot(Ok(state)));

    app.tab = Tab::Messages;
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    let screen = rendered(&terminal);
    assert!(screen.contains("Bob"), "the open conversation is titled");
    assert!(
        screen.contains("Hej Bob"),
        "the daemon's plaintext is drawn"
    );
    assert!(!screen.contains("ciphertext-1"), "ciphertext stays hidden");

    app.tab = Tab::Feed;
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    let screen = rendered(&terminal);
    assert!(screen.contains("Haloj from the daemon"), "{screen}");

    app.tab = Tab::Contacts;
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    let screen = rendered(&terminal);
    assert!(
        screen.contains("Bob") && screen.contains("verified"),
        "{screen}"
    );

    app.tab = Tab::Profile;
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    let screen = rendered(&terminal);
    assert!(screen.contains("snartnet://profile/"), "{screen}");

    app.tab = Tab::Network;
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    let screen = rendered(&terminal);
    assert!(screen.contains("192.168.1.5:47470"), "{screen}");
    assert!(
        screen.contains("carol@laptop"),
        "discovered peers are listed"
    );

    // Revealing a payload swaps in the stored ciphertext, nothing else.
    app.tab = Tab::Messages;
    app.selected_message = 0;
    app.dispatch(Message::ToggleCiphertext);
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    let screen = rendered(&terminal);
    assert!(screen.contains("ciphertext-1"), "{screen}");
    assert!(!screen.contains("Hej Bob"), "{screen}");

    app.help = true;
    terminal.draw(|frame| ui::render(frame, &app)).unwrap();
    assert!(rendered(&terminal).contains("SnartNet"), "help is titled");
}

/// Waits until the daemon answers, or fails the test with the reason.
fn wait_for_daemon(client: &Client) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if client.health().is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the daemon never answered on its loopback API"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Executes one action exactly the way `main`'s worker does, so these tests cover
/// the same path the program runs.
fn perform(backend: &Backend, action: Action) -> Message {
    match action {
        Action::Refresh => Message::snapshot(
            backend
                .snapshot()
                .and_then(|snapshot| DaemonState::from_snapshot(&snapshot)),
        ),
        Action::Sync => Message::Synced(backend.sync()),
        Action::SetMode(requested) => Message::ModeChanged {
            requested,
            result: backend.set_sync_mode(requested),
        },
        Action::Command(command) => Message::CommandFinished {
            result: backend.command(command.clone()),
            command,
        },
        Action::Invite => Message::Invitation(backend.invite()),
        Action::StartDaemon => Message::DaemonStarted(backend.ensure_running()),
        Action::StopDaemon => Message::DaemonStopped(backend.stop()),
    }
}

/// One input or result through the whole loop: reducer, daemon, reducer.
///
/// Terminates because every daemon result settles to a snapshot, and a snapshot
/// asks for nothing.
fn press(backend: &Backend, app: &mut App, message: Message) {
    let mut message = message;
    while let Some(action) = app.update(message) {
        message = perform(backend, action);
    }
}

/// The contract that matters most (ADR 0001): everything the view shows and
/// everything it changes goes through a real daemon over the local API.
#[test]
fn the_terminal_reads_and_writes_only_through_a_real_daemon() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths_at(root.path());
    let server_paths = paths.clone();
    let server = std::thread::spawn(move || {
        let bind = "127.0.0.1:0".parse().expect("loopback bind");
        snartnet_daemon::run_with(server_paths, 0, bind)
    });
    let client = Client::new(paths.clone()).unwrap();
    wait_for_daemon(&client);
    // The view enters through the same door as these tests: the SDK.
    let backend = Backend::connect_to(paths.clone()).unwrap();

    // A fresh data directory has no identity yet, so the view says how to start.
    let mut app = App::new();
    press(&backend, &mut app, Message::Refresh);
    assert!(app.loaded);
    assert!(app.state.profile.is_none());
    assert!(app.state.posts.is_empty());
    assert!(app.state.contacts.is_empty());
    assert_eq!(app.state.network.last_sync, "never");
    assert!(!app.state.network.paused, "the daemon starts unpaused");
    assert_eq!(app.mode(), SyncMode::Balanced);
    assert!(app.status.contains("no identity yet"), "{}", app.status);
    assert!(app.state.identity_uri.is_none());

    // Creating the profile is typing plus Enter; no key material is made here.
    press(&backend, &mut app, Message::Focus(Focus::ProfileUsername));
    for character in "alice".chars() {
        press(&backend, &mut app, Message::Insert(character));
    }
    press(&backend, &mut app, Message::Submit);
    let profile = app
        .state
        .profile
        .as_ref()
        .expect("the daemon owns the identity");
    assert_eq!(profile.username, "alice");
    assert_eq!(app.form.username, "alice", "the form keeps the typed name");
    let fingerprint = profile.fingerprint.clone();
    let uri = app
        .state
        .identity_uri
        .clone()
        .expect("the daemon builds the identity link");
    assert!(uri.starts_with("snartnet://profile/"), "{uri}");
    assert!(app.status.contains("Profile published"), "{}", app.status);

    // Publishing a post is a command, and the feed renders the daemon's copy.
    press(&backend, &mut app, Message::SwitchTab(Tab::Feed));
    press(&backend, &mut app, Message::Focus(Focus::PostContent));
    for character in "Haloj from the terminal".chars() {
        press(&backend, &mut app, Message::Insert(character));
    }
    press(&backend, &mut app, Message::Submit);
    assert!(app.form.post.is_empty(), "a committed draft is cleared");
    assert_eq!(app.state.posts.len(), 1);
    assert_eq!(app.state.posts[0].post.content, "Haloj from the terminal");
    assert_eq!(app.state.posts[0].post.author_fingerprint, fingerprint);

    // Invitation links, magnet included, stay daemon-generated.
    press(&backend, &mut app, Message::Invite);
    let invite = app.invitation.as_ref().expect("the daemon answers");
    assert!(
        invite.uri.starts_with("snartnet://invite/"),
        "{}",
        invite.uri
    );
    if let Some(magnet) = invite.magnet.as_deref() {
        assert!(magnet.starts_with("magnet:?xt="), "{magnet}");
    }

    // A rejected contact import is the daemon's answer, and it keeps the paste.
    press(&backend, &mut app, Message::SwitchTab(Tab::Contacts));
    press(&backend, &mut app, Message::Focus(Focus::ContactInput));
    for character in "not-a-fingerprint".chars() {
        press(&backend, &mut app, Message::Insert(character));
    }
    press(&backend, &mut app, Message::Submit);
    assert!(
        app.state.contacts.is_empty(),
        "the daemon validated the input"
    );
    assert!(
        app.status.contains("Could not add the contact"),
        "{}",
        app.status
    );
    assert_eq!(app.form.contact_input, "not-a-fingerprint");

    // The daemon is stopped deliberately, so this test does not leak one.
    press(&backend, &mut app, Message::StopDaemon);
    press(&backend, &mut app, Message::StopDaemon);
    server.join().unwrap().expect("the daemon exits cleanly");
}

/// The daemon outlives this view, and its mode is the daemon's to decide.
#[test]
fn the_daemon_outlives_the_view_and_owns_its_mode() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths_at(root.path());
    let server_paths = paths.clone();
    let server = std::thread::spawn(move || {
        let bind = "127.0.0.1:0".parse().expect("loopback bind");
        snartnet_daemon::run_with(server_paths, 0, bind)
    });
    let client = Client::new(paths.clone()).unwrap();
    wait_for_daemon(&client);
    let backend = Backend::connect_to(paths.clone()).unwrap();

    let mut app = App::new();
    press(&backend, &mut app, Message::Refresh);
    assert_eq!(app.mode(), SyncMode::Balanced);
    assert!(app.state.contacts.is_empty());

    // `m` cycles the mode, and the view shows what the daemon accepted.
    press(&backend, &mut app, Message::CycleMode);
    assert_eq!(app.mode(), SyncMode::Paused);
    assert!(app.state.network.paused, "pausing must be visible");
    assert!(app.status.contains("paused"), "{}", app.status);

    // A paused daemon refuses explicit syncs, and the view says so.
    press(&backend, &mut app, Message::Sync);
    assert!(app.status.starts_with("Sync failed"), "{}", app.status);
    assert!(!app.connection_lost, "a refusal is not an outage");

    // Cycling onward puts the daemon back to work, and the view shows the mode
    // the daemon reported. (A live sync needs peers, so the daemon tests cover
    // the transfer itself.)
    press(&backend, &mut app, Message::CycleMode);
    assert_eq!(app.mode(), SyncMode::AlwaysOn);
    assert!(!app.state.network.paused);

    // Discovery and cache cleanup are daemon commands. Whether LAN discovery is
    // actually running needs an identity and a usable interface, so these assert
    // the daemon's answer instead of a flag this machine may not support.
    press(&backend, &mut app, Message::ToggleDiscovery);
    assert!(app.status.starts_with("Discovery turned"), "{}", app.status);
    press(&backend, &mut app, Message::Cleanup);
    assert!(app.status.contains("cleaned up"), "{}", app.status);

    // Quitting this view leaves the daemon doing its background work.
    press(&backend, &mut app, Message::Quit);
    assert!(app.quit);
    assert!(
        backend.snapshot().is_ok(),
        "quitting the view must not touch the daemon"
    );

    // Stopping the daemon is deliberate: one press warns, the second stops it.
    press(&backend, &mut app, Message::StopDaemon);
    assert!(app.status.contains("Press S again"), "{}", app.status);
    assert!(backend.snapshot().is_ok(), "the first press stops nothing");
    press(&backend, &mut app, Message::StopDaemon);
    assert!(
        app.status.contains("Press D to start it again"),
        "{}",
        app.status
    );
    server.join().unwrap().expect("the daemon exits cleanly");

    // With the daemon gone the view reports the outage instead of pretending, and
    // keeps the last snapshot it received on screen.
    let before_stop = app.state.network.last_sync.clone();
    press(&backend, &mut app, Message::Refresh);
    assert!(app.connection_lost);
    assert!(app.status.contains("not running"), "{}", app.status);
    assert_eq!(app.state.network.last_sync, before_stop);
    assert!(
        backend.command(Command::Cleanup).is_err(),
        "the daemon is really gone"
    );
}

#[test]
fn narrow_terminals_still_render_every_tab_and_the_help_overlay() {
    let (state, _, _) = populated_state();
    let mut app = app_with(state);
    // A terminal can be resized to anywhere, including a split pane a few cells wide.
    for (width, height) in [(80, 24), (60, 18), (40, 12), (24, 8), (12, 5), (4, 3)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for tab in Tab::ALL {
            app.tab = tab;
            app.help = false;
            terminal
                .draw(|frame| ui::render(frame, &app))
                .unwrap_or_else(|error| panic!("{width}x{height} {tab:?}: {error}"));
            app.help = true;
            terminal
                .draw(|frame| ui::render(frame, &app))
                .unwrap_or_else(|error| panic!("{width}x{height} help: {error}"));
        }
        if width >= 40 && height >= 12 {
            app.help = false;
            app.tab = Tab::Network;
            terminal.draw(|frame| ui::render(frame, &app)).unwrap();
            // Long daemon values are wrapped or clipped, never lost to a panic.
            let screen = rendered(&terminal);
            assert!(screen.contains("Daemon"), "{width}x{height}: {screen}");
            assert!(screen.contains("Sync mode"), "{width}x{height}: {screen}");
            assert!(
                width < 60 || screen.contains("Last sync"),
                "{width}x{height}: {screen}"
            );
        }
    }
}
