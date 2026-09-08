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
            return container(
                column![
                    mark(72.0),
                    text("Opening your space").size(26),
                    text(self.status_line.clone()).size(15)
                ]
                .spacing(20),
            )
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
            .profile
            .as_ref()
            .map(|p| format!("@{}", p.profile.username))
            .unwrap_or_else(|| "Welcome to SnartNet".into());
        navigation = navigation
            .push(text(name).size(13).color(Color::WHITE))
            .push(
                text(if self.syncing {
                    "Syncing with peers…"
                } else if self.network.bittorrent_running {
                    "Peer sync enabled"
                } else {
                    "Peer sync paused"
                })
                .size(11)
                .color(Color::from_rgb8(162, 237, 209)),
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
        let contact = selected.and_then(|fp| self.contacts.iter().find(|c| &c.fingerprint == fp));
        let mut people = column![
            row![
                text("Conversations").size(18),
                horizontal_space(),
                text(self.contacts.len().to_string()).size(13).color(MUTED)
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
            .contacts
            .iter()
            .filter(|c| c.alias.to_lowercase().contains(&query))
        {
            matches += 1;
            let thread = self
                .threads
                .iter()
                .find(|t| t.contact_fingerprint == person.fingerprint);
            let count = thread.map(|t| t.unread_count).unwrap_or(0);
            let preview = thread
                .and_then(|t| t.messages.last())
                .map(|m| {
                    if !m.incoming && m.delivery == DeliveryState::Queued {
                        "Waiting to send"
                    } else {
                        "Encrypted conversation"
                    }
                })
                .unwrap_or("Say hello");
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
            people = people.push(muted(if self.contacts.is_empty() {
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
            let ready = self.keypair.is_some()
                && contact.verification == VerificationState::Verified
                && contact.known_encryption_public_key.is_some();
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
            if let Some(thread) = self
                .threads
                .iter()
                .find(|t| t.contact_fingerprint == contact.fingerprint)
            {
                for item in &thread.messages {
                    let hidden = self.revealed_message_ids.contains(&item.id);
                    let body = if hidden {
                        item.content.clone()
                    } else {
                        decrypt_for_display(
                            item,
                            self.keypair.as_ref(),
                            contact.known_encryption_public_key.as_deref(),
                        )
                        .unwrap_or_else(|_| {
                            "Unable to decrypt this message. Check the contact's verified profile."
                                .into()
                        })
                    };
                    let meta = if item.incoming {
                        item.created_label.clone()
                    } else {
                        format!(
                            "{} · {}",
                            item.created_label,
                            if item.delivery == DeliveryState::Relayed {
                                "Relayed"
                            } else {
                                "Queued"
                            }
                        )
                    };
                    let bubble = column![
                        text(body).size(16),
                        row![
                            muted(meta),
                            horizontal_space(),
                            button(
                                text(if hidden {
                                    "Read message"
                                } else {
                                    "View ciphertext"
                                })
                                .size(10)
                            )
                            .style(button::text)
                            .on_press(Message::ToggleMessageView(item.id.clone()))
                        ]
                        .align_y(Alignment::Center)
                    ]
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
                if self.discovered_peers.is_empty() {
                    form = form.push(muted(
                        "No one nearby yet. Open SnartNet on another device on the same network.",
                    ));
                }
                if !self.network.lan_discovery_active {
                    form = form.push(
                        button("Enable nearby discovery")
                            .padding(12)
                            .on_press(Message::LanDiscoveryToggle),
                    );
                }
                for peer in &self.discovered_peers {
                    let known = self
                        .contacts
                        .iter()
                        .any(|c| c.fingerprint == peer.fingerprint);
                    form = form.push(
                        row![
                            avatar(&peer.username, 40.0),
                            text(peer.username.clone()),
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
            text(format!("Your contacts · {}", self.contacts.len())).size(19)
        ]
        .spacing(22);
        for contact in &self.contacts {
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
                    .profile
                    .as_ref()
                    .map(|p| p.profile.fingerprint.clone())
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
                    muted(if self.profile.is_none() {
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
                    (self.profile.is_some() && !self.forms.compose_post_input.trim().is_empty())
                        .then_some(Message::CreatePost)
                )
            ]
            .spacing(16)
        )]
        .spacing(18);
        for post in &self.local_posts {
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
        for contact in &self.contacts {
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
        if self.local_posts.is_empty() {
            feed = feed.push(muted(
                "A space for the things you want to share. Your updates will appear here.",
            ));
        }
        scrollable(feed).height(Length::Fill).into()
    }

    fn view_network(&self) -> Element<'_, Message> {
        let endpoint = self
            .transport
            .advertised_addr()
            .unwrap_or_else(|| "Unavailable".into());
        let (dht_status, torrent_status) = self.transport.distributed_status();
        let dht_label = dht_status
            .map(|status| if status.bootstrapped { "bootstrapped" } else { "starting" })
            .unwrap_or("disabled");
        let torrent_label = torrent_status
            .map(|status| format!("{} ({}, {} peers)", if status.listening { "listening" } else { "stopped" }, status.reachability, status.peer_count))
            .unwrap_or_else(|| "disabled".into());
        let gossip_label = self
            .gossip
            .as_ref()
            .map(|g| g.status())
            .filter(|status| status.active)
            .map(|status| format!("active ({} peers)", status.peer_count))
            .unwrap_or_else(|| "disabled".into());
        let mut connection = column![text("Connected on your terms").size(24),
            muted("SnartNet exchanges signed profiles and encrypted messages directly over DHT-discovered torrent peers."),
            text(format!("Your address: {endpoint}")).size(16),
            text(format!("Known peer addresses: {}", self.transport.peer_snapshot().len())).size(16),
            text(format!("DHT: {dht_label} · Torrent: {torrent_label}")).size(16),
            text(format!("Last sync: {}", self.network.last_poll_label)).size(16),
            row![button(if self.syncing { "Syncing…" } else { "Sync now" }).padding(12).on_press_maybe((!self.syncing && self.network.bittorrent_running).then_some(Message::RunSyncNow)),
                button(if self.network.bittorrent_running { "Pause sync" } else { "Resume sync" }).padding(12).style(button::secondary).on_press(Message::ToggleBittorrent)].spacing(10),
            muted("Pausing sync keeps outgoing messages queued. The TCP listener remains available to peers."),
        ].spacing(17);
        if let Some(error) = &self.listener_error {
            connection = connection.push(text(error.clone()).color(Color::from_rgb8(184, 53, 70)));
        }
        let nearby = card(column![text("Discover people nearby").size(21), muted("Discovery announces your name, fingerprint, and connection address on your local network."),
            text(format!("{} nearby · {}", self.discovered_peers.len(), if self.network.lan_discovery_active { "Discovery active" } else { "Discovery inactive" })),
            button(if self.network.lan_discovery_active { "Turn off discovery" } else { "Turn on discovery" }).padding(12).style(button::secondary).on_press(Message::LanDiscoveryToggle),
        ].spacing(16));
        scrollable(column![card(connection).width(Length::Fill), nearby,
            card(column![text("Across networks").size(21), muted("Messages use direct torrent peers discovered through public DHT bootstrap nodes. IPv6 and UPnP are attempted automatically. If both peers are behind unreachable NAT, use a VPN or configure port forwarding; SnartNet does not operate a relay."), text(format!("Internet discovery (iroh gossip): {gossip_label}")).size(16), button("Edit invitation address").padding(12).style(button::secondary).on_press(Message::SwitchPanel(Panel::Profile))].spacing(16)),
            button("Clean unused cache files older than 7 days").style(button::text).on_press(Message::CleanupLocalFiles),
        ].spacing(20)).height(Length::Fill).into()
    }
}
