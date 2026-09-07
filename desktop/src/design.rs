//! Shared palette and small native components keep spacing, contrast, and shape consistent.
use super::*;
use iced::{Background, Border, Color, Theme};

pub(crate) const INK: Color = Color {
    r: 29.0 / 255.0,
    g: 42.0 / 255.0,
    b: 61.0 / 255.0,
    a: 1.0,
};
pub(crate) const MUTED: Color = Color {
    r: 101.0 / 255.0,
    g: 115.0 / 255.0,
    b: 133.0 / 255.0,
    a: 1.0,
};
pub(crate) const ACCENT: Color = Color {
    r: 64.0 / 255.0,
    g: 86.0 / 255.0,
    b: 218.0 / 255.0,
    a: 1.0,
};
pub(crate) const NAVY: Color = Color {
    r: 21.0 / 255.0,
    g: 32.0 / 255.0,
    b: 52.0 / 255.0,
    a: 1.0,
};
pub(crate) const LINE: Color = Color {
    r: 227.0 / 255.0,
    g: 232.0 / 255.0,
    b: 240.0 / 255.0,
    a: 1.0,
};
pub(crate) const TINT: Color = Color {
    r: 235.0 / 255.0,
    g: 239.0 / 255.0,
    b: 1.0,
    a: 1.0,
};

pub(crate) fn theme() -> Theme {
    Theme::custom(
        "SnartNet Daylight".into(),
        iced::theme::Palette {
            background: Color::from_rgb8(246, 248, 252),
            text: INK,
            primary: ACCENT,
            success: Color::from_rgb8(26, 128, 105),
            danger: Color::from_rgb8(184, 53, 70),
        },
    )
}

pub(crate) fn surface(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 16.0.into(),
        },
        ..Default::default()
    }
}

pub(crate) fn card<'a>(
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Container<'a, Message> {
    container(content)
        .padding(22)
        .style(|_| surface(Color::WHITE))
}

pub(crate) fn muted(
    value: impl iced::widget::text::IntoFragment<'static>,
) -> iced::widget::Text<'static> {
    text(value).size(13).color(MUTED)
}

pub(crate) fn avatar(name: &str, size: f32) -> Element<'static, Message> {
    let initials: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase();
    container(text(initials).size(size * 0.34).color(ACCENT))
        .center_x(size)
        .center_y(size)
        .style(|_| container::Style {
            border: Border {
                radius: 14.0.into(),
                ..Default::default()
            },
            background: Some(TINT.into()),
            ..Default::default()
        })
        .into()
}

pub(crate) fn contact_avatar(contact: &Contact, size: f32) -> Element<'static, Message> {
    if let Some(handle) = contact
        .avatar_data_url
        .as_deref()
        .and_then(image_handle_from_data_url)
    {
        image(handle)
            .width(size)
            .height(size)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        avatar(&contact.alias, size)
    }
}

/// The brand mark is native SVG, so it stays crisp at any desktop scale.
pub(crate) fn mark(size: f32) -> Element<'static, Message> {
    svg(svg::Handle::from_memory(
        include_bytes!("../assets/mark.svg").as_slice(),
    ))
    .width(size)
    .height(size)
    .into()
}
