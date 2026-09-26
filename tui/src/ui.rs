//! Ratatui rendering for the daemon-backed terminal client (ADR 0001).
//!
//! Every frame is derived from `App`, which only ever holds what the daemon
//! reported plus the user's own uncommitted input. Nothing here caches state.

use crate::app::{App, Focus, Tab};
use crate::state::{short, Contact, MessageView, NearbyPeer};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const DIMMED: Color = Color::DarkGray;
const WARNING: Color = Color::Yellow;

pub(crate) fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(frame.area());
    frame.render_widget(tabs(app), chunks[0]);
    match app.tab {
        Tab::Messages => messages(frame, app, chunks[1]),
        Tab::Contacts => contacts(frame, app, chunks[1]),
        Tab::Feed => feed(frame, app, chunks[1]),
        Tab::Profile => profile(frame, app, chunks[1]),
        Tab::Network => network(frame, app, chunks[1]),
    }
    frame.render_widget(status_line(app), chunks[2]);
    frame.render_widget(hints(app), chunks[3]);
    if app.help {
        help_overlay(frame);
    }
}

/// Numbered tabs, so `1`–`5` in the hint bar match what is on screen.
fn tabs(app: &App) -> Tabs<'static> {
    let unread = app.state.total_unread();
    let titles: Vec<Line<'static>> = Tab::ALL
        .iter()
        .map(|tab| {
            let title = if *tab == Tab::Messages && unread > 0 {
                format!(" {} {} ({unread}) ", tab.index() + 1, tab.title())
            } else {
                format!(" {} {} ", tab.index() + 1, tab.title())
            };
            Line::from(title)
        })
        .collect();
    Tabs::new(titles)
        .select(app.tab.index())
        .divider("·")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )
}

fn status_line(app: &App) -> Paragraph<'static> {
    let text = if !app.status.is_empty() {
        app.status.clone()
    } else if app.loaded {
        String::new()
    } else {
        "Connecting to the SnartNet daemon…".to_string()
    };
    let style = if app.connection_lost {
        Style::default().fg(WARNING)
    } else {
        Style::default().fg(ACCENT)
    };
    Paragraph::new(Line::from(text)).style(style)
}

/// The hint bar always shows what the current tab and focus can do.
fn hints(app: &App) -> Paragraph<'static> {
    if app.help {
        return Paragraph::new(Line::from("Press ? or Esc to close this help."))
            .style(Style::default().fg(DIMMED));
    }
    let text = if app.focus != Focus::None {
        format!(
            "typing in the {} field · Enter submit · Esc stop editing · Tab next field · ↑↓ scroll",
            app.focus.label()
        )
    } else {
        let shared = "r refresh · y sync now · m sync mode · d discovery · c cleanup · u invite · D start daemon · S stop daemon · 1-5 tabs · ? help · q quit";
        match app.tab {
            Tab::Messages => format!(
                "↑↓ message · ←→ conversation · Enter/i write · x stored payload · {shared}"
            ),
            Tab::Contacts => format!("↑↓ move · Enter open chat · i write · {shared}"),
            Tab::Feed => format!("↑↓ move · Enter/i write · {shared}"),
            Tab::Profile => format!("i write · {shared}"),
            Tab::Network => shared.to_string(),
        }
    };
    Paragraph::new(Line::from(text)).style(Style::default().fg(DIMMED))
}

fn messages(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.current_contact() {
        Some(contact) => format!(
            " Conversation with {} · {} ",
            contact.label(),
            contact.verification.label()
        ),
        None => " Conversation ".to_string(),
    };
    let block = Block::bordered().title(title);
    if app.selected_contact.is_none() {
        frame.render_widget(
            paragraph(
                vec![Line::from(
                    "No conversation yet. Add a contact on the Contacts tab (2), then press Enter on it.",
                )],
                block,
            ),
            area,
        );
        return;
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = message_lines(app);
    // Keep the highlighted message visible; one row per message per screen row.
    let height = inner.height.max(1) as usize;
    let offset = app.selected_message.saturating_sub(height - 1) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((offset, 0)),
        inner,
    );
}

fn message_lines(app: &App) -> Vec<Line<'static>> {
    let Some(thread) = app.current_thread() else {
        return Vec::new();
    };
    if thread.messages.is_empty() {
        return vec![Line::from(
            "No messages yet. Press i and type the first one.",
        )];
    }
    thread
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            message_line(
                index == app.selected_message,
                message,
                app.reveal(&message.id),
            )
        })
        .collect()
}

