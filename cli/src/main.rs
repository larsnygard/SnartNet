use clap::{Parser, Subcommand};
use snartnet_sdk::{Client, DaemonPaths};

#[derive(Parser)]
#[command(name = "snartnet", version, about = "SnartNet daemon administration")]
struct Cli {
    /// Override the daemon data directory
    #[arg(long, global = true, env = "SNARTNET_DATA_DIR")]
    data_dir: Option<String>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Manage the persistent local daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}
#[derive(Subcommand)]
enum DaemonAction {
    /// Run in the foreground
    Run,
    /// Start in the background if needed
    Start,
    /// Show authenticated health and API compatibility
    Status,
    /// Request graceful shutdown
    Stop,
}
fn main() {
    if let Err(error) = execute(Cli::parse()) {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
fn execute(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let paths = DaemonPaths::from_data_dir(cli.data_dir.as_deref())?;
    let Commands::Daemon { action } = cli.command;
    if matches!(action, DaemonAction::Run) {
        snartnet_daemon::run(paths)?;
        return Ok(());
    }
    let client = Client::new(paths)?;
    match action {
        DaemonAction::Run => unreachable!(),
        DaemonAction::Start => {
            client.ensure_running(&std::env::current_exe()?)?;
            println!("SnartNet daemon is running");
        }
        DaemonAction::Status => {
            let health = client.health()?;
            println!("{}", serde_json::to_string_pretty(&health)?);
        }
        DaemonAction::Stop => {
            client.stop()?;
            println!("SnartNet daemon shutdown requested");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_daemon_administration_is_accepted() {
        for action in ["run", "start", "status", "stop"] {
            assert!(Cli::try_parse_from(["snartnet", "daemon", action]).is_ok());
        }
        for action in ["init", "profile", "post", "keys"] {
            assert!(Cli::try_parse_from(["snartnet", action]).is_err());
        }
    }
}
