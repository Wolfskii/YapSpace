//! Entry point for the `yapspace` binary. Subcommands are stubs until their tasks are DONE.

use clap::Parser;
use yapspace_server::cli::{AdminCommand, Cli, Command};
use yapspace_server::config::Config;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let name = match &cli.command {
        Command::Serve => {
            if let Err(e) = Config::from_env() {
                eprintln!("yapspace serve: {e}");
                std::process::exit(2);
            }
            "serve"
        }
        Command::Migrate => "migrate",
        Command::Openapi { .. } => "openapi",
        Command::Healthcheck => "healthcheck",
        Command::Admin {
            command: AdminCommand::SetRole { .. },
        } => "admin set-role",
    };
    eprintln!("yapspace {name}: not implemented yet");
    std::process::exit(1);
}