/// One message row: direction, body (or the stored payload), delivery, time.
fn message_line(selected: bool, message: &MessageView, revealed: bool) -> Line<'static> {
    let marker = if selected { "▶" } else { " " };
    let (arrow, colour) = if message.incoming {
        ("←", ACCENT)
    } else {
        ("→", Color::Green)
    };
    let state = if message.encrypted {
        format!("encrypted · {}", message.state_label())
    } else {
        message.state_label()
    };
    // A message that is still waiting (or whose copy could not be published) stands out, so a
    // stuck queue is visible in the list rather than only in the log (M7.5).
    let state_style = if message.delivery.is_pending() {
        Style::default().fg(WARNING)
    } else {
        Style::default().fg(DIMMED)
    };
    let line = Line::from(vec![
        Span::styled(format!("{marker} {arrow} "), Style::default().fg(colour)),
        Span::raw(message.body(revealed)),
        Span::styled(
            format!("  [{state} · {}]", message.created_label),
            state_style,
        ),
    ]);
    if selected {
        line.style(Style::default().add_modifier(Modifier::BOLD))
    } else {
        line
    }
}

fn contacts(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(4)]).split(area);
    let title = format!(" Contacts ({}) ", app.state.contacts.len());
    let items: Vec<ListItem<'static>> = if app.state.contacts.is_empty() {
        vec![ListItem::new(
            "No contacts yet. Paste an invite link, a magnet link, or a fingerprint below.",
        )]
    } else {
        app.state
            .contacts
            .iter()
            .enumerate()
            .map(|(index, contact)| contact_item(index == app.contacts_index, app, contact))
            .collect()
    };
    frame.render_widget(list(items, title), chunks[0]);

    let fields = vec![
        field_line(
            "Invite / magnet / fingerprint",
            &app.form.contact_input,
            app.focus == Focus::ContactInput,
        ),
        field_line(
            "Alias (optional)",
            &app.form.contact_alias,
            app.focus == Focus::ContactAlias,
        ),
    ];
    frame.render_widget(
        paragraph(
            fields,
            Block::bordered().title(" Add a contact · i to write · Enter to add "),
        )
        .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn feed(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(4)]).split(area);
    let title = format!(" Feed ({}) ", app.state.posts.len());
    let items: Vec<ListItem<'static>> = if app.state.posts.is_empty() {
        vec![ListItem::new(
            "No posts yet. Write one below, or sync with a peer after adding contacts.",
        )]
    } else {
        app.state
            .posts
            .iter()
            .enumerate()
            .map(|(index, signed)| {
                let author = app.state.contact_label(&signed.post.author_fingerprint);
                let header = Line::from(vec![
                    Span::styled(
                        format!("{author} "),
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        signed
                            .post
                            .created_at
                            .format("%Y-%m-%d %H:%M UTC")
                            .to_string(),
                        Style::default().fg(DIMMED),
                    ),
                ]);
                let mut lines = vec![header, Line::from(signed.post.content.clone())];
                if !signed.post.tags.is_empty() {
                    lines.push(Line::from(Span::styled(
                        signed
                            .post
                            .tags
                            .iter()
                            .map(|tag| format!("#{tag}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                        Style::default().fg(DIMMED),
                    )));
                }
                let item = ListItem::new(lines);
                if index == app.feed_index {
                    item.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    item
                }
            })
            .collect()
    };
    frame.render_widget(list(items, title), chunks[0]);

    frame.render_widget(
        paragraph(
            vec![field_line(
                "Post",
                &app.form.post,
                app.focus == Focus::PostContent,
            )],
            Block::bordered().title(" New post · i to write · Enter to publish "),
        )
        .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn profile(frame: &mut Frame, app: &App, area: Rect) {
    let form_height = Focus::fields(Tab::Profile).len() as u16 + 2;
    let chunks =
        Layout::vertical([Constraint::Min(5), Constraint::Length(form_height)]).split(area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    match &app.state.profile {
        Some(profile) => {
            let name = profile
                .display_name
                .clone()
                .unwrap_or_else(|| profile.username.clone());
            lines.push(Line::from(Span::styled(
                name,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(format!("@{}", profile.username)));
            if let Some(bio) = &profile.bio {
                lines.push(Line::from(bio.clone()));
            }
            lines.push(Line::from(Span::styled(
                format!(
                    "fingerprint {} · created {}",
                    short(&profile.fingerprint),
                    profile.created_at.format("%Y-%m-%d")
                ),
                Style::default().fg(DIMMED),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "encryption key {}",
                    profile
                        .encryption_public_key
                        .as_deref()
                        .map(short)
                        .unwrap_or_else(|| "not published".to_string())
                ),
                Style::default().fg(DIMMED),
            )));
            if let Some(uri) = &app.state.identity_uri {
                lines.push(Line::from(vec![
                    Span::styled("identity ", Style::default().fg(DIMMED)),
                    Span::raw(uri.clone()),
                ]));
            }
            if let Some(magnet) = &profile.magnet_uri {
                lines.push(Line::from(vec![
                    Span::styled("magnet ", Style::default().fg(DIMMED)),
                    Span::raw(magnet.clone()),
                ]));
            }
        }
        None => lines.push(Line::from(
            "This daemon has no identity yet. Fill in a username below and press Enter to create one.",
        )),
    }
    match &app.invitation {
        Some(invite) => {
            lines.push(Line::from(vec![
                Span::styled("invite ", Style::default().fg(DIMMED)),
                Span::raw(invite.uri.clone()),
            ]));
            if let Some(magnet) = &invite.magnet {
                lines.push(Line::from(vec![
                    Span::styled("invite magnet ", Style::default().fg(DIMMED)),
                    Span::raw(magnet.clone()),
                ]));
            }
        }
        None => lines.push(Line::from(Span::styled(
            "Press u for a fresh invitation link (the key stays inside the daemon).",
            Style::default().fg(DIMMED),
        ))),
    }
    frame.render_widget(
        paragraph(lines, Block::bordered().title(" Profile ")).wrap(Wrap { trim: false }),
        chunks[0],
    );

    let fields = vec![
        field_line(
            "Username",
            &app.form.username,
            app.focus == Focus::ProfileUsername,
        ),
        field_line(
            "Display name",
            &app.form.display_name,
            app.focus == Focus::ProfileDisplayName,
        ),
        field_line("Bio", &app.form.bio, app.focus == Focus::ProfileBio),
        field_line(
            "Advertised address",
            &app.form.address,
            app.focus == Focus::ProfileAddress,
        ),
    ];
    frame.render_widget(
        paragraph(
            fields,
            Block::bordered().title(" Edit profile · i to write · Enter to publish "),
        )
        .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn network(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(6), Constraint::Min(4)]).split(area);
    let status = &app.state.network;
    let mut lines = vec![
        kv(
            "Daemon",
            if app.connection_lost {
                "unreachable".to_string()
            } else if app.loaded {
                "connected".to_string()
            } else {
                "connecting".to_string()
            },
        ),
        kv(
            "Sync mode",
            if status.paused {
                format!(
                    "{} (scheduler paused)",
                    crate::app::mode_label(status.sync_mode)
                )
            } else {
                crate::app::mode_label(status.sync_mode).to_string()
            },
        ),
        kv("Discovery", if status.discovery { "on" } else { "off" }),
        kv("Connected peers", status.peers.to_string()),
        kv("Last sync", status.last_sync.clone()),
    ];
    if let Some(listening) = &status.listening {
        lines.push(kv("Listening on", listening.clone()));
    }
    if !status.address.is_empty() {
        lines.push(kv("Advertised address", status.address.clone()));
    }
    if let Some(error) = &status.listener_error {
        lines.push(Line::from(vec![
            Span::styled("Listener error: ", Style::default().fg(WARNING)),
            Span::styled(error.clone(), Style::default().fg(WARNING)),
        ]));
    }
    lines.push(kv(
        "Durable publication",
        if status.delivery.durable {
            "available".to_string()
        } else {
            "unavailable on this host".to_string()
        },
    ));
    if let Some(error) = &status.delivery.failed {
        lines.push(Line::from(vec![
            Span::styled("Publish error: ", Style::default().fg(WARNING)),
            Span::styled(error.clone(), Style::default().fg(WARNING)),
        ]));
    }
    if status.delivery.spooled > 0 {
        lines.push(kv(
            "Spooled inbound",
            format!(
                "{} object(s) stored before acknowledgement",
                status.delivery.spooled
            ),
        ));
    }
    for (name, summary) in &status.subsystems {
        lines.push(kv(name, summary.clone()));
    }
    frame.render_widget(
        paragraph(lines, Block::bordered().title(" Daemon and network "))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let title = format!(" Nearby peers ({}) ", app.state.nearby.len());
    let items: Vec<ListItem<'static>> = if app.state.nearby.is_empty() {
        vec![ListItem::new(
            "No peers seen on the local network yet. Discovery must be on and a peer must be running.",
        )]
    } else {
        app.state
            .nearby
            .iter()
            .map(|peer| ListItem::new(nearby_line(peer)))
            .collect()
    };
    frame.render_widget(list(items, title), chunks[1]);
}

fn nearby_line(peer: &NearbyPeer) -> Line<'static> {
    let name = if peer.alias.is_empty() {
        short(&peer.fingerprint)
    } else {
        format!("{} ({})", peer.alias, short(&peer.fingerprint))
    };
    Line::from(vec![
        Span::styled(name, Style::default().fg(ACCENT)),
        Span::styled(
            format!(
                "  {}",
                peer.address
                    .clone()
                    .unwrap_or_else(|| "no address".to_string())
            ),
            Style::default().fg(DIMMED),
        ),
    ])
}

fn help_overlay(frame: &mut Frame) {
    let area = centered(frame.area(), 72, 20);
    frame.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled(
            "SnartNet terminal client",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        kv("1-5, Tab", "switch tabs"),
        kv("? / F1", "toggle this help"),
        kv("i, Enter", "start writing in this tab's field"),
        kv("Esc", "stop editing"),
        kv("Enter", "submit the field, or open the highlighted row"),
        kv("↑↓ j k, PgUp PgDn, g G", "move the cursor"),
        kv("←→, [ ]", "previous or next conversation"),
        kv("x", "show the stored payload of the selected message"),
        kv("r, y", "refresh the view, or sync with peers now"),
        kv("m", "cycle the sync mode (always on, balanced, paused)"),
        kv(
            "d, c, u",
            "toggle discovery, clean up caches, build an invite",
        ),
        kv("D / S", "start or stop the daemon (q only quits this view)"),
        kv("q, Ctrl+C", "quit this view; the daemon keeps running"),
    ];
    frame.render_widget(
        paragraph(lines, Block::bordered().title(" Help ")).wrap(Wrap { trim: false }),
        area,
    );
}

fn list<'a>(items: Vec<ListItem<'a>>, title: String) -> List<'a> {
    List::new(items).block(Block::bordered().title(title))
}

fn paragraph<'a>(lines: Vec<Line<'a>>, block: Block<'a>) -> Paragraph<'a> {
    Paragraph::new(lines).block(block)
}

fn kv(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(DIMMED)),
        Span::raw(value.into()),
    ])
}

