//! System tray integration: the window is disposable, the daemon is not.
//!
//! Linux uses the freedesktop StatusNotifierItem protocol through `ksni`, which
//! owns its own thread and never blocks the iced event loop, so the daemon keeps
//! syncing while the window is hidden. macOS and Windows need the platform event
//! loop on the main thread; that remains the tracked M4.3 follow-up, so those
//! targets compile with the tray disabled.

use iced::futures::stream;
use iced::futures::Stream;
use std::{pin::Pin, sync::Mutex};

/// What the tray asks the application to do, sent from the tray thread to iced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    /// Bring the window back without restarting anything.
    Show,
    /// Stop this frontend only; background syncing continues in the daemon.
    Quit,
    /// Ask the daemon to stop through its API, then exit.
    StopDaemonAndQuit,
}

/// The command channel is created once, because [`App::subscription`] needs a
/// `fn()` builder that cannot capture the receiver.
static COMMANDS: std::sync::OnceLock<
    Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<TrayCommand>>>,
> = std::sync::OnceLock::new();

/// Handle to the running tray. Dropping it leaves the tray thread alive until
/// the process exits, which is what "hide the window, keep syncing" needs.
pub(crate) struct TrayHandle {
    #[cfg(target_os = "linux")]
    handle: ksni::Handle<Handler>,
}

impl TrayHandle {
    /// Tooltip text follows the same status line the window shows.
    pub(crate) fn set_status(&self, status: &str) {
        #[cfg(target_os = "linux")]
        self.handle
            .update(|handler| handler.status = status.to_string());
        #[cfg(not(target_os = "linux"))]
        let _ = status;
    }
}

/// Stream of tray clicks. iced calls this once per subscription.
pub(crate) fn commands() -> Pin<Box<dyn Stream<Item = TrayCommand> + Send + 'static>> {
    let receiver = COMMANDS
        .get()
        .and_then(|slot| slot.lock().expect("tray channel lock").take());
    match receiver {
        Some(receiver) => Box::pin(stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|command| (command, receiver))
        })),
        // No tray: stay pending instead of producing spurious messages.
        None => Box::pin(stream::pending()),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn spawn() -> Option<TrayHandle> {
    let (commands, receiver) = tokio::sync::mpsc::unbounded_channel();
    *COMMANDS.get_or_init(|| Mutex::new(None)).lock().ok()? = Some(receiver);
    let service = ksni::TrayService::new(Handler {
        commands,
        status: "Starting…".to_string(),
    });
    let handle = service.handle();
    std::thread::Builder::new()
        .name("snartnet-tray".to_string())
        .spawn(move || {
            // A missing StatusNotifierWatcher is a supported desktop state, not a
            // crash: the window keeps working on its own.
            if let Err(error) = service.run() {
                eprintln!("SnartNet tray is unavailable on this desktop: {error}");
            }
        })
        .ok()?;
    Some(TrayHandle { handle })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn spawn() -> Option<TrayHandle> {
    None
}

#[cfg(target_os = "linux")]
struct Handler {
    commands: tokio::sync::mpsc::UnboundedSender<TrayCommand>,
    status: String,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for Handler {
    fn id(&self) -> String {
        "snartnet-desktop".to_string()
    }

    fn title(&self) -> String {
        "SnartNet".to_string()
    }

    fn icon_name(&self) -> String {
        "snartnet".to_string()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "SnartNet".to_string(),
            description: self.status.clone(),
            ..Default::default()
        }
    }

    /// A left click opens the window, matching most desktop conventions.
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.commands.send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            standard_item("Show SnartNet", TrayCommand::Show),
            ksni::MenuItem::Separator,
            standard_item("Quit (daemon keeps running)", TrayCommand::Quit),
            standard_item("Stop the daemon and quit", TrayCommand::StopDaemonAndQuit),
        ]
    }
}

/// Builds one menu entry that forwards a [`TrayCommand`] to the application.
#[cfg(target_os = "linux")]
fn standard_item(label: &str, command: TrayCommand) -> ksni::MenuItem<Handler> {
    ksni::menu::StandardItem {
        label: label.to_string(),
        activate: Box::new(move |handler: &mut Handler| {
            let _ = handler.commands.send(command);
        }),
        ..Default::default()
    }
    .into()
}
