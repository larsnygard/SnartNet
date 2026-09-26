//! Native screens. Views build widgets only; network operations run in `sync`.
use super::*;
use design::{avatar, card, contact_avatar, mark, muted, ACCENT, INK, MUTED, NAVY, TINT};
use iced::widget::column;
use iced::{
    widget::{horizontal_space, vertical_space},
    Color,
};

impl App {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        if !self.loaded {
            let mut loader = column![
                mark(72.0),
                text("Opening your space").size(26),
                text(self.status_line.clone()).size(15)
            ]
            .spacing(20);
            // The status line promises this button, and a frontend that cannot
            // reach its daemon must always offer a way back in.
            if self.daemon_unreachable {
                loader = loader.push(
                    button("Start the daemon")
                        .padding([12, 20])
                        .style(button::primary)
                        .on_press(Message::StartDaemon),
                );
            }
            return container(loader)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        let content = match self.panel {
            Panel::Messages => self.view_messages(),
            Panel::Contacts => self.view_contacts(),
            Panel::Profile => self.view_profile(),
            Panel::Feed => self.view_feed(),
            Panel::Network => self.view_network(),
        };
        let title = match self.panel {
            Panel::Messages => "Messages",
            Panel::Contacts => "Your people",
            Panel::Profile => "Your profile",
            Panel::Feed => "Community feed",
            Panel::Network => "Connection",
        };
        let header = row![
            column![
                text(title).size(28).color(INK),
                muted("A little closer. On your own terms.")
            ]
            .spacing(5),
            horizontal_space(),
            button("Invite a friend")
                .padding([11, 18])
                .style(button::secondary)
                .on_press(Message::SwitchPanel(Panel::Profile)),
        ]
        .align_y(Alignment::Center);
        let body = column![header, content, muted(self.status_line.clone())]
            .spacing(22)
            .padding(28)
            .height(Length::Fill)
            .width(Length::Fill);
        row![self.view_menu(), body].height(Length::Fill).into()
    }