/// One contact row: name, verification, trust, and why sending may be blocked.
fn contact_item(selected: bool, app: &App, contact: &Contact) -> ListItem<'static> {
    let unread = app
        .state
        .thread(&contact.fingerprint)
        .map_or(0, |thread| thread.unread_count);
    let badge = if unread > 0 {
        format!("  {unread} unread")
    } else {
        String::new()
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                contact.label().to_string(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {} · trust {}{badge}",
                    contact.verification.label(),
                    contact.trust_score
                ),
                Style::default().fg(DIMMED),
            ),
        ]),
        Line::from(Span::styled(
            format!("  {}", short(&contact.fingerprint)),
            Style::default().fg(DIMMED),
        )),
    ];
    let mut detail: Vec<String> = Vec::new();
    if !contact.profile_summary.is_empty() {
        detail.push(contact.profile_summary.clone());
    }
    if !contact.latest_post_preview.is_empty() {
        detail.push(format!("latest: {}", contact.latest_post_preview));
    }
    if !contact.last_sync_label.is_empty() {
        detail.push(format!("last sync {}", contact.last_sync_label));
    }
    if detail.is_empty() {
        detail.push("no profile synced yet".to_string());
    }
    lines.push(Line::from(Span::styled(
        format!("  {}", detail.join(" · ")),
        Style::default().fg(DIMMED),
    )));
    if !contact.ready_to_message() {
        lines.push(Line::from(Span::styled(
            "  cannot send yet: no verified encryption key from this peer",
            Style::default().fg(WARNING),
        )));
    }
    if let Some(error) = &contact.last_sync_error {
        lines.push(Line::from(Span::styled(
            format!("  last sync failed: {error}"),
            Style::default().fg(WARNING),
        )));
    }
    let item = ListItem::new(lines);
    if selected {
        item.style(Style::default().add_modifier(Modifier::REVERSED))
    } else {
        item
    }
}

/// A labelled input row, with a cursor mark on the focused field.
fn field_line(label: &str, value: &str, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIMMED)
    };
    let mut spans = vec![
        Span::styled(
            format!("{} {label}: ", if focused { "▶" } else { " " }),
            label_style,
        ),
        Span::raw(value.to_string()),
    ];
    if focused {
        spans.push(Span::styled("▏", Style::default().fg(ACCENT)));
    }
    Line::from(spans)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}
