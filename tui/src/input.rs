//! Key mapping: terminal events in, semantic messages out.
//!
//! The mapping is explicit rather than mode-less: while a field has focus, plain
//! characters type and shortcuts need a modifier, so a binding can never steal a
//! character the user meant to type.

use crate::app::{App, Focus, Message, Tab};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Mapped key, or `None` when the key means nothing in the current context.
pub(crate) fn translate(key: KeyEvent, app: &App) -> Option<Message> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        // Ctrl+C always quits the view; the daemon is never stopped implicitly.
        return matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q'))
            .then_some(Message::Quit);
    }
    if key.code == KeyCode::F(1) {
        return Some(Message::ToggleHelp);
    }
    if app.help {
        // While help is open it swallows everything except closing it.
        return matches!(key.code, KeyCode::Esc | KeyCode::Char('?'))
            .then_some(Message::ToggleHelp);
    }

    if app.focus != Focus::None {
        return match key.code {
            KeyCode::Esc => Some(Message::Unfocus),
            KeyCode::Enter => Some(Message::Submit),
            KeyCode::Backspace => Some(Message::Backspace),
            KeyCode::Up => Some(Message::Move(-1)),
            KeyCode::Down => Some(Message::Move(1)),
            KeyCode::Left => neighbour(app, -1),
            KeyCode::Right => neighbour(app, 1),
            // Tab and Shift+Tab walk the fields of the current tab.
            KeyCode::Tab => Some(Message::FocusNext),
            KeyCode::BackTab => Some(Message::FocusPrevious),
            KeyCode::Char(character) => Some(Message::Insert(character)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char(character) => match character {
            '1'..='5' => Tab::from_digit(character).map(Message::SwitchTab),
            'q' => Some(Message::Quit),
            '?' => Some(Message::ToggleHelp),
            'j' => Some(Message::Move(1)),
            'k' => Some(Message::Move(-1)),
            'g' => Some(Message::Move(i32::MIN / 2)),
            'G' => Some(Message::Move(i32::MAX / 2)),
            'i' | 'e' => Some(Message::Focus(Focus::primary(app.tab))),
            'x' => Some(Message::ToggleCiphertext),
            'r' => Some(Message::Refresh),
            'y' => Some(Message::Sync),
            'm' => Some(Message::CycleMode),
            'd' => Some(Message::ToggleDiscovery),
            'c' => Some(Message::Cleanup),
            'u' => Some(Message::Invite),
            'D' => Some(Message::StartDaemon),
            'S' => Some(Message::StopDaemon),
            '[' => neighbour(app, -1),
            ']' => neighbour(app, 1),
            _ => None,
        },
        KeyCode::Up => Some(Message::Move(-1)),
        KeyCode::Down => Some(Message::Move(1)),
        KeyCode::PageUp => Some(Message::Move(-10)),
        KeyCode::PageDown => Some(Message::Move(10)),
        KeyCode::Left => neighbour(app, -1),
        KeyCode::Right => neighbour(app, 1),
        KeyCode::Enter => Some(Message::Submit),
        KeyCode::Tab => Some(Message::CycleTab(1)),
        KeyCode::BackTab => Some(Message::CycleTab(-1)),
        _ => None,
    }
}

/// Conversations keep the order the daemon reported; `[`/`]` and ←/→ walk it.
fn neighbour(app: &App, step: i8) -> Option<Message> {
    let threads = &app.state.threads;
    if threads.is_empty() {
        return None;
    }
    let index = match app.selected_contact.as_deref().and_then(|fingerprint| {
        threads
            .iter()
            .position(|thread| thread.contact_fingerprint == fingerprint)
    }) {
        Some(index) => (index as i8 + step).rem_euclid(threads.len() as i8) as usize,
        None if step < 0 => threads.len() - 1,
        None => 0,
    };
    Some(Message::SelectThread(
        threads[index].contact_fingerprint.clone(),
    ))
}
