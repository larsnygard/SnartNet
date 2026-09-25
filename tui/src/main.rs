//! `snartnet-tui`: a terminal frontend that is only a client of the daemon.
//!
//! This process renders and requests (ADR 0001). Identity, storage, torrents, and
//! the peer listener stay inside the daemon, so quitting this view never
//! interrupts background work.

mod app;
mod daemon;
mod input;
mod state;
mod ui;

#[cfg(test)]
mod tests;

use crate::app::{Action, App, Message};
use crate::daemon::Backend;
use crate::state::DaemonState;
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use snartnet_sdk::{Command, SyncMode};
use std::io::{self, Stdout};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// Long enough to stay cheap, short enough that worker results feel immediate.
const POLL: Duration = Duration::from_millis(100);
/// Retry pace for the push channel while the daemon is unreachable.
const RETRY: Duration = Duration::from_secs(2);

#[derive(Debug, Parser)]
#[command(
    name = "snartnet-tui",
    version,
    about = "Terminal frontend for the SnartNet daemon"
)]
struct Cli {
    /// Data directory of the daemon to attach to.
    #[arg(long, env = "SNARTNET_DATA_DIR", value_name = "DIR")]
    data_dir: Option<String>,
}

/// One daemon call the reducer asked for; the worker runs them one at a time.
enum Job {
    Snapshot,
    Sync,
    SetMode(SyncMode),
    Command(Command),
    Invite,
    StartDaemon,
    StopDaemon,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("snartnet-tui: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let backend = Backend::connect(cli.data_dir.as_deref())?;
    let (messages_tx, messages) = mpsc::channel();
    let (jobs, jobs_rx) = mpsc::channel();

    // The push channel keeps the view honest when other clients change daemon
    // state; the worker turns reducer actions into daemon calls.
    let pushes = {
        let backend = backend.clone();
        let messages = messages_tx.clone();
        std::thread::spawn(move || push_snapshots(backend, messages))
    };
    let worker = {
        let backend = backend.clone();
        std::thread::spawn(move || serve_jobs(backend, jobs_rx, messages_tx))
    };

    let mut screen = Screen::start()?;
    let mut app = App::new();
    let outcome = event_loop(&mut screen, &mut app, &messages, &jobs);

    // Dropping the job sender ends the worker, and the screen restores the
    // terminal on drop. Both threads block on I/O that can outlive this view
    // (a daemon call, an SSE read), and the daemon must keep running, so the
    // process simply exits and lets the OS reclaim them.
    drop(jobs);
    drop(messages);
    let _ = (pushes, worker);
    outcome
}

/// Owns raw mode and the alternate screen, so quitting always restores the shell.
struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Screen {
    fn start() -> Result<Self, String> {
        enable_raw_mode().map_err(|error| format!("cannot enable raw mode: {error}"))?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(format!("cannot enter the alternate screen: {error}"));
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                Err(format!("cannot start the terminal renderer: {error}"))
            }
        }
    }

    fn restore(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        self.restore();
    }
}

fn event_loop(
    screen: &mut Screen,
    app: &mut App,
    messages: &Receiver<Message>,
    jobs: &Sender<Job>,
) -> Result<(), String> {
    loop {
        // Drain everything the daemon produced since the last frame, so one
        // frame never shows a state the reducer has already moved past.
        while let Ok(message) = messages.try_recv() {
            if let Some(action) = app.update(message) {
                dispatch(action, jobs)?;
            }
        }
        screen
            .terminal
            .draw(|frame| ui::render(frame, app))
            .map_err(|error| format!("cannot draw the terminal: {error}"))?;
        if app.quit {
            return Ok(());
        }
        if !event::poll(POLL).map_err(|error| format!("cannot wait for input: {error}"))? {
            continue;
        }
        match event::read().map_err(|error| format!("cannot read input: {error}"))? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(message) = input::translate(key, app) {
                    if let Some(action) = app.update(message) {
                        dispatch(action, jobs)?;
                    }
                }
            }
            _ => {}
        }
    }
}

/// The only place an `Action` becomes a daemon call, so the reducer stays pure.
fn dispatch(action: Action, jobs: &Sender<Job>) -> Result<(), String> {
    let job = match action {
        Action::Refresh => Job::Snapshot,
        Action::Sync => Job::Sync,
        Action::SetMode(mode) => Job::SetMode(mode),
        Action::Command(command) => Job::Command(command),
        Action::Invite => Job::Invite,
        Action::StartDaemon => Job::StartDaemon,
        Action::StopDaemon => Job::StopDaemon,
    };
    jobs.send(job)
        .map_err(|_| "This program's daemon worker stopped.".to_string())
}

/// One daemon call at a time, in request order, so two quick taps cannot race.
fn serve_jobs(backend: Backend, jobs: Receiver<Job>, messages: Sender<Message>) {
    for job in jobs {
        let message = match job {
            Job::Snapshot => load(&backend),
            Job::Sync => Message::Synced(backend.sync()),
            Job::SetMode(requested) => Message::ModeChanged {
                requested,
                result: backend.set_sync_mode(requested),
            },
            Job::Command(command) => {
                let result = backend.command(command.clone());
                Message::CommandFinished { command, result }
            }
            Job::Invite => Message::Invitation(backend.invite()),
            Job::StartDaemon => Message::DaemonStarted(backend.ensure_running()),
            Job::StopDaemon => Message::DaemonStopped(backend.stop()),
        };
        if messages.send(message).is_err() {
            return;
        }
    }
}

/// Daemon pushes every state change; this thread never invents state of its own.
fn push_snapshots(backend: Backend, messages: Sender<Message>) {
    let mut subscription = backend.subscribe();
    loop {
        match subscription.next_snapshot() {
            Ok(snapshot) => {
                let state = DaemonState::from_snapshot(&snapshot);
                if messages.send(Message::snapshot(state)).is_err() {
                    return;
                }
            }
            Err(error) => {
                if messages
                    .send(Message::snapshot(Err(daemon::describe(error))))
                    .is_err()
                {
                    return;
                }
                // The daemon may be down for a while; retry slowly rather than spin.
                std::thread::sleep(RETRY);
            }
        }
    }
}

/// Explicit refresh: identical mapping to the push channel, one snapshot only.
fn load(backend: &Backend) -> Message {
    Message::snapshot(
        backend
            .snapshot()
            .and_then(|snapshot| DaemonState::from_snapshot(&snapshot)),
    )
}