    fn view_menu(&self) -> Element<'_, Message> {
        let mut navigation = column![
            row![mark(42.0), text("snartnet").size(25).color(Color::WHITE)]
                .spacing(10)
                .align_y(Alignment::Center),
            text("MAKE ROOM FOR CONNECTION")
                .size(9)
                .color(Color::from_rgb8(157, 177, 204)),
            vertical_space().height(28),
        ]
        .spacing(12);
        for (panel, label) in [
            (
                Panel::Messages,
                format!(
                    "Messages  {}",
                    if self.total_unread_count() > 0 {
                        self.total_unread_count().to_string()
                    } else {
                        String::new()
                    }
                ),
            ),
            (Panel::Contacts, "Contacts".into()),
            (Panel::Feed, "Feed".into()),
            (Panel::Profile, "My profile".into()),
            (Panel::Network, "Connection".into()),
        ] {
            let selected = self.panel == panel;
            navigation = navigation.push(
                button(text(label).size(15))
                    .width(Length::Fill)
                    .padding([13, 16])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        button::Style {
                            background: Some(
                                if selected {
                                    ACCENT
                                } else if hovered {
                                    Color::from_rgb8(38, 53, 78)
                                } else {
                                    NAVY
                                }
                                .into(),
                            ),
                            text_color: Color::WHITE,
                            border: iced::Border {
                                radius: 10.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    })
                    .on_press(Message::SwitchPanel(panel)),
            );
        }
        navigation = navigation
            .push(vertical_space())
            .push(
                column![
                    mark(58.0),
                    text("Yours to share.").size(19).color(Color::WHITE),
                    text("Your identity. Your people.\nDirect, encrypted conversations.")
                        .size(12)
                        .color(Color::from_rgb8(170, 189, 213))
                ]
                .spacing(12),
            )
            .push(vertical_space().height(24));
        let name = self
            .state
            .profile
            .as_ref()
            .map(|p| format!("@{}", p.username))
            .unwrap_or_else(|| "Welcome to SnartNet".into());
        navigation = navigation
            .push(text(name).size(13).color(Color::WHITE))
            .push(
                text(if self.syncing {
                    "Syncing with peers…"
                } else if !self.state.network.paused {
                    "Peer sync enabled"
                } else {
                    "Peer sync paused"
                })
                .size(11)
                .color(Color::from_rgb8(162, 237, 209)),
            )
            .push(
                text(concat!(
                    "v",
                    env!("CARGO_PKG_VERSION"),
                    " · ",
                    env!("SNARTNET_BUILD_PROFILE"),
                    "\nBuild ",
                    env!("SNARTNET_BUILD")
                ))
                .size(11)
                .color(Color::from_rgb8(170, 189, 213)),
            );
        container(navigation)
            .padding(22)
            .width(220)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(NAVY.into()),
                ..Default::default()
            })
            .into()
    }

    fn view_messages(&self) -> Element<'_, Message> {
        let selected = self.forms.selected_contact_for_chat.as_ref();
        let contact = selected.and_then(|fp| self.state.contact(fp));
        let mut people = column![
            row![
                text("Conversations").size(18),
                horizontal_space(),
                text(self.state.contacts.len().to_string())
                    .size(13)
                    .color(MUTED)
            ]
            .align_y(Alignment::Center),
            text_input("Find a conversation", &self.forms.search)
                .padding(12)
                .on_input(Message::SearchChanged),
        ]
        .spacing(16);
        let query = self.forms.search.to_lowercase();
        let mut matches = 0;
        for person in self
            .state
            .contacts
            .iter()
            .filter(|c| c.alias.to_lowercase().contains(&query))
        {
            matches += 1;
            let thread = self.state.thread(&person.fingerprint);
            let count = thread.map(|t| t.unread_count).unwrap_or(0);
            // The daemon already decrypted for this frontend, so the newest
            // readable line is also the most useful preview.
            let preview = thread
                .and_then(|t| t.messages.last())
                .map(|m| {
                    if m.incoming {
                        return match &m.plaintext {
                            Ok(body) => body.chars().take(60).collect::<String>(),
                            Err(_) => "Encrypted conversation".to_string(),
                        };
                    }
                    // An outbound message explains itself: the reason a copy could not be
                    // published comes first, then how far it has travelled.
                    if let Some(error) = &m.delivery_error {
                        return format!("Not stored: {error}");
                    }
                    if m.delivery.is_pending() {
                        return "Waiting to send".to_string();
                    }
                    match m.delivery {
                        DeliveryState::Available => "Stored, waiting for the recipient".to_string(),
                        DeliveryState::Stored => "Stored by a contact".to_string(),
                        _ => match &m.plaintext {
                            Ok(body) => body.chars().take(60).collect::<String>(),
                            Err(_) => "Encrypted conversation".to_string(),
                        },
                    }
                })
                .unwrap_or_else(|| "Say hello".to_string());
            let mut content = row![
                contact_avatar(person, 38.0),
                column![text(person.alias.clone()).size(15), muted(preview)].spacing(5)
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            if count > 0 {
                content = content.push(text(count.to_string()).size(13).color(ACCENT));
            }
            let active = selected == Some(&person.fingerprint);
            people = people.push(
                button(content)
                    .width(Length::Fill)
                    .padding(12)
                    .style(move |theme, status| {
                        let mut style = button::text(theme, status);
                        if active {
                            style.background = Some(TINT.into());
                        }
                        style.text_color = INK;
                        style.border.radius = 12.0.into();
                        style
                    })
                    .on_press(Message::SelectChatContact(person.fingerprint.clone())),
            );
        }
        if matches == 0 {
            people = people.push(muted(if self.state.contacts.is_empty() {
                "Your first conversation starts with an invitation."
            } else {
                "No matching conversations."
            }));
        }
        let people = column![
            scrollable(people).height(Length::Fill),
            button("+ Add a contact")
                .padding(12)
                .width(Length::Fill)
                .on_press(Message::SwitchPanel(Panel::Contacts))
        ]
        .spacing(16);
        let sidebar = card(people).width(275).height(Length::Fill);
        let conversation: Element<'_, Message> = if let Some(contact) = contact {
            // The daemon holds every key; a verified peer key is all this
            // frontend needs to know that sending can succeed.
            let ready = self.state.is_ready_to_message(&contact.fingerprint);
            let header = row![
                contact_avatar(contact, 44.0),
                column![
                    text(contact.alias.clone()).size(21),
                    muted(if ready {
                        "End-to-end encrypted"
                    } else {
                        "Connecting · waiting for a verified profile"
                    })
                ]
                .spacing(4),
                horizontal_space()
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            let mut messages = column![].spacing(14).width(Length::Fill);
            if let Some(thread) = self.state.thread(&contact.fingerprint) {
                for item in &thread.messages {
                    // The daemon decrypted this for the local frontend, so the
                    // only failure left is a changed or unverified peer key.
                    let hidden = self.revealed_message_ids.contains(&item.id);
                    let body = if hidden {
                        item.ciphertext.clone()
                    } else {
                        item.plaintext
                            .clone()
                            .unwrap_or_else(|error| format!("Unable to read this message: {error}"))
                    };
                    let meta = if item.incoming {
                        item.created_label.clone()
                    } else {
                        // The reason a copy is missing is part of the state, not a detail: a
                        // message that looks queued because the disk is full must say so.
                        match &item.delivery_error {
                            Some(error) => format!(
                                "{} · {} ({error})",
                                item.created_label,
                                item.delivery.label()
                            ),
                            None => format!("{} · {}", item.created_label, item.delivery.label()),
                        }
                    };
                    let mut meta_row = row![muted(meta), horizontal_space()];
                    // Only an encrypted payload has ciphertext worth revealing;
                    // a plaintext message has nothing to hide behind.
                    if item.encrypted {
                        meta_row = meta_row.push(
                            button(
                                text(if hidden {
                                    "Read message"
                                } else {
                                    "View ciphertext"
                                })
                                .size(10),
                            )
                            .style(button::text)
                            .on_press(Message::ToggleMessageView(item.id.clone())),
                        );
                    }
                    let bubble = column![text(body).size(16), meta_row.align_y(Alignment::Center)]
                        .spacing(9);
                    let incoming = item.incoming;
                    messages =
                        messages.push(
                            container(container(bubble).padding(16).max_width(440).style(
                                move |_| {
                                    design::surface(if incoming {
                                        Color::from_rgb8(245, 247, 251)
                                    } else {
                                        TINT
                                    })
                                },
                            ))
                            .width(Length::Fill)
                            .align_x(if incoming {
                                Alignment::Start
                            } else {
                                Alignment::End
                            }),
                        );
                }
                if thread.messages.is_empty() {
                    messages = messages.push(self.chat_welcome(&contact.alias));
                }
            } else {
                messages = messages.push(self.chat_welcome(&contact.alias));
            }
            let can_send = ready
                && !self.forms.compose_message_input.trim().is_empty()
                && self.sending.is_none();
            let input = text_input("Write a message…", &self.forms.compose_message_input)
                .padding(15)
                .on_input(Message::ComposeMessageChanged)
                .on_submit_maybe(can_send.then_some(Message::SendChat));
            let send = button(if self.sending.is_some() {
                "Sending…"
            } else {
                "Send"
            })
            .padding([15, 20])
            .on_press_maybe(can_send.then_some(Message::SendChat));
            let footer = if ready {
                "Messages stay encrypted on disk. Relayed means a peer accepted the message."
            } else {
                "Keep both apps open. Sync to exchange profile keys before your first message."
            };
            card(
                column![
                    header,
                    iced::widget::horizontal_rule(1),
                    scrollable(messages.padding([8, 0]))
                        .id(scrollable::Id::new("chat-history"))
                        .height(Length::Fill),
                    row![input, send].spacing(10),
                    muted(footer),
                ]
                .spacing(17),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            card(
                container(
                    column![
                        mark(96.0),
                        text("Good conversations start here.").size(26),
                        muted("Add someone you know, exchange an invitation, and say hello."),
                        button("Find your people")
                            .padding([13, 22])
                            .on_press(Message::SwitchPanel(Panel::Contacts))
                    ]
                    .spacing(20)
                    .align_x(Alignment::Center),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        row![sidebar, conversation]
            .spacing(18)
            .height(Length::Fill)
            .into()
    }

    fn chat_welcome(&self, name: &str) -> Element<'static, Message> {
        container(
            column![
                avatar(name, 64.0),
                text(format!("Say hello to {name}")).size(22),
                muted("The beginning of something worth keeping.")
            ]
            .spacing(14)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding([48, 8])
        .into()
    }

    fn view_contacts(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(8);
        for (mode, label) in [
            (AddContactMode::Invite, "Invitation"),
            (AddContactMode::LanPeer, "Nearby"),
            (AddContactMode::Manual, "Fingerprint"),
            (AddContactMode::Magnet, "Magnet"),
        ] {
            let active = self.forms.add_contact_mode == mode;
            tabs = tabs.push(
                button(label)
                    .padding([10, 16])
                    .style(if active {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::AddContactModeChanged(mode)),
            );
        }
        let mut form = column![
            text("Bring your people closer").size(24),
            muted("Choose how you would like to connect."),
            tabs
        ]
        .spacing(18);
        match self.forms.add_contact_mode {
            AddContactMode::Invite => {
                form = form
                    .push(
                        text_input(
                            "Paste a snartnet:// invitation link or invite code",
                            &self.forms.invite_code_input,
                        )
                        .padding(14)
                        .on_input(Message::InviteCodeChanged)
                        .on_submit(Message::ImportFromInvite),
                    )
                    .push(
                        button("Add contact & start chatting")
                            .padding([12, 18])
                            .on_press_maybe(
                                (!self.forms.invite_code_input.trim().is_empty())
                                    .then_some(Message::ImportFromInvite),
                            ),
                    )
                    .push(muted(
                        "Have a QR image? Import a saved PNG or JPG invitation.",
                    ))
                    .push(
                        row![
                            text_input("Path to QR image", &self.forms.qr_path)
                                .padding(12)
                                .on_input(Message::QrPathChanged)
                                .on_submit(Message::ImportQr),
                            button("Import QR").padding(12).on_press_maybe(
                                (!self.forms.qr_path.trim().is_empty())
                                    .then_some(Message::ImportQr)
                            )
                        ]
                        .spacing(10),
                    );
            }
            AddContactMode::Manual => {
                form = form.push(muted("Paste the full fingerprint from your friend's profile. Nearby discovery or a configured peer address is needed to connect."))
                    .push(text_input("Contact fingerprint", &self.forms.contact_fingerprint_input).padding(14).on_input(Message::ContactFingerprintChanged))
                    .push(text_input("Name or nickname", &self.forms.contact_alias_input).padding(14).on_input(Message::ContactAliasChanged).on_submit(Message::AddContact))
                    .push(button("Add contact").padding(12).on_press(Message::AddContact));
            }
            AddContactMode::Magnet => {
                form = form
                    .push(muted(
                        "Import a SnartNet profile magnet. A reachable peer is still required.",
                    ))
                    .push(
                        text_input("magnet:?…", &self.forms.magnet_uri_input)
                            .padding(14)
                            .on_input(Message::MagnetUriChanged)
                            .on_submit(Message::ImportFromMagnet),
                    )
                    .push(
                        button("Add contact")
                            .padding(12)
                            .on_press(Message::ImportFromMagnet),
                    );
            }
            AddContactMode::LanPeer => {
                if self.state.nearby.is_empty() {
                    form = form.push(muted(
                        "No one nearby yet. Open SnartNet on another device on the same network.",
                    ));
                }
                if !self.state.network.discovery {
                    form = form.push(
                        button("Enable nearby discovery")
                            .padding(12)
                            .on_press(Message::LanDiscoveryToggle),
                    );
                }
                for peer in &self.state.nearby {
                    let known = self
                        .state
                        .contacts
                        .iter()
                        .any(|c| c.fingerprint == peer.fingerprint);
                    form = form.push(
                        row![
                            avatar(&peer.alias, 40.0),
                            text(peer.alias.clone()),
                            horizontal_space(),
                            button(if known { "Added" } else { "Connect" })
                                .padding(10)
                                .on_press_maybe((!known).then_some(Message::AddDiscoveredPeer(
                                    peer.fingerprint.clone()
                                )))
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    );
                }
            }
        }
        let mut page = column![
            card(form).width(Length::Fill),
            text(format!("Your contacts · {}", self.state.contacts.len())).size(19)
        ]
        .spacing(22);
        for contact in &self.state.contacts {
            page = page.push(card(
                row![
                    contact_avatar(contact, 44.0),
                    column![
                        text(contact.alias.clone()).size(17),
                        muted(format!(
                            "{} · {}",
                            short_fp(&contact.fingerprint),
                            contact.verification.label()
                        ))
                    ]
                    .spacing(5),
                    horizontal_space(),
                    button("Message")
                        .padding([11, 20])
                        .on_press(Message::SelectChatContact(contact.fingerprint.clone()))
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            ));
        }
        scrollable(page).height(Length::Fill).into()
    }

    fn view_profile(&self) -> Element<'_, Message> {
        let mut form = column![
            text("This is you.").size(25),
            muted("Give your conversations a familiar face."),
            text("Username").size(13),
            text_input("e.g. alex", &self.forms.username_input)
                .padding(13)
                .on_input(Message::UsernameChanged),
            text("Display name").size(13),
            text_input(
                "What should friends call you?",
                &self.forms.display_name_input
            )
            .padding(13)
            .on_input(Message::DisplayNameChanged),
            text("About you").size(13),
            text_input("A little about yourself", &self.forms.bio_input)
                .padding(13)
                .on_input(Message::BioChanged),
        ]
        .spacing(12);
        if let Some(handle) = self
            .forms
            .avatar_data_url
            .as_deref()
            .and_then(image_handle_from_data_url)
        {
            form = form.push(image(handle).width(72).height(72));
        }
        form = form
            .push(text("Profile picture").size(13))
            .push(
                text_input("Path to a PNG or JPG photo", &self.forms.avatar_path_input)
                    .padding(13)
                    .on_input(Message::AvatarPathChanged),
            )
            .push(
                row![
                    button("Choose photo…")
                        .padding(10)
                        .style(button::secondary)
                        .on_press(Message::BrowseForAvatar),
                    button("Use camera")
                        .padding(10)
                        .style(button::secondary)
                        .on_press(Message::CaptureAvatarFromCamera),
                    button("Load photo")
                        .padding(10)
                        .style(button::secondary)
                        .on_press(Message::LoadAvatarFromPath),
                    button("Clear")
                        .padding(10)
                        .style(button::text)
                        .on_press(Message::ClearAvatar)
                ]
                .spacing(8),
            )
            .push(text("Connection address (optional)").size(13))
            .push(
                text_input(
                    "Automatic local network address",
                    &self.forms.advertise_addr,
                )
                .padding(13)
                .on_input(Message::AdvertiseAddrChanged),
            )
            .push(muted(
                "For remote friends, use your reachable IP:port or VPN address.",
            ))
            .push(
                button(if self.saving_profile {
                    "Saving…"
                } else {
                    "Save profile"
                })
                .padding([13, 20])
                .on_press_maybe((!self.saving_profile).then_some(Message::SaveProfile)),
            );
        let share: Element<'_, Message> = match self.invite_uri() {
            Ok(uri) => {
                let fp = self
                    .state
                    .profile
                    .as_ref()
                    .map(|p| p.fingerprint.clone())
                    .unwrap_or_default();
                card(
                    column![
                        text("An invitation to connect").size(21),
                        muted("Scan, share, say hello."),
                        container(svg(generate_qr_svg_handle(&uri)).width(248).height(248))
                            .padding(10)
                            .style(|_| design::surface(Color::WHITE)),
                        button("Copy invitation link")
                            .padding(13)
                            .width(Length::Fill)
                            .on_press(Message::CopyInviteCode),
                        row![
                            button("Save PNG")
                                .padding(10)
                                .style(button::secondary)
                                .on_press(Message::SaveQrPng),
                            button("SVG")
                                .padding(10)
                                .style(button::text)
                                .on_press(Message::SaveQrSvg),
                            button("JPG")
                                .padding(10)
                                .style(button::text)
                                .on_press(Message::SaveQrJpg)
                        ]
                        .spacing(6),
                        muted("Your friend can paste the link or import the QR image in Contacts."),
                        text("YOUR FINGERPRINT").size(10).color(MUTED),
                        text(fp).size(12),
                        text("YOUR IDENTITY LINK").size(10).color(MUTED),
                        text(
                            self.state
                                .identity_uri
                                .clone()
                                .unwrap_or_else(|| "Not published yet".to_string()),
                        )
                        .size(12),
                        muted("Identity links carry no address, so they work from any network."),
                        button("Copy profile magnet")
                            .style(button::text)
                            .on_press(Message::CopyMagnetUri),
                    ]
                    .spacing(16),
                )
                .width(320)
                .into()
            }
            Err(error) => card(
                column![
                    mark(72.0),
                    text("Make it yours.").size(23),
                    muted(if self.state.profile.is_none() {
                        "Save your profile to create a shareable invitation and QR code."
                            .to_string()
                    } else {
                        error
                    })
                ]
                .spacing(20),
            )
            .width(320)
            .into(),
        };
        scrollable(row![card(form).width(Length::Fill), share].spacing(20))
            .height(Length::Fill)
            .into()
    }

    fn view_feed(&self) -> Element<'_, Message> {
        let mut feed = column![card(
            column![
                text("What's on your mind?").size(23),
                text_input(
                    "Share an update with your people",
                    &self.forms.compose_post_input
                )
                .padding(15)
                .on_input(Message::ComposePostChanged),
                button("Publish update").padding([12, 18]).on_press_maybe(
                    (self.state.profile.is_some()
                        && !self.forms.compose_post_input.trim().is_empty())
                    .then_some(Message::CreatePost)
                )
            ]
            .spacing(16)
        )]
        .spacing(18);
        for post in &self.state.posts {
            feed = feed.push(
                card(
                    column![
                        muted(format!(
                            "You · {}",
                            post.post.created_at.format("%d %b %H:%M UTC")
                        )),
                        text(post.post.content.clone()).size(17)
                    ]
                    .spacing(12),
                )
                .width(Length::Fill),
            );
        }
        for contact in &self.state.contacts {
            if contact.synced_post_count > 0 && !contact.latest_post_preview.is_empty() {
                feed = feed.push(
                    card(
                        column![
                            text(contact.alias.clone()).size(16),
                            text(contact.latest_post_preview.clone()).size(17)
                        ]
                        .spacing(12),
                    )
                    .width(Length::Fill),
                );
            }
        }
        if self.state.posts.is_empty() {
            feed = feed.push(muted(
                "A space for the things you want to share. Your updates will appear here.",
            ));
        }
        scrollable(feed).height(Length::Fill).into()
    }

    fn view_network(&self) -> Element<'_, Message> {
        let network = &self.state.network;
        let endpoint = network
            .listening
            .clone()
            .unwrap_or_else(|| "Unavailable".into());
        let dht_label = match &network.dht {
            Some(status) if status.bootstrapped => "bootstrapped".to_string(),
            Some(_) => "starting".to_string(),
            None => "disabled".to_string(),
        };
        let torrent_label = match &network.torrent {
            Some(status) => format!(
                "{} ({}, {} peers)",
                if status.listening {
                    "listening"
                } else {
                    "stopped"
                },
                status.reachability,
                status.peer_count
            ),
            None => "disabled".to_string(),
        };
        let peer_label = match &network.peer {
            Some(status) if status.active => format!(
                "active ({}, {} peers)",
                if status.discovery.is_empty() {
                    "discovery unknown".to_string()
                } else {
                    format!("discovery {}", status.discovery)
                },
                status.peer_count
            ),
            Some(_) => "idle".to_string(),
            None => "disabled".to_string(),
        };
        let mut connection = column![text("Connected on your terms").size(24),
            muted("SnartNet exchanges signed profiles and encrypted messages directly over DHT-discovered torrent peers. The daemon owns the network; this window just shows what it reports."),
            text(format!("Your address: {endpoint}")).size(16),
            text(format!("Known peer addresses: {}", network.peers)).size(16),
            text(format!("DHT: {dht_label} · Torrent: {torrent_label}")).size(16),
            text(format!("Last sync: {}", network.last_sync)).size(16),
            self.sync_mode_row(),
            row![button(if self.syncing { "Syncing…" } else { "Sync now" }).padding(12).on_press_maybe((!self.syncing && !network.paused).then_some(Message::RunSyncNow))].spacing(10),
            muted("Pausing sync keeps outgoing messages queued. The daemon keeps its listener open for peers."),
        ];
        for note in self.subsystem_notes() {
            connection = connection.push(text(note).size(13).color(MUTED));
        }
        connection = connection.spacing(17);
        if let Some(error) = &network.listener_error {
            connection = connection.push(text(error.clone()).color(Color::from_rgb8(184, 53, 70)));
        }
        let nearby = card(column![text("Discover people nearby").size(21), muted("Discovery announces your name, fingerprint, and connection address on your local network."),
            text(format!("{} nearby · {}", self.state.nearby.len(), if network.discovery { "Discovery active" } else { "Discovery inactive" })),
            button(if network.discovery { "Turn off discovery" } else { "Turn on discovery" }).padding(12).style(button::secondary).on_press(Message::LanDiscoveryToggle),
        ].spacing(16));
        scrollable(column![card(connection).width(Length::Fill), nearby,
            card(column![text("Across networks").size(21), muted("Messages use direct torrent peers discovered through public DHT bootstrap nodes. IPv6 and UPnP are attempted automatically. Devices that cannot be reached directly or over torrent also exchange updates over iroh, which hole-punches through NAT or falls back to an iroh relay server (test relays by default). Those connections are authenticated per device: a peer must present a certificate signed by a contact's profile key, so only people you added can reach you."), text(format!("Authenticated peers (iroh): {peer_label}")).size(16), button("Edit invitation address").padding(12).style(button::secondary).on_press(Message::SwitchPanel(Panel::Profile))].spacing(16)),
            button("Clean unused cache files older than 7 days").style(button::text).on_press(Message::CleanupLocalFiles),
        ].spacing(20)).height(Length::Fill).into()
    }

    /// The daemon's scheduler mode as an explicit choice: a frontend must be
    /// able to pick always-on or balanced again, not only toggle a pause.
    fn sync_mode_row(&self) -> Element<'_, Message> {
        let current = self.state.network.sync_mode;
        let mut modes = row![text("Sync mode").size(16)].spacing(10);
        for (mode, label) in [
            (SyncMode::AlwaysOn, "Always on"),
            (SyncMode::Balanced, "Balanced"),
            (SyncMode::Paused, "Paused"),
        ] {
            let active = mode == current;
            modes = modes.push(
                button(label)
                    .padding([8, 14])
                    .style(if active {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press_maybe((!active).then_some(Message::SyncModeChanged(mode))),
            );
        }
        modes.align_y(Alignment::Center).into()
    }

    /// Details behind the one-line summaries, so a stalled subsystem is visible
    /// instead of looking healthy.
    fn subsystem_notes(&self) -> Vec<String> {
        let network = &self.state.network;
        let label = |value: &Option<String>| value.clone().unwrap_or_else(|| "never".to_string());
        let mut notes = Vec::new();
        if let Some(dht) = &network.dht {
            notes.push(format!(
                "DHT last lookup {} · last publish {}",
                label(&dht.last_lookup),
                label(&dht.last_publish)
            ));
            if let Some(error) = &dht.last_error {
                notes.push(format!("DHT error: {error}"));
            }
        }
        if let Some(torrent) = &network.torrent {
            notes.push(format!(
                "Torrent last fetch {} · last publish {}",
                label(&torrent.last_fetch),
                label(&torrent.last_publish)
            ));
            if let Some(error) = &torrent.last_error {
                notes.push(format!("Torrent error: {error}"));
            }
        }
        if let Some(peer) = &network.peer {
            if let Some(node) = &peer.node_id {
                notes.push(format!("Device endpoint: {node}"));
            }
            if let Some(error) = &peer.last_error {
                notes.push(format!("Peer error: {error}"));
            }
        }
        // The delivery summary explains the message states: a queued message with a reason
        // here is a publication problem, not a slow recipient.
        notes.push(format!(
            "Durable publication: {}",
            if network.delivery.durable {
                "available"
            } else {
                "unavailable on this host"
            }
        ));
        if let Some(error) = &network.delivery.failed {
            notes.push(format!("Publish error: {error}"));
        }
        if network.delivery.spooled > 0 {
            notes.push(format!(
                "{} inbound object(s) spooled before acknowledgement",
                network.delivery.spooled
            ));
        }
        // Relay selection is a policy decision, and a relay that keeps failing is the reason a
        // connection is slow rather than broken (M8.2/M8.4).
        if !network.relay.plan.is_empty() {
            notes.push(format!("Relay selection: {}", network.relay.plan));
        }
        if network.relay.disabled {
            // A device with relaying off is only reachable through its direct addresses, which
            // is worth stating rather than discovering during a failed connection.
            notes.push("Relaying is switched off; only direct addresses are used".into());
        }
        if let Some(source) = &network.relay.source {
            notes.push(format!("Relay source: {source}"));
        }
        notes.push(format!(
            "Relays applied to the endpoint: {}",
            network.relay.active.len()
        ));
        for relay in &network.relay.health {
            notes.push(format!(
                "Relay {}: {} · score {} · {} failure(s){}",
                relay.url,
                if relay.connected {
                    "connected"
                } else {
                    "not connected"
                },
                relay.score,
                relay.failures,
                match &relay.last_error {
                    Some(error) => format!(" · {error}"),
                    None => String::new(),
                }
            ));
        }
        // Replication and storage (M9): what this device hosts, how much it holds, and what
        // contacts confirmed for its own objects.
        notes.push(format!(
            "Replica hosting: {} ({} · quota {} MiB · used {} KiB)",
            if network.storage.hosting { "on" } else { "off" },
            if network.storage.platform.is_empty() {
                "platform unknown"
            } else {
                network.storage.platform.as_str()
            },
            network.storage.quota_bytes / (1024 * 1024),
            network.storage.used_bytes / 1024
        ));
        notes.push(format!(
            "Replicas: {} of {} stored · {} lease(s) issued · {} receipt(s)",
            network.storage.stored,
            network.storage.held,
            network.storage.issued,
            network.storage.receipts
        ));
        if let Some(free) = network.storage.free_bytes {
            notes.push(format!("Free space: {} MiB", free / (1024 * 1024)));
        }
        if let Some(note) = &network.storage.note {
            notes.push(format!("Storage: {note}"));
        }
        notes
    }
}
